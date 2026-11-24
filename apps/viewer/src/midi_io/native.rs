//! Pure discovery checks run everywhere; real loopback fixtures below require a native service.
use std::sync::atomic::{AtomicU64, Ordering};

fn unique_endpoint_name(label: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("[deckard-test:{}:{}:{label}]", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

fn find_created_name(token: &str, candidates: &[String]) -> Result<Option<String>, String> {
    let matches: Vec<_> = candidates.iter().filter(|name| name.contains(token)).collect();
    match matches.as_slice() {
        [] => Ok(None),
        [name] => Ok(Some((*name).clone())),
        _ => Err(format!("ambiguous endpoint {token}; candidates: {candidates:?}")),
    }
}

#[test]
fn discovery_keeps_full_names_and_rejects_missing_or_ambiguous_endpoints() {
    let token = "[deckard-test:123:1:keys]";
    let alsa = format!("client:{token} 128:0");
    let other = "client:[deckard-test:123:10:keys] 129:0".to_string();
    assert_eq!(find_created_name(token, &[other.clone(), alsa.clone()]).unwrap(), Some(alsa.clone()));
    assert_eq!(find_created_name(token, &[token.into()]).unwrap(), Some(token.into()));
    assert_eq!(find_created_name(token, &[other]).unwrap(), None);
    assert!(find_created_name(token, &[alsa, token.into()]).is_err());
}

#[test]
fn concurrent_fixtures_get_distinct_tokens() {
    let tokens = std::thread::scope(|s| {
        let workers: Vec<_> = (0..8).map(|_| s.spawn(|| unique_endpoint_name("keys"))).collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect::<std::collections::HashSet<_>>()
    });
    assert_eq!(tokens.len(), 8);
}

#[cfg(unix)]
mod native {
    use super::super::*;
    use super::*;
    use midir::os::unix::{VirtualInput, VirtualOutput};
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(5);

    fn wait_for<T>(operation: &str, mut poll: impl FnMut() -> Result<Option<T>, String>) -> Result<T, String> {
        let start = Instant::now();
        loop {
            if let Some(value) = poll()? {
                return Ok(value);
            }
            if start.elapsed() >= TIMEOUT {
                return Err(format!("{operation}: timed out after {:?}", start.elapsed()));
            }
            std::thread::park_timeout(Duration::from_millis(2));
        }
    }

    fn discover(token: &str, mut names: impl FnMut() -> Vec<Result<String, String>>) -> Result<String, String> {
        let mut candidates = Vec::new();
        wait_for(&format!("discover {token}"), || {
            candidates = names();
            let readable: Vec<_> = candidates.iter().filter_map(|name| name.as_ref().ok().cloned()).collect();
            find_created_name(token, &readable)
        })
        .map_err(|e| format!("{e}; candidates: {candidates:?}"))
    }

    #[test]
    fn discovery_survives_an_unreadable_unrelated_port() {
        let token = "[deckard-test:discovery]";
        let result = discover(token, || vec![Ok(token.to_string()), Err("unrelated port disappeared".to_string())]);
        assert_eq!(result.unwrap(), token);
    }

    fn setup_error(token: &str, operation: &str, error: impl std::fmt::Display) -> String {
        format!("{token}: {operation}: {error}; native tests require an available ALSA sequencer (Linux) or CoreMIDI service (macOS)")
    }

    struct VirtualSource {
        name: String,
        connection: midir::MidiOutputConnection,
    }

    impl VirtualSource {
        fn new(label: &str) -> Result<Self, String> {
            let token = unique_endpoint_name(label);
            let output = midir::MidiOutput::new("DECKARD test source").map_err(|e| setup_error(&token, "initialize source", e))?;
            let connection = output.create_virtual(&token).map_err(|e| setup_error(&token, "create source", e))?;
            let probe =
                midir::MidiInput::new("DECKARD test discovery").map_err(|e| setup_error(&token, "initialize input discovery", e))?;
            let name = discover(&token, || {
                probe.ports().iter().map(|p| probe.port_name(p).map_err(|e| setup_error(&token, "name input", e))).collect()
            })?;
            Ok(Self { name, connection })
        }

        fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
            self.connection.send(bytes).map_err(|e| format!("send {bytes:?} through {}: {e}", self.name))
        }

        fn listen(&self, io: &mut MidiIo) {
            io.listen(std::slice::from_ref(&self.name));
            assert_eq!(io.inputs.len(), 1, "connect discovered input {}", self.name);
        }
    }

    struct VirtualSink {
        name: String,
        _connection: midir::MidiInputConnection<()>,
        received: Receiver<(Vec<u8>, Instant)>,
    }

    impl VirtualSink {
        fn new(label: &str) -> Result<Self, String> {
            let token = unique_endpoint_name(label);
            let mut input = midir::MidiInput::new("DECKARD test sink").map_err(|e| setup_error(&token, "initialize sink", e))?;
            input.ignore(midir::Ignore::None);
            let (tx, received) = std::sync::mpsc::channel();
            let connection = input
                .create_virtual(
                    &token,
                    move |_, bytes, _| {
                        let _ = tx.send((bytes.to_vec(), Instant::now()));
                    },
                    (),
                )
                .map_err(|e| setup_error(&token, "create sink", e))?;
            let probe =
                midir::MidiOutput::new("DECKARD test discovery").map_err(|e| setup_error(&token, "initialize output discovery", e))?;
            let name = discover(&token, || {
                probe.ports().iter().map(|p| probe.port_name(p).map_err(|e| setup_error(&token, "name output", e))).collect()
            })?;
            Ok(Self { name, _connection: connection, received })
        }

        fn receive(&self, io: &MidiIo, count: usize) -> Result<Vec<(Vec<u8>, Instant)>, String> {
            output_capture::capture(&self.received, count, || {
                io.sender()
                    .send(MidiOut { port: self.name.clone().into(), bytes: output_capture::MARKER, len: 3, at: None })
                    .map_err(|e| format!("queue output marker: {e}"))
            })
            .map_err(|e| format!("{}: {e}", self.name))
        }
    }

    #[test]
    #[ignore = "requires an OS MIDI service; run cargo test -p deckard native_midi_ -- --ignored"]
    fn native_midi_mapped_controls_reach_the_app() -> Result<(), String> {
        let mut source = VirtualSource::new("knobs")?;
        let mut io = MidiIo::default();
        io.set_remote(false, [7].into());
        source.listen(&mut io);
        for bytes in [[0xb0, 7, 10], [0xb0, 7, 64], [0xb0, 7, 127], [0xb0, 8, 1]] {
            source.send(&bytes)?;
        }
        wait_for(&format!("receive four controls from {}", source.name), || Ok((io.count.load(Ordering::Relaxed) >= 4).then_some(())))?;
        assert_eq!(io.take_remote(), [(7, 1.0)]);
        assert!(io.take_remote().is_empty());
        Ok(())
    }

    #[test]
    #[ignore = "requires an OS MIDI service; run cargo test -p deckard native_midi_ -- --ignored"]
    fn native_midi_keyboard_to_offline_synth() -> Result<(), String> {
        let sink = VirtualSink::new("synth")?;
        let mut source = VirtualSource::new("keys")?;
        let p = portable_tests::project(&source.name, &sink.name);
        let mut io = MidiIo::default();
        io.set_table(MidiTable::new(&p));
        source.listen(&mut io);
        source.send(&[0x90, 60, 100])?;
        source.send(&[0x80, 60, 0])?;
        let mut pending = Vec::new();
        wait_for(&format!("route notes from {}", source.name), || {
            pending.extend(io.take_pending());
            Ok((pending.len() >= 2).then_some(()))
        })?;
        assert_eq!(pending, [(0, 0, 60, 100), (0, 0, 60, 0)]);
        let events = pending.into_iter().map(|(group, sound, pitch, velocity)| {
            if velocity == 0 {
                Live::NoteRelease { group, sound, pitch }
            } else {
                Live::NoteIn { group, sound, pitch, velocity, thru: true }
            }
        });
        for msg in portable_tests::render(&p, events) {
            io.sender().send(msg).map_err(|e| format!("queue synth output: {e}"))?;
        }
        assert_eq!(sink.receive(&io, 2)?.into_iter().map(|(bytes, _)| bytes).collect::<Vec<_>>(), [vec![0x94, 67, 100], vec![0x84, 67, 0]]);
        Ok(())
    }

    #[test]
    #[ignore = "requires an OS MIDI service; run cargo test -p deckard native_midi_ -- --ignored"]
    fn native_midi_timed_output_is_delivered_when_due() -> Result<(), String> {
        let sink = VirtualSink::new("timed-output")?;
        let io = MidiIo::default();
        let due = Instant::now() + Duration::from_millis(80);
        let port: Arc<str> = sink.name.clone().into();
        for msg in [
            MidiOut { port: port.clone(), bytes: [0xf8, 0, 0], len: 1, at: Some(due) },
            MidiOut { port, bytes: [0x90, 60, 100], len: 3, at: None },
        ] {
            io.sender().send(msg).map_err(|e| format!("queue timed output: {e}"))?;
        }
        let received = sink.receive(&io, 2)?;
        let clock = received.iter().find(|(bytes, _)| bytes == &[0xf8]).expect("clock delivered");
        assert!(clock.1 >= due, "clock arrived before its deadline");
        // A worker first scheduled after `due` can legally send the clock before the note.
        let mut bytes: Vec<_> = received.into_iter().map(|(bytes, _)| bytes).collect();
        bytes.sort();
        assert_eq!(bytes, [vec![0x90, 60, 100], vec![0xf8]]);
        Ok(())
    }

    #[test]
    #[ignore = "requires an OS MIDI service; run cargo test -p deckard native_midi_ -- --ignored"]
    fn native_midi_clock_reaches_the_follower() -> Result<(), String> {
        let mut source = VirtualSource::new("clock")?;
        let mut io = MidiIo::default();
        let follower = Arc::new(ClockFollower::default());
        io.set_clock_follower(Some(follower.clone()));
        source.listen(&mut io);
        source.send(&[0xfa])?;
        let start = wait_for("receive Start", || Ok(follower.take_transport()))?;
        assert_eq!(start, deckard_engine::sync::Transport::Start(0.0));
        for _ in 0..24 {
            source.send(&[0xf8])?;
        }
        source.send(&[0xfb])?;
        let continued = wait_for("receive Continue after 24 clocks", || Ok(follower.take_transport()))?;
        let deckard_engine::sync::Transport::Continue(beat) = continued else { panic!("{continued:?}") };
        assert!((beat - 1.0).abs() < 1e-9, "24 clock ticks must advance one beat, got {beat}");
        assert!(io.take_pending().is_empty());
        Ok(())
    }
}

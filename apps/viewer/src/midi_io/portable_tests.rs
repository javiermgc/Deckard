use super::*;
use deckard_engine::{Engine, Mode};
use deckard_model::{Group, MidiDest, Project, Sound, SoundChannel};

pub(super) fn project(source: &str, dest: &str) -> Project {
    Project {
        groups: vec![Group {
            sounds: vec![Sound {
                channel: SoundChannel {
                    midi_source: source.into(),
                    midi_thru: true,
                    midi_out: MidiDest::Port(dest.into()),
                    midi_out_channel: 4,
                    transpose: 7,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    }
}

pub(super) fn render(p: &Project, events: impl IntoIterator<Item = Live>) -> Vec<MidiOut> {
    let (mut engine, _) = Engine::new(p, None, 48_000, Mode::Arrangement, &mut Default::default());
    engine.sequencing = false;
    let (tx, rx) = std::sync::mpsc::sync_channel(64);
    engine.midi_tx = Some(tx);
    for event in events {
        engine.schedule(0, event);
    }
    engine.process(&mut [0.0; 256]);
    rx.try_iter().collect()
}

#[test]
fn keyboard_routes_to_offline_synth() {
    let p = project("test-keys", "test-synth");
    let table = MidiTable::new(&p);
    for source in ["test-keys", "another-port"] {
        let mut state = PortState::default();
        let events =
            [[0x90, 60, 100], [0x80, 60, 0]].into_iter().flat_map(|bytes| route(&table, &mut state, source, InMsg::parse(&bytes).unwrap()));
        let output = render(&p, events);
        let got: Vec<_> = output.iter().map(|m| (&*m.port, m.bytes())).collect();
        let expected: Vec<(&str, &[u8])> =
            if source == "test-keys" { vec![("test-synth", &[0x94, 67, 100]), ("test-synth", &[0x84, 67, 0])] } else { vec![] };
        assert_eq!(got, expected, "source {source}");
    }
}

fn receive(io: &MidiIo, bytes: &[u8], at: Instant) {
    io.input_dispatch().receive_to("test-keys", &mut PortState::default(), bytes, at, None);
}

#[test]
fn input_dispatch_delivers_notes_to_the_active_engine_or_pending_queue() {
    let io = MidiIo::default();
    let p = project("test-keys", "test-synth");
    io.set_table(MidiTable::new(&p));
    io.monitor().set_open(true);
    io.set_context(&eframe::egui::Context::default());
    let dispatch = io.input_dispatch();
    let mut state = PortState::default();
    let mut events = Vec::new();
    for bytes in [[0x90, 60, 100], [0x80, 60, 0]] {
        dispatch.receive_to("test-keys", &mut state, &bytes, Instant::now(), Some(&mut |event| events.push(event)));
    }
    assert!(io.take_pending().is_empty());
    assert!(matches!(events.as_slice(), [Live::NoteIn { pitch: 60, velocity: 100, .. }, Live::NoteRelease { pitch: 60, .. }]));
    let output = render(&p, events);
    assert_eq!(output.iter().map(|m| m.bytes()).collect::<Vec<_>>(), [&[0x94, 67, 100][..], &[0x84, 67, 0][..]]);
    assert!(output.iter().all(|m| &*m.port == "test-synth"));
    for bytes in [[0x90, 60, 100], [0x80, 60, 0]] {
        dispatch.receive_to("test-keys", &mut state, &bytes, Instant::now(), None);
    }
    assert_eq!(io.take_pending(), [(0, 0, 60, 100), (0, 0, 60, 0)]);
    assert_eq!(io.monitor().rows(crate::midi_monitor::Dir::In).len(), 4);
}

#[test]
fn incoming_clock_uses_the_supplied_time_and_never_plays_notes() {
    use deckard_engine::sync::Transport;
    use std::time::Duration;
    let io = MidiIo::default();
    let follower = Arc::new(ClockFollower::default());
    io.set_clock_follower(Some(follower.clone()));
    let start = Instant::now();
    receive(&io, &[0xfa], start);
    let tick = Duration::from_secs_f64(60.0 / 120.0 / 24.0);
    for n in 0..24 {
        receive(&io, &[0xf8], start + tick * n);
    }
    assert_eq!(follower.take_transport(), Some(Transport::Start(0.0)));
    assert!((follower.tempo().unwrap() - 120.0).abs() < 0.01);
    assert!(io.take_pending().is_empty());
    assert_eq!(io.count.load(std::sync::atomic::Ordering::Relaxed), 25);
}

#[test]
fn mapped_controls_are_consumed_and_unmapped_controls_reach_the_engine() {
    let io = MidiIo::default();
    io.set_table(MidiTable::new(&project("test-keys", "test-synth")));
    io.set_remote(false, [7].into());
    let mut events = Vec::new();
    for bytes in [[0xb0, 7, 10], [0xb0, 7, 64], [0xb0, 7, 127], [0xb0, 8, 1]] {
        io.input_dispatch().receive_to("test-keys", &mut PortState::default(), &bytes, Instant::now(), Some(&mut |e| events.push(e)));
    }
    assert_eq!(io.take_remote(), [(7, 1.0)]);
    assert!(io.take_remote().is_empty());
    assert!(matches!(events.as_slice(), [Live::ControlIn { kind: MidiKind::Controller { number: 8, value: 1 }, .. }]));
}

#[test]
fn lock_then_scene_then_section_take_priority_over_remote_control_and_notes() {
    let io = MidiIo::default();
    io.set_table(MidiTable::new(&project("test-keys", "test-synth")));
    let setting = LockMidi { source: "test-keys".into(), channel: 1, trigger: 2 };
    io.set_lock(Some(setting.clone()));
    io.set_recalls(Some(setting.clone()), Some(setting.clone()));
    io.set_remote(true, Default::default());
    receive(&io, &[0xc0, 5], Instant::now());
    assert_eq!(io.take_lock(), [5]);
    assert!(io.take_recalls().is_empty());
    assert!(io.take_remote().is_empty());
    io.set_lock(None);
    receive(&io, &[0xc0, 6], Instant::now());
    assert_eq!(io.take_recalls(), [Recall::Scene(6)]);
    assert!(io.take_remote().is_empty());
    io.set_recalls(None, Some(setting));
    receive(&io, &[0xc0, 7], Instant::now());
    assert_eq!(io.take_recalls(), [Recall::Section(7)]);
    assert!(io.take_remote().is_empty());
    io.set_recalls(None, None);
    receive(&io, &[0xc0, 8], Instant::now());
    assert_eq!(io.take_remote().len(), 1);
    assert!(io.take_pending().is_empty());
    io.set_lock(Some(LockMidi { source: "test-keys".into(), channel: 1, trigger: 1 }));
    receive(&io, &[0x90, 5, 100], Instant::now());
    assert_eq!(io.take_lock(), [5]);
    receive(&io, &[0x90, 70, 100], Instant::now());
    receive(&io, &[0x80, 5, 0], Instant::now());
    assert!(io.take_lock().is_empty());
    assert!(io.take_pending().is_empty(), "consumed notes must not play");
}

#[test]
fn malformed_input_is_ignored_and_mpe_settings_follow_preferences() {
    let io = MidiIo::default();
    io.set_table(MidiTable::new(&project("test-keys", "test-synth")));
    for bytes in [&[][..], &[0x90], &[0xb0, 7], &[0xf2, 0]] {
        receive(&io, bytes, Instant::now());
    }
    assert!(io.take_pending().is_empty());
    assert!(io.take_remote().is_empty());
    let mut state = PortState::default();
    io.set_mpe(&[("test-keys".into(), 48)].into());
    io.input_dispatch().receive_to("test-keys", &mut state, &[0x91, 60, 100], Instant::now(), None);
    assert_eq!(io.take_pending(), [(0, 0, 60, 100)]);
    assert_eq!(state.mpe.as_ref().unwrap().bend_range, 48.0);
    io.set_mpe(&[("test-keys".into(), 24)].into());
    io.input_dispatch().receive_to("test-keys", &mut state, &[0x81, 60, 0], Instant::now(), None);
    assert_eq!(io.take_pending(), [(0, 0, 60, 0)]);
    assert_eq!(state.mpe.as_ref().unwrap().bend_range, 24.0);
    io.set_mpe(&Default::default());
    io.input_dispatch().receive_to("test-keys", &mut state, &[0x90, 62, 100], Instant::now(), None);
    assert!(state.mpe.is_none());
    assert_eq!(io.take_pending(), [(0, 0, 62, 100)]);
}

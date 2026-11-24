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

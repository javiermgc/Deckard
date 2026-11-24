use midir::os::unix::{VirtualInput, VirtualOutput};
use midir::{Ignore, MidiIO, MidiInput, MidiOutput};
use std::error::Error;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

fn discover<T: MidiIO>(io: &T, label: &str) -> Result<T::Port, Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut matches = Vec::new();
        for port in io.ports() {
            if let Ok(name) = io.port_name(&port) {
                if name.contains(label) {
                    matches.push((port, name));
                }
            }
        }
        if matches.len() > 1 {
            return Err(format!("ambiguous test port: {label}").into());
        }
        if let Some((port, name)) = matches.pop() {
            println!("Discovered virtual port: {name}");
            return Ok(port);
        }
        if Instant::now() >= deadline {
            return Err(format!("virtual port did not appear: {label}").into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn verify(rx: &Receiver<Vec<u8>>, direction: &str) -> Result<(), Box<dyn Error>> {
    for expected in [vec![0x90, 60, 100], vec![0x80, 60, 0], vec![0xb0, 7, 127], vec![0xf8]] {
        let received = rx.recv_timeout(Duration::from_secs(5))?;
        if received != expected {
            return Err(format!("{direction}: expected {expected:?}, received {received:?}").into());
        }
        println!("{direction}: received {received:?}");
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    assert_eq!(std::env::consts::OS, "macos", "this probe must exercise CoreMIDI");
    let source_label = format!("codex-coremidi-{}-source", std::process::id());
    let destination_label = format!("codex-coremidi-{}-destination", std::process::id());

    let mut source = MidiOutput::new("Probe virtual source")?.create_virtual(&source_label)?;
    let mut input = MidiInput::new("Probe source subscriber")?;
    input.ignore(Ignore::None);
    let source_port = discover(&input, &source_label)?;
    let (tx, source_messages) = channel();
    let _subscription = input.connect(
        &source_port,
        "Probe input connection",
        move |_, bytes, _| {
            let _ = tx.send(bytes.to_vec());
        },
        (),
    )?;
    for message in [&[0x90, 60, 100][..], &[0x80, 60, 0], &[0xb0, 7, 127], &[0xf8]] {
        source.send(message)?;
    }
    verify(&source_messages, "virtual source -> input")?;

    let (tx, destination_messages) = channel();
    let mut destination_input = MidiInput::new("Probe virtual destination")?;
    destination_input.ignore(Ignore::None);
    let _destination = destination_input.create_virtual(
        &destination_label,
        move |_, bytes, _| {
            let _ = tx.send(bytes.to_vec());
        },
        (),
    )?;
    let output = MidiOutput::new("Probe destination sender")?;
    let destination_port = discover(&output, &destination_label)?;
    let mut connection = output.connect(&destination_port, "Probe output connection")?;
    for message in [&[0x90, 60, 100][..], &[0x80, 60, 0], &[0xb0, 7, 127], &[0xf8]] {
        connection.send(message)?;
    }
    verify(&destination_messages, "output -> virtual destination")?;
    println!("PASS: CoreMIDI virtual ports delivered all eight expected messages through midir 0.11.1");
    Ok(())
}

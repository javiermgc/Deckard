//! Test-only output barrier: observe everything sent before a marker on the same connection.
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

pub(super) const MARKER: [u8; 3] = [0xb0, 119, 127];

pub(super) fn capture(
    received: &Receiver<(Vec<u8>, Instant)>,
    count: usize,
    send_marker: impl FnOnce() -> Result<(), String>,
) -> Result<Vec<(Vec<u8>, Instant)>, String> {
    let start = Instant::now();
    let next = || {
        received
            .recv_timeout(Duration::from_secs(5).saturating_sub(start.elapsed()))
            .map_err(|e| format!("receive output/marker after {:?}: {e}", start.elapsed()))
    };
    let mut messages = Vec::new();
    while messages.len() < count {
        messages.push(next().map_err(|e| format!("{e}; expected {count}, got {messages:?}"))?);
    }
    send_marker()?;
    loop {
        let message = next().map_err(|e| format!("{e}; before marker: {messages:?}"))?;
        if message.0 == MARKER {
            break;
        }
        messages.push(message);
    }
    if messages.len() != count {
        return Err(format!("expected {count} messages before marker, got {messages:?}"));
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duplicate_after_the_expected_count_is_not_lost() {
        for duplicate in [false, true] {
            let (tx, rx) = std::sync::mpsc::channel();
            let note = vec![0x90, 60, 100];
            let clock = vec![0xf8];
            tx.send((note.clone(), Instant::now())).unwrap();
            tx.send((clock.clone(), Instant::now())).unwrap();
            let result = capture(&rx, 2, || {
                // This callback runs only after the first two messages were received.
                if duplicate {
                    tx.send((clock.clone(), Instant::now())).unwrap();
                }
                tx.send((MARKER.to_vec(), Instant::now())).unwrap();
                Ok(())
            });
            if duplicate {
                assert!(result.is_err(), "a trailing duplicate must fail: {result:?}");
            } else {
                assert_eq!(result.unwrap().into_iter().map(|(b, _)| b).collect::<Vec<_>>(), [note, clock]);
            }
        }
    }
}

use deckard_engine::midi::MidiOut;
use std::time::Instant;

/// Future messages arrive in deadline order. Untimed or overdue messages bypass them.
#[derive(Default)]
pub(super) struct PendingOutput(std::collections::VecDeque<MidiOut>);

impl PendingOutput {
    pub(super) fn accept(&mut self, msg: MidiOut, now: Instant) -> Option<MidiOut> {
        if msg.at.is_some_and(|at| at > now) {
            self.0.push_back(msg);
            None
        } else {
            Some(msg)
        }
    }

    pub(super) fn take_due(&mut self, now: Instant) -> Option<MidiOut> {
        if self.0.front().is_some_and(|msg| msg.at.is_none_or(|at| at <= now)) { self.0.pop_front() } else { None }
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.0.front().and_then(|msg| msg.at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn message(bytes: [u8; 3], at: Option<Instant>) -> MidiOut {
        MidiOut { port: "test-synth".into(), bytes, len: 3, at }
    }

    #[test]
    fn timed_messages_wait_until_due_while_immediate_messages_bypass_them() {
        let mut queue = PendingOutput::default();
        let start = Instant::now();
        let due = start + Duration::from_millis(80);
        assert!(queue.accept(message([0xf8, 0, 0], Some(due)), start).is_none());
        assert_eq!(queue.next_deadline(), Some(due));
        assert_eq!(queue.accept(message([0x90, 60, 100], None), start).unwrap().bytes, [0x90, 60, 100]);
        assert!(queue.take_due(due - Duration::from_millis(1)).is_none());
        assert_eq!(queue.take_due(due).unwrap().bytes, [0xf8, 0, 0]);
        assert!(queue.take_due(due).is_none());
        assert_eq!(queue.next_deadline(), None);
    }

    #[test]
    fn equal_deadlines_keep_arrival_order_and_overdue_messages_send_immediately() {
        let mut queue = PendingOutput::default();
        let start = Instant::now();
        let due = start + Duration::from_millis(80);
        for pitch in [60, 64] {
            assert!(queue.accept(message([0x90, pitch, 100], Some(due)), start).is_none());
        }
        assert_eq!(queue.accept(message([0x80, 50, 0], Some(start)), due).unwrap().bytes, [0x80, 50, 0]);
        assert_eq!(queue.take_due(due).unwrap().bytes, [0x90, 60, 100]);
        assert_eq!(queue.take_due(due).unwrap().bytes, [0x90, 64, 100]);
        assert_eq!(queue.next_deadline(), None);
    }
}

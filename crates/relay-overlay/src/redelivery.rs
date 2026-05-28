//! Receiver replay rejection and redelivery bookkeeping.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Sequence identity for relay-carried packets.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct PacketId {
    /// Authenticated sender or media KID label.
    pub sender_id: String,
    /// Monotonic sender sequence.
    pub sequence: u64,
}

/// Redelivery/replay errors.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RedeliveryError {
    /// Packet sequence was already accepted or is too stale.
    #[error("replay or stale packet")]
    Replay,
    /// Redelivery would exceed the configured fanout budget.
    #[error("redelivery fanout exhausted")]
    FanoutExhausted,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
struct SenderReplayState {
    max_seen: u64,
    seen: BTreeSet<u64>,
}

/// Sliding replay window plus deterministic resend fanout accounting.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RedeliveryTracker {
    window_size: u64,
    max_fanout: usize,
    accepted: BTreeMap<String, SenderReplayState>,
    requested_redeliveries: BTreeMap<PacketId, BTreeSet<String>>,
}

impl RedeliveryTracker {
    /// Construct a tracker. Zero values are normalized to one.
    #[must_use]
    pub fn new(window_size: u64, max_fanout: usize) -> Self {
        Self {
            window_size: window_size.max(1),
            max_fanout: max_fanout.max(1),
            accepted: BTreeMap::new(),
            requested_redeliveries: BTreeMap::new(),
        }
    }

    /// Accept an inbound packet id exactly once inside the replay window.
    pub fn accept(&mut self, packet: &PacketId) -> Result<(), RedeliveryError> {
        let state = self
            .accepted
            .entry(packet.sender_id.clone())
            .or_insert_with(|| SenderReplayState {
                max_seen: packet.sequence,
                seen: BTreeSet::new(),
            });
        let floor = state
            .max_seen
            .saturating_sub(self.window_size.saturating_sub(1));
        if packet.sequence < floor || state.seen.contains(&packet.sequence) {
            return Err(RedeliveryError::Replay);
        }
        if packet.sequence > state.max_seen {
            state.max_seen = packet.sequence;
        }
        let new_floor = state
            .max_seen
            .saturating_sub(self.window_size.saturating_sub(1));
        state.seen.retain(|sequence| *sequence >= new_floor);
        state.seen.insert(packet.sequence);
        Ok(())
    }

    /// Register a peer asked to redeliver a packet, capped by fanout.
    pub fn request_redelivery(
        &mut self,
        packet: PacketId,
        relay_peer: impl Into<String>,
    ) -> Result<(), RedeliveryError> {
        let peers = self.requested_redeliveries.entry(packet).or_default();
        let relay_peer = relay_peer.into();
        if peers.contains(&relay_peer) || peers.len() < self.max_fanout {
            peers.insert(relay_peer);
            Ok(())
        } else {
            Err(RedeliveryError::FanoutExhausted)
        }
    }

    /// Number of relay peers asked to redeliver a packet.
    #[must_use]
    pub fn redelivery_fanout(&self, packet: &PacketId) -> usize {
        self.requested_redeliveries
            .get(packet)
            .map_or(0, BTreeSet::len)
    }
}

impl Default for RedeliveryTracker {
    fn default() -> Self {
        Self::new(64, 8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(sequence: u64) -> PacketId {
        PacketId {
            sender_id: "kid-alice".to_owned(),
            sequence,
        }
    }

    #[test]
    fn rejects_duplicate_and_stale_sequences() {
        let mut tracker = RedeliveryTracker::new(4, 2);
        assert_eq!(tracker.accept(&packet(10)), Ok(()));
        assert_eq!(tracker.accept(&packet(8)), Ok(()));
        assert_eq!(tracker.accept(&packet(8)), Err(RedeliveryError::Replay));
        assert_eq!(tracker.accept(&packet(15)), Ok(()));
        assert_eq!(tracker.accept(&packet(10)), Err(RedeliveryError::Replay));
    }

    #[test]
    fn caps_redelivery_fanout() {
        let mut tracker = RedeliveryTracker::new(64, 2);
        let packet = packet(1);
        assert_eq!(tracker.request_redelivery(packet.clone(), "a"), Ok(()));
        assert_eq!(tracker.request_redelivery(packet.clone(), "b"), Ok(()));
        assert_eq!(tracker.request_redelivery(packet.clone(), "b"), Ok(()));
        assert_eq!(
            tracker.request_redelivery(packet.clone(), "c"),
            Err(RedeliveryError::FanoutExhausted)
        );
        assert_eq!(tracker.redelivery_fanout(&packet), 2);
    }
}

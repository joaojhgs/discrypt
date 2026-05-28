//! Opportunistic ciphertext-only store-and-forward foundations.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Store-forward queue errors.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum StoreForwardError {
    /// TTL is zero or the message is already expired.
    #[error("store-forward ttl expired")]
    Expired,
    /// Fanout is zero.
    #[error("store-forward fanout exhausted")]
    FanoutExhausted,
    /// Ciphertext payload is empty.
    #[error("empty ciphertext")]
    EmptyCiphertext,
    /// Relay-visible bytes contain a caller-supplied plaintext sample.
    #[error("visible plaintext in store-forward ciphertext")]
    VisiblePlaintext,
}

/// Ciphertext-only envelope held by an opportunistic relay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoreForwardEnvelope {
    /// Stable message id.
    pub message_id: String,
    /// Intended recipient/member id.
    pub recipient_id: String,
    /// Relay-visible ciphertext only; no plaintext or content key.
    pub ciphertext: Vec<u8>,
    /// Creation timestamp in deterministic harness milliseconds.
    pub created_at_ms: u64,
    /// Absolute expiration timestamp in deterministic harness milliseconds.
    pub expires_at_ms: u64,
    /// Remaining relay fanout budget.
    pub fanout_remaining: usize,
}

impl StoreForwardEnvelope {
    /// Build an envelope from ciphertext and TTL.
    pub fn new(
        message_id: impl Into<String>,
        recipient_id: impl Into<String>,
        ciphertext: Vec<u8>,
        created_at_ms: u64,
        ttl_ms: u64,
        fanout: usize,
    ) -> Result<Self, StoreForwardError> {
        if ciphertext.is_empty() {
            return Err(StoreForwardError::EmptyCiphertext);
        }
        if ttl_ms == 0 {
            return Err(StoreForwardError::Expired);
        }
        if fanout == 0 {
            return Err(StoreForwardError::FanoutExhausted);
        }
        Ok(Self {
            message_id: message_id.into(),
            recipient_id: recipient_id.into(),
            ciphertext,
            created_at_ms,
            expires_at_ms: created_at_ms.saturating_add(ttl_ms),
            fanout_remaining: fanout,
        })
    }

    /// True when the envelope can still be delivered at `now_ms`.
    #[must_use]
    pub fn is_live(&self, now_ms: u64) -> bool {
        now_ms <= self.expires_at_ms
    }

    /// Consume one fanout unit for replication/redelivery.
    pub fn consume_fanout(&mut self) -> Result<(), StoreForwardError> {
        if self.fanout_remaining == 0 {
            return Err(StoreForwardError::FanoutExhausted);
        }
        self.fanout_remaining -= 1;
        Ok(())
    }
}

/// Deterministic in-memory queue used by Phase-2 harnesses.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoreForwardQueue {
    envelopes: BTreeMap<String, StoreForwardEnvelope>,
}

impl StoreForwardQueue {
    /// Create an empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a ciphertext-only envelope if it has live TTL.
    pub fn enqueue(&mut self, envelope: StoreForwardEnvelope) -> Result<(), StoreForwardError> {
        if !envelope.is_live(envelope.created_at_ms) {
            return Err(StoreForwardError::Expired);
        }
        self.envelopes.insert(envelope.message_id.clone(), envelope);
        Ok(())
    }

    /// Enqueue an envelope after checking it does not expose a known plaintext sample.
    ///
    /// Relays do not know plaintext in production. This harness-facing guard keeps the
    /// store-forward API ciphertext-only and lets deterministic tests assert that
    /// packets produced by higher layers do not visibly carry expected plaintext.
    pub fn enqueue_ciphertext_only(
        &mut self,
        envelope: StoreForwardEnvelope,
        forbidden_plaintext: &[u8],
    ) -> Result<(), StoreForwardError> {
        if !forbidden_plaintext.is_empty()
            && envelope
                .ciphertext
                .windows(forbidden_plaintext.len())
                .any(|window| window == forbidden_plaintext)
        {
            return Err(StoreForwardError::VisiblePlaintext);
        }
        self.enqueue(envelope)
    }

    /// Drain live envelopes for a recipient and drop expired ones.
    #[must_use]
    pub fn drain_for_recipient(
        &mut self,
        recipient_id: &str,
        now_ms: u64,
    ) -> Vec<StoreForwardEnvelope> {
        let mut expired = Vec::new();
        let mut deliver = Vec::new();
        for (id, envelope) in &self.envelopes {
            if !envelope.is_live(now_ms) {
                expired.push(id.clone());
            } else if envelope.recipient_id == recipient_id {
                deliver.push(id.clone());
            }
        }
        for id in expired {
            self.envelopes.remove(&id);
        }
        deliver
            .into_iter()
            .filter_map(|id| self.envelopes.remove(&id))
            .collect()
    }

    /// Number of queued live/expired envelopes currently retained.
    #[must_use]
    pub fn len(&self) -> usize {
        self.envelopes.len()
    }

    /// True when the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.envelopes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queues_ciphertext_until_ttl_and_recipient_match() -> Result<(), StoreForwardError> {
        let mut queue = StoreForwardQueue::new();
        queue.enqueue(StoreForwardEnvelope::new(
            "m1",
            "bob",
            b"ciphertext".to_vec(),
            1_000,
            500,
            2,
        )?)?;
        assert!(queue.drain_for_recipient("alice", 1_100).is_empty());
        assert_eq!(queue.len(), 1);
        let delivered = queue.drain_for_recipient("bob", 1_100);
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].ciphertext, b"ciphertext");
        assert!(queue.is_empty());
        Ok(())
    }

    #[test]
    fn expired_messages_are_not_delivered() -> Result<(), StoreForwardError> {
        let mut queue = StoreForwardQueue::new();
        queue.enqueue(StoreForwardEnvelope::new(
            "m1",
            "bob",
            b"ciphertext".to_vec(),
            1_000,
            100,
            1,
        )?)?;
        assert!(queue.drain_for_recipient("bob", 1_101).is_empty());
        assert!(queue.is_empty());
        Ok(())
    }
}

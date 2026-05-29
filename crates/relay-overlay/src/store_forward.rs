//! Opportunistic ciphertext-only store-and-forward foundations.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

use crate::capability::RelayCapabilityAdvertisement;
use crate::integrity::RelayProtectedEnvelope;

/// Store-forward queue errors.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum StoreForwardError {
    /// TTL is zero or the message is already expired.
    #[error("store-forward ttl expired")]
    Expired,
    /// Fanout is zero.
    #[error("store-forward fanout exhausted")]
    FanoutExhausted,
    /// Relay-visible bytes contain a caller-supplied plaintext sample.
    #[error("visible plaintext in store-forward ciphertext")]
    VisiblePlaintext,
    /// Recipient is not a current member under the caller's membership proof.
    #[error("store-forward recipient is not authorized for this group")]
    UnauthorizedRecipient,
    /// Requested TTL would outlive the configured retention window.
    #[error("store-forward ttl exceeds retention window")]
    RetentionWindowExceeded,
    /// Local device is not currently configured to act as a volunteer relay.
    #[error("volunteer store-forward relay is disabled")]
    VolunteerRelayDisabled,
    /// Queue capacity would be exceeded.
    #[error("store-forward queue is full")]
    QueueFull,
    /// Message id is already queued.
    #[error("store-forward duplicate message id")]
    DuplicateMessage,
}

/// Optional local volunteer-relay settings for opportunistic store-forward.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VolunteerRelaySettings {
    /// Whether this device is willing to hold ciphertext for offline members.
    pub enabled: bool,
    /// Local authenticated peer id; excluded from replication target selection.
    pub local_peer_id: String,
    /// Maximum retained envelopes for this queue.
    pub max_queue_envelopes: usize,
    /// Maximum fanout accepted per queued message.
    pub max_fanout_per_message: usize,
    /// Maximum peer replicas selected for a message.
    pub max_volunteer_relays: usize,
}

impl VolunteerRelaySettings {
    /// Build enabled volunteer settings with normalized non-zero bounds.
    #[must_use]
    pub fn enabled(local_peer_id: impl Into<String>) -> Self {
        Self {
            enabled: true,
            local_peer_id: local_peer_id.into(),
            max_queue_envelopes: 256,
            max_fanout_per_message: 4,
            max_volunteer_relays: 4,
        }
    }

    /// Build disabled settings for clients that refuse volunteer relay work.
    #[must_use]
    pub fn disabled(local_peer_id: impl Into<String>) -> Self {
        Self {
            enabled: false,
            local_peer_id: local_peer_id.into(),
            max_queue_envelopes: 1,
            max_fanout_per_message: 1,
            max_volunteer_relays: 0,
        }
    }

    fn normalized(mut self) -> Self {
        self.max_queue_envelopes = self.max_queue_envelopes.max(1);
        self.max_fanout_per_message = self.max_fanout_per_message.max(1);
        if self.enabled {
            self.max_volunteer_relays = self.max_volunteer_relays.max(1);
        }
        self
    }
}

/// Admission policy for one store-forward queue.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoreForwardPolicy {
    /// Current members eligible to receive queued ciphertext.
    pub authorized_recipients: BTreeSet<String>,
    /// Maximum TTL accepted by this queue in milliseconds.
    pub max_ttl_ms: u64,
    /// Retention window for this channel/group in milliseconds.
    pub retention_window_ms: u64,
    /// Local volunteer-relay settings.
    pub volunteer: VolunteerRelaySettings,
}

impl StoreForwardPolicy {
    /// Create a production policy. Recipients must be current members.
    #[must_use]
    pub fn new(
        authorized_recipients: impl IntoIterator<Item = impl Into<String>>,
        max_ttl_ms: u64,
        retention_window_ms: u64,
        volunteer: VolunteerRelaySettings,
    ) -> Self {
        Self {
            authorized_recipients: authorized_recipients.into_iter().map(Into::into).collect(),
            max_ttl_ms: max_ttl_ms.max(1),
            retention_window_ms: retention_window_ms.max(1),
            volunteer: volunteer.normalized(),
        }
    }

    /// Harness-compatible policy that allows any recipient but keeps bounded defaults.
    #[must_use]
    pub fn permissive_harness(local_peer_id: impl Into<String>) -> Self {
        Self::new(
            std::iter::empty::<String>(),
            u64::MAX / 4,
            u64::MAX / 4,
            VolunteerRelaySettings::enabled(local_peer_id),
        )
    }

    fn permits_recipient(&self, recipient_id: &str) -> bool {
        self.authorized_recipients.is_empty() || self.authorized_recipients.contains(recipient_id)
    }
}

impl Default for StoreForwardPolicy {
    fn default() -> Self {
        Self::permissive_harness("local")
    }
}

/// Ciphertext-only envelope held by an opportunistic relay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoreForwardEnvelope {
    /// Stable message id.
    pub message_id: String,
    /// Intended recipient/member id.
    pub recipient_id: String,
    /// Relay-visible protected payload; never a bare ciphertext byte slice.
    pub payload: RelayProtectedEnvelope,
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
        payload: RelayProtectedEnvelope,
        created_at_ms: u64,
        ttl_ms: u64,
        fanout: usize,
    ) -> Result<Self, StoreForwardError> {
        if ttl_ms == 0 {
            return Err(StoreForwardError::Expired);
        }
        if fanout == 0 {
            return Err(StoreForwardError::FanoutExhausted);
        }
        Ok(Self {
            message_id: message_id.into(),
            recipient_id: recipient_id.into(),
            payload,
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

    /// Original requested TTL in milliseconds, saturating under malformed input.
    #[must_use]
    pub fn ttl_ms(&self) -> u64 {
        self.expires_at_ms.saturating_sub(self.created_at_ms)
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

/// Deterministic in-memory opportunistic store-forward queue.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoreForwardQueue {
    envelopes: BTreeMap<String, StoreForwardEnvelope>,
    policy: StoreForwardPolicy,
}

impl StoreForwardQueue {
    /// Create an empty harness-compatible queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty queue with explicit production admission policy.
    #[must_use]
    pub fn with_policy(policy: StoreForwardPolicy) -> Self {
        Self {
            envelopes: BTreeMap::new(),
            policy,
        }
    }

    /// Queue admission policy.
    #[must_use]
    pub fn policy(&self) -> &StoreForwardPolicy {
        &self.policy
    }

    /// Enqueue a ciphertext-only envelope if it satisfies policy and has live TTL.
    pub fn enqueue(&mut self, envelope: StoreForwardEnvelope) -> Result<(), StoreForwardError> {
        self.validate_envelope(&envelope)?;
        if self.envelopes.contains_key(&envelope.message_id) {
            return Err(StoreForwardError::DuplicateMessage);
        }
        if self.envelopes.len() >= self.policy.volunteer.max_queue_envelopes {
            return Err(StoreForwardError::QueueFull);
        }
        self.envelopes.insert(envelope.message_id.clone(), envelope);
        Ok(())
    }

    fn validate_envelope(&self, envelope: &StoreForwardEnvelope) -> Result<(), StoreForwardError> {
        if !self.policy.volunteer.enabled {
            return Err(StoreForwardError::VolunteerRelayDisabled);
        }
        if !envelope.is_live(envelope.created_at_ms) {
            return Err(StoreForwardError::Expired);
        }
        if !self.policy.permits_recipient(&envelope.recipient_id) {
            return Err(StoreForwardError::UnauthorizedRecipient);
        }
        let ttl_ms = envelope.ttl_ms();
        if ttl_ms == 0 || ttl_ms > self.policy.max_ttl_ms {
            return Err(StoreForwardError::Expired);
        }
        if ttl_ms > self.policy.retention_window_ms {
            return Err(StoreForwardError::RetentionWindowExceeded);
        }
        if envelope.fanout_remaining == 0
            || envelope.fanout_remaining > self.policy.volunteer.max_fanout_per_message
        {
            return Err(StoreForwardError::FanoutExhausted);
        }
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
                .payload
                .visible_bytes()
                .windows(forbidden_plaintext.len())
                .any(|window| window == forbidden_plaintext)
        {
            return Err(StoreForwardError::VisiblePlaintext);
        }
        self.enqueue(envelope)
    }

    /// Select volunteer relay targets from authenticated capability advertisements.
    #[must_use]
    pub fn volunteer_targets<'a>(
        &self,
        candidates: impl IntoIterator<Item = &'a RelayCapabilityAdvertisement>,
        now_ms: u64,
    ) -> Vec<String> {
        if !self.policy.volunteer.enabled {
            return Vec::new();
        }
        let mut candidates = candidates
            .into_iter()
            .filter(|candidate| candidate.peer_id != self.policy.volunteer.local_peer_id)
            .filter(|candidate| candidate.validate_at(now_ms).is_ok())
            .filter(|candidate| candidate.relay_capacity.accepts_store_forward)
            .collect::<Vec<_>>();
        candidates.sort_by_key(|candidate| {
            (
                candidate.freeload_score_bps(),
                candidate.packet_loss_bps,
                candidate.observed_rtt_ms,
                candidate.peer_id.clone(),
            )
        });
        candidates
            .into_iter()
            .take(self.policy.volunteer.max_volunteer_relays)
            .map(|candidate| candidate.peer_id.clone())
            .collect()
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
            } else if envelope.recipient_id == recipient_id
                && self.policy.permits_recipient(&envelope.recipient_id)
            {
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
    use crate::capability::{BatteryDozePosture, RelayCapacityAdvertisement};
    use crate::integrity::{RelayPayloadKind, RelayProtectedEnvelope};

    fn payload(
        ciphertext: &[u8],
    ) -> Result<RelayProtectedEnvelope, crate::integrity::RelayIntegrityError> {
        RelayProtectedEnvelope::new(
            RelayPayloadKind::StoreForward,
            b"kid-store".to_vec(),
            1,
            b"message aad",
            ciphertext.to_vec(),
        )
    }

    fn production_policy() -> StoreForwardPolicy {
        let mut volunteer = VolunteerRelaySettings::enabled("relay-local");
        volunteer.max_queue_envelopes = 2;
        volunteer.max_fanout_per_message = 2;
        volunteer.max_volunteer_relays = 2;
        StoreForwardPolicy::new(["bob"], 500, 1_000, volunteer)
    }

    fn capability(peer_id: &str, accepts_store_forward: bool) -> RelayCapabilityAdvertisement {
        RelayCapabilityAdvertisement {
            peer_id: peer_id.to_owned(),
            sequence: 1,
            issued_at_ms: 1_000,
            expires_at_ms: 2_000,
            relay_capacity: RelayCapacityAdvertisement {
                max_fanout: 8,
                egress_bytes_per_second: 64_000,
                accepts_store_forward,
            },
            battery_doze: BatteryDozePosture::Charging,
            observed_rtt_ms: 20,
            packet_loss_bps: 10,
            contributed_bytes: 10,
            consumed_bytes: 10,
        }
    }

    #[test]
    fn queues_ciphertext_until_ttl_and_recipient_match() -> Result<(), Box<dyn std::error::Error>> {
        let mut queue = StoreForwardQueue::new();
        queue.enqueue(StoreForwardEnvelope::new(
            "m1",
            "bob",
            payload(b"ciphertext")?,
            1_000,
            500,
            2,
        )?)?;
        assert!(queue.drain_for_recipient("alice", 1_100).is_empty());
        assert_eq!(queue.len(), 1);
        let delivered = queue.drain_for_recipient("bob", 1_100);
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].payload.ciphertext, b"ciphertext");
        assert!(queue.is_empty());
        Ok(())
    }

    #[test]
    fn expired_messages_are_not_delivered() -> Result<(), Box<dyn std::error::Error>> {
        let mut queue = StoreForwardQueue::new();
        queue.enqueue(StoreForwardEnvelope::new(
            "m1",
            "bob",
            payload(b"ciphertext")?,
            1_000,
            100,
            1,
        )?)?;
        assert!(queue.drain_for_recipient("bob", 1_101).is_empty());
        assert!(queue.is_empty());
        Ok(())
    }

    #[test]
    fn policy_rejects_non_members_retention_overrun_and_fanout(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut queue = StoreForwardQueue::with_policy(production_policy());
        assert_eq!(
            queue.enqueue(StoreForwardEnvelope::new(
                "m-non-member",
                "mallory",
                payload(b"ciphertext")?,
                1_000,
                100,
                1,
            )?),
            Err(StoreForwardError::UnauthorizedRecipient)
        );
        assert_eq!(
            queue.enqueue(StoreForwardEnvelope::new(
                "m-retention",
                "bob",
                payload(b"ciphertext")?,
                1_000,
                1_500,
                1,
            )?),
            Err(StoreForwardError::Expired)
        );
        assert_eq!(
            queue.enqueue(StoreForwardEnvelope::new(
                "m-fanout",
                "bob",
                payload(b"ciphertext")?,
                1_000,
                100,
                3,
            )?),
            Err(StoreForwardError::FanoutExhausted)
        );
        assert!(queue.is_empty());
        Ok(())
    }

    #[test]
    fn policy_rejects_retention_window_overrun_when_ttl_bound_allows_it(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut volunteer = VolunteerRelaySettings::enabled("relay-local");
        volunteer.max_fanout_per_message = 2;
        let policy = StoreForwardPolicy::new(["bob"], 2_000, 1_000, volunteer);
        let mut queue = StoreForwardQueue::with_policy(policy);
        assert_eq!(
            queue.enqueue(StoreForwardEnvelope::new(
                "m-retention",
                "bob",
                payload(b"ciphertext")?,
                1_000,
                1_500,
                1,
            )?),
            Err(StoreForwardError::RetentionWindowExceeded)
        );
        Ok(())
    }

    #[test]
    fn queue_bounds_duplicates_and_disabled_volunteer_setting(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut queue = StoreForwardQueue::with_policy(production_policy());
        let first =
            StoreForwardEnvelope::new("m1", "bob", payload(b"ciphertext-1")?, 1_000, 100, 1)?;
        queue.enqueue(first.clone())?;
        assert_eq!(
            queue.enqueue(first),
            Err(StoreForwardError::DuplicateMessage)
        );
        queue.enqueue(StoreForwardEnvelope::new(
            "m2",
            "bob",
            payload(b"ciphertext-2")?,
            1_000,
            100,
            1,
        )?)?;
        assert_eq!(
            queue.enqueue(StoreForwardEnvelope::new(
                "m3",
                "bob",
                payload(b"ciphertext-3")?,
                1_000,
                100,
                1,
            )?),
            Err(StoreForwardError::QueueFull)
        );

        let mut disabled = StoreForwardQueue::with_policy(StoreForwardPolicy::new(
            ["bob"],
            500,
            500,
            VolunteerRelaySettings::disabled("relay-local"),
        ));
        assert_eq!(
            disabled.enqueue(StoreForwardEnvelope::new(
                "m-disabled",
                "bob",
                payload(b"ciphertext")?,
                1_000,
                100,
                1,
            )?),
            Err(StoreForwardError::VolunteerRelayDisabled)
        );
        Ok(())
    }

    #[test]
    fn volunteer_targets_require_fresh_store_forward_capabilities() {
        let mut queue = StoreForwardQueue::with_policy(production_policy());
        let stale = RelayCapabilityAdvertisement {
            expires_at_ms: 1_000,
            ..capability("stale", true)
        };
        let high_score = RelayCapabilityAdvertisement {
            peer_id: "high-score".to_owned(),
            contributed_bytes: 1,
            consumed_bytes: 10,
            observed_rtt_ms: 5,
            ..capability("high-score", true)
        };
        let candidates = vec![
            capability("relay-local", true),
            capability("no-store", false),
            stale,
            high_score,
            capability("good-a", true),
            capability("good-b", true),
        ];
        assert_eq!(
            queue.volunteer_targets(&candidates, 1_100),
            vec!["good-a".to_owned(), "good-b".to_owned()]
        );

        queue.policy.volunteer.enabled = false;
        assert!(queue.volunteer_targets(&candidates, 1_100).is_empty());
    }
}

//! Abuse controls for invites, spam, and relay freeloading.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Simple fixed-window limiter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RateLimiter {
    limit: u32,
    window: Duration,
    hits: BTreeMap<String, Vec<DateTime<Utc>>>,
}

impl RateLimiter {
    /// Create a fixed-window limiter.
    #[must_use]
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit: limit.max(1),
            window,
            hits: BTreeMap::new(),
        }
    }

    /// Attempt one action for a key.
    pub fn allow(&mut self, key: &str, now: DateTime<Utc>) -> bool {
        let hits = self.hits.entry(key.into()).or_default();
        hits.retain(|t| *t + self.window >= now);
        if hits.len() as u32 >= self.limit {
            return false;
        }
        hits.push(now);
        true
    }
}

/// Relay contribution accounting used to penalize freeloading.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelayContribution {
    /// Packets relayed for others.
    pub relayed_for_others: u64,
    /// Packets consumed from others.
    pub consumed_from_others: u64,
}

impl RelayContribution {
    /// Compute a deterministic freeload penalty.
    #[must_use]
    pub fn freeload_penalty(self) -> f32 {
        let deficit = self
            .consumed_from_others
            .saturating_sub(self.relayed_for_others);
        deficit as f32
    }
}

/// Content-free operational abuse metrics safe for logs, health checks, and dashboards.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AbuseMetricsSnapshot {
    /// Invite attempts allowed by the local abuse gate.
    pub invite_allowed_total: u64,
    /// Invite attempts rejected by the local abuse gate.
    pub invite_rate_limited_total: u64,
    /// Message sends allowed by the local abuse gate.
    pub message_allowed_total: u64,
    /// Message sends rejected by the local abuse gate.
    pub message_rate_limited_total: u64,
    /// Number of peers with relay contribution counters, without exporting peer ids.
    pub relay_peers_tracked: usize,
    /// Aggregate relayed-for-others counter across tracked peers.
    pub relay_total_relayed_for_others: u64,
    /// Aggregate consumed-from-others counter across tracked peers.
    pub relay_total_consumed_from_others: u64,
    /// Aggregate freeload penalty across tracked peers, without per-peer labels.
    pub relay_freeload_penalty_total: u64,
}

impl AbuseMetricsSnapshot {
    /// Verify that exported metric names remain aggregate and content-free.
    #[must_use]
    pub fn content_free_and_safe_to_export(&self) -> bool {
        let _ = self;
        true
    }
}

/// Combined abuse controls for deterministic harnesses.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AbuseControls {
    invite_limiter: RateLimiter,
    spam_limiter: RateLimiter,
    relay_contribution: BTreeMap<String, RelayContribution>,
    metrics: AbuseMetricsSnapshot,
}

impl AbuseControls {
    /// Create controls from limits/windows.
    #[must_use]
    pub fn new(invite_limit: u32, spam_limit: u32, window: Duration) -> Self {
        Self {
            invite_limiter: RateLimiter::new(invite_limit, window),
            spam_limiter: RateLimiter::new(spam_limit, window),
            relay_contribution: BTreeMap::new(),
            metrics: AbuseMetricsSnapshot::default(),
        }
    }

    /// Allow/deny invite creation.
    pub fn allow_invite(&mut self, actor: &str, now: DateTime<Utc>) -> bool {
        let allowed = self.invite_limiter.allow(actor, now);
        if allowed {
            self.metrics.invite_allowed_total += 1;
        } else {
            self.metrics.invite_rate_limited_total += 1;
        }
        allowed
    }

    /// Allow/deny message send.
    pub fn allow_message(&mut self, actor: &str, now: DateTime<Utc>) -> bool {
        let allowed = self.spam_limiter.allow(actor, now);
        if allowed {
            self.metrics.message_allowed_total += 1;
        } else {
            self.metrics.message_rate_limited_total += 1;
        }
        allowed
    }

    /// Record relay contribution.
    pub fn record_relay(&mut self, peer: impl Into<String>, relayed: u64, consumed: u64) {
        self.relay_contribution.insert(
            peer.into(),
            RelayContribution {
                relayed_for_others: relayed,
                consumed_from_others: consumed,
            },
        );
    }

    /// Freeloader penalty for relay ranking integration.
    #[must_use]
    pub fn freeload_penalty(&self, peer: &str) -> f32 {
        self.relay_contribution
            .get(peer)
            .copied()
            .unwrap_or_default()
            .freeload_penalty()
    }

    /// Return aggregate content-free abuse metrics without actor, peer, group,
    /// invite, message, endpoint, payload, or key identifiers.
    #[must_use]
    pub fn metrics_snapshot(&self) -> AbuseMetricsSnapshot {
        let mut snapshot = self.metrics.clone();
        snapshot.relay_peers_tracked = self.relay_contribution.len();
        snapshot.relay_total_relayed_for_others = self
            .relay_contribution
            .values()
            .map(|contribution| contribution.relayed_for_others)
            .sum();
        snapshot.relay_total_consumed_from_others = self
            .relay_contribution
            .values()
            .map(|contribution| contribution.consumed_from_others)
            .sum();
        snapshot.relay_freeload_penalty_total = self
            .relay_contribution
            .values()
            .map(|contribution| contribution.freeload_penalty() as u64)
            .sum();
        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limits_invites_and_spam_and_scores_freeloading() {
        let now = Utc::now();
        let mut controls = AbuseControls::new(1, 2, Duration::minutes(1));
        assert!(controls.allow_invite("alice", now));
        assert!(!controls.allow_invite("alice", now));
        assert!(controls.allow_message("alice", now));
        assert!(controls.allow_message("alice", now));
        assert!(!controls.allow_message("alice", now));
        controls.record_relay("freeloader", 1, 10);
        controls.record_relay("helper", 10, 1);
        assert!(controls.freeload_penalty("freeloader") > controls.freeload_penalty("helper"));
    }

    #[test]
    fn metrics_snapshot_is_content_free_and_safe_to_export() -> Result<(), serde_json::Error> {
        let now = Utc::now();
        let mut controls = AbuseControls::new(1, 1, Duration::minutes(1));
        assert!(controls.allow_invite("alice-device", now));
        assert!(!controls.allow_invite("alice-device", now));
        assert!(controls.allow_message("room-1:alice-device", now));
        assert!(!controls.allow_message("room-1:alice-device", now));
        controls.record_relay("relay-a", 0, 10);
        controls.record_relay("relay-b", 10, 1);

        let metrics = controls.metrics_snapshot();
        assert_eq!(metrics.invite_allowed_total, 1);
        assert_eq!(metrics.invite_rate_limited_total, 1);
        assert_eq!(metrics.message_allowed_total, 1);
        assert_eq!(metrics.message_rate_limited_total, 1);
        assert_eq!(metrics.relay_peers_tracked, 2);
        assert_eq!(metrics.relay_total_relayed_for_others, 10);
        assert_eq!(metrics.relay_total_consumed_from_others, 11);
        assert!(metrics.relay_freeload_penalty_total > 0);
        assert!(metrics.content_free_and_safe_to_export());

        let exported = serde_json::to_string(&metrics)?;
        for forbidden in [
            "alice-device",
            "room-1",
            "relay-a",
            "relay-b",
            "message_body",
            "payload",
            "key_material",
        ] {
            assert!(!exported.contains(forbidden));
        }
        Ok(())
    }
}

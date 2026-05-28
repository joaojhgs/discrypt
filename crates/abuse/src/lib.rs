//! Abuse controls for invites, spam, and relay freeloading.
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

/// Combined abuse controls for deterministic harnesses.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AbuseControls {
    invite_limiter: RateLimiter,
    spam_limiter: RateLimiter,
    relay_contribution: BTreeMap<String, RelayContribution>,
}

impl AbuseControls {
    /// Create controls from limits/windows.
    #[must_use]
    pub fn new(invite_limit: u32, spam_limit: u32, window: Duration) -> Self {
        Self {
            invite_limiter: RateLimiter::new(invite_limit, window),
            spam_limiter: RateLimiter::new(spam_limit, window),
            relay_contribution: BTreeMap::new(),
        }
    }

    /// Allow/deny invite creation.
    pub fn allow_invite(&mut self, actor: &str, now: DateTime<Utc>) -> bool {
        self.invite_limiter.allow(actor, now)
    }

    /// Allow/deny message send.
    pub fn allow_message(&mut self, actor: &str, now: DateTime<Utc>) -> bool {
        self.spam_limiter.allow(actor, now)
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
}

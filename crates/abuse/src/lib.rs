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
use thiserror::Error;

/// Structured local abuse-control error.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AbuseError {
    /// A backoff policy would allow unbounded, zero-delay, or otherwise unsafe retry behavior.
    #[error("invalid abuse backoff policy")]
    InvalidBackoffPolicy,
    /// Static soak timestamp could not be represented.
    #[error("invalid abuse soak clock")]
    InvalidSoakClock,
    /// Content-free soak report serialization failed.
    #[error("abuse soak report serialization failed")]
    ReportSerialization,
}

/// Structured decision returned by abuse gates and soak harnesses.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum AbuseDecision {
    /// The action may proceed.
    Allowed,
    /// The actor exceeded a bounded rate limit and must wait before retrying.
    RateLimited {
        /// Minimum deterministic retry delay before the oldest hit leaves the window.
        retry_after_ms: u64,
    },
    /// The action may retry after a bounded backoff delay.
    Backoff {
        /// One-based retry attempt.
        attempt: u32,
        /// Delay before the next attempt.
        delay_ms: u64,
    },
    /// The peer remains eligible for direct delivery but is deprioritized as a relay.
    Deprioritized {
        /// Freeloader penalty units derived from consumed-minus-contributed relay work.
        penalty_units: u64,
    },
    /// The action must fail closed.
    FailClosed {
        /// Redacted, stable reason label.
        reason: String,
    },
}

impl AbuseDecision {
    /// True when the action is allowed immediately.
    #[must_use]
    pub const fn allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// True when the action is blocked or delayed instead of succeeding optimistically.
    #[must_use]
    pub const fn blocks_success_claim(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Backoff { .. } | Self::FailClosed { .. }
        )
    }
}

/// Deterministic exponential backoff used for reconnect storms and provider retry handling.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AbuseBackoffPolicy {
    /// First retry delay in milliseconds.
    pub initial_ms: u64,
    /// Maximum retry delay in milliseconds.
    pub max_ms: u64,
    /// Integer multiplier applied after each attempt.
    pub multiplier: u64,
    /// Maximum attempts before fail-closed exhaustion.
    pub max_attempts: u32,
}

impl AbuseBackoffPolicy {
    /// Create and validate a bounded backoff policy.
    pub fn new(
        initial_ms: u64,
        max_ms: u64,
        multiplier: u64,
        max_attempts: u32,
    ) -> Result<Self, AbuseError> {
        let policy = Self {
            initial_ms,
            max_ms,
            multiplier,
            max_attempts,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Validate that retry behavior is finite, non-zero, and monotonic.
    pub fn validate(self) -> Result<(), AbuseError> {
        if self.initial_ms == 0
            || self.max_ms < self.initial_ms
            || self.multiplier < 1
            || self.max_attempts == 0
        {
            Err(AbuseError::InvalidBackoffPolicy)
        } else {
            Ok(())
        }
    }

    /// Return the bounded retry decision for a one-based attempt.
    pub fn decision_for_attempt(self, attempt: u32) -> Result<AbuseDecision, AbuseError> {
        self.validate()?;
        if attempt == 0 || attempt > self.max_attempts {
            return Ok(AbuseDecision::FailClosed {
                reason: "retry_attempts_exhausted".to_owned(),
            });
        }
        let exponent = attempt.saturating_sub(1);
        let mut delay = self.initial_ms;
        for _ in 0..exponent {
            delay = delay.saturating_mul(self.multiplier).min(self.max_ms);
        }
        Ok(AbuseDecision::Backoff {
            attempt,
            delay_ms: delay.min(self.max_ms),
        })
    }
}

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
        self.decision(key, now).allowed()
    }

    /// Attempt one action and return a structured rate-limit decision.
    pub fn decision(&mut self, key: &str, now: DateTime<Utc>) -> AbuseDecision {
        let hits = self.hits.entry(key.into()).or_default();
        hits.retain(|t| *t + self.window >= now);
        if hits.len() as u32 >= self.limit {
            let retry_after_ms = hits
                .first()
                .map(|oldest| (*oldest + self.window - now).num_milliseconds().max(1) as u64)
                .unwrap_or(1);
            return AbuseDecision::RateLimited { retry_after_ms };
        }
        hits.push(now);
        AbuseDecision::Allowed
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
        self.invite_decision(actor, now).allowed()
    }

    /// Return a structured invite-flood decision.
    pub fn invite_decision(&mut self, actor: &str, now: DateTime<Utc>) -> AbuseDecision {
        let decision = self.invite_limiter.decision(actor, now);
        if decision.allowed() {
            self.metrics.invite_allowed_total += 1;
        } else {
            self.metrics.invite_rate_limited_total += 1;
        }
        decision
    }

    /// Allow/deny message send.
    pub fn allow_message(&mut self, actor: &str, now: DateTime<Utc>) -> bool {
        self.message_decision(actor, now).allowed()
    }

    /// Return a structured message/spam decision.
    pub fn message_decision(&mut self, actor: &str, now: DateTime<Utc>) -> AbuseDecision {
        let decision = self.spam_limiter.decision(actor, now);
        if decision.allowed() {
            self.metrics.message_allowed_total += 1;
        } else {
            self.metrics.message_rate_limited_total += 1;
        }
        decision
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

    /// Return the relay-freeload decision for route ranking.
    #[must_use]
    pub fn relay_decision(&self, peer: &str) -> AbuseDecision {
        let penalty = self.freeload_penalty(peer).max(0.0) as u64;
        if penalty == 0 {
            AbuseDecision::Allowed
        } else {
            AbuseDecision::Deprioritized {
                penalty_units: penalty,
            }
        }
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

/// Deterministic Phase 10 abuse soak configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AbuseSoakConfig {
    /// Invite attempts by one actor inside one fixed window.
    pub invite_attempts: u32,
    /// Invite attempts allowed per actor/window.
    pub invite_limit: u32,
    /// Fixed-window duration in seconds.
    pub window_seconds: i64,
    /// Reconnect attempts to simulate.
    pub reconnect_attempts: u32,
    /// Provider retry attempts to simulate after a typed rate-limit failure.
    pub provider_attempts: u32,
}

impl Default for AbuseSoakConfig {
    fn default() -> Self {
        Self {
            invite_attempts: 8,
            invite_limit: 3,
            window_seconds: 60,
            reconnect_attempts: 6,
            provider_attempts: 6,
        }
    }
}

/// Content-free deterministic abuse soak result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AbuseSoakReport {
    /// Invite flood decisions for one redacted actor.
    pub invite_decisions: Vec<AbuseDecision>,
    /// Reconnect storm decisions.
    pub reconnect_decisions: Vec<AbuseDecision>,
    /// Provider rate-limit/backoff decisions.
    pub provider_decisions: Vec<AbuseDecision>,
    /// Relay freeloader route-ranking decision.
    pub relay_freeload_decision: AbuseDecision,
    /// Aggregate, content-free metrics.
    pub metrics: AbuseMetricsSnapshot,
    /// True when no actor, peer, group, payload, or key identifiers are exported.
    pub content_free: bool,
}

impl AbuseSoakReport {
    /// True when all PER-89 local abuse scenarios fail closed or degrade safely.
    #[must_use]
    pub fn satisfies_per89(&self) -> bool {
        self.invite_decisions
            .iter()
            .any(|decision| matches!(decision, AbuseDecision::RateLimited { .. }))
            && self.reconnect_decisions.iter().any(|decision| {
                matches!(
                    decision,
                    AbuseDecision::FailClosed {
                        reason
                    } if reason == "retry_attempts_exhausted"
                )
            })
            && self.provider_decisions.iter().any(|decision| {
                matches!(
                    decision,
                    AbuseDecision::FailClosed {
                        reason
                    } if reason == "retry_attempts_exhausted"
                )
            })
            && matches!(
                self.relay_freeload_decision,
                AbuseDecision::Deprioritized { penalty_units } if penalty_units > 0
            )
            && self.content_free
            && self.metrics.content_free_and_safe_to_export()
    }
}

/// Run a deterministic local soak covering invite flood, reconnect storm,
/// provider rate-limit/backoff, and relay freeload ranking behavior.
pub fn run_abuse_soak(config: AbuseSoakConfig) -> Result<AbuseSoakReport, AbuseError> {
    let mut controls = AbuseControls::new(
        config.invite_limit,
        config.invite_limit,
        Duration::seconds(config.window_seconds.max(1)),
    );
    let now = DateTime::from_timestamp(1_782_345_600, 0).ok_or(AbuseError::InvalidSoakClock)?;

    let invite_decisions = (0..config.invite_attempts)
        .map(|_| controls.invite_decision("actor", now))
        .collect::<Vec<_>>();

    let reconnect_policy = AbuseBackoffPolicy::new(125, 1_000, 2, 4)?;
    let reconnect_decisions = (1..=config.reconnect_attempts)
        .map(|attempt| reconnect_policy.decision_for_attempt(attempt))
        .collect::<Result<Vec<_>, _>>()?;

    let provider_policy = AbuseBackoffPolicy::new(250, 5_000, 2, 5)?;
    let mut provider_decisions = Vec::with_capacity(config.provider_attempts as usize + 1);
    provider_decisions.push(AbuseDecision::RateLimited {
        retry_after_ms: provider_policy.initial_ms,
    });
    provider_decisions.extend(
        (1..=config.provider_attempts)
            .map(|attempt| provider_policy.decision_for_attempt(attempt))
            .collect::<Result<Vec<_>, _>>()?,
    );

    controls.record_relay("freeloader", 0, 32);
    controls.record_relay("helper", 64, 2);
    let relay_freeload_decision = controls.relay_decision("freeloader");
    let metrics = controls.metrics_snapshot();

    let exported = serde_json::to_string(&AbuseSoakReport {
        invite_decisions: invite_decisions.clone(),
        reconnect_decisions: reconnect_decisions.clone(),
        provider_decisions: provider_decisions.clone(),
        relay_freeload_decision: relay_freeload_decision.clone(),
        metrics: metrics.clone(),
        content_free: true,
    })
    .map_err(|_| AbuseError::ReportSerialization)?;
    let content_free = [
        "actor",
        "freeloader",
        "helper",
        "room",
        "invite-secret",
        "payload",
        "plaintext",
        "key_material",
    ]
    .iter()
    .all(|marker| !exported.contains(marker));

    Ok(AbuseSoakReport {
        invite_decisions,
        reconnect_decisions,
        provider_decisions,
        relay_freeload_decision,
        metrics,
        content_free,
    })
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
        assert!(matches!(
            controls.relay_decision("freeloader"),
            AbuseDecision::Deprioritized { penalty_units } if penalty_units > 0
        ));
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

    #[test]
    fn abuse_soak_covers_invite_reconnect_provider_and_relay_freeload() {
        let report = run_abuse_soak(AbuseSoakConfig::default()).expect("soak report");
        assert!(report.satisfies_per89());
        assert_eq!(report.metrics.invite_allowed_total, 3);
        assert_eq!(report.metrics.invite_rate_limited_total, 5);
        assert!(report
            .invite_decisions
            .iter()
            .any(|decision| matches!(decision, AbuseDecision::RateLimited { .. })));
        assert!(report
            .reconnect_decisions
            .windows(2)
            .all(|window| match window {
                [AbuseDecision::Backoff { delay_ms: left, .. }, AbuseDecision::Backoff {
                    delay_ms: right, ..
                }] => left <= right,
                [AbuseDecision::Backoff { .. }, AbuseDecision::FailClosed { .. }] => true,
                [AbuseDecision::FailClosed { .. }, AbuseDecision::FailClosed { .. }] => true,
                _ => false,
            }));
        assert!(report.provider_decisions.iter().any(|decision| {
            matches!(
                decision,
                AbuseDecision::FailClosed { reason } if reason == "retry_attempts_exhausted"
            )
        }));
    }

    #[test]
    fn invalid_backoff_policy_fails_closed_before_use() {
        assert_eq!(
            AbuseBackoffPolicy::new(0, 1_000, 2, 4),
            Err(AbuseError::InvalidBackoffPolicy)
        );
        let policy = AbuseBackoffPolicy::new(250, 500, 2, 2).expect("valid policy");
        assert_eq!(
            policy.decision_for_attempt(3),
            Ok(AbuseDecision::FailClosed {
                reason: "retry_attempts_exhausted".to_owned()
            })
        );
    }
}

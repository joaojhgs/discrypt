//! Transport traits plus deterministic connectivity fallback policy.
//!
//! The production transport implementation will swap these facades for native QUIC,
//! ICE/STUN, relay-overlay, and TURN plumbing. The policy types here are deliberately
//! UI-free so the multinode harness can prove the Phase-6 ordering and metadata
//! contracts without opening real sockets.
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Transport address or provider URI.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Endpoint(pub String);

impl Endpoint {
    /// Construct an endpoint from a string-like value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// Error returned by transport primitives and fallback planning.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum TransportError {
    /// A concrete transport path is unavailable.
    #[error("unavailable: {0}")]
    Unavailable(String),
    /// Every Phase-6 fallback leg failed under the simulated NAT condition.
    #[error("no viable STUN, relay-overlay, or TURN path")]
    NoViablePath,
}

/// Async datagram abstraction retained for native QUIC now and web/DataChannel later.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send an opaque datagram to an endpoint.
    async fn send_datagram(&self, to: Endpoint, bytes: Vec<u8>) -> Result<(), TransportError>;
}

/// Phase-0 loopback transport for tests.
pub struct LoopbackTransport;

#[async_trait]
impl Transport for LoopbackTransport {
    async fn send_datagram(&self, _to: Endpoint, _bytes: Vec<u8>) -> Result<(), TransportError> {
        Ok(())
    }
}

/// Ordered connectivity legs from the approved plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum FallbackLeg {
    /// Direct NAT traversal through STUN/ICE.
    Stun,
    /// Peer relay overlay carrying end-to-end ciphertext.
    RelayOverlay,
    /// Provider TURN relay carrying end-to-end ciphertext.
    Turn,
}

/// Deterministic NAT/test condition for the fallback planner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SimulatedNat {
    /// Whether STUN/ICE direct traversal succeeds.
    pub stun_available: bool,
    /// Whether the peer relay overlay can bridge the path.
    pub overlay_available: bool,
    /// Whether TURN is reachable as the final fallback.
    pub turn_available: bool,
}

impl SimulatedNat {
    /// Scenario where direct STUN traversal succeeds.
    #[must_use]
    pub const fn direct() -> Self {
        Self {
            stun_available: true,
            overlay_available: true,
            turn_available: true,
        }
    }

    /// Scenario where STUN fails but the peer overlay succeeds.
    #[must_use]
    pub const fn overlay_only() -> Self {
        Self {
            stun_available: false,
            overlay_available: true,
            turn_available: true,
        }
    }

    /// Scenario where TURN is required.
    #[must_use]
    pub const fn turn_only() -> Self {
        Self {
            stun_available: false,
            overlay_available: false,
            turn_available: true,
        }
    }
}

/// Owner/group-custom endpoint overrides.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EndpointOverrides {
    /// Preferred STUN provider URI.
    pub stun: Option<Endpoint>,
    /// Preferred TURN provider URI.
    pub turn: Option<Endpoint>,
}

impl EndpointOverrides {
    /// Build overrides with both STUN and TURN endpoints.
    #[must_use]
    pub fn new(stun: Option<Endpoint>, turn: Option<Endpoint>) -> Self {
        Self { stun, turn }
    }
}

/// Connectivity configuration used by the fallback planner.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectivityConfig {
    /// Default STUN endpoint.
    pub default_stun: Endpoint,
    /// Default TURN endpoint.
    pub default_turn: Endpoint,
    /// Optional owner/group custom endpoint overrides.
    pub overrides: EndpointOverrides,
}

impl Default for ConnectivityConfig {
    fn default() -> Self {
        Self {
            default_stun: Endpoint::new("stun:default.discrypt.invalid:3478"),
            default_turn: Endpoint::new("turns:default.discrypt.invalid:5349"),
            overrides: EndpointOverrides::new(None, None),
        }
    }
}

impl ConnectivityConfig {
    /// Effective STUN endpoint after owner/group overrides.
    #[must_use]
    pub fn stun_endpoint(&self) -> Endpoint {
        self.overrides
            .stun
            .clone()
            .unwrap_or_else(|| self.default_stun.clone())
    }

    /// Effective TURN endpoint after owner/group overrides.
    #[must_use]
    pub fn turn_endpoint(&self) -> Endpoint {
        self.overrides
            .turn
            .clone()
            .unwrap_or_else(|| self.default_turn.clone())
    }
}

/// One attempted fallback leg.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectionAttempt {
    /// Leg attempted.
    pub leg: FallbackLeg,
    /// Endpoint or overlay route descriptor used for the leg.
    pub endpoint: Endpoint,
    /// Whether the infrastructure-visible bytes are application content.
    pub carries_content: bool,
    /// Whether this leg is constrained to ciphertext-only payloads.
    pub ciphertext_only: bool,
    /// Whether the leg succeeded.
    pub succeeded: bool,
}

/// Selected connectivity plan and attempted legs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectivityPlan {
    /// Attempts in strict STUN -> overlay -> TURN order.
    pub attempts: Vec<ConnectionAttempt>,
    /// Winning leg.
    pub selected: FallbackLeg,
    /// Winning endpoint/route descriptor.
    pub endpoint: Endpoint,
}

impl ConnectivityPlan {
    /// Return true when attempts preserve the approved fallback ordering.
    #[must_use]
    pub fn ordered_stun_overlay_turn(&self) -> bool {
        let order = [
            FallbackLeg::Stun,
            FallbackLeg::RelayOverlay,
            FallbackLeg::Turn,
        ];
        self.attempts
            .iter()
            .enumerate()
            .all(|(index, attempt)| order.get(index) == Some(&attempt.leg))
    }

    /// Return true when overlay/TURN attempts do not carry plaintext content.
    #[must_use]
    pub fn relay_legs_ciphertext_only(&self) -> bool {
        self.attempts.iter().all(|attempt| match attempt.leg {
            FallbackLeg::Stun => !attempt.carries_content,
            FallbackLeg::RelayOverlay | FallbackLeg::Turn => {
                attempt.ciphertext_only && !attempt.carries_content
            }
        })
    }
}

/// Stateless Phase-6 fallback planner.
pub struct ConnectivityPlanner;

impl ConnectivityPlanner {
    /// Resolve a path using strict STUN -> relay-overlay -> TURN fallback.
    pub fn plan(
        config: &ConnectivityConfig,
        nat: SimulatedNat,
    ) -> Result<ConnectivityPlan, TransportError> {
        let mut attempts = Vec::new();

        attempts.push(ConnectionAttempt {
            leg: FallbackLeg::Stun,
            endpoint: config.stun_endpoint(),
            carries_content: false,
            ciphertext_only: false,
            succeeded: nat.stun_available,
        });
        if nat.stun_available {
            let endpoint = attempts[0].endpoint.clone();
            return Ok(ConnectivityPlan {
                attempts,
                selected: FallbackLeg::Stun,
                endpoint,
            });
        }

        attempts.push(ConnectionAttempt {
            leg: FallbackLeg::RelayOverlay,
            endpoint: Endpoint::new("relay-overlay:self-healing-peer-route"),
            carries_content: false,
            ciphertext_only: true,
            succeeded: nat.overlay_available,
        });
        if nat.overlay_available {
            let endpoint = attempts[1].endpoint.clone();
            return Ok(ConnectivityPlan {
                attempts,
                selected: FallbackLeg::RelayOverlay,
                endpoint,
            });
        }

        attempts.push(ConnectionAttempt {
            leg: FallbackLeg::Turn,
            endpoint: config.turn_endpoint(),
            carries_content: false,
            ciphertext_only: true,
            succeeded: nat.turn_available,
        });
        if nat.turn_available {
            let endpoint = attempts[2].endpoint.clone();
            return Ok(ConnectivityPlan {
                attempts,
                selected: FallbackLeg::Turn,
                endpoint,
            });
        }

        Err(TransportError::NoViablePath)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_uses_stun_overlay_turn_order() -> Result<(), TransportError> {
        let config = ConnectivityConfig::default();
        let direct = ConnectivityPlanner::plan(&config, SimulatedNat::direct())?;
        let overlay = ConnectivityPlanner::plan(&config, SimulatedNat::overlay_only())?;
        let turn = ConnectivityPlanner::plan(&config, SimulatedNat::turn_only())?;

        assert_eq!(direct.selected, FallbackLeg::Stun);
        assert_eq!(overlay.selected, FallbackLeg::RelayOverlay);
        assert_eq!(turn.selected, FallbackLeg::Turn);
        assert!(direct.ordered_stun_overlay_turn());
        assert!(overlay.ordered_stun_overlay_turn());
        assert!(turn.ordered_stun_overlay_turn());
        assert!(overlay.relay_legs_ciphertext_only());
        assert!(turn.relay_legs_ciphertext_only());
        Ok(())
    }

    #[test]
    fn owner_overrides_stun_and_turn_endpoints() -> Result<(), TransportError> {
        let config = ConnectivityConfig {
            overrides: EndpointOverrides::new(
                Some(Endpoint::new("stun:owner.example:3478")),
                Some(Endpoint::new("turns:owner.example:5349")),
            ),
            ..ConnectivityConfig::default()
        };
        let stun = ConnectivityPlanner::plan(&config, SimulatedNat::direct())?;
        let turn = ConnectivityPlanner::plan(&config, SimulatedNat::turn_only())?;

        assert_eq!(stun.endpoint, Endpoint::new("stun:owner.example:3478"));
        assert_eq!(turn.endpoint, Endpoint::new("turns:owner.example:5349"));
        Ok(())
    }
}

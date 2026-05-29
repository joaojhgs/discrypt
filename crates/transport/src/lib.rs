//! Transport traits, ICE/WebRTC negotiation, and deterministic fallback policy.
//!
//! This crate now owns real WebRTC offer/answer plus candidate primitives while
//! keeping route selection and media/data transport claims separate. Policy types
//! remain UI-free so the multinode harness can prove fallback ordering and
//! metadata contracts without overstating connectivity.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod ice;
pub mod production_status;
pub mod session;
pub mod webrtc_negotiation;

use async_trait::async_trait;
pub use ice::{IceEndpointPolicy, IceServerConfig, TurnCredentialMode, TurnServerConfig};
use serde::{Deserialize, Serialize};
pub use session::{
    TransportRoute, TransportRouteStatus, TransportSession, TransportSessionError,
    TransportSessionEvent, TransportSessionSnapshot, TransportSessionState,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;
use thiserror::Error;
pub use webrtc_negotiation::{
    SealedWebRtcNegotiationPayload, WebRtcDirectPathMetrics, WebRtcIceCandidate,
    WebRtcNegotiationConfig, WebRtcNegotiationPayloadKind, WebRtcNegotiationSealer,
    WebRtcNegotiator, WebRtcSdpType, WebRtcSessionDescription,
};

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
    /// Local socket I/O failed.
    #[error("local socket I/O failed: {0}")]
    Io(String),
    /// A conformance gate caught caller-supplied plaintext in relay-visible bytes.
    #[error("relay-visible payload contains forbidden plaintext")]
    PlaintextLeak,
    /// Every Phase-6 fallback leg failed under the simulated NAT condition.
    #[error("no viable STUN, relay-overlay, or TURN path")]
    NoViablePath,
    /// ICE/STUN/TURN endpoint policy is malformed or unsupported.
    #[error("invalid ICE endpoint policy: {0}")]
    InvalidIcePolicy(String),
    /// A transport session event was invalid for the current state.
    #[error(transparent)]
    InvalidSessionTransition(#[from] TransportSessionError),
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

    /// Build an honest route report for UI/diagnostic surfaces.
    #[must_use]
    pub fn route_report(&self) -> RouteReport {
        RouteReport {
            selected: self.selected,
            endpoint: self.endpoint.clone(),
            attempted_legs: self.attempts.iter().map(|attempt| attempt.leg).collect(),
            ciphertext_only_relay_legs: self.relay_legs_ciphertext_only(),
            limitation:
                "deterministic local-process conformance only; not a production NAT/pcap proof"
                    .to_owned(),
        }
    }
}

/// Honest route report surfaced by harnesses and command-backed UI gates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteReport {
    /// Winning leg selected by the fallback planner.
    pub selected: FallbackLeg,
    /// Winning endpoint or route descriptor.
    pub endpoint: Endpoint,
    /// Attempted legs in order.
    pub attempted_legs: Vec<FallbackLeg>,
    /// Whether relay-overlay/TURN legs are marked ciphertext-only.
    pub ciphertext_only_relay_legs: bool,
    /// Honest limitation copy for deterministic local tests.
    pub limitation: String,
}

impl RouteReport {
    /// True when the report is ordered and includes the local-test limitation.
    #[must_use]
    pub fn honest_and_ordered(&self) -> bool {
        let expected = [
            FallbackLeg::Stun,
            FallbackLeg::RelayOverlay,
            FallbackLeg::Turn,
        ];
        self.attempted_legs
            .iter()
            .enumerate()
            .all(|(index, leg)| expected.get(index) == Some(leg))
            && self.limitation.contains("local-process")
            && self.ciphertext_only_relay_legs
    }
}

/// Result of a socket-backed local-process adapter conformance run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocalProcessConformance {
    /// Loopback socket endpoint used by the adapter.
    pub endpoint: Endpoint,
    /// Route report for the simulated NAT condition.
    pub route_report: RouteReport,
    /// Whether the ciphertext payload was delivered byte-for-byte.
    pub ciphertext_delivered: bool,
    /// Whether a caller-supplied plaintext sample was rejected before socket send.
    pub plaintext_rejected: bool,
    /// Number of bytes delivered over the local socket.
    pub delivered_len: usize,
}

impl LocalProcessConformance {
    /// True when socket delivery, plaintext rejection, and route reporting all pass.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.ciphertext_delivered
            && self.plaintext_rejected
            && self.route_report.honest_and_ordered()
    }
}

/// Loopback TCP adapter used to prove local-process transport conformance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocalProcessSocketAdapter {
    config: ConnectivityConfig,
    nat: SimulatedNat,
    forbidden_plaintext: Vec<u8>,
}

impl LocalProcessSocketAdapter {
    /// Create a local adapter bound to a fallback configuration and NAT scenario.
    #[must_use]
    pub fn new(
        config: ConnectivityConfig,
        nat: SimulatedNat,
        forbidden_plaintext: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            config,
            nat,
            forbidden_plaintext: forbidden_plaintext.into(),
        }
    }

    /// Run one loopback socket conformance delivery with ciphertext-only checks.
    pub fn run_conformance(
        &self,
        ciphertext: &[u8],
    ) -> Result<LocalProcessConformance, TransportError> {
        if ciphertext.is_empty() {
            return Err(TransportError::Unavailable("empty ciphertext".to_owned()));
        }
        let plaintext_rejected = self.rejects_plaintext_sample()?;
        self.ensure_ciphertext_only(ciphertext)?;

        let plan = ConnectivityPlanner::plan(&self.config, self.nat)?;
        let route_report = plan.route_report();
        let listener = TcpListener::bind("127.0.0.1:0").map_err(io_error)?;
        listener.set_nonblocking(false).map_err(io_error)?;
        let address = listener.local_addr().map_err(io_error)?;
        let expected_len = ciphertext.len();

        let receiver = thread::spawn(move || -> Result<Vec<u8>, TransportError> {
            let (mut stream, _peer) = listener.accept().map_err(io_error)?;
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .map_err(io_error)?;
            let mut len_bytes = [0u8; 4];
            stream.read_exact(&mut len_bytes).map_err(io_error)?;
            let len = u32::from_be_bytes(len_bytes) as usize;
            let mut bytes = vec![0u8; len];
            stream.read_exact(&mut bytes).map_err(io_error)?;
            Ok(bytes)
        });

        let mut stream = TcpStream::connect(address).map_err(io_error)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(io_error)?;
        stream
            .write_all(&(ciphertext.len() as u32).to_be_bytes())
            .map_err(io_error)?;
        stream.write_all(ciphertext).map_err(io_error)?;
        stream.flush().map_err(io_error)?;

        let delivered = receiver
            .join()
            .map_err(|_| TransportError::Io("local socket receiver panicked".to_owned()))??;
        let endpoint = Endpoint::new(format!("tcp://{address}"));

        Ok(LocalProcessConformance {
            endpoint,
            route_report,
            ciphertext_delivered: delivered == ciphertext && delivered.len() == expected_len,
            plaintext_rejected,
            delivered_len: delivered.len(),
        })
    }

    fn rejects_plaintext_sample(&self) -> Result<bool, TransportError> {
        if self.forbidden_plaintext.is_empty() {
            return Ok(true);
        }
        match self.ensure_ciphertext_only(&self.forbidden_plaintext) {
            Err(TransportError::PlaintextLeak) => Ok(true),
            Err(error) => Err(error),
            Ok(()) => Ok(false),
        }
    }

    fn ensure_ciphertext_only(&self, payload: &[u8]) -> Result<(), TransportError> {
        if !self.forbidden_plaintext.is_empty()
            && payload
                .windows(self.forbidden_plaintext.len())
                .any(|window| window == self.forbidden_plaintext.as_slice())
        {
            Err(TransportError::PlaintextLeak)
        } else {
            Ok(())
        }
    }
}

fn io_error(error: std::io::Error) -> TransportError {
    TransportError::Io(error.to_string())
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

    #[test]
    fn route_report_is_honest_about_local_process_limitations() -> Result<(), TransportError> {
        let config = ConnectivityConfig::default();
        let report =
            ConnectivityPlanner::plan(&config, SimulatedNat::overlay_only())?.route_report();
        assert_eq!(report.selected, FallbackLeg::RelayOverlay);
        assert!(report.honest_and_ordered());
        assert!(report
            .limitation
            .contains("not a production NAT/pcap proof"));
        Ok(())
    }

    #[test]
    fn socket_adapter_delivers_ciphertext_and_rejects_plaintext() -> Result<(), TransportError> {
        let adapter = LocalProcessSocketAdapter::new(
            ConnectivityConfig::default(),
            SimulatedNat::overlay_only(),
            b"hello plaintext".to_vec(),
        );
        let report = adapter.run_conformance(b"sframe-like ciphertext bytes")?;
        assert!(report.ready());
        assert_eq!(report.delivered_len, b"sframe-like ciphertext bytes".len());
        Ok(())
    }
}

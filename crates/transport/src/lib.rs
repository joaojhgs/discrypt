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
pub mod policy;
pub mod production_status;
pub mod provider_adapters;
pub mod route_graph;
pub mod session;
pub mod signaling;
pub mod webrtc_negotiation;

use async_trait::async_trait;
pub use ice::{
    turn_stun_credential_decision, IceEndpointPolicy, IceServerConfig, TurnCredentialIssuer,
    TurnCredentialIssuerConfig, TurnCredentialMode, TurnServerConfig, TurnStunCredentialDecision,
};
pub use policy::{
    derive_scope_commitment, AdapterFallbackBehavior, AdapterTrustLabel, ConnectivityPolicy,
    ConnectivityPolicySource, ConnectivityPolicyStore, ConnectivityScopeLevel, ConversationScope,
    EffectiveConnectivityPolicy, IceProfile, ProviderMetadataPosture, RendezvousCapability,
    SignalingAdapterCapabilities, SignalingAdapterKind, SignalingAdapterProfile,
    SignalingEndpointSecurity, SignalingProviderEndpoint,
};
#[cfg(any(test, feature = "harness"))]
pub use provider_adapters::start_local_conformance_provider_webrtc_text_control_runtime_pair_between_peers_with_answerer;
#[cfg(feature = "ipfs-pubsub-adapter")]
pub use provider_adapters::IpfsPubsubProviderAdapter;
#[cfg(feature = "mqtt-adapter")]
pub use provider_adapters::MqttProviderAdapter;
#[cfg(feature = "nostr-adapter")]
pub use provider_adapters::NostrProviderAdapter;
pub use provider_adapters::{
    adapter_boundary_for_kind, plan_signaling_adapter_fallback, probe_provider_adapter_roundtrip,
    probe_provider_webrtc_datachannel_request_response_roundtrip,
    probe_provider_webrtc_datachannel_request_response_with_config,
    probe_provider_webrtc_datachannel_request_response_with_config_and_answerer,
    probe_provider_webrtc_datachannel_roundtrip,
    probe_provider_webrtc_datachannel_text_frame_roundtrip, required_provider_adapter_boundaries,
    required_provider_adapter_registry, resume_text_control_runtime_from_probe,
    resume_text_control_runtime_from_spec,
    start_provider_webrtc_text_control_answer_runtime_with_answerer,
    start_provider_webrtc_text_control_offer_runtime,
    start_provider_webrtc_text_control_runtime_pair_between_peers_with_answerer,
    start_provider_webrtc_text_control_runtime_pair_with_answerer, AdapterReadinessState,
    FeatureGatedProviderAdapter, LocalConformanceProviderAdapter, LocalConformanceProviderBus,
    ProviderAdapterBoundary, ProviderAdapterReadiness, ProviderAdapterRoundtripProbe,
    ProviderTextControlRuntime, ProviderTextControlRuntimeAttachment,
    ProviderTextControlRuntimeEvidence, ProviderTextControlRuntimePair,
    ProviderTextControlRuntimePeerEvidence, ProviderTextControlRuntimePeerRole,
    ProviderTextControlRuntimeSpec, ProviderWebRtcDataChannelProbe, SignalingAdapterFactory,
    SignalingAdapterFallbackAttempt, SignalingAdapterFallbackPlan, SignalingAdapterRegistryEntry,
    PROVIDER_TEXT_CONTROL_RUNTIME_SPEC_SCHEMA_VERSION,
    TEXT_CONTROL_RUNTIME_NOT_IMPLEMENTED_MESSAGE,
    TEXT_CONTROL_RUNTIME_NOT_IMPLEMENTED_RECOVERY_HINT,
    TEXT_CONTROL_RUNTIME_SPEC_INCOMPATIBLE_MESSAGE, TEXT_CONTROL_RUNTIME_SPEC_MISSING_MESSAGE,
    TEXT_CONTROL_RUNTIME_SPEC_STALE_MESSAGE,
};
pub use route_graph::{
    GroupRouteGraph, RouteGraphEdge, RouteGraphScope, RouteIntent, ROUTE_GRAPH_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
pub use session::{
    ReconnectBackoffPolicy, ReconnectDecision, TransportRoute, TransportRouteStatus,
    TransportSession, TransportSessionError, TransportSessionEvent, TransportSessionSnapshot,
    TransportSessionState,
};
pub use signaling::{
    AdapterSession, ControlBroadcast, OpaqueSignalingPayload, PeerSignal, PresenceEvent,
    RendezvousRoom, SignalingAdapter, SignalingHealth, SignalingHealthState,
    SignalingObservability, SignalingPeerId,
};
#[cfg(any(test, feature = "harness", feature = "local-dev"))]
pub use signaling::{LocalConformanceSignalingAdapter, LocalConformanceSignalingBus};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;
use thiserror::Error;
pub use webrtc_negotiation::{
    SealedWebRtcNegotiationPayload, TextControlDataTransport, WebRtcDataTransportMetrics,
    WebRtcDiagnosticEvent, WebRtcDiagnosticTimeline, WebRtcDirectPathMetrics, WebRtcIceCandidate,
    WebRtcIceTransportPolicy, WebRtcNegotiationConfig, WebRtcNegotiationPayloadKind,
    WebRtcNegotiationSealer, WebRtcNegotiator, WebRtcSdpType, WebRtcSessionDescription,
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
    /// Every configured WebRTC path failed under the simulated NAT condition.
    #[error("no viable direct WebRTC or configured TURN path")]
    NoViablePath,
    /// ICE/STUN/TURN endpoint policy is malformed or unsupported.
    #[error("invalid ICE endpoint policy: {0}")]
    InvalidIcePolicy(String),
    /// Connectivity/signaling policy is malformed or unsupported.
    #[error("invalid connectivity policy: {0}")]
    InvalidConnectivityPolicy(String),
    /// Signaling adapter contract failed.
    #[error("signaling adapter failed: {0}")]
    SignalingAdapter(String),
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
    /// Legacy peer overlay route. Kept for old diagnostics only; the planner no longer selects it.
    RelayOverlay,
    /// Provider TURN relay carrying end-to-end ciphertext.
    Turn,
}

/// Deterministic NAT/test condition for the fallback planner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SimulatedNat {
    /// Whether STUN/ICE direct traversal succeeds.
    pub stun_available: bool,
    /// Legacy overlay flag retained for old fixtures; ignored by the current planner.
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

    /// Legacy overlay-only scenario. Current policy must fail unless TURN is configured.
    #[must_use]
    pub const fn overlay_only() -> Self {
        Self {
            stun_available: false,
            overlay_available: true,
            turn_available: false,
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
            default_stun: Endpoint::new("stun:stun.l.google.com:19302"),
            default_turn: Endpoint::new(Self::UNCONFIGURED_TURN_ENDPOINT),
            overrides: EndpointOverrides::new(None, None),
        }
    }
}

impl ConnectivityConfig {
    /// Placeholder TURN endpoint used when no relay credentials have been configured.
    pub const UNCONFIGURED_TURN_ENDPOINT: &'static str = "turn:unconfigured.discrypt.invalid";

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

    /// True only when TURN relay metadata came from configured policy/credentials.
    #[must_use]
    pub fn turn_relay_configured(&self) -> bool {
        self.overrides.turn.is_some() || self.default_turn.0 != Self::UNCONFIGURED_TURN_ENDPOINT
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
    /// Attempts in strict direct/STUN -> configured TURN order.
    pub attempts: Vec<ConnectionAttempt>,
    /// Winning leg.
    pub selected: FallbackLeg,
    /// Winning endpoint/route descriptor.
    pub endpoint: Endpoint,
}

impl ConnectivityPlan {
    /// Return true when attempts preserve the approved direct/STUN -> configured TURN ordering.
    #[must_use]
    pub fn ordered_direct_turn(&self) -> bool {
        let order = [FallbackLeg::Stun, FallbackLeg::Turn];
        self.attempts
            .iter()
            .enumerate()
            .all(|(index, attempt)| order.get(index) == Some(&attempt.leg))
    }

    /// Compatibility alias for older call sites; overlay is no longer part of the valid order.
    #[must_use]
    pub fn ordered_stun_overlay_turn(&self) -> bool {
        self.ordered_direct_turn()
    }

    /// Return true when TURN attempts do not carry plaintext content and no overlay route is selected.
    #[must_use]
    pub fn relay_legs_ciphertext_only(&self) -> bool {
        self.attempts.iter().all(|attempt| match attempt.leg {
            FallbackLeg::Stun => !attempt.carries_content,
            FallbackLeg::Turn => attempt.ciphertext_only && !attempt.carries_content,
            FallbackLeg::RelayOverlay => false,
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
    /// Whether TURN legs are marked ciphertext-only and no overlay relay was selected.
    pub ciphertext_only_relay_legs: bool,
    /// Honest limitation copy for deterministic local tests.
    pub limitation: String,
}

impl RouteReport {
    /// True when the report is ordered and includes the local-test limitation.
    #[must_use]
    pub fn honest_and_ordered(&self) -> bool {
        let expected = [FallbackLeg::Stun, FallbackLeg::Turn];
        !self.attempted_legs.is_empty()
            && self.attempted_legs.len() <= expected.len()
            && self
                .attempted_legs
                .last()
                .is_some_and(|last_attempted| *last_attempted == self.selected)
            && self
                .attempted_legs
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

/// Stateless fail-closed WebRTC route planner.
pub struct ConnectivityPlanner;

impl ConnectivityPlanner {
    /// Resolve a path using strict direct/STUN -> configured TURN fallback.
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
            leg: FallbackLeg::Turn,
            endpoint: config.turn_endpoint(),
            carries_content: false,
            ciphertext_only: true,
            succeeded: nat.turn_available && config.turn_relay_configured(),
        });
        if nat.turn_available && config.turn_relay_configured() {
            let endpoint = attempts[1].endpoint.clone();
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
    fn fallback_uses_direct_then_configured_turn_order() -> Result<(), TransportError> {
        let config = ConnectivityConfig::default();
        let turn_config = ConnectivityConfig {
            overrides: EndpointOverrides::new(
                None,
                Some(Endpoint::new("turns:relay.example:5349")),
            ),
            ..ConnectivityConfig::default()
        };
        let direct = ConnectivityPlanner::plan(&config, SimulatedNat::direct())?;
        let overlay = ConnectivityPlanner::plan(&config, SimulatedNat::overlay_only());
        let turn = ConnectivityPlanner::plan(&turn_config, SimulatedNat::turn_only())?;

        assert_eq!(direct.selected, FallbackLeg::Stun);
        assert_eq!(overlay, Err(TransportError::NoViablePath));
        assert_eq!(turn.selected, FallbackLeg::Turn);
        assert_eq!(turn.attempts.len(), 2);
        assert!(direct.ordered_direct_turn());
        assert!(turn.ordered_direct_turn());
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
    fn turn_only_without_configured_relay_fails_closed() -> Result<(), TransportError> {
        let plan =
            ConnectivityPlanner::plan(&ConnectivityConfig::default(), SimulatedNat::turn_only());

        assert_eq!(plan, Err(TransportError::NoViablePath));
        assert!(!ConnectivityConfig::default().turn_relay_configured());
        Ok(())
    }

    #[test]
    fn route_report_is_honest_about_local_process_limitations() -> Result<(), TransportError> {
        let config = ConnectivityConfig::default();
        let report = ConnectivityPlanner::plan(&config, SimulatedNat::direct())?.route_report();
        assert_eq!(report.selected, FallbackLeg::Stun);
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
            SimulatedNat::direct(),
            b"hello plaintext".to_vec(),
        );
        let report = adapter.run_conformance(b"sframe-like ciphertext bytes")?;
        assert!(report.ready());
        assert_eq!(report.delivered_len, b"sframe-like ciphertext bytes".len());
        Ok(())
    }
}

//! Feature-gated production boundaries for required signaling providers.
//!
//! Each required provider has a concrete adapter boundary that validates
//! profiles, exposes redacted health, and fails closed unless an audited
//! provider client is compiled behind its explicit Cargo feature. MQTT, Nostr,
//! IPFS/libp2p PubSub, and the separate Discrypt rendezvous service have real
//! provider clients behind explicit adapter features.

#[cfg(any(
    feature = "mqtt-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
use crate::SignalingProviderEndpoint;
use crate::WebRtcNegotiator;
use crate::{
    AdapterFallbackBehavior, AdapterSession, AdapterTrustLabel, ControlBroadcast,
    ConversationScope, IceServerConfig, OpaqueSignalingPayload, PeerSignal, PresenceEvent,
    RendezvousCapability, RendezvousRoom, SealedWebRtcNegotiationPayload, SignalingAdapter,
    SignalingAdapterCapabilities, SignalingAdapterKind, SignalingAdapterProfile,
    SignalingEndpointSecurity, SignalingHealth, SignalingHealthState, SignalingObservability,
    SignalingPeerId, TextControlDataTransport, TransportError, WebRtcNegotiationConfig,
};
#[cfg(any(
    test,
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
use crate::{WebRtcNegotiationPayloadKind, WebRtcNegotiationSealer};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(feature = "ipfs-pubsub-adapter")]
use std::collections::BTreeSet;
#[cfg(feature = "ipfs-pubsub-adapter")]
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter"
))]
use tokio::sync::Mutex as AsyncMutex;
#[cfg(any(
    test,
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
use tokio::time::timeout;
#[cfg(any(
    test,
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
use tokio::time::{Duration, Instant};

/// End-to-end readiness state used by registry and fallback planning.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterReadinessState {
    /// Adapter feature is not enabled in this build.
    FeatureDisabled,
    /// Feature is enabled but no audited provider client is wired yet.
    ImplementationUnavailable,
    /// The implementation is present and selectable for attempts.
    Available,
    /// Adapter health check or connect attempt failed.
    ProviderUnhealthy,
    /// Adapter is currently rate limited.
    ProviderRateLimited,
    /// Adapter requires authentication to continue.
    ProviderAuthRequired,
    /// Provider rejected payload because it exceeds limits.
    ProviderMessageTooLarge,
    /// Adapter trust posture check failed.
    TrustMismatch,
    /// ICE fallback contract indicates relay path is required now.
    IceFailedRequiresTurn,
    /// Adapter returned a connected/healthy state.
    Connected,
}

impl AdapterReadinessState {
    /// Return true when the adapter can be selected.
    #[must_use]
    pub const fn selectable(self) -> bool {
        matches!(self, Self::Available | Self::Connected)
    }

    /// Redacted failure class for diagnostics and persistence.
    #[must_use]
    pub const fn failure_class(self) -> &'static str {
        match self {
            Self::FeatureDisabled => "feature_disabled",
            Self::ImplementationUnavailable => "implementation_unavailable",
            Self::Available => "available",
            Self::ProviderUnhealthy => "provider_unhealthy",
            Self::ProviderRateLimited => "provider_rate_limited",
            Self::ProviderAuthRequired => "provider_auth_required",
            Self::ProviderMessageTooLarge => "provider_message_too_large",
            Self::TrustMismatch => "trust_mismatch",
            Self::IceFailedRequiresTurn => "ice_failed_requires_turn",
            Self::Connected => "connected",
        }
    }

    /// Convert a readiness state into redacted health.
    #[must_use]
    pub const fn to_health_state(self) -> SignalingHealthState {
        match self {
            Self::Available | Self::Connected => SignalingHealthState::Healthy,
            Self::ProviderRateLimited => SignalingHealthState::ProviderRateLimited,
            Self::ProviderAuthRequired => SignalingHealthState::ProviderAuthRequired,
            Self::ProviderMessageTooLarge => SignalingHealthState::ProviderMessageTooLarge,
            Self::TrustMismatch => SignalingHealthState::TrustMismatch,
            Self::FeatureDisabled
            | Self::ImplementationUnavailable
            | Self::ProviderUnhealthy
            | Self::IceFailedRequiresTurn => SignalingHealthState::ProviderUnhealthy,
        }
    }

    /// Classify common provider/relay failure strings into a redacted readiness state.
    ///
    /// Provider SDKs do not always expose a stable typed error for relay notices,
    /// so this parser intentionally recognizes conservative public-relay wording
    /// while falling back to `ProviderUnhealthy` for unknown failures.
    #[must_use]
    pub fn classify_provider_failure(message: &str) -> Self {
        let normalized = message.to_ascii_lowercase();
        if normalized.contains("rate-limit")
            || normalized.contains("ratelimit")
            || normalized.contains("too much")
            || normalized.contains("slow down")
        {
            Self::ProviderRateLimited
        } else if normalized.contains("auth")
            || normalized.contains("unauthorized")
            || normalized.contains("not authorized")
            || normalized.contains("forbidden")
            || normalized.contains("blocked")
        {
            Self::ProviderAuthRequired
        } else if normalized.contains("too large")
            || normalized.contains("message size")
            || normalized.contains("payload size")
            || normalized.contains("max message")
        {
            Self::ProviderMessageTooLarge
        } else if normalized.contains("trust")
            || normalized.contains("fingerprint")
            || normalized.contains("certificate")
        {
            Self::TrustMismatch
        } else {
            Self::ProviderUnhealthy
        }
    }
}

/// One required adapter in the ordered registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SignalingAdapterRegistryEntry {
    /// Adapter kind in the registry.
    pub kind: SignalingAdapterKind,
    /// Static feature/build boundary.
    pub boundary: ProviderAdapterBoundary,
}

impl SignalingAdapterRegistryEntry {
    /// Static readiness projected into runtime planning state.
    #[must_use]
    pub fn readiness_state(self) -> AdapterReadinessState {
        match self.boundary.readiness {
            ProviderAdapterReadiness::FeatureDisabled => AdapterReadinessState::FeatureDisabled,
            ProviderAdapterReadiness::ImplementationUnavailable => {
                AdapterReadinessState::ImplementationUnavailable
            }
            ProviderAdapterReadiness::ImplementationAvailable => AdapterReadinessState::Available,
        }
    }
}

/// Registry-backed factory for required adapter kinds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignalingAdapterFactory {
    /// MQTT real implementation when feature-gated client is enabled.
    #[cfg(feature = "mqtt-adapter")]
    Mqtt(MqttProviderAdapter),
    /// MQTT fail-closed boundary when feature is disabled.
    #[cfg(not(feature = "mqtt-adapter"))]
    Mqtt(FeatureGatedProviderAdapter),
    /// Nostr real implementation when feature-gated client is enabled.
    #[cfg(feature = "nostr-adapter")]
    Nostr(NostrProviderAdapter),
    /// Nostr fail-closed boundary when feature is disabled.
    #[cfg(not(feature = "nostr-adapter"))]
    Nostr(FeatureGatedProviderAdapter),
    /// IPFS/libp2p PubSub real implementation when feature-gated client is enabled.
    #[cfg(feature = "ipfs-pubsub-adapter")]
    IpfsPubsub(IpfsPubsubProviderAdapter),
    /// IPFS/libp2p PubSub fail-closed boundary when feature is disabled.
    #[cfg(not(feature = "ipfs-pubsub-adapter"))]
    IpfsPubsub(FeatureGatedProviderAdapter),
    /// Rust QUIC rendezvous real implementation when feature-gated client is enabled.
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    DiscryptQuicRendezvous(DiscryptQuicRendezvousProviderAdapter),
    /// Rust QUIC rendezvous fail-closed boundary when feature is disabled.
    #[cfg(not(feature = "discrypt-quic-rendezvous-adapter"))]
    DiscryptQuicRendezvous(FeatureGatedProviderAdapter),
}

impl SignalingAdapterFactory {
    /// Build a concrete adapter entry for one required kind.
    #[must_use]
    pub const fn for_kind(kind: SignalingAdapterKind) -> Self {
        match kind {
            SignalingAdapterKind::Mqtt => {
                #[cfg(feature = "mqtt-adapter")]
                {
                    Self::Mqtt(MqttProviderAdapter)
                }
                #[cfg(not(feature = "mqtt-adapter"))]
                {
                    Self::Mqtt(FeatureGatedProviderAdapter::new(kind))
                }
            }
            SignalingAdapterKind::Nostr => {
                #[cfg(feature = "nostr-adapter")]
                {
                    Self::Nostr(NostrProviderAdapter)
                }
                #[cfg(not(feature = "nostr-adapter"))]
                {
                    Self::Nostr(FeatureGatedProviderAdapter::new(kind))
                }
            }
            SignalingAdapterKind::IpfsPubsub => {
                #[cfg(feature = "ipfs-pubsub-adapter")]
                {
                    Self::IpfsPubsub(IpfsPubsubProviderAdapter)
                }
                #[cfg(not(feature = "ipfs-pubsub-adapter"))]
                {
                    Self::IpfsPubsub(FeatureGatedProviderAdapter::new(kind))
                }
            }
            SignalingAdapterKind::DiscryptQuicRendezvous => {
                #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
                {
                    Self::DiscryptQuicRendezvous(DiscryptQuicRendezvousProviderAdapter)
                }
                #[cfg(not(feature = "discrypt-quic-rendezvous-adapter"))]
                {
                    Self::DiscryptQuicRendezvous(FeatureGatedProviderAdapter::new(kind))
                }
            }
        }
    }

    /// Registry boundary for this entry.
    #[must_use]
    pub const fn boundary(self) -> ProviderAdapterBoundary {
        match self {
            #[cfg(feature = "mqtt-adapter")]
            Self::Mqtt(_) => adapter_boundary_for_kind(SignalingAdapterKind::Mqtt),
            #[cfg(not(feature = "mqtt-adapter"))]
            Self::Mqtt(adapter) => adapter.boundary(),
            #[cfg(feature = "nostr-adapter")]
            Self::Nostr(_) => adapter_boundary_for_kind(SignalingAdapterKind::Nostr),
            #[cfg(not(feature = "nostr-adapter"))]
            Self::Nostr(adapter) => adapter.boundary(),
            #[cfg(feature = "ipfs-pubsub-adapter")]
            Self::IpfsPubsub(_) => adapter_boundary_for_kind(SignalingAdapterKind::IpfsPubsub),
            #[cfg(not(feature = "ipfs-pubsub-adapter"))]
            Self::IpfsPubsub(adapter) => adapter.boundary(),
            #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
            Self::DiscryptQuicRendezvous(_) => {
                adapter_boundary_for_kind(SignalingAdapterKind::DiscryptQuicRendezvous)
            }
            #[cfg(not(feature = "discrypt-quic-rendezvous-adapter"))]
            Self::DiscryptQuicRendezvous(adapter) => adapter.boundary(),
        }
    }

    /// Adapter kind for this factory entry.
    #[must_use]
    pub const fn kind(self) -> SignalingAdapterKind {
        self.boundary().kind
    }

    /// Readiness for this entry in fallback planning.
    #[must_use]
    pub fn readiness_state(self) -> AdapterReadinessState {
        match self.boundary().readiness {
            ProviderAdapterReadiness::FeatureDisabled => AdapterReadinessState::FeatureDisabled,
            ProviderAdapterReadiness::ImplementationUnavailable => {
                AdapterReadinessState::ImplementationUnavailable
            }
            ProviderAdapterReadiness::ImplementationAvailable => AdapterReadinessState::Available,
        }
    }

    /// True when this entry can be selected for attempts.
    #[must_use]
    pub fn selectable(self) -> bool {
        self.readiness_state().selectable()
    }
}

/// Ordered registry of all required adapter boundaries.
#[must_use]
pub const fn required_provider_adapter_registry() -> [SignalingAdapterRegistryEntry; 4] {
    [
        SignalingAdapterRegistryEntry {
            kind: SignalingAdapterKind::Mqtt,
            boundary: adapter_boundary_for_kind(SignalingAdapterKind::Mqtt),
        },
        SignalingAdapterRegistryEntry {
            kind: SignalingAdapterKind::Nostr,
            boundary: adapter_boundary_for_kind(SignalingAdapterKind::Nostr),
        },
        SignalingAdapterRegistryEntry {
            kind: SignalingAdapterKind::IpfsPubsub,
            boundary: adapter_boundary_for_kind(SignalingAdapterKind::IpfsPubsub),
        },
        SignalingAdapterRegistryEntry {
            kind: SignalingAdapterKind::DiscryptQuicRendezvous,
            boundary: adapter_boundary_for_kind(SignalingAdapterKind::DiscryptQuicRendezvous),
        },
    ]
}

/// One fallback attempt record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingAdapterFallbackAttempt {
    /// Candidate adapter kind.
    pub kind: SignalingAdapterKind,
    /// Planned readiness state for this attempt.
    pub readiness: AdapterReadinessState,
    /// Adapter was attempted under the selected behavior.
    pub attempted: bool,
    /// Adapter won selection under the active behavior.
    pub selected: bool,
}

/// Deterministic adapter fallback contract output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingAdapterFallbackPlan {
    /// Requested fallback behavior.
    pub behavior: AdapterFallbackBehavior,
    /// Ordered candidate attempts.
    pub attempts: Vec<SignalingAdapterFallbackAttempt>,
    /// Selected adapter kind, if any.
    pub selected: Option<SignalingAdapterKind>,
}

/// Evidence returned by an active provider-adapter roundtrip probe.
///
/// This is intentionally limited to signaling/rendezvous evidence. It proves
/// that two local peers can use the selected provider adapter to exchange
/// opaque presence, one sealed WebRTC-negotiation envelope, and one sealed
/// control payload over the configured provider profile. It does **not** claim
/// that ICE, WebRTC data channels, or media/audio are connected.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderAdapterRoundtripProbe {
    /// Adapter that completed the probe.
    pub kind: SignalingAdapterKind,
    /// Profile id supplied by app/runtime policy.
    pub profile_id: String,
    /// Redacted provider endpoint label used by the profile.
    pub endpoint_label: String,
    /// Committed scope used to derive the rendezvous topic.
    pub scope_commitment: String,
    /// Provider-visible derived rendezvous topic/tag.
    pub rendezvous_topic: String,
    /// Presence event was received by the peer.
    pub presence_roundtrip: bool,
    /// Sealed offer/control signal was received by the peer.
    pub signal_roundtrip: bool,
    /// Sealed room-control broadcast was received by the peer.
    pub control_roundtrip: bool,
}

/// Evidence returned by a provider-signaled WebRTC DataChannel probe.
///
/// Unlike [`ProviderAdapterRoundtripProbe`], this proves that the selected
/// provider can carry encrypted WebRTC negotiation payloads far enough for two
/// local transport peers to open a real WebRTC DataChannel and exchange one
/// opaque text/control frame. It is still a transport-layer proof, not a
/// Tauri UI, installed-device, or voice/media proof.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderWebRtcDataChannelProbe {
    /// Adapter that completed the probe.
    pub kind: SignalingAdapterKind,
    /// Profile id supplied by app/runtime policy.
    pub profile_id: String,
    /// Redacted provider endpoint label used by the profile.
    pub endpoint_label: String,
    /// Committed scope used to derive the rendezvous topic.
    pub scope_commitment: String,
    /// Provider-visible derived rendezvous topic/tag.
    pub rendezvous_topic: String,
    /// WebRTC direct path reached connected/completed state on the offer side.
    pub offerer_direct_path_ready: bool,
    /// WebRTC direct path reached connected/completed state on the answer side.
    pub answerer_direct_path_ready: bool,
    /// Offer-side TURN relay evidence exists with TURN configured.
    pub offerer_turn_fallback_ready: bool,
    /// Answer-side TURN relay evidence exists with TURN configured.
    pub answerer_turn_fallback_ready: bool,
    /// Offer-side configured TURN server count.
    pub offerer_configured_turn_servers: u64,
    /// Answer-side configured TURN server count.
    pub answerer_configured_turn_servers: u64,
    /// Offer-side gathered relay candidates.
    pub offerer_local_relay_candidates_gathered: u64,
    /// Answer-side gathered relay candidates.
    pub answerer_local_relay_candidates_gathered: u64,
    /// Offer-side applied remote relay candidates.
    pub offerer_remote_relay_candidates_applied: u64,
    /// Answer-side applied remote relay candidates.
    pub answerer_remote_relay_candidates_applied: u64,
    /// Offer-side DataChannel opened.
    pub offerer_data_channel_open: bool,
    /// Answer-side DataChannel opened.
    pub answerer_data_channel_open: bool,
    /// Opaque text/control frame crossed the DataChannel from offerer to answerer.
    pub text_control_frame_roundtrip: bool,
    /// SHA-256 of the opaque text/control frame used for the proof.
    pub text_control_frame_sha256: String,
    /// Opaque return receipt/control frame crossed the DataChannel from answerer to offerer.
    pub receipt_frame_roundtrip: bool,
    /// SHA-256 of the opaque return receipt/control frame used for the proof.
    pub receipt_frame_sha256: String,
    /// Versioned serialized runtime handoff material captured during the probe.
    #[serde(default)]
    pub runtime_spec: Option<ProviderTextControlRuntimeSpec>,
}

/// Evidence returned when a live provider-signaled text/control runtime is attached.
///
/// This proves that the selected provider carried sealed WebRTC negotiation far
/// enough to open a real DataChannel and return an app-facing transport handle.
/// It intentionally does not claim message delivery; delivery is only proven by
/// signed app-level receipts after the runtime is used.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderTextControlRuntimeEvidence {
    /// Adapter that opened the runtime.
    pub kind: SignalingAdapterKind,
    /// Profile id supplied by app/runtime policy.
    pub profile_id: String,
    /// Redacted provider endpoint label used by the profile.
    pub endpoint_label: String,
    /// Committed scope used to derive the rendezvous topic.
    pub scope_commitment: String,
    /// Provider-visible derived rendezvous topic/tag.
    pub rendezvous_topic: String,
    /// Offer-side direct path reached connected/completed state.
    pub offerer_direct_path_ready: bool,
    /// Answer-side direct path reached connected/completed state.
    pub answerer_direct_path_ready: bool,
    /// Offer-side DataChannel opened.
    pub offerer_data_channel_open: bool,
    /// Answer-side DataChannel opened.
    pub answerer_data_channel_open: bool,
    /// Versioned serialized runtime handoff material captured during attach.
    pub runtime_spec: ProviderTextControlRuntimeSpec,
}

/// Live app-facing provider-backed text/control runtime pair.
///
/// The offerer side is returned as the sender's app-facing transport. The
/// answerer side is kept alive by this handle and runs a receiver loop that
/// turns incoming opaque frames into opaque responses. Dropping the handle aborts
/// that receiver loop; call [`ProviderTextControlRuntimePair::close`] in tests or
/// owned runtimes for deterministic teardown.
pub struct ProviderTextControlRuntimePair {
    evidence: ProviderTextControlRuntimeEvidence,
    offerer: Option<Arc<WebRtcNegotiator>>,
    answerer: Option<Arc<WebRtcNegotiator>>,
    answerer_task: Option<tokio::task::JoinHandle<()>>,
}

impl ProviderTextControlRuntimePair {
    /// Runtime attach evidence safe to surface to the app/UI.
    #[must_use]
    pub const fn evidence(&self) -> &ProviderTextControlRuntimeEvidence {
        &self.evidence
    }

    /// Offerer-side app-facing text/control DataChannel transport.
    #[must_use]
    pub fn transport(&self) -> Arc<dyn TextControlDataTransport> {
        self.offerer
            .as_ref()
            .expect("provider runtime transport is present until close")
            .clone()
    }

    /// Abort the receiver loop and close both WebRTC peer connections.
    pub async fn close(mut self) -> Result<(), TransportError> {
        if let Some(task) = self.answerer_task.take() {
            task.abort();
            let _ = task.await;
        }

        let mut first_error = None;
        if let Some(offerer) = self.offerer.take() {
            if let Err(error) = offerer.tear_down().await {
                first_error.get_or_insert(error);
            }
        }
        if let Some(answerer) = self.answerer.take() {
            if let Err(error) = answerer.tear_down().await {
                first_error.get_or_insert(error);
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for ProviderTextControlRuntimePair {
    fn drop(&mut self) {
        if let Some(task) = self.answerer_task.take() {
            task.abort();
        }
    }
}

/// Inputs required to resume a provider-backed WebRTC text/control runtime from a
/// previously proven peer rendezvous path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderTextControlRuntimeAttachment {
    /// Canonical adapter kind label (for example, `mqtt`, `nostr`, `ipfs_pubsub`).
    pub adapter_kind: String,
    /// Signaling profile identifier used by the proven peer contract.
    pub profile_id: String,
    /// Redacted profile endpoint label from probe evidence.
    pub endpoint_label: String,
    /// Rendezvous topic that the peer proof already established.
    pub rendezvous_topic: String,
    /// Scope commitment used to derive the rendezvous topic.
    #[serde(default)]
    pub scope_commitment: String,
    /// Serialized runtime handoff material derived from the same proof.
    #[serde(default)]
    pub runtime_spec: Option<Box<ProviderTextControlRuntimeSpec>>,
}

/// Current schema version for persisted provider text/control runtime specs.
pub const PROVIDER_TEXT_CONTROL_RUNTIME_SPEC_SCHEMA_VERSION: u16 = 1;

/// Explicitly exposed transport-blocker message for provider-backed long-lived
/// text/control runtime attachment.
pub const TEXT_CONTROL_RUNTIME_NOT_IMPLEMENTED_MESSAGE: &str =
    "provider-backed text/control runtime attachment is not implemented in this build";
/// Recovery guidance for the missing long-lived runtime constructor path.
pub const TEXT_CONTROL_RUNTIME_NOT_IMPLEMENTED_RECOVERY_HINT: &str =
    "Persisted provider offer/answer/ICE handoff and a long-lived receiver loop are still required before runtime attachment can be established";

/// Typed boundary when a runtime spec is missing from a resume attachment.
pub const TEXT_CONTROL_RUNTIME_SPEC_MISSING_MESSAGE: &str =
    "provider text/control runtime handoff spec is required for resume";

/// Typed boundary when a runtime spec is stale beyond its validity window.
pub const TEXT_CONTROL_RUNTIME_SPEC_STALE_MESSAGE: &str =
    "provider text/control runtime handoff spec is stale";

/// Typed boundary when a runtime spec does not match the resume attachment.
pub const TEXT_CONTROL_RUNTIME_SPEC_INCOMPATIBLE_MESSAGE: &str =
    "provider text/control runtime handoff spec is incompatible with requested attachment";

/// Persistable handoff from a provider WebRTC proof to a future long-lived text/control runtime.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderTextControlRuntimeSpec {
    /// Version for compatibility checks before runtime attachment.
    pub schema_version: u16,
    /// Provider route metadata captured from a proofed rendezvous path.
    pub attachment: ProviderTextControlRuntimeAttachment,
    /// Unix timestamp when the handoff was created by the app-service.
    pub created_at_unix_seconds: i64,
    /// Unix timestamp after which the handoff must be considered stale.
    pub expires_at_unix_seconds: i64,
    /// Sealed WebRTC offer payload needed to resume/complete negotiation.
    #[serde(default)]
    pub sealed_offer: Option<SealedWebRtcNegotiationPayload>,
    /// Sealed WebRTC answer payload needed to resume/complete negotiation.
    #[serde(default)]
    pub sealed_answer: Option<SealedWebRtcNegotiationPayload>,
    /// Sealed ICE candidates cached for the runtime handoff.
    #[serde(default)]
    pub sealed_ice_candidates: Vec<SealedWebRtcNegotiationPayload>,
    /// Missing production materials that still prevent live runtime attachment.
    #[serde(default)]
    pub missing_material: Vec<String>,
}

impl ProviderTextControlRuntimeSpec {
    /// Build a fail-closed handoff from probe evidence when live negotiated material is unavailable.
    #[must_use]
    pub fn from_probe_without_negotiation_material(
        probe: &ProviderWebRtcDataChannelProbe,
        created_at_unix_seconds: i64,
        ttl_seconds: i64,
    ) -> Self {
        Self {
            schema_version: PROVIDER_TEXT_CONTROL_RUNTIME_SPEC_SCHEMA_VERSION,
            attachment: ProviderTextControlRuntimeAttachment {
                adapter_kind: probe.kind.canonical_name().to_owned(),
                profile_id: probe.profile_id.clone(),
                endpoint_label: probe.endpoint_label.clone(),
                rendezvous_topic: probe.rendezvous_topic.clone(),
                scope_commitment: probe.scope_commitment.clone(),
                runtime_spec: None,
            },
            created_at_unix_seconds,
            expires_at_unix_seconds: created_at_unix_seconds.saturating_add(ttl_seconds.max(0)),
            sealed_offer: None,
            sealed_answer: None,
            sealed_ice_candidates: Vec::new(),
            missing_material: vec![
                "sealed WebRTC offer payload was not retained by the probe API".to_owned(),
                "sealed WebRTC answer payload was not retained by the probe API".to_owned(),
                "ICE candidate cache was not retained for runtime resume".to_owned(),
                "installed-app receiver loop is not yet bound to this handoff".to_owned(),
            ],
        }
    }

    /// Build a complete runtime spec from a successful offer/answer/ICE probe path.
    #[must_use]
    pub fn from_webrtc_probe(
        probe: &ProviderWebRtcDataChannelProbe,
        created_at_unix_seconds: i64,
        ttl_seconds: i64,
        sealed_offer: Option<SealedWebRtcNegotiationPayload>,
        sealed_answer: Option<SealedWebRtcNegotiationPayload>,
        sealed_ice_candidates: Vec<SealedWebRtcNegotiationPayload>,
    ) -> Self {
        Self {
            schema_version: PROVIDER_TEXT_CONTROL_RUNTIME_SPEC_SCHEMA_VERSION,
            attachment: ProviderTextControlRuntimeAttachment {
                adapter_kind: probe.kind.canonical_name().to_owned(),
                profile_id: probe.profile_id.clone(),
                endpoint_label: probe.endpoint_label.clone(),
                rendezvous_topic: probe.rendezvous_topic.clone(),
                scope_commitment: probe.scope_commitment.clone(),
                runtime_spec: None,
            },
            created_at_unix_seconds,
            expires_at_unix_seconds: created_at_unix_seconds.saturating_add(ttl_seconds.max(0)),
            sealed_offer,
            sealed_answer,
            sealed_ice_candidates,
            missing_material: Vec::new(),
        }
    }

    /// Validate the spec before attempting a long-lived runtime attachment.
    pub fn validate_for_runtime_attach(
        &self,
        now_unix_seconds: i64,
        attachment: &ProviderTextControlRuntimeAttachment,
    ) -> Result<(), TransportError> {
        self.validate_for_runtime_attach_without_attachment(now_unix_seconds)?;
        if self.attachment.adapter_kind != attachment.adapter_kind
            || self.attachment.profile_id != attachment.profile_id
            || self.attachment.endpoint_label != attachment.endpoint_label
            || self.attachment.rendezvous_topic != attachment.rendezvous_topic
            || (!self.attachment.scope_commitment.is_empty()
                && self.attachment.scope_commitment != attachment.scope_commitment)
        {
            return Err(TransportError::Unavailable(
                TEXT_CONTROL_RUNTIME_SPEC_INCOMPATIBLE_MESSAGE.to_owned(),
            ));
        }
        Ok(())
    }

    /// Validate this spec for runtime attachment before compatibility binding to the
    /// active attachment metadata.
    pub fn validate_for_runtime_attach_without_attachment(
        &self,
        now_unix_seconds: i64,
    ) -> Result<(), TransportError> {
        if self.schema_version != PROVIDER_TEXT_CONTROL_RUNTIME_SPEC_SCHEMA_VERSION {
            return Err(TransportError::Unavailable(format!(
                "unsupported provider text/control runtime spec version {}",
                self.schema_version
            )));
        }
        if self.attachment.adapter_kind.is_empty()
            || self.attachment.profile_id.is_empty()
            || self.attachment.rendezvous_topic.is_empty()
        {
            return Err(TransportError::Unavailable(
                "provider text/control runtime spec is missing attachment metadata".to_owned(),
            ));
        }
        if now_unix_seconds > self.expires_at_unix_seconds {
            return Err(TransportError::Unavailable(
                TEXT_CONTROL_RUNTIME_SPEC_STALE_MESSAGE.to_owned(),
            ));
        }
        if !self.missing_material.is_empty() {
            return Err(TransportError::Unavailable(format!(
                "provider text/control runtime spec is missing negotiated material: {}",
                self.missing_material.join("; ")
            )));
        }
        if self.sealed_offer.is_none() || self.sealed_answer.is_none() {
            return Err(TransportError::Unavailable(
                "provider text/control runtime spec lacks sealed offer/answer handoff".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Build a live provider-backed text/control transport runtime from prior peer
/// proof material.
///
/// This seam is intentionally currently unavailable while the transport backend
/// grows the long-lived attachment contract. It is explicitly fail-closed to
/// avoid silently pretending that short-lived test probes imply a production
/// long-lived runtime.
pub fn resume_text_control_runtime_from_probe(
    attachment: ProviderTextControlRuntimeAttachment,
) -> Result<std::sync::Arc<dyn TextControlDataTransport>, TransportError> {
    let now_unix_seconds = Utc::now().timestamp();
    let spec = attachment.runtime_spec.as_deref().ok_or_else(|| {
        TransportError::Unavailable(TEXT_CONTROL_RUNTIME_SPEC_MISSING_MESSAGE.to_owned())
    })?;
    resume_text_control_runtime_from_spec(&spec, &attachment, now_unix_seconds)
}

/// Build a live provider-backed text/control transport runtime from a persisted handoff spec.
pub fn resume_text_control_runtime_from_spec(
    spec: &ProviderTextControlRuntimeSpec,
    attachment: &ProviderTextControlRuntimeAttachment,
    now_unix_seconds: i64,
) -> Result<std::sync::Arc<dyn TextControlDataTransport>, TransportError> {
    spec.validate_for_runtime_attach(now_unix_seconds, attachment)?;
    Err(TransportError::Unavailable(
        TEXT_CONTROL_RUNTIME_NOT_IMPLEMENTED_MESSAGE.to_owned(),
    ))
}

impl SignalingAdapterFallbackPlan {
    /// True when at least one candidate was selected.
    #[must_use]
    pub fn has_selected(&self) -> bool {
        self.selected.is_some()
    }

    /// True when every candidate was non-selectable.
    #[must_use]
    pub fn all_unavailable(&self) -> bool {
        !self.has_selected()
    }
}

/// Build a deterministic fallback plan from requested candidates and policy.
#[must_use]
pub fn plan_signaling_adapter_fallback(
    requested: &[SignalingAdapterKind],
    behavior: AdapterFallbackBehavior,
    manual: Option<SignalingAdapterKind>,
) -> SignalingAdapterFallbackPlan {
    let mut requested_unique = Vec::new();
    for kind in requested {
        if !requested_unique.contains(kind) {
            requested_unique.push(*kind);
        }
    }

    let ordered = if matches!(behavior, AdapterFallbackBehavior::ManualOnly) {
        manual.into_iter().collect::<Vec<_>>()
    } else {
        requested_unique
    };

    let mut selected = None;
    let mut attempts = Vec::new();
    for kind in ordered {
        let readiness = SignalingAdapterFactory::for_kind(kind).readiness_state();
        let is_selected = match behavior {
            AdapterFallbackBehavior::TryAll => selected.is_none() && readiness.selectable(),
            AdapterFallbackBehavior::FirstHealthy => selected.is_none() && readiness.selectable(),
            AdapterFallbackBehavior::ManualOnly => readiness.selectable(),
        };
        let attempted = match behavior {
            AdapterFallbackBehavior::TryAll => true,
            AdapterFallbackBehavior::FirstHealthy => selected.is_none(),
            AdapterFallbackBehavior::ManualOnly => true,
        };

        attempts.push(SignalingAdapterFallbackAttempt {
            kind,
            readiness,
            attempted,
            selected: is_selected,
        });

        if is_selected {
            selected = Some(kind);
        }

        if matches!(
            behavior,
            AdapterFallbackBehavior::FirstHealthy | AdapterFallbackBehavior::ManualOnly
        ) && is_selected
        {
            break;
        }
    }

    // The contract records ordered attempts and selected leg metadata for
    // transport-level diagnostics and session persistence.
    SignalingAdapterFallbackPlan {
        behavior,
        attempts,
        selected,
    }
}

/// Run a real provider-adapter signaling roundtrip for a runtime-selected profile.
///
/// The probe connects two local peer sessions to the selected provider, joins
/// the same derived rendezvous capability, then verifies opaque presence,
/// sealed WebRTC negotiation, and sealed control delivery. It is suitable for
/// Tauri/backend diagnostics because it exercises the same adapter contract as
/// production signaling without exposing raw SDP, ICE credentials, identities,
/// room names, message plaintext, or audio.
pub async fn probe_provider_adapter_roundtrip(
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
) -> Result<ProviderAdapterRoundtripProbe, TransportError> {
    profile.validate()?;
    scope.validate()?;
    let factory = SignalingAdapterFactory::for_kind(profile.kind);
    #[cfg(not(any(
        feature = "mqtt-adapter",
        feature = "nostr-adapter",
        feature = "ipfs-pubsub-adapter",
        feature = "discrypt-quic-rendezvous-adapter"
    )))]
    let _ = (bootstrap_secret, random_entropy);
    probe_provider_adapter_roundtrip_with_factory(
        factory,
        profile,
        scope,
        bootstrap_secret,
        random_entropy,
    )
    .await
}

async fn probe_provider_adapter_roundtrip_with_factory(
    factory: SignalingAdapterFactory,
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
) -> Result<ProviderAdapterRoundtripProbe, TransportError> {
    #[cfg(not(any(
        feature = "mqtt-adapter",
        feature = "nostr-adapter",
        feature = "ipfs-pubsub-adapter",
        feature = "discrypt-quic-rendezvous-adapter"
    )))]
    let _ = (profile, scope, bootstrap_secret, random_entropy);
    match factory {
        SignalingAdapterFactory::Mqtt(adapter) => {
            #[cfg(feature = "mqtt-adapter")]
            {
                probe_with_adapter(adapter, profile, scope, bootstrap_secret, random_entropy).await
            }
            #[cfg(not(feature = "mqtt-adapter"))]
            {
                Err(adapter.boundary().unavailable_error())
            }
        }
        SignalingAdapterFactory::Nostr(adapter) => {
            #[cfg(feature = "nostr-adapter")]
            {
                probe_with_adapter(adapter, profile, scope, bootstrap_secret, random_entropy).await
            }
            #[cfg(not(feature = "nostr-adapter"))]
            {
                Err(adapter.boundary().unavailable_error())
            }
        }
        SignalingAdapterFactory::IpfsPubsub(adapter) => {
            #[cfg(feature = "ipfs-pubsub-adapter")]
            {
                probe_with_adapter(adapter, profile, scope, bootstrap_secret, random_entropy).await
            }
            #[cfg(not(feature = "ipfs-pubsub-adapter"))]
            {
                Err(adapter.boundary().unavailable_error())
            }
        }
        SignalingAdapterFactory::DiscryptQuicRendezvous(adapter) => {
            #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
            {
                probe_with_adapter(adapter, profile, scope, bootstrap_secret, random_entropy).await
            }
            #[cfg(not(feature = "discrypt-quic-rendezvous-adapter"))]
            {
                Err(adapter.boundary().unavailable_error())
            }
        }
    }
}

/// Run a provider-signaled WebRTC DataChannel roundtrip for a runtime-selected profile.
///
/// The probe uses the selected adapter as the encrypted rendezvous path for
/// SDP offer/answer exchange, then requires a real WebRTC DataChannel to open
/// and carry one opaque text/control frame plus one opaque receipt/control
/// frame back. It deliberately uses a real network
/// UDP bind (`0.0.0.0:0`) so public STUN endpoints are not accidentally tested
/// through loopback-only sockets.
pub async fn probe_provider_webrtc_datachannel_roundtrip(
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    ice_servers: IceServerConfig,
) -> Result<ProviderWebRtcDataChannelProbe, TransportError> {
    probe_provider_webrtc_datachannel_text_frame_roundtrip(
        profile,
        scope,
        bootstrap_secret,
        random_entropy,
        ice_servers,
        b"ciphertext:public-provider-signaled-webrtc-text-frame:v1".to_vec(),
    )
    .await
}

/// Run a provider-signaled WebRTC DataChannel roundtrip with a caller-supplied opaque frame.
///
/// This is used by app-service tests and diagnostics to prove that the selected
/// provider can negotiate WebRTC and carry an already-protected text/control
/// frame derived from the actual command path. The frame must be non-empty and
/// must already be ciphertext/opaque to this transport layer.
pub async fn probe_provider_webrtc_datachannel_text_frame_roundtrip(
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    ice_servers: IceServerConfig,
    text_control_frame: Vec<u8>,
) -> Result<ProviderWebRtcDataChannelProbe, TransportError> {
    let response_frame = default_receipt_control_frame(&text_control_frame);
    probe_provider_webrtc_datachannel_request_response_roundtrip(
        profile,
        scope,
        bootstrap_secret,
        random_entropy,
        ice_servers,
        text_control_frame,
        response_frame,
    )
    .await
}

/// Run a provider-signaled WebRTC DataChannel request/response proof with caller-supplied opaque frames.
///
/// This exercises the production-shaped text/control direction plus the receipt/control
/// return direction without interpreting either payload at the transport layer. Callers
/// can pass an encrypted message envelope as the request and a signed receipt/control
/// payload as the response, then verify the payload semantics at the app layer.
pub async fn probe_provider_webrtc_datachannel_request_response_roundtrip(
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    ice_servers: IceServerConfig,
    text_control_frame: Vec<u8>,
    receipt_control_frame: Vec<u8>,
) -> Result<ProviderWebRtcDataChannelProbe, TransportError> {
    probe_provider_webrtc_datachannel_request_response_with_config(
        profile,
        scope,
        bootstrap_secret,
        random_entropy,
        WebRtcNegotiationConfig::new(ice_servers),
        text_control_frame,
        receipt_control_frame,
    )
    .await
}

/// Run the provider-signaled WebRTC request/response proof with explicit WebRTC policy.
///
/// This is used by release-only TURN gates that need relay-only candidate
/// gathering while keeping the normal production probe on all candidates.
pub async fn probe_provider_webrtc_datachannel_request_response_with_config(
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    negotiation_config: WebRtcNegotiationConfig,
    text_control_frame: Vec<u8>,
    receipt_control_frame: Vec<u8>,
) -> Result<ProviderWebRtcDataChannelProbe, TransportError> {
    probe_provider_webrtc_datachannel_request_response_with_config_and_answerer(
        profile,
        scope,
        bootstrap_secret,
        random_entropy,
        negotiation_config,
        text_control_frame,
        move |_| Ok(receipt_control_frame),
    )
    .await
}

/// Run the provider-signaled WebRTC request/response proof with a live answerer callback.
///
/// The callback is invoked only after the answerer peer receives the opaque
/// request frame over the DataChannel. This gives app-service harnesses a
/// production-shaped hook for receiver-side frame verification/receipt
/// generation without precomputing the response before transport delivery.
pub async fn probe_provider_webrtc_datachannel_request_response_with_config_and_answerer<F>(
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    negotiation_config: WebRtcNegotiationConfig,
    text_control_frame: Vec<u8>,
    answerer: F,
) -> Result<ProviderWebRtcDataChannelProbe, TransportError>
where
    F: FnOnce(Vec<u8>) -> Result<Vec<u8>, TransportError> + Send,
{
    if text_control_frame.is_empty() {
        return Err(TransportError::Unavailable(
            "text/control proof frame must be non-empty opaque bytes".to_owned(),
        ));
    }
    profile.validate()?;
    scope.validate()?;
    negotiation_config
        .ice_servers
        .validate_credentials_at(chrono::Utc::now())?;
    let factory = SignalingAdapterFactory::for_kind(profile.kind);
    #[cfg(not(any(
        feature = "mqtt-adapter",
        feature = "nostr-adapter",
        feature = "ipfs-pubsub-adapter",
        feature = "discrypt-quic-rendezvous-adapter"
    )))]
    let _ = (
        bootstrap_secret,
        random_entropy,
        &negotiation_config,
        &text_control_frame,
    );
    probe_provider_webrtc_datachannel_request_response_with_factory(
        factory,
        profile,
        scope,
        bootstrap_secret,
        random_entropy,
        negotiation_config,
        text_control_frame,
        answerer,
    )
    .await
}

/// Start a live provider-signaled WebRTC text/control runtime pair.
///
/// The selected adapter carries only sealed SDP/candidate negotiation payloads.
/// The returned offerer transport is a real WebRTC DataChannel transport, while
/// the answerer loop invokes `answerer` for each received opaque frame and sends
/// any non-empty opaque response back over the same DataChannel.
pub async fn start_provider_webrtc_text_control_runtime_pair_with_answerer<F>(
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    negotiation_config: WebRtcNegotiationConfig,
    answerer: F,
) -> Result<ProviderTextControlRuntimePair, TransportError>
where
    F: Fn(Vec<u8>) -> Result<Vec<u8>, TransportError> + Send + Sync + 'static,
{
    profile.validate()?;
    scope.validate()?;
    negotiation_config
        .ice_servers
        .validate_credentials_at(chrono::Utc::now())?;
    let factory = SignalingAdapterFactory::for_kind(profile.kind);
    start_provider_webrtc_text_control_runtime_pair_with_factory(
        factory,
        profile,
        scope,
        bootstrap_secret,
        random_entropy,
        negotiation_config,
        answerer,
    )
    .await
}

async fn start_provider_webrtc_text_control_runtime_pair_with_factory<F>(
    factory: SignalingAdapterFactory,
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    negotiation_config: WebRtcNegotiationConfig,
    answerer: F,
) -> Result<ProviderTextControlRuntimePair, TransportError>
where
    F: Fn(Vec<u8>) -> Result<Vec<u8>, TransportError> + Send + Sync + 'static,
{
    match factory {
        SignalingAdapterFactory::Mqtt(adapter) => {
            #[cfg(feature = "mqtt-adapter")]
            {
                start_provider_webrtc_text_control_runtime_pair_with_adapter(
                    adapter,
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    answerer,
                )
                .await
            }
            #[cfg(not(feature = "mqtt-adapter"))]
            {
                let _ = (
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    answerer,
                );
                Err(adapter.boundary().unavailable_error())
            }
        }
        SignalingAdapterFactory::Nostr(adapter) => {
            #[cfg(feature = "nostr-adapter")]
            {
                start_provider_webrtc_text_control_runtime_pair_with_adapter(
                    adapter,
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    answerer,
                )
                .await
            }
            #[cfg(not(feature = "nostr-adapter"))]
            {
                let _ = (
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    answerer,
                );
                Err(adapter.boundary().unavailable_error())
            }
        }
        SignalingAdapterFactory::IpfsPubsub(adapter) => {
            #[cfg(feature = "ipfs-pubsub-adapter")]
            {
                start_provider_webrtc_text_control_runtime_pair_with_adapter(
                    adapter,
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    answerer,
                )
                .await
            }
            #[cfg(not(feature = "ipfs-pubsub-adapter"))]
            {
                let _ = (
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    answerer,
                );
                Err(adapter.boundary().unavailable_error())
            }
        }
        SignalingAdapterFactory::DiscryptQuicRendezvous(adapter) => {
            #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
            {
                start_provider_webrtc_text_control_runtime_pair_with_adapter(
                    adapter,
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    answerer,
                )
                .await
            }
            #[cfg(not(feature = "discrypt-quic-rendezvous-adapter"))]
            {
                let _ = (
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    answerer,
                );
                Err(adapter.boundary().unavailable_error())
            }
        }
    }
}

async fn probe_provider_webrtc_datachannel_request_response_with_factory<F>(
    factory: SignalingAdapterFactory,
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    negotiation_config: WebRtcNegotiationConfig,
    text_control_frame: Vec<u8>,
    answerer: F,
) -> Result<ProviderWebRtcDataChannelProbe, TransportError>
where
    F: FnOnce(Vec<u8>) -> Result<Vec<u8>, TransportError> + Send,
{
    #[cfg(not(any(
        feature = "mqtt-adapter",
        feature = "nostr-adapter",
        feature = "ipfs-pubsub-adapter",
        feature = "discrypt-quic-rendezvous-adapter"
    )))]
    let _ = (
        profile,
        scope,
        bootstrap_secret,
        random_entropy,
        negotiation_config,
        text_control_frame,
        answerer,
    );
    match factory {
        SignalingAdapterFactory::Mqtt(adapter) => {
            #[cfg(feature = "mqtt-adapter")]
            {
                probe_webrtc_with_adapter_and_answerer(
                    adapter,
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    text_control_frame,
                    answerer,
                )
                .await
            }
            #[cfg(not(feature = "mqtt-adapter"))]
            {
                Err(adapter.boundary().unavailable_error())
            }
        }
        SignalingAdapterFactory::Nostr(adapter) => {
            #[cfg(feature = "nostr-adapter")]
            {
                probe_webrtc_with_adapter_and_answerer(
                    adapter,
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    text_control_frame,
                    answerer,
                )
                .await
            }
            #[cfg(not(feature = "nostr-adapter"))]
            {
                Err(adapter.boundary().unavailable_error())
            }
        }
        SignalingAdapterFactory::IpfsPubsub(adapter) => {
            #[cfg(feature = "ipfs-pubsub-adapter")]
            {
                probe_webrtc_with_adapter_and_answerer(
                    adapter,
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    text_control_frame,
                    answerer,
                )
                .await
            }
            #[cfg(not(feature = "ipfs-pubsub-adapter"))]
            {
                Err(adapter.boundary().unavailable_error())
            }
        }
        SignalingAdapterFactory::DiscryptQuicRendezvous(adapter) => {
            #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
            {
                probe_webrtc_with_adapter_and_answerer(
                    adapter,
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    negotiation_config,
                    text_control_frame,
                    answerer,
                )
                .await
            }
            #[cfg(not(feature = "discrypt-quic-rendezvous-adapter"))]
            {
                Err(adapter.boundary().unavailable_error())
            }
        }
    }
}

#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
async fn probe_with_adapter<A>(
    adapter: A,
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
) -> Result<ProviderAdapterRoundtripProbe, TransportError>
where
    A: SignalingAdapter,
{
    let endpoint_label = profile
        .endpoints
        .first()
        .map(|endpoint| redacted_endpoint_label(&endpoint.endpoint.0))
        .unwrap_or_else(|| "endpoint#missing".to_owned());
    let capability = RendezvousCapability::derive(
        scope.clone(),
        profile.kind,
        bootstrap_secret,
        random_entropy,
        120,
        profile.metadata_posture,
        profile.trust_label.clone(),
    )?;
    let rendezvous_topic = capability.topic.clone();
    let alice = SignalingPeerId::new("runtime-probe-alice")?;
    let bob = SignalingPeerId::new("runtime-probe-bob")?;
    let alice_session = adapter.connect(profile.clone()).await?;
    let bob_session = adapter.connect(profile.clone()).await?;
    let alice_room = alice_session
        .join(scope.clone(), capability.clone(), alice.clone())
        .await?;
    let bob_room = bob_session
        .join(scope.clone(), capability, bob.clone())
        .await?;

    tokio::time::sleep(Duration::from_millis(500)).await;
    alice_room
        .publish_presence(
            OpaqueSignalingPayload::new(b"sealed-runtime-probe-presence".to_vec())?,
            120,
        )
        .await?;
    let presence_roundtrip = wait_for_probe(|| async {
        let events = bob_room.subscribe_presence().await?;
        Ok(events.into_iter().any(|event| event.peer_id == alice))
    })
    .await?;

    let offer = SealedWebRtcNegotiationPayload {
        version: 1,
        kind: crate::WebRtcNegotiationPayloadKind::Offer,
        nonce: [7_u8; 12],
        ciphertext: b"sealed-runtime-probe-offer".to_vec(),
    };
    alice_room.send_signal(bob.clone(), offer.clone()).await?;
    let signal_roundtrip = wait_for_probe(|| async {
        let signals = bob_room.take_signals().await?;
        Ok(signals.into_iter().any(|signal| {
            signal.from_peer == alice && signal.to_peer == bob && signal.payload == offer
        }))
    })
    .await?;

    bob_room
        .broadcast_control(OpaqueSignalingPayload::new(
            b"sealed-runtime-probe-control".to_vec(),
        )?)
        .await?;
    let control_roundtrip = wait_for_probe(|| async {
        let controls = alice_room.take_control_payloads().await?;
        Ok(controls.into_iter().any(|control| control.from_peer == bob))
    })
    .await?;

    alice_room.leave().await?;
    bob_room.leave().await?;
    alice_session.close().await?;
    bob_session.close().await?;

    Ok(ProviderAdapterRoundtripProbe {
        kind: profile.kind,
        profile_id: profile.profile_id,
        endpoint_label,
        scope_commitment: scope.scope_id_commitment,
        rendezvous_topic,
        presence_roundtrip,
        signal_roundtrip,
        control_roundtrip,
    })
}

#[cfg(any(
    test,
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
async fn start_provider_webrtc_text_control_runtime_pair_with_adapter<A, F>(
    adapter: A,
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    negotiation_config: WebRtcNegotiationConfig,
    answerer: F,
) -> Result<ProviderTextControlRuntimePair, TransportError>
where
    A: SignalingAdapter,
    F: Fn(Vec<u8>) -> Result<Vec<u8>, TransportError> + Send + Sync + 'static,
{
    let endpoint_label = profile
        .endpoints
        .first()
        .map(|endpoint| redacted_endpoint_label(&endpoint.endpoint.0))
        .unwrap_or_else(|| "endpoint#missing".to_owned());
    let capability = RendezvousCapability::derive(
        scope.clone(),
        profile.kind,
        bootstrap_secret,
        random_entropy,
        120,
        profile.metadata_posture,
        profile.trust_label.clone(),
    )?;
    let rendezvous_topic = capability.topic.clone();
    let alice = SignalingPeerId::new("runtime-webrtc-live-alice")?;
    let bob = SignalingPeerId::new("runtime-webrtc-live-bob")?;
    let alice_session = adapter.connect(profile.clone()).await?;
    let bob_session = adapter.connect(profile.clone()).await?;
    let alice_room = alice_session
        .join(scope.clone(), capability.clone(), alice.clone())
        .await?;
    let bob_room = bob_session
        .join(scope.clone(), capability, bob.clone())
        .await?;

    tokio::time::sleep(Duration::from_secs(1)).await;

    let alice_webrtc = Arc::new(WebRtcNegotiator::new(negotiation_config.clone()).await?);
    let bob_webrtc = Arc::new(WebRtcNegotiator::new(negotiation_config).await?);
    let sealer = WebRtcNegotiationSealer::new([0x9d; 32]);

    let offer = alice_webrtc.create_offer().await?;
    let sealed_offer = sealer.seal_description(&offer)?;
    let opaque_offer = sealed_offer.to_opaque_bytes()?;
    if opaque_offer.windows(3).any(|window| window == b"v=0") {
        return Err(TransportError::PlaintextLeak);
    }
    let captured_sealed_offer = Some(sealed_offer.clone());
    let mut captured_sealed_answer = None;
    let mut offerer_ice_candidates = Vec::new();
    let mut answerer_ice_candidates = Vec::new();
    alice_room.send_signal(bob.clone(), sealed_offer).await?;

    let mut answer_applied = false;
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        for signal in bob_room.take_signals().await? {
            if signal.from_peer != alice || signal.to_peer != bob {
                continue;
            }
            match signal.payload.kind {
                WebRtcNegotiationPayloadKind::Offer => {
                    let offer = sealer.open_description(&signal.payload)?;
                    let answer = bob_webrtc.create_answer(offer).await?;
                    let sealed_answer = sealer.seal_description(&answer)?;
                    captured_sealed_answer = Some(sealed_answer.clone());
                    bob_room.send_signal(alice.clone(), sealed_answer).await?;
                }
                WebRtcNegotiationPayloadKind::Candidate => {
                    bob_webrtc
                        .add_remote_candidate(sealer.open_candidate(&signal.payload)?)
                        .await?;
                }
                WebRtcNegotiationPayloadKind::Answer => {}
            }
        }

        for signal in alice_room.take_signals().await? {
            if signal.from_peer != bob || signal.to_peer != alice {
                continue;
            }
            match signal.payload.kind {
                WebRtcNegotiationPayloadKind::Answer if !answer_applied => {
                    alice_webrtc
                        .accept_answer(sealer.open_description(&signal.payload)?)
                        .await?;
                    answer_applied = true;
                }
                WebRtcNegotiationPayloadKind::Candidate => {
                    alice_webrtc
                        .add_remote_candidate(sealer.open_candidate(&signal.payload)?)
                        .await?;
                }
                WebRtcNegotiationPayloadKind::Offer | WebRtcNegotiationPayloadKind::Answer => {}
            }
        }

        for candidate in alice_webrtc.drain_local_candidates().await {
            let sealed_candidate = sealer.seal_candidate(&candidate)?;
            offerer_ice_candidates.push(sealed_candidate.clone());
            alice_room
                .send_signal(bob.clone(), sealed_candidate)
                .await?;
        }
        for candidate in bob_webrtc.drain_local_candidates().await {
            let sealed_candidate = sealer.seal_candidate(&candidate)?;
            answerer_ice_candidates.push(sealed_candidate.clone());
            bob_room
                .send_signal(alice.clone(), sealed_candidate)
                .await?;
        }

        if answer_applied
            && alice_webrtc.direct_path_metrics().await.direct_path_ready
            && bob_webrtc.direct_path_metrics().await.direct_path_ready
        {
            alice_webrtc
                .wait_text_control_transport_ready(Duration::from_secs(5))
                .await?;
            bob_webrtc
                .wait_text_control_transport_ready(Duration::from_secs(5))
                .await?;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let offerer_data = alice_webrtc.text_control_transport_metrics().await;
    let answerer_data = bob_webrtc.text_control_transport_metrics().await;
    if !offerer_data.open || !answerer_data.open {
        return Err(TransportError::Unavailable(format!(
            "provider-signaled WebRTC runtime data channel did not open: alice={:?} bob={:?}",
            alice_webrtc.direct_path_metrics().await,
            bob_webrtc.direct_path_metrics().await
        )));
    }

    let alice_direct = alice_webrtc.direct_path_metrics().await;
    let bob_direct = bob_webrtc.direct_path_metrics().await;
    let now_unix_seconds = Utc::now().timestamp();
    let mut runtime_ice_candidates = Vec::new();
    runtime_ice_candidates.extend(offerer_ice_candidates);
    runtime_ice_candidates.extend(answerer_ice_candidates);
    let runtime_spec = ProviderTextControlRuntimeSpec {
        schema_version: PROVIDER_TEXT_CONTROL_RUNTIME_SPEC_SCHEMA_VERSION,
        attachment: ProviderTextControlRuntimeAttachment {
            adapter_kind: profile.kind.canonical_name().to_owned(),
            profile_id: profile.profile_id.clone(),
            endpoint_label: endpoint_label.clone(),
            rendezvous_topic: rendezvous_topic.clone(),
            scope_commitment: scope.scope_id_commitment.clone(),
            runtime_spec: None,
        },
        created_at_unix_seconds: now_unix_seconds,
        expires_at_unix_seconds: now_unix_seconds.saturating_add(60 * 60),
        sealed_offer: captured_sealed_offer,
        sealed_answer: captured_sealed_answer,
        sealed_ice_candidates: runtime_ice_candidates,
        missing_material: Vec::new(),
    };
    let evidence = ProviderTextControlRuntimeEvidence {
        kind: profile.kind,
        profile_id: profile.profile_id,
        endpoint_label,
        scope_commitment: scope.scope_id_commitment,
        rendezvous_topic,
        offerer_direct_path_ready: alice_direct.direct_path_ready,
        answerer_direct_path_ready: bob_direct.direct_path_ready,
        offerer_data_channel_open: offerer_data.open,
        answerer_data_channel_open: answerer_data.open,
        runtime_spec,
    };

    alice_room.leave().await?;
    bob_room.leave().await?;
    alice_session.close().await?;
    bob_session.close().await?;

    let answerer_webrtc = bob_webrtc.clone();
    let answerer = Arc::new(answerer);
    let answerer_task = tokio::spawn(async move {
        loop {
            let received = match answerer_webrtc.recv_text_control_frame().await {
                Ok(received) => received,
                Err(_) => break,
            };
            let response = match answerer(received) {
                Ok(response) => response,
                Err(_) => break,
            };
            if response.is_empty() {
                continue;
            }
            if answerer_webrtc
                .send_text_control_frame(response)
                .await
                .is_err()
            {
                break;
            }
        }
    });

    Ok(ProviderTextControlRuntimePair {
        evidence,
        offerer: Some(alice_webrtc),
        answerer: Some(bob_webrtc),
        answerer_task: Some(answerer_task),
    })
}

#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
async fn probe_webrtc_with_adapter_and_answerer<A, F>(
    adapter: A,
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    negotiation_config: WebRtcNegotiationConfig,
    text_control_frame: Vec<u8>,
    answerer: F,
) -> Result<ProviderWebRtcDataChannelProbe, TransportError>
where
    A: SignalingAdapter,
    F: FnOnce(Vec<u8>) -> Result<Vec<u8>, TransportError> + Send,
{
    let endpoint_label = profile
        .endpoints
        .first()
        .map(|endpoint| redacted_endpoint_label(&endpoint.endpoint.0))
        .unwrap_or_else(|| "endpoint#missing".to_owned());
    let capability = RendezvousCapability::derive(
        scope.clone(),
        profile.kind,
        bootstrap_secret,
        random_entropy,
        120,
        profile.metadata_posture,
        profile.trust_label.clone(),
    )?;
    let rendezvous_topic = capability.topic.clone();
    let alice = SignalingPeerId::new("runtime-webrtc-probe-alice")?;
    let bob = SignalingPeerId::new("runtime-webrtc-probe-bob")?;
    let alice_session = adapter.connect(profile.clone()).await?;
    let bob_session = adapter.connect(profile.clone()).await?;
    let alice_room = alice_session
        .join(scope.clone(), capability.clone(), alice.clone())
        .await?;
    let bob_room = bob_session
        .join(scope.clone(), capability, bob.clone())
        .await?;

    tokio::time::sleep(Duration::from_secs(1)).await;

    let alice_config = negotiation_config.clone();
    let bob_config = negotiation_config;
    let alice_webrtc = WebRtcNegotiator::new(alice_config).await?;
    let bob_webrtc = WebRtcNegotiator::new(bob_config).await?;
    let sealer = WebRtcNegotiationSealer::new([0x9d; 32]);

    let offer = alice_webrtc
        .create_complete_offer(Duration::from_secs(45))
        .await?;
    let sealed_offer = sealer.seal_description(&offer)?;
    let opaque_offer = sealed_offer.to_opaque_bytes()?;
    if opaque_offer.windows(3).any(|window| window == b"v=0") {
        return Err(TransportError::PlaintextLeak);
    }
    let captured_sealed_offer = Some(sealed_offer.clone());
    let mut captured_sealed_answer = None;
    let mut offerer_ice_candidates = Vec::new();
    let mut answerer_ice_candidates = Vec::new();
    alice_room.send_signal(bob.clone(), sealed_offer).await?;

    let mut answer_applied = false;
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        for signal in bob_room.take_signals().await? {
            if signal.from_peer != alice || signal.to_peer != bob {
                continue;
            }
            match signal.payload.kind {
                WebRtcNegotiationPayloadKind::Offer => {
                    let offer = sealer.open_description(&signal.payload)?;
                    let answer = bob_webrtc
                        .create_complete_answer(offer, Duration::from_secs(45))
                        .await?;
                    let sealed_answer = sealer.seal_description(&answer)?;
                    captured_sealed_answer = Some(sealed_answer.clone());
                    bob_room.send_signal(alice.clone(), sealed_answer).await?;
                }
                WebRtcNegotiationPayloadKind::Candidate => {
                    bob_webrtc
                        .add_remote_candidate(sealer.open_candidate(&signal.payload)?)
                        .await?;
                }
                WebRtcNegotiationPayloadKind::Answer => {}
            }
        }

        for signal in alice_room.take_signals().await? {
            if signal.from_peer != bob || signal.to_peer != alice {
                continue;
            }
            match signal.payload.kind {
                WebRtcNegotiationPayloadKind::Answer if !answer_applied => {
                    alice_webrtc
                        .accept_answer(sealer.open_description(&signal.payload)?)
                        .await?;
                    answer_applied = true;
                }
                WebRtcNegotiationPayloadKind::Candidate => {
                    alice_webrtc
                        .add_remote_candidate(sealer.open_candidate(&signal.payload)?)
                        .await?;
                }
                WebRtcNegotiationPayloadKind::Offer | WebRtcNegotiationPayloadKind::Answer => {}
            }
        }

        for candidate in alice_webrtc.drain_local_candidates().await {
            let sealed_candidate = sealer.seal_candidate(&candidate)?;
            offerer_ice_candidates.push(sealed_candidate.clone());
            alice_room
                .send_signal(bob.clone(), sealed_candidate)
                .await?;
        }
        for candidate in bob_webrtc.drain_local_candidates().await {
            let sealed_candidate = sealer.seal_candidate(&candidate)?;
            answerer_ice_candidates.push(sealed_candidate.clone());
            bob_room
                .send_signal(alice.clone(), sealed_candidate)
                .await?;
        }

        if answer_applied
            && alice_webrtc.direct_path_metrics().await.direct_path_ready
            && bob_webrtc.direct_path_metrics().await.direct_path_ready
        {
            alice_webrtc
                .wait_text_control_transport_ready(Duration::from_secs(5))
                .await?;
            bob_webrtc
                .wait_text_control_transport_ready(Duration::from_secs(5))
                .await?;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let offerer_data = alice_webrtc.text_control_transport_metrics().await;
    let answerer_data = bob_webrtc.text_control_transport_metrics().await;
    if !offerer_data.open || !answerer_data.open {
        return Err(TransportError::Unavailable(format!(
            "provider-signaled WebRTC data channel did not open: alice={:?} bob={:?}",
            alice_webrtc.direct_path_metrics().await,
            bob_webrtc.direct_path_metrics().await
        )));
    }

    let frame = text_control_frame;
    let frame_sha256 = sha256_hex(&frame);
    alice_webrtc.send_text_control_frame(frame.clone()).await?;
    let received = timeout(Duration::from_secs(5), bob_webrtc.recv_text_control_frame())
        .await
        .map_err(|_| {
            TransportError::Unavailable("timed out receiving WebRTC data frame".to_owned())
        })??;
    let frame_roundtrip = received == frame;

    let receipt_frame = answerer(received)?;
    if receipt_frame.is_empty() {
        return Err(TransportError::Unavailable(
            "receipt/control proof frame must be non-empty opaque bytes".to_owned(),
        ));
    }
    let receipt_frame_sha256 = sha256_hex(&receipt_frame);
    bob_webrtc
        .send_text_control_frame(receipt_frame.clone())
        .await?;
    let received_receipt = timeout(
        Duration::from_secs(5),
        alice_webrtc.recv_text_control_frame(),
    )
    .await
    .map_err(|_| {
        TransportError::Unavailable("timed out receiving WebRTC receipt/control frame".to_owned())
    })??;
    let receipt_frame_roundtrip = received_receipt == receipt_frame;

    let alice_direct = alice_webrtc.direct_path_metrics().await;
    let bob_direct = bob_webrtc.direct_path_metrics().await;
    let offerer_data = alice_webrtc.text_control_transport_metrics().await;
    let answerer_data = bob_webrtc.text_control_transport_metrics().await;

    let now_unix_seconds = Utc::now().timestamp();
    let mut runtime_ice_candidates = Vec::new();
    runtime_ice_candidates.extend(offerer_ice_candidates);
    runtime_ice_candidates.extend(answerer_ice_candidates);
    let runtime_spec = ProviderTextControlRuntimeSpec {
        schema_version: PROVIDER_TEXT_CONTROL_RUNTIME_SPEC_SCHEMA_VERSION,
        attachment: ProviderTextControlRuntimeAttachment {
            adapter_kind: profile.kind.canonical_name().to_owned(),
            profile_id: profile.profile_id.clone(),
            endpoint_label: endpoint_label.clone(),
            rendezvous_topic: rendezvous_topic.clone(),
            scope_commitment: scope.scope_id_commitment.clone(),
            runtime_spec: None,
        },
        created_at_unix_seconds: now_unix_seconds,
        expires_at_unix_seconds: now_unix_seconds.saturating_add(60 * 60),
        sealed_offer: captured_sealed_offer,
        sealed_answer: captured_sealed_answer,
        sealed_ice_candidates: runtime_ice_candidates,
        missing_material: Vec::new(),
    };

    alice_webrtc.tear_down().await?;
    bob_webrtc.tear_down().await?;
    alice_room.leave().await?;
    bob_room.leave().await?;
    alice_session.close().await?;
    bob_session.close().await?;

    Ok(ProviderWebRtcDataChannelProbe {
        kind: profile.kind,
        profile_id: profile.profile_id,
        endpoint_label,
        scope_commitment: scope.scope_id_commitment,
        rendezvous_topic,
        offerer_direct_path_ready: alice_direct.direct_path_ready,
        answerer_direct_path_ready: bob_direct.direct_path_ready,
        offerer_turn_fallback_ready: alice_direct.turn_fallback_ready,
        answerer_turn_fallback_ready: bob_direct.turn_fallback_ready,
        offerer_configured_turn_servers: alice_direct.configured_turn_servers,
        answerer_configured_turn_servers: bob_direct.configured_turn_servers,
        offerer_local_relay_candidates_gathered: alice_direct.local_relay_candidates_gathered,
        answerer_local_relay_candidates_gathered: bob_direct.local_relay_candidates_gathered,
        offerer_remote_relay_candidates_applied: alice_direct.remote_relay_candidates_applied,
        answerer_remote_relay_candidates_applied: bob_direct.remote_relay_candidates_applied,
        offerer_data_channel_open: offerer_data.open,
        answerer_data_channel_open: answerer_data.open,
        text_control_frame_roundtrip: frame_roundtrip,
        text_control_frame_sha256: frame_sha256,
        receipt_frame_roundtrip,
        receipt_frame_sha256,
        runtime_spec: Some(runtime_spec),
    })
}

fn default_receipt_control_frame(text_control_frame: &[u8]) -> Vec<u8> {
    format!(
        "ciphertext:provider-webrtc-receipt:v1:{}",
        sha256_hex(text_control_frame)
    )
    .into_bytes()
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
async fn wait_for_probe<Fut>(mut poll: impl FnMut() -> Fut) -> Result<bool, TransportError>
where
    Fut: std::future::Future<Output = Result<bool, TransportError>>,
{
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if poll().await? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Err(TransportError::SignalingAdapter(
                "timed out waiting for provider adapter roundtrip probe".to_owned(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[cfg(any(
    test,
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
fn redacted_endpoint_label(endpoint: &str) -> String {
    use sha2::Digest as _;

    let mut digest = sha2::Sha256::new();
    digest.update(endpoint.as_bytes());
    let digest = digest.finalize();
    let digest = digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let scheme = endpoint
        .split_once(':')
        .map(|(scheme, _)| scheme)
        .unwrap_or("endpoint");
    format!("{scheme}#{digest}")
}

/// Production readiness for a provider adapter boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAdapterReadiness {
    /// The provider adapter Cargo feature is not enabled in this build.
    FeatureDisabled,
    /// The Cargo feature is enabled but no audited provider client is wired yet.
    ImplementationUnavailable,
    /// A real provider client is wired behind the Cargo feature in this build.
    ImplementationAvailable,
}

/// Static production boundary metadata for one required signaling adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderAdapterBoundary {
    /// Adapter kind covered by this boundary.
    pub kind: SignalingAdapterKind,
    /// Cargo feature that must gate a future real provider implementation.
    pub cargo_feature: &'static str,
    /// Current readiness state for this build.
    pub readiness: ProviderAdapterReadiness,
}

impl ProviderAdapterBoundary {
    /// Static readiness projected into runtime planning state.
    #[must_use]
    pub const fn readiness_state(self) -> AdapterReadinessState {
        match self.readiness {
            ProviderAdapterReadiness::FeatureDisabled => AdapterReadinessState::FeatureDisabled,
            ProviderAdapterReadiness::ImplementationUnavailable => {
                AdapterReadinessState::ImplementationUnavailable
            }
            ProviderAdapterReadiness::ImplementationAvailable => AdapterReadinessState::Available,
        }
    }

    /// True when a real provider client is available in this build.
    #[must_use]
    pub const fn implementation_available(self) -> bool {
        matches!(
            self.readiness,
            ProviderAdapterReadiness::ImplementationAvailable
        )
    }

    /// Redacted failure label for health/observability.
    #[must_use]
    pub const fn failure_class(self) -> &'static str {
        match self.readiness {
            ProviderAdapterReadiness::FeatureDisabled => "feature_disabled",
            ProviderAdapterReadiness::ImplementationUnavailable => "implementation_unavailable",
            ProviderAdapterReadiness::ImplementationAvailable => "implementation_available",
        }
    }

    fn unavailable_error(self) -> TransportError {
        match self.readiness {
            ProviderAdapterReadiness::FeatureDisabled => TransportError::SignalingAdapter(format!(
                "{} adapter is not enabled; compile with Cargo feature {} only after an audited provider client is wired",
                self.kind.canonical_name(),
                self.cargo_feature
            )),
            ProviderAdapterReadiness::ImplementationUnavailable => TransportError::SignalingAdapter(format!(
                "{} adapter feature {} is enabled but no audited production provider client is wired",
                self.kind.canonical_name(),
                self.cargo_feature
            )),
            ProviderAdapterReadiness::ImplementationAvailable => TransportError::SignalingAdapter(format!(
                "{} adapter feature {} is available; use its concrete production adapter instead of the fail-closed boundary",
                self.kind.canonical_name(),
                self.cargo_feature
            )),
        }
    }
}

/// Return the feature-gated boundary for a required provider adapter kind.
#[must_use]
pub const fn adapter_boundary_for_kind(kind: SignalingAdapterKind) -> ProviderAdapterBoundary {
    match kind {
        SignalingAdapterKind::Mqtt => ProviderAdapterBoundary {
            kind,
            cargo_feature: "mqtt-adapter",
            readiness: mqtt_feature_readiness(),
        },
        SignalingAdapterKind::Nostr => ProviderAdapterBoundary {
            kind,
            cargo_feature: "nostr-adapter",
            readiness: nostr_feature_readiness(),
        },
        SignalingAdapterKind::IpfsPubsub => ProviderAdapterBoundary {
            kind,
            cargo_feature: "ipfs-pubsub-adapter",
            readiness: ipfs_pubsub_feature_readiness(),
        },
        SignalingAdapterKind::DiscryptQuicRendezvous => ProviderAdapterBoundary {
            kind,
            cargo_feature: "discrypt-quic-rendezvous-adapter",
            readiness: discrypt_quic_rendezvous_feature_readiness(),
        },
    }
}

/// Return all required production provider adapter boundaries.
#[must_use]
pub const fn required_provider_adapter_boundaries() -> [ProviderAdapterBoundary; 4] {
    [
        adapter_boundary_for_kind(SignalingAdapterKind::Mqtt),
        adapter_boundary_for_kind(SignalingAdapterKind::Nostr),
        adapter_boundary_for_kind(SignalingAdapterKind::IpfsPubsub),
        adapter_boundary_for_kind(SignalingAdapterKind::DiscryptQuicRendezvous),
    ]
}

const fn mqtt_feature_readiness() -> ProviderAdapterReadiness {
    if cfg!(feature = "mqtt-adapter") {
        ProviderAdapterReadiness::ImplementationAvailable
    } else {
        ProviderAdapterReadiness::FeatureDisabled
    }
}

const fn nostr_feature_readiness() -> ProviderAdapterReadiness {
    if cfg!(feature = "nostr-adapter") {
        ProviderAdapterReadiness::ImplementationAvailable
    } else {
        ProviderAdapterReadiness::FeatureDisabled
    }
}

const fn ipfs_pubsub_feature_readiness() -> ProviderAdapterReadiness {
    if cfg!(feature = "ipfs-pubsub-adapter") {
        ProviderAdapterReadiness::ImplementationAvailable
    } else {
        ProviderAdapterReadiness::FeatureDisabled
    }
}

const fn discrypt_quic_rendezvous_feature_readiness() -> ProviderAdapterReadiness {
    if cfg!(feature = "discrypt-quic-rendezvous-adapter") {
        ProviderAdapterReadiness::ImplementationAvailable
    } else {
        ProviderAdapterReadiness::FeatureDisabled
    }
}

/// Fail-closed adapter implementation used until a real provider client is wired.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeatureGatedProviderAdapter {
    boundary: ProviderAdapterBoundary,
}

impl FeatureGatedProviderAdapter {
    /// Construct a boundary adapter for a required provider kind.
    #[must_use]
    pub const fn new(kind: SignalingAdapterKind) -> Self {
        Self {
            boundary: adapter_boundary_for_kind(kind),
        }
    }

    /// Return this adapter's static boundary metadata.
    #[must_use]
    pub const fn boundary(&self) -> ProviderAdapterBoundary {
        self.boundary
    }
}

/// Session type for fail-closed provider boundaries. It should never be reached
/// in production because `connect` returns an error while no implementation is wired.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeatureGatedProviderSession {
    boundary: ProviderAdapterBoundary,
}

/// Room type for fail-closed provider boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeatureGatedProviderRoom {
    boundary: ProviderAdapterBoundary,
}

/// Shared deterministic in-memory provider bus for local adapter conformance tests.
///
/// This is not a production provider client. It deliberately uses the same
/// [`SignalingAdapter`] contract as real providers so every required adapter
/// kind can prove opaque presence, sealed negotiation payload, and sealed
/// control delivery without reaching an external network.
#[derive(Clone, Debug, Default)]
pub struct LocalConformanceProviderBus {
    inner: Arc<Mutex<LocalConformanceState>>,
}

#[derive(Debug, Default)]
struct LocalConformanceState {
    rooms: BTreeMap<LocalRoomKey, LocalRoomState>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct LocalRoomKey {
    kind: SignalingAdapterKind,
    topic: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct LocalRoomState {
    presence: Vec<PresenceEvent>,
    signals: Vec<PeerSignal>,
    controls: Vec<ControlBroadcast>,
}

impl LocalConformanceProviderBus {
    fn ensure_room(&self, key: LocalRoomKey) -> Result<(), TransportError> {
        self.with_state(|state| {
            state.rooms.entry(key).or_default();
        })
    }

    fn with_state<T>(
        &self,
        update: impl FnOnce(&mut LocalConformanceState) -> T,
    ) -> Result<T, TransportError> {
        let mut state = self.inner.lock().map_err(|_| {
            TransportError::SignalingAdapter("local conformance bus lock poisoned".to_owned())
        })?;
        Ok(update(&mut state))
    }

    /// Return relay-visible test material currently held by the local bus.
    ///
    /// Tests use this to assert display names, raw SDP, and raw ICE candidate
    /// strings never entered provider-visible state.
    #[must_use]
    pub fn relay_visible_material_for_tests(&self) -> Vec<Vec<u8>> {
        self.inner
            .lock()
            .map(|state| {
                state
                    .rooms
                    .iter()
                    .flat_map(|(key, room)| {
                        let mut material = vec![
                            key.kind.canonical_name().as_bytes().to_vec(),
                            key.topic.as_bytes().to_vec(),
                        ];
                        material.extend(room.presence.iter().map(|event| {
                            let mut bytes = event.peer_id.0.as_bytes().to_vec();
                            bytes.extend_from_slice(&event.encrypted_presence.bytes);
                            bytes
                        }));
                        material.extend(room.signals.iter().map(|signal| {
                            let mut bytes = signal.from_peer.0.as_bytes().to_vec();
                            bytes.extend_from_slice(signal.to_peer.0.as_bytes());
                            bytes.extend_from_slice(&signal.payload.ciphertext);
                            bytes
                        }));
                        material.extend(room.controls.iter().map(|control| {
                            let mut bytes = control.from_peer.0.as_bytes().to_vec();
                            bytes.extend_from_slice(&control.payload.bytes);
                            bytes
                        }));
                        material
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Deterministic local adapter that exercises the shared provider contract.
#[derive(Clone, Debug)]
pub struct LocalConformanceProviderAdapter {
    kind: SignalingAdapterKind,
    bus: LocalConformanceProviderBus,
}

impl LocalConformanceProviderAdapter {
    /// Construct a deterministic local adapter for one required provider kind.
    #[must_use]
    pub const fn new(kind: SignalingAdapterKind, bus: LocalConformanceProviderBus) -> Self {
        Self { kind, bus }
    }
}

/// Connected deterministic local provider session.
#[derive(Clone, Debug)]
pub struct LocalConformanceProviderSession {
    kind: SignalingAdapterKind,
    bus: LocalConformanceProviderBus,
}

/// Joined deterministic local provider room.
#[derive(Clone, Debug)]
pub struct LocalConformanceProviderRoom {
    bus: LocalConformanceProviderBus,
    key: LocalRoomKey,
    local_peer_id: SignalingPeerId,
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
/// Real adapter for the separate Discrypt rendezvous service.
///
/// The extracted sibling service currently exposes a content-blind HTTPS/WSS
/// HTTP API at `/v1/signals/*`; native `quic://` transport is still reserved by
/// the service ADR. This adapter is intentionally named for the product
/// adapter slot, but it only connects to validated `https://` service URLs or
/// loopback `http://127.0.0.1` development URLs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiscryptQuicRendezvousProviderAdapter;

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[derive(Clone, Debug)]
pub struct DiscryptQuicRendezvousProviderSession {
    profile: SignalingAdapterProfile,
    endpoint_base: String,
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
pub struct DiscryptQuicRendezvousProviderRoom {
    endpoint_base: String,
    local_peer_id: SignalingPeerId,
    topic: String,
    nonce_counter: Arc<std::sync::atomic::AtomicU64>,
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DiscryptRendezvousWireEnvelope {
    Presence {
        schema: u8,
        from_peer: SignalingPeerId,
        payload: OpaqueSignalingPayload,
        ttl_seconds: u32,
    },
    Signal {
        schema: u8,
        from_peer: SignalingPeerId,
        to_peer: SignalingPeerId,
        payload: SealedWebRtcNegotiationPayload,
    },
    Control {
        schema: u8,
        from_peer: SignalingPeerId,
        payload: OpaqueSignalingPayload,
    },
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DiscryptRendezvousSignalKind {
    Rendezvous,
    Offer,
    Answer,
    Candidate,
    AdmissionHelper,
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[derive(Serialize)]
struct DiscryptRendezvousPublishSignalRequest {
    client_token_hex: String,
    nonce_hex: String,
    kind: DiscryptRendezvousSignalKind,
    key_hex: String,
    payload_hex: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[derive(Serialize)]
struct DiscryptRendezvousTakeSignalRequest {
    client_token_hex: String,
    nonce_hex: String,
    kind: DiscryptRendezvousSignalKind,
    key_hex: String,
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[derive(Deserialize)]
struct DiscryptRendezvousTakeSignalsResponse {
    signals: Vec<DiscryptRendezvousTakenSignal>,
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[derive(Deserialize)]
struct DiscryptRendezvousTakenSignal {
    kind: DiscryptRendezvousSignalKind,
    payload_hex: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[derive(Clone, Debug, Deserialize)]
struct DiscryptRendezvousHealthResponse {
    #[serde(default)]
    schema_version: Option<u16>,
    #[serde(default)]
    protocol_version: Option<String>,
    status: String,
    service: String,
    public_base_url: String,
    #[serde(default)]
    max_body_bytes: Option<usize>,
    #[serde(default)]
    rate_limit_window_seconds: Option<u64>,
    #[serde(default)]
    rate_limit_max_requests: Option<u32>,
    #[serde(default)]
    service_identity_fingerprint: Option<String>,
    #[serde(default)]
    tls_alpn_protocols: Vec<String>,
    #[serde(default)]
    service_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    rotation_policy: Option<String>,
    #[serde(default)]
    endpoint_allowlist_commitment: Option<String>,
    at_rest_records: usize,
}

#[cfg(feature = "mqtt-adapter")]
/// Real MQTT signaling adapter.
///
/// This adapter publishes only already-sealed Discrypt signaling envelopes to
/// broker topics derived from [`RendezvousCapability`]. It does not receive raw
/// SDP, ICE credentials, group names, display names, invite secrets, or message
/// plaintext. Public production profiles must use `mqtts://` or `wss://`;
/// `mqtt://` is accepted only when the profile itself validates as loopback
/// local development.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MqttProviderAdapter;

#[cfg(feature = "mqtt-adapter")]
#[derive(Clone, Debug)]
pub struct MqttProviderSession {
    profile: SignalingAdapterProfile,
}

#[cfg(feature = "mqtt-adapter")]
pub struct MqttProviderRoom {
    local_peer_id: SignalingPeerId,
    client: rumqttc::AsyncClient,
    events: AsyncMutex<MqttEventReceiver>,
    rendezvous_topic: String,
    topics: MqttTopics,
    inbox: AsyncMutex<MqttInbox>,
}

#[cfg(feature = "mqtt-adapter")]
type MqttEventReceiver = tokio::sync::mpsc::Receiver<MqttProviderEvent>;

#[cfg(feature = "mqtt-adapter")]
#[derive(Debug)]
enum MqttProviderEvent {
    Publish { topic: String, payload: Vec<u8> },
    SubAck,
    Error(String),
}

#[cfg(feature = "mqtt-adapter")]
#[derive(Clone, Debug, Eq, PartialEq)]
struct MqttTopics {
    presence: String,
    control: String,
    signal_for_local_peer: String,
}

#[cfg(feature = "mqtt-adapter")]
#[derive(Debug, Default)]
struct MqttInbox {
    presence: Vec<PresenceEvent>,
    signals: Vec<PeerSignal>,
    controls: Vec<ControlBroadcast>,
}

#[cfg(feature = "mqtt-adapter")]
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum MqttWireEnvelope {
    Presence {
        schema: u8,
        from_peer: SignalingPeerId,
        payload: OpaqueSignalingPayload,
        ttl_seconds: u32,
    },
    Signal {
        schema: u8,
        from_peer: SignalingPeerId,
        to_peer: SignalingPeerId,
        payload: SealedWebRtcNegotiationPayload,
    },
    Control {
        schema: u8,
        from_peer: SignalingPeerId,
        payload: OpaqueSignalingPayload,
    },
}

#[cfg(feature = "nostr-adapter")]
/// Real Nostr relay signaling adapter.
///
/// The relay receives NIP-01 events whose content is a Discrypt-specific JSON
/// envelope containing only already-sealed presence, control, and WebRTC
/// negotiation payload bytes. The public event kind is intentionally scoped to a
/// random/hashed rendezvous topic (`d` tag); display names, room labels, raw SDP,
/// ICE credentials, and invite secrets are not serialized into provider-visible
/// fields.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NostrProviderAdapter;

#[cfg(feature = "nostr-adapter")]
#[derive(Clone, Debug)]
pub struct NostrProviderSession {
    profile: SignalingAdapterProfile,
}

#[cfg(feature = "nostr-adapter")]
pub struct NostrProviderRoom {
    local_peer_id: SignalingPeerId,
    client: nostr_sdk::Client,
    relay_urls: Vec<String>,
    subscription_id: nostr_sdk::SubscriptionId,
    topic: String,
    notifications: AsyncMutex<tokio::sync::broadcast::Receiver<nostr_sdk::RelayPoolNotification>>,
    inbox: AsyncMutex<NostrInbox>,
}

#[cfg(feature = "nostr-adapter")]
#[derive(Debug, Default)]
struct NostrInbox {
    presence: Vec<PresenceEvent>,
    signals: Vec<PeerSignal>,
    controls: Vec<ControlBroadcast>,
}

#[cfg(feature = "nostr-adapter")]
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum NostrWireEnvelope {
    Presence {
        schema: u8,
        from_peer: SignalingPeerId,
        payload: OpaqueSignalingPayload,
        ttl_seconds: u32,
    },
    Signal {
        schema: u8,
        from_peer: SignalingPeerId,
        to_peer: SignalingPeerId,
        payload: SealedWebRtcNegotiationPayload,
    },
    Control {
        schema: u8,
        from_peer: SignalingPeerId,
        payload: OpaqueSignalingPayload,
    },
}

#[cfg(feature = "ipfs-pubsub-adapter")]
/// Real IPFS/libp2p PubSub signaling adapter.
///
/// This adapter uses rust-libp2p gossipsub over configured bootstrap
/// multiaddrs. The PubSub topic is derived from [`RendezvousCapability`] and
/// every published message is a Discrypt-specific JSON envelope containing
/// only already-sealed presence, WebRTC negotiation, or control bytes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IpfsPubsubProviderAdapter;

#[cfg(feature = "ipfs-pubsub-adapter")]
#[derive(Clone, Debug)]
pub struct IpfsPubsubProviderSession {
    profile: SignalingAdapterProfile,
}

#[cfg(feature = "ipfs-pubsub-adapter")]
pub struct IpfsPubsubProviderRoom {
    local_peer_id: SignalingPeerId,
    local_libp2p_peer_id: libp2p::PeerId,
    commands: tokio::sync::mpsc::Sender<IpfsPubsubCommand>,
    inbox: Arc<AsyncMutex<IpfsPubsubInbox>>,
    listen_addresses: Arc<AsyncMutex<Vec<String>>>,
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[derive(Debug, Default)]
struct IpfsPubsubInbox {
    presence: Vec<PresenceEvent>,
    signals: Vec<PeerSignal>,
    controls: Vec<ControlBroadcast>,
    seen_messages: BTreeSet<String>,
    duplicate_counts: BTreeMap<String, usize>,
    health_faults: Vec<String>,
}

#[cfg(feature = "ipfs-pubsub-adapter")]
enum IpfsPubsubCommand {
    Publish {
        payload: Vec<u8>,
        result: tokio::sync::oneshot::Sender<Result<(), TransportError>>,
    },
    Leave {
        result: tokio::sync::oneshot::Sender<Result<(), TransportError>>,
    },
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[derive(libp2p::swarm::NetworkBehaviour)]
#[behaviour(prelude = "libp2p::swarm::derive_prelude")]
struct IpfsPubsubBehaviour {
    gossipsub: libp2p::gossipsub::Behaviour<
        libp2p::gossipsub::IdentityTransform,
        libp2p::gossipsub::AllowAllSubscriptionFilter,
    >,
    kademlia: libp2p::kad::Behaviour<libp2p::kad::store::MemoryStore>,
    identify: libp2p::identify::Behaviour,
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum IpfsPubsubWireEnvelope {
    Presence {
        schema: u8,
        from_peer: SignalingPeerId,
        payload: OpaqueSignalingPayload,
        ttl_seconds: u32,
    },
    Signal {
        schema: u8,
        from_peer: SignalingPeerId,
        to_peer: SignalingPeerId,
        payload: SealedWebRtcNegotiationPayload,
    },
    Control {
        schema: u8,
        from_peer: SignalingPeerId,
        payload: OpaqueSignalingPayload,
    },
}

#[cfg(feature = "nostr-adapter")]
const NOSTR_DISCRYPT_EVENT_KIND: u16 = 31_733;

#[cfg(feature = "nostr-adapter")]
const NOSTR_EVENT_SCHEMA: u8 = 1;

fn reject_forbidden_plaintext(bytes: &[u8]) -> Result<(), TransportError> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok(());
    };
    let lower = text.to_ascii_lowercase();
    if [
        "alice display",
        "bob display",
        "family voice",
        "raw sdp",
        "raw ice",
        "v=0",
        "a=ice-ufrag",
        "a=ice-pwd",
        "candidate:",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return Err(TransportError::PlaintextLeak);
    }
    Ok(())
}

#[cfg(feature = "mqtt-adapter")]
fn mqtt_err(context: &str, err: impl std::fmt::Display) -> TransportError {
    TransportError::SignalingAdapter(format!("mqtt {context} failed: {err}"))
}

#[cfg(feature = "nostr-adapter")]
fn nostr_err(context: &str, err: impl std::fmt::Display) -> TransportError {
    TransportError::SignalingAdapter(format!("nostr {context} failed: {err}"))
}

#[cfg(feature = "nostr-adapter")]
fn nostr_endpoints_for_profile(
    profile: &SignalingAdapterProfile,
) -> Result<Vec<String>, TransportError> {
    if profile.endpoints.is_empty() {
        return Err(TransportError::InvalidConnectivityPolicy(
            "nostr adapter profile must contain at least one relay endpoint".to_owned(),
        ));
    }
    Ok(profile
        .endpoints
        .iter()
        .map(|endpoint| endpoint.endpoint.0.clone())
        .collect())
}

#[cfg(feature = "nostr-adapter")]
fn nostr_client_secret(
    topic: &str,
    peer: &SignalingPeerId,
) -> Result<nostr_sdk::Keys, TransportError> {
    let mut material = Vec::new();
    material.extend_from_slice(b"discrypt-nostr-signaling-v1");
    material.extend_from_slice(topic.as_bytes());
    material.extend_from_slice(peer.0.as_bytes());
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(&material);
    let hex_secret = hex::encode(digest);
    nostr_sdk::Keys::parse(&hex_secret).map_err(|err| nostr_err("client key derivation", err))
}

#[cfg(feature = "nostr-adapter")]
fn nostr_subscription_id(topic: &str, peer: &SignalingPeerId) -> nostr_sdk::SubscriptionId {
    let mut material = Vec::new();
    material.extend_from_slice(b"discrypt-nostr-subscription-v1");
    material.extend_from_slice(topic.as_bytes());
    material.extend_from_slice(peer.0.as_bytes());
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(&material);
    nostr_sdk::SubscriptionId::new(format!("dc-{}", &hex::encode(digest)[..24]))
}

#[cfg(feature = "nostr-adapter")]
fn nostr_filter(topic: &str) -> nostr_sdk::Filter {
    nostr_sdk::Filter::new()
        .kind(nostr_sdk::Kind::Custom(NOSTR_DISCRYPT_EVENT_KIND))
        .identifier(topic.to_owned())
        .since(nostr_sdk::Timestamp::now())
}

#[cfg(feature = "nostr-adapter")]
fn nostr_discrypt_tag(topic: &str) -> Result<nostr_sdk::Tag, TransportError> {
    nostr_sdk::Tag::parse(["d", topic]).map_err(|err| nostr_err("topic tag", err))
}

#[cfg(feature = "nostr-adapter")]
fn nostr_notice_text_is_failure(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    [
        "auth",
        "blocked",
        "closed",
        "error",
        "failed",
        "forbidden",
        "limit",
        "pow",
        "rate",
        "reject",
        "restricted",
        "too large",
        "unauthorized",
        "unsupported",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

#[cfg(feature = "nostr-adapter")]
fn nostr_relay_message_failure(
    message: &nostr_sdk::RelayMessage<'_>,
) -> Option<AdapterReadinessState> {
    match message {
        nostr_sdk::RelayMessage::Notice(notice) if nostr_notice_text_is_failure(notice) => {
            Some(AdapterReadinessState::classify_provider_failure(notice))
        }
        nostr_sdk::RelayMessage::Closed { message, .. } => {
            Some(AdapterReadinessState::classify_provider_failure(message))
        }
        nostr_sdk::RelayMessage::Ok {
            status: false,
            message,
            ..
        } => Some(AdapterReadinessState::classify_provider_failure(message)),
        _ => None,
    }
}

#[cfg(feature = "nostr-adapter")]
fn nostr_relay_message_error(
    relay_url: &nostr_sdk::RelayUrl,
    message: &nostr_sdk::RelayMessage<'_>,
) -> Option<TransportError> {
    let readiness = nostr_relay_message_failure(message)?;
    Some(TransportError::SignalingAdapter(format!(
        "nostr relay message failed: relay={relay_url} failure_class={} health_state={:?}",
        readiness.failure_class(),
        readiness.to_health_state()
    )))
}

#[cfg(feature = "mqtt-adapter")]
fn mqtt_endpoint_for_profile(
    profile: &SignalingAdapterProfile,
) -> Result<&SignalingProviderEndpoint, TransportError> {
    profile.endpoints.first().ok_or_else(|| {
        TransportError::InvalidConnectivityPolicy(
            "mqtt adapter profile must contain at least one endpoint".to_owned(),
        )
    })
}

#[cfg(feature = "mqtt-adapter")]
fn mqtt_client_id(topic: &str, _peer: &SignalingPeerId) -> String {
    let mut random = [0_u8; 8];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut random);
    let suffix = topic.rsplit('-').next().unwrap_or(topic);
    let topic_suffix = &suffix[..suffix.len().min(8)];
    format!("dc{topic_suffix}{}", hex::encode(random))
}

#[cfg(feature = "mqtt-adapter")]
fn mqtt_options_from_endpoint(
    endpoint: &str,
    client_id: &str,
) -> Result<rumqttc::MqttOptions, TransportError> {
    let _ = rumqttc::tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
    let (parse_endpoint, secure_transport) = if let Some(rest) = endpoint.strip_prefix("mqtts://") {
        (format!("mqtt://{rest}"), Some("tls"))
    } else if let Some(rest) = endpoint.strip_prefix("ssl://") {
        (format!("mqtt://{rest}"), Some("tls"))
    } else if let Some(rest) = endpoint.strip_prefix("wss://") {
        (format!("ws://{rest}"), Some("wss"))
    } else {
        (endpoint.to_owned(), None)
    };
    let separator = if parse_endpoint.contains('?') {
        '&'
    } else {
        '?'
    };
    let url = format!("{parse_endpoint}{separator}client_id={client_id}");
    let mut options =
        rumqttc::MqttOptions::parse_url(url).map_err(|err| mqtt_err("endpoint parse", err))?;
    match secure_transport {
        Some("tls") => options.set_transport(rumqttc::Transport::tls_with_default_config()),
        Some("wss") => options.set_transport(rumqttc::Transport::wss_with_default_config()),
        _ => &mut options,
    };
    options.set_keep_alive(10);
    options.set_clean_start(true);
    options.set_max_packet_size(Some(64 * 1024));
    Ok(options)
}

#[cfg(feature = "mqtt-adapter")]
fn mqtt_topics(rendezvous: &RendezvousCapability, local_peer_id: &SignalingPeerId) -> MqttTopics {
    let base = format!("discrypt/v1/rendezvous/{}", rendezvous.topic);
    MqttTopics {
        presence: format!("{base}/presence"),
        control: format!("{base}/control"),
        signal_for_local_peer: format!("{base}/signal/{}", local_peer_id.0),
    }
}

#[cfg(feature = "mqtt-adapter")]
fn mqtt_signal_topic(rendezvous_topic: &str, peer_id: &SignalingPeerId) -> String {
    format!(
        "discrypt/v1/rendezvous/{rendezvous_topic}/signal/{}",
        peer_id.0
    )
}

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_EVENT_SCHEMA: u8 = 1;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_MAX_MESSAGE_BYTES: usize = 64 * 1024;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_MAX_BOOTSTRAP_ENDPOINTS: usize = 16;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_COMMAND_QUEUE_DEPTH: usize = 128;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_HISTORY_LENGTH: usize = 8;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_HISTORY_GOSSIP: usize = 3;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_MESH_N_LOW: usize = 2;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_MESH_N: usize = 4;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_MESH_N_HIGH: usize = 8;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_DUPLICATE_CACHE_SECS: u64 = 60;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_DUPLICATE_STORM_THRESHOLD: usize = 32;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_KAD_QUERY_TIMEOUT_SECS: u64 = 20;

#[cfg(feature = "ipfs-pubsub-adapter")]
const IPFS_PUBSUB_BOOTSTRAP_CONNECT_TIMEOUT_SECS: u64 = 5;

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
const DISCRYPT_RENDEZVOUS_HEALTH_SCHEMA_VERSION: u16 = 1;

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
const DISCRYPT_RENDEZVOUS_HEALTH_PROTOCOL_VERSION: &str = "discrypt-signaling-http-v1";

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
const DISCRYPT_RENDEZVOUS_MIN_MAX_BODY_BYTES: usize = 4096;

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
const DISCRYPT_RENDEZVOUS_MAX_MAX_BODY_BYTES: usize = 1024 * 1024;

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
const DISCRYPT_RENDEZVOUS_ACCEPTED_ALPN: &[&str] = &["h2", "http/1.1"];

/// Versioned public bootstrap policy for explicit IPFS/libp2p adapter profiles.
///
/// DNS bootstrap is intentionally disabled while the libp2p DNS stack is
/// audit-blocked. IPFS profiles must provide explicit `/ip4` or `/ip6`
/// multiaddrs for reachable Discrypt topic peers.
#[cfg(feature = "ipfs-pubsub-adapter")]
pub const IPFS_PUBSUB_BOOTSTRAP_POLICY_VERSION: u8 = 1;

#[cfg(feature = "ipfs-pubsub-adapter")]
pub const IPFS_PUBSUB_PUBLIC_BOOTSTRAP_MULTIADDRS: &[&str] = &[];

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_err(context: &str, err: impl std::fmt::Display) -> TransportError {
    TransportError::SignalingAdapter(format!("ipfs_pubsub {context} failed: {err}"))
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_typed_error(
    context: &str,
    readiness: AdapterReadinessState,
    details: impl std::fmt::Display,
) -> TransportError {
    TransportError::SignalingAdapter(format!(
        "ipfs_pubsub {context} failed: failure_class={} health_state={:?} details={details}",
        readiness.failure_class(),
        readiness.to_health_state()
    ))
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_topic(rendezvous: &RendezvousCapability) -> libp2p::gossipsub::IdentTopic {
    libp2p::gossipsub::IdentTopic::new(format!("discrypt/v1/rendezvous/{}", rendezvous.topic))
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_provider_key(topic: &libp2p::gossipsub::IdentTopic) -> libp2p::kad::RecordKey {
    libp2p::kad::RecordKey::new(&format!("discrypt-pubsub-provider-v1/{}", topic.hash()))
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_multiaddr_from_endpoint(
    endpoint: &SignalingProviderEndpoint,
) -> Result<libp2p::Multiaddr, TransportError> {
    let value = endpoint.endpoint.0.as_str();
    let multiaddr = value.strip_prefix("libp2p://").unwrap_or(value);
    multiaddr
        .parse::<libp2p::Multiaddr>()
        .map_err(|err| ipfs_err("multiaddr parse", err))
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_pubsub_gossipsub_config() -> Result<libp2p::gossipsub::Config, TransportError> {
    let mut config = libp2p::gossipsub::ConfigBuilder::default();
    config
        .heartbeat_initial_delay(Duration::from_millis(200))
        .heartbeat_interval(Duration::from_millis(300))
        .history_length(IPFS_PUBSUB_HISTORY_LENGTH)
        .history_gossip(IPFS_PUBSUB_HISTORY_GOSSIP)
        .mesh_n_low(IPFS_PUBSUB_MESH_N_LOW)
        .mesh_n(IPFS_PUBSUB_MESH_N)
        .mesh_n_high(IPFS_PUBSUB_MESH_N_HIGH)
        .max_transmit_size(IPFS_PUBSUB_MAX_MESSAGE_BYTES)
        .duplicate_cache_time(Duration::from_secs(IPFS_PUBSUB_DUPLICATE_CACHE_SECS))
        .validation_mode(libp2p::gossipsub::ValidationMode::Strict)
        .flood_publish(false);
    config
        .build()
        .map_err(|err| ipfs_err("gossipsub config", err))
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_validate_bootstrap_policy(addrs: &[libp2p::Multiaddr]) -> Result<(), TransportError> {
    if addrs.len() > IPFS_PUBSUB_MAX_BOOTSTRAP_ENDPOINTS {
        return Err(TransportError::InvalidConnectivityPolicy(format!(
            "ipfs_pubsub bootstrap endpoint count exceeds resource policy limit: max={}",
            IPFS_PUBSUB_MAX_BOOTSTRAP_ENDPOINTS
        )));
    }
    let mut seen = BTreeSet::new();
    for addr in addrs {
        let text = addr.to_string();
        if !seen.insert(text.clone()) {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "ipfs_pubsub duplicate bootstrap endpoint rejected by resource policy: {text}"
            )));
        }
        if ipfs_contains_dns_component(addr) {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "ipfs_pubsub DNS bootstrap endpoint rejected while DNS discovery remains audit-blocked: {text}"
            )));
        }
        if ipfs_should_dial(addr) {
            if !ipfs_has_direct_ip_component(addr) {
                return Err(TransportError::InvalidConnectivityPolicy(format!(
                    "ipfs_pubsub dialable bootstrap endpoint must use explicit /ip4 or /ip6 addressing: {text}"
                )));
            }
            if ipfs_peer_id_from_multiaddr(addr).is_none() {
                return Err(TransportError::InvalidConnectivityPolicy(format!(
                    "ipfs_pubsub dialable bootstrap endpoint must include /p2p/<peer-id> for a reachable Discrypt topic peer: {text}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(feature = "ipfs-pubsub-adapter")]
pub fn ipfs_pubsub_default_bootstrap_addrs() -> Result<Vec<libp2p::Multiaddr>, TransportError> {
    IPFS_PUBSUB_PUBLIC_BOOTSTRAP_MULTIADDRS
        .iter()
        .map(|endpoint| {
            endpoint
                .parse::<libp2p::Multiaddr>()
                .map_err(|err| ipfs_err("default bootstrap multiaddr parse", err))
        })
        .collect()
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_should_dial(addr: &libp2p::Multiaddr) -> bool {
    let text = addr.to_string();
    !(text.contains("/tcp/0") || text.contains("/udp/0"))
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_contains_dns_component(addr: &libp2p::Multiaddr) -> bool {
    addr.iter().any(|protocol| {
        matches!(
            protocol,
            libp2p::multiaddr::Protocol::Dns(_)
                | libp2p::multiaddr::Protocol::Dns4(_)
                | libp2p::multiaddr::Protocol::Dns6(_)
                | libp2p::multiaddr::Protocol::Dnsaddr(_)
        )
    })
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_has_direct_ip_component(addr: &libp2p::Multiaddr) -> bool {
    addr.iter().any(|protocol| {
        matches!(
            protocol,
            libp2p::multiaddr::Protocol::Ip4(_) | libp2p::multiaddr::Protocol::Ip6(_)
        )
    })
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_peer_id_from_multiaddr(addr: &libp2p::Multiaddr) -> Option<libp2p::PeerId> {
    addr.iter().find_map(|protocol| match protocol {
        libp2p::multiaddr::Protocol::P2p(peer_id) => Some(peer_id),
        _ => None,
    })
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_pubsub_no_topic_mesh_error(
    error: libp2p::gossipsub::PublishError,
    connected_peers: usize,
    subscribed_peers: usize,
    mesh_peers: usize,
) -> TransportError {
    if matches!(
        error,
        libp2p::gossipsub::PublishError::NoPeersSubscribedToTopic
    ) {
        ipfs_typed_error(
            "topic_mesh_unavailable",
            AdapterReadinessState::ProviderUnhealthy,
            format!(
                "publish has insufficient topic peers (connected_peers={connected_peers}, \
                 subscribed_topic_peers={subscribed_peers}, mesh_peers={mesh_peers}); generic \
                 libp2p bootstrap peers do not relay arbitrary gossipsub topics. Configure at \
                 least one reachable Discrypt/IPFS pubsub peer for this rendezvous topic, or a \
                 public DHT/rendezvous path where participants can advertise and discover the \
                 same topic provider record before publishing"
            ),
        )
    } else {
        ipfs_err("publish", error)
    }
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_message_fingerprint(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(bytes);
    hex::encode(digest)
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_duplicate_storm_error(fingerprint: &str, duplicate_count: usize) -> TransportError {
    ipfs_typed_error(
        "duplicate_storm",
        AdapterReadinessState::ProviderUnhealthy,
        format!(
            "duplicate envelope fingerprint exceeded resource policy threshold: \
             fingerprint={fingerprint} duplicates={duplicate_count} \
             threshold={IPFS_PUBSUB_DUPLICATE_STORM_THRESHOLD}"
        ),
    )
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_swarm_provider_unhealthy_error(
    context: &str,
    details: impl std::fmt::Display,
) -> TransportError {
    ipfs_typed_error(
        context,
        AdapterReadinessState::ProviderUnhealthy,
        format!("libp2p swarm reported provider/runtime failure: {details}"),
    )
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn ipfs_encode_envelope(envelope: &IpfsPubsubWireEnvelope) -> Result<Vec<u8>, TransportError> {
    let bytes =
        serde_json::to_vec(envelope).map_err(|err| ipfs_err("wire envelope encode", err))?;
    if bytes.len() > IPFS_PUBSUB_MAX_MESSAGE_BYTES {
        return Err(ipfs_typed_error(
            "envelope_size",
            AdapterReadinessState::ProviderMessageTooLarge,
            format!(
                "envelope exceeds max message size: bytes={} max={}",
                bytes.len(),
                IPFS_PUBSUB_MAX_MESSAGE_BYTES
            ),
        ));
    }
    reject_forbidden_plaintext(&bytes)?;
    Ok(bytes)
}

#[cfg(feature = "ipfs-pubsub-adapter")]
async fn ipfs_command(
    commands: &tokio::sync::mpsc::Sender<IpfsPubsubCommand>,
    payload: Vec<u8>,
) -> Result<(), TransportError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    commands
        .send(IpfsPubsubCommand::Publish {
            payload,
            result: tx,
        })
        .await
        .map_err(|_| {
            TransportError::SignalingAdapter("ipfs_pubsub swarm task stopped".to_owned())
        })?;
    rx.await.map_err(|_| {
        TransportError::SignalingAdapter("ipfs_pubsub publish result dropped".to_owned())
    })?
}

#[cfg(feature = "ipfs-pubsub-adapter")]
impl IpfsPubsubProviderRoom {
    /// Return local libp2p listen multiaddrs for integration tests and diagnostics.
    #[must_use]
    pub fn listen_addresses_for_tests(&self) -> Vec<String> {
        self.listen_addresses
            .try_lock()
            .map(|addresses| addresses.clone())
            .unwrap_or_default()
    }

    /// Return direct peer multiaddrs suitable for explicit topic-peer bootstrap profiles.
    ///
    /// Production/self-hosted IPFS profiles require an explicit `/p2p/<peer-id>`
    /// suffix so peers dial a Discrypt topic peer rather than an arbitrary generic
    /// IPFS bootstrap node. This helper exposes that exact shape for diagnostics
    /// and deterministic E2E tests.
    #[must_use]
    pub fn direct_topic_peer_multiaddrs_for_tests(&self) -> Vec<String> {
        self.listen_addresses
            .try_lock()
            .map(|addresses| {
                addresses
                    .iter()
                    .filter(|address| !address.contains("/p2p/"))
                    .map(|address| format!("{address}/p2p/{}", self.local_libp2p_peer_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn record_message(
        inbox: &Arc<AsyncMutex<IpfsPubsubInbox>>,
        local_peer_id: &SignalingPeerId,
        bytes: Vec<u8>,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&bytes)?;
        let fingerprint = ipfs_message_fingerprint(&bytes);
        let envelope: IpfsPubsubWireEnvelope =
            serde_json::from_slice(&bytes).map_err(|err| ipfs_err("wire envelope decode", err))?;
        let mut inbox = inbox.lock().await;
        if !inbox.seen_messages.insert(fingerprint.clone()) {
            let duplicate_count = {
                let count = inbox
                    .duplicate_counts
                    .entry(fingerprint.clone())
                    .or_insert(0);
                *count += 1;
                *count
            };
            if duplicate_count == IPFS_PUBSUB_DUPLICATE_STORM_THRESHOLD {
                let error = ipfs_duplicate_storm_error(&fingerprint, duplicate_count);
                inbox.health_faults.push(error.to_string());
                return Err(error);
            }
            return Ok(());
        }
        match envelope {
            IpfsPubsubWireEnvelope::Presence {
                schema,
                from_peer,
                payload,
                ttl_seconds,
            } if schema == IPFS_PUBSUB_EVENT_SCHEMA && from_peer != *local_peer_id => {
                inbox.presence.push(PresenceEvent {
                    peer_id: from_peer,
                    encrypted_presence: payload,
                    ttl_seconds,
                });
            }
            IpfsPubsubWireEnvelope::Signal {
                schema,
                from_peer,
                to_peer,
                payload,
            } if schema == IPFS_PUBSUB_EVENT_SCHEMA && to_peer == *local_peer_id => {
                inbox.signals.push(PeerSignal {
                    from_peer,
                    to_peer,
                    payload,
                });
            }
            IpfsPubsubWireEnvelope::Control {
                schema,
                from_peer,
                payload,
            } if schema == IPFS_PUBSUB_EVENT_SCHEMA && from_peer != *local_peer_id => {
                inbox.controls.push(ControlBroadcast { from_peer, payload });
            }
            _ => {}
        }
        Ok(())
    }

    async fn take_health_fault(&self) -> Result<(), TransportError> {
        let mut inbox = self.inbox.lock().await;
        if let Some(message) = inbox.health_faults.pop() {
            return Err(TransportError::SignalingAdapter(message));
        }
        Ok(())
    }

    async fn record_health_fault(inbox: &Arc<AsyncMutex<IpfsPubsubInbox>>, error: TransportError) {
        let mut inbox = inbox.lock().await;
        inbox.health_faults.push(error.to_string());
    }
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn discrypt_rendezvous_endpoint_base(
    profile: &SignalingAdapterProfile,
) -> Result<String, TransportError> {
    let endpoint = profile
        .endpoints
        .first()
        .ok_or_else(|| {
            TransportError::InvalidConnectivityPolicy(
                "discrypt rendezvous profile requires one endpoint".to_owned(),
            )
        })?
        .endpoint
        .0
        .trim_end_matches('/')
        .to_owned();
    if endpoint.starts_with("quic://") {
        return Err(TransportError::SignalingAdapter(
            "discrypt_quic_rendezvous native quic:// transport remains reserved; use the sibling service HTTPS/WSS API endpoint until the native QUIC client is audited"
                .to_owned(),
        ));
    }
    let base = if let Some(rest) = endpoint.strip_prefix("wss://") {
        format!("https://{rest}")
    } else {
        endpoint
    };
    Ok(base)
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn discrypt_rendezvous_trust_fingerprint_for_endpoint(endpoint: &str) -> String {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"external-signaling-endpoint-fingerprint-v1");
    hasher.update(endpoint.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn discrypt_rendezvous_validate_endpoint_trust(
    endpoint: &SignalingProviderEndpoint,
) -> Result<(), TransportError> {
    let expected = discrypt_rendezvous_trust_fingerprint_for_endpoint(endpoint.endpoint.0.as_str());
    match (&endpoint.trust_fingerprint, endpoint.security) {
        (Some(actual), _) if actual.eq_ignore_ascii_case(&expected) => Ok(()),
        (Some(_), _) => Err(TransportError::SignalingAdapter(
            "discrypt rendezvous endpoint trust fingerprint mismatch".to_owned(),
        )),
        (None, SignalingEndpointSecurity::LocalDevLoopback) => Ok(()),
        (None, _) => Err(TransportError::SignalingAdapter(
            "discrypt rendezvous production/self-hosted endpoint requires signed endpoint trust fingerprint"
                .to_owned(),
        )),
    }
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn discrypt_rendezvous_validate_profile_trust(
    profile: &SignalingAdapterProfile,
) -> Result<(), TransportError> {
    let endpoint = profile.endpoints.first().ok_or_else(|| {
        TransportError::InvalidConnectivityPolicy(
            "discrypt rendezvous profile requires one endpoint".to_owned(),
        )
    })?;
    discrypt_rendezvous_validate_endpoint_trust(endpoint)
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn discrypt_rendezvous_is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.as_bytes().iter().all(u8::is_ascii_hexdigit)
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn discrypt_rendezvous_validate_health(
    endpoint_base: &str,
    endpoint_security: SignalingEndpointSecurity,
    endpoint_trust_fingerprint: Option<&str>,
    health: &DiscryptRendezvousHealthResponse,
) -> Result<(), TransportError> {
    if health.status != "ok" {
        return Err(TransportError::SignalingAdapter(format!(
            "discrypt rendezvous service health is not ok: {}",
            health.status
        )));
    }
    if health.service.trim().is_empty() || health.service.trim() != health.service {
        return Err(TransportError::SignalingAdapter(
            "discrypt rendezvous service health label is invalid".to_owned(),
        ));
    }
    if health.public_base_url.trim().is_empty()
        || health.public_base_url.trim() != health.public_base_url
    {
        return Err(TransportError::SignalingAdapter(
            "discrypt rendezvous service public_base_url is invalid".to_owned(),
        ));
    }
    let advertised = health.public_base_url.trim_end_matches('/');
    if endpoint_security != SignalingEndpointSecurity::LocalDevLoopback
        && advertised != endpoint_base
    {
        return Err(TransportError::SignalingAdapter(format!(
            "discrypt rendezvous service public_base_url mismatch: expected {endpoint_base}, got {advertised}"
        )));
    }
    if endpoint_security != SignalingEndpointSecurity::LocalDevLoopback {
        if health.schema_version != Some(DISCRYPT_RENDEZVOUS_HEALTH_SCHEMA_VERSION) {
            return Err(TransportError::SignalingAdapter(
                "discrypt rendezvous service health schema_version is unsupported".to_owned(),
            ));
        }
        if health.protocol_version.as_deref() != Some(DISCRYPT_RENDEZVOUS_HEALTH_PROTOCOL_VERSION) {
            return Err(TransportError::SignalingAdapter(
                "discrypt rendezvous service protocol_version is unsupported".to_owned(),
            ));
        }
        let Some(max_body_bytes) = health.max_body_bytes else {
            return Err(TransportError::SignalingAdapter(
                "discrypt rendezvous service health is missing max_body_bytes".to_owned(),
            ));
        };
        if !(DISCRYPT_RENDEZVOUS_MIN_MAX_BODY_BYTES..=DISCRYPT_RENDEZVOUS_MAX_MAX_BODY_BYTES)
            .contains(&max_body_bytes)
        {
            return Err(TransportError::SignalingAdapter(format!(
                "discrypt rendezvous service max_body_bytes is outside policy bounds: {max_body_bytes}"
            )));
        }
        if health.rate_limit_window_seconds.unwrap_or(0) == 0 {
            return Err(TransportError::SignalingAdapter(
                "discrypt rendezvous service health is missing rate_limit_window_seconds"
                    .to_owned(),
            ));
        }
        if health.rate_limit_max_requests.unwrap_or(0) == 0 {
            return Err(TransportError::SignalingAdapter(
                "discrypt rendezvous service health is missing rate_limit_max_requests".to_owned(),
            ));
        }
        let expected_identity = endpoint_trust_fingerprint.ok_or_else(|| {
            TransportError::SignalingAdapter(
                "discrypt rendezvous production endpoint is missing signed identity fingerprint"
                    .to_owned(),
            )
        })?;
        let service_identity = health
            .service_identity_fingerprint
            .as_deref()
            .ok_or_else(|| {
                TransportError::SignalingAdapter(
                    "discrypt rendezvous service health is missing service_identity_fingerprint"
                        .to_owned(),
                )
            })?;
        if !discrypt_rendezvous_is_sha256_hex(service_identity)
            || !service_identity.eq_ignore_ascii_case(expected_identity)
        {
            return Err(TransportError::SignalingAdapter(
                "discrypt rendezvous service identity fingerprint does not match signed endpoint trust".to_owned(),
            ));
        }
        if !health.tls_alpn_protocols.iter().any(|protocol| {
            DISCRYPT_RENDEZVOUS_ACCEPTED_ALPN
                .iter()
                .any(|accepted| protocol == accepted)
        }) {
            return Err(TransportError::SignalingAdapter(
                "discrypt rendezvous service health is missing accepted TLS ALPN evidence"
                    .to_owned(),
            ));
        }
        let expires_at = health.service_expires_at.ok_or_else(|| {
            TransportError::SignalingAdapter(
                "discrypt rendezvous service health is missing service_expires_at".to_owned(),
            )
        })?;
        if expires_at <= chrono::Utc::now() {
            return Err(TransportError::SignalingAdapter(
                "discrypt rendezvous service identity/allowlist proof is expired".to_owned(),
            ));
        }
        let rotation_policy = health.rotation_policy.as_deref().unwrap_or_default();
        if !(rotation_policy.contains("rotate") && rotation_policy.contains("expires")) {
            return Err(TransportError::SignalingAdapter(
                "discrypt rendezvous service health is missing endpoint rotation policy".to_owned(),
            ));
        }
        let Some(allowlist_commitment) = health.endpoint_allowlist_commitment.as_deref() else {
            return Err(TransportError::SignalingAdapter(
                "discrypt rendezvous service health is missing endpoint allowlist commitment"
                    .to_owned(),
            ));
        };
        if !allowlist_commitment.eq_ignore_ascii_case(expected_identity) {
            return Err(TransportError::SignalingAdapter(
                "discrypt rendezvous service endpoint allowlist proof does not match signed endpoint trust".to_owned(),
            ));
        }
    }
    let _ = health.at_rest_records;
    Ok(())
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn discrypt_rendezvous_key(topic: &str, label: &str, peer: Option<&SignalingPeerId>) -> Vec<u8> {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"discrypt-rendezvous-service-key-v1");
    hasher.update(topic.as_bytes());
    hasher.update(label.as_bytes());
    if let Some(peer) = peer {
        hasher.update(peer.0.as_bytes());
    }
    hasher.finalize().to_vec()
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn discrypt_rendezvous_client_token(topic: &str, local_peer_id: &SignalingPeerId) -> Vec<u8> {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"discrypt-rendezvous-service-client-token-v1");
    hasher.update(topic.as_bytes());
    hasher.update(local_peer_id.0.as_bytes());
    hasher.finalize().to_vec()
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn discrypt_rendezvous_signal_kind(
    kind: &WebRtcNegotiationPayloadKind,
) -> DiscryptRendezvousSignalKind {
    match kind {
        WebRtcNegotiationPayloadKind::Offer => DiscryptRendezvousSignalKind::Offer,
        WebRtcNegotiationPayloadKind::Answer => DiscryptRendezvousSignalKind::Answer,
        WebRtcNegotiationPayloadKind::Candidate => DiscryptRendezvousSignalKind::Candidate,
    }
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn discrypt_rendezvous_post_json<T, R>(
    endpoint_base: String,
    path: &'static str,
    body: T,
) -> Result<Option<R>, TransportError>
where
    T: Serialize + Send + 'static,
    R: for<'de> Deserialize<'de> + Send + 'static,
{
    let url = format!("{endpoint_base}{path}");
    let response = ureq::post(&url).send_json(body);
    match response {
        Ok(mut response) => response
            .body_mut()
            .read_json::<R>()
            .map(Some)
            .map_err(|error| {
                TransportError::SignalingAdapter(format!(
                    "discrypt rendezvous service response decode failed: {error}"
                ))
            }),
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(error) => Err(TransportError::SignalingAdapter(format!(
            "discrypt rendezvous service request failed: {error}"
        ))),
    }
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
impl DiscryptQuicRendezvousProviderRoom {
    fn next_nonce_hex(&self) -> String {
        use sha2::Digest as _;
        let counter = self
            .nonce_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"discrypt-rendezvous-service-nonce-v1");
        hasher.update(self.topic.as_bytes());
        hasher.update(self.local_peer_id.0.as_bytes());
        hasher.update(counter.to_be_bytes());
        hex::encode(hasher.finalize())
    }

    fn client_token_hex(&self) -> String {
        hex::encode(discrypt_rendezvous_client_token(
            &self.topic,
            &self.local_peer_id,
        ))
    }

    async fn publish_envelope(
        &self,
        kind: DiscryptRendezvousSignalKind,
        key: Vec<u8>,
        envelope: DiscryptRendezvousWireEnvelope,
        ttl_seconds: u32,
    ) -> Result<(), TransportError> {
        let payload = serde_json::to_vec(&envelope).map_err(|err| {
            TransportError::SignalingAdapter(format!(
                "discrypt rendezvous envelope encode failed: {err}"
            ))
        })?;
        reject_forbidden_plaintext(&payload)?;
        let request = DiscryptRendezvousPublishSignalRequest {
            client_token_hex: self.client_token_hex(),
            nonce_hex: self.next_nonce_hex(),
            kind,
            key_hex: hex::encode(key),
            payload_hex: hex::encode(payload),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(i64::from(ttl_seconds)),
        };
        let endpoint_base = self.endpoint_base.clone();
        tokio::task::spawn_blocking(move || {
            discrypt_rendezvous_post_json::<_, serde_json::Value>(
                endpoint_base,
                "/v1/signals/publish",
                request,
            )
            .map(|_| ())
        })
        .await
        .map_err(|error| {
            TransportError::SignalingAdapter(format!(
                "discrypt rendezvous publish task failed: {error}"
            ))
        })?
    }

    async fn take_envelopes(
        &self,
        kind: DiscryptRendezvousSignalKind,
        key: Vec<u8>,
    ) -> Result<Vec<DiscryptRendezvousWireEnvelope>, TransportError> {
        let request = DiscryptRendezvousTakeSignalRequest {
            client_token_hex: self.client_token_hex(),
            nonce_hex: self.next_nonce_hex(),
            kind,
            key_hex: hex::encode(key),
        };
        let endpoint_base = self.endpoint_base.clone();
        let response = tokio::task::spawn_blocking(move || {
            discrypt_rendezvous_post_json::<_, DiscryptRendezvousTakeSignalsResponse>(
                endpoint_base,
                "/v1/signals/take",
                request,
            )
        })
        .await
        .map_err(|error| {
            TransportError::SignalingAdapter(format!(
                "discrypt rendezvous take task failed: {error}"
            ))
        })??;
        let Some(response) = response else {
            return Ok(Vec::new());
        };
        response
            .signals
            .into_iter()
            .filter(|signal| signal.kind == kind && signal.expires_at >= chrono::Utc::now())
            .map(|signal| {
                let bytes = hex::decode(&signal.payload_hex).map_err(|error| {
                    TransportError::SignalingAdapter(format!(
                        "discrypt rendezvous payload hex decode failed: {error}"
                    ))
                })?;
                reject_forbidden_plaintext(&bytes)?;
                serde_json::from_slice::<DiscryptRendezvousWireEnvelope>(&bytes).map_err(|error| {
                    TransportError::SignalingAdapter(format!(
                        "discrypt rendezvous envelope decode failed: {error}"
                    ))
                })
            })
            .collect()
    }
}

#[cfg(feature = "ipfs-pubsub-adapter")]
async fn spawn_ipfs_pubsub_room(
    profile: SignalingAdapterProfile,
    rendezvous: RendezvousCapability,
    local_peer_id: SignalingPeerId,
) -> Result<IpfsPubsubProviderRoom, TransportError> {
    let topic = ipfs_topic(&rendezvous);
    let provider_key = ipfs_provider_key(&topic);
    let gossipsub_config = ipfs_pubsub_gossipsub_config()?;
    let bootstrap_addrs = profile
        .endpoints
        .iter()
        .map(ipfs_multiaddr_from_endpoint)
        .collect::<Result<Vec<_>, _>>()?;
    ipfs_validate_bootstrap_policy(&bootstrap_addrs)?;
    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .map_err(|err| ipfs_err("tcp transport", err))?
        .with_behaviour(|keypair| {
            let local_peer_id = libp2p::PeerId::from(keypair.public());
            let gossipsub = libp2p::gossipsub::Behaviour::<
                libp2p::gossipsub::IdentityTransform,
                libp2p::gossipsub::AllowAllSubscriptionFilter,
            >::new(
                libp2p::gossipsub::MessageAuthenticity::Signed(keypair.clone()),
                gossipsub_config,
            )
            .unwrap_or_else(|error| {
                panic!("validated ipfs_pubsub gossipsub config failed: {error}")
            });
            let mut kad_config = libp2p::kad::Config::new(libp2p::kad::PROTOCOL_NAME);
            kad_config
                .set_query_timeout(Duration::from_secs(IPFS_PUBSUB_KAD_QUERY_TIMEOUT_SECS))
                .set_replication_factor(NonZeroUsize::new(3).expect("non-zero replication"));
            let kademlia = libp2p::kad::Behaviour::with_config(
                local_peer_id,
                libp2p::kad::store::MemoryStore::new(local_peer_id),
                kad_config,
            );
            let identify = libp2p::identify::Behaviour::new(
                libp2p::identify::Config::new(
                    "/discrypt/ipfs-pubsub/1.0.0".to_owned(),
                    keypair.public(),
                )
                .with_agent_version("discrypt-ipfs-pubsub/0.1.0".to_owned())
                .with_push_listen_addr_updates(true),
            );
            IpfsPubsubBehaviour {
                gossipsub,
                kademlia,
                identify,
            }
        })
        .map_err(|error| ipfs_err("swarm behaviour", error))?
        .build();
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topic)
        .map_err(|err| ipfs_err("subscribe topic", err))?;
    swarm
        .listen_on(
            "/ip4/127.0.0.1/tcp/0"
                .parse::<libp2p::Multiaddr>()
                .map_err(|err| ipfs_err("listen multiaddr parse", err))?,
        )
        .map_err(|err| ipfs_err("listen", err))?;
    for addr in &bootstrap_addrs {
        if let Some(peer_id) = ipfs_peer_id_from_multiaddr(addr) {
            swarm
                .behaviour_mut()
                .kademlia
                .add_address(&peer_id, addr.clone());
        }
        if ipfs_should_dial(&addr) {
            let _ = swarm.dial(addr.clone());
        }
    }
    let _ = swarm.behaviour_mut().kademlia.bootstrap();
    let _ = swarm
        .behaviour_mut()
        .kademlia
        .start_providing(provider_key.clone());
    let _ = swarm
        .behaviour_mut()
        .kademlia
        .get_providers(provider_key.clone());

    let local_libp2p_peer_id = *swarm.local_peer_id();
    let (command_tx, mut command_rx) = tokio::sync::mpsc::channel(IPFS_PUBSUB_COMMAND_QUEUE_DEPTH);
    let (listen_tx, listen_rx) = tokio::sync::oneshot::channel();
    let inbox = Arc::new(AsyncMutex::new(IpfsPubsubInbox::default()));
    let task_inbox = inbox.clone();
    let listen_addresses = Arc::new(AsyncMutex::new(Vec::<String>::new()));
    let task_listen_addresses = listen_addresses.clone();
    let connected_peers = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let failed_bootstrap_dials = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let task_connected_peers = connected_peers.clone();
    let task_failed_bootstrap_dials = failed_bootstrap_dials.clone();
    let bootstrap_dial_count = bootstrap_addrs
        .iter()
        .filter(|addr| ipfs_should_dial(addr))
        .count();
    let task_local_peer_id = local_peer_id.clone();
    let mut listen_tx = Some(listen_tx);
    let task_topic = topic.clone();
    let task_topic_hash = topic.hash();
    let task_provider_key = provider_key.clone();

    tokio::spawn(async move {
        use futures::StreamExt;
        loop {
            tokio::select! {
                command = command_rx.recv() => {
                    let Some(command) = command else { break; };
                    match command {
                        IpfsPubsubCommand::Publish { payload, result } => {
                            let mesh_peers = swarm
                                .behaviour()
                                .gossipsub
                                .mesh_peers(&task_topic_hash)
                                .count();
                            let subscribed_peers = swarm
                                .behaviour()
                                .gossipsub
                                .all_peers()
                                .filter(|(_, topics)| topics.iter().any(|topic| **topic == task_topic_hash))
                                .count();
                            let connected_peers = swarm.connected_peers().count();
                            let outcome = swarm
                                .behaviour_mut()
                                .gossipsub
                                .publish(task_topic.clone(), payload)
                                .map(|_| ())
                                .map_err(|err| {
                                    ipfs_pubsub_no_topic_mesh_error(
                                        err,
                                        connected_peers,
                                        subscribed_peers,
                                        mesh_peers,
                                    )
                                });
                            let _ = result.send(outcome);
                        }
                        IpfsPubsubCommand::Leave { result } => {
                            let outcome = if swarm
                                .behaviour_mut()
                                .gossipsub
                                .unsubscribe(&task_topic)
                            {
                                Ok(())
                            } else {
                                Err(ipfs_err(
                                    "unsubscribe",
                                    "topic was not subscribed by this adapter session",
                                ))
                            };
                            let _ = result.send(outcome);
                            break;
                        }
                    }
                }
                event = swarm.select_next_some() => {
                    match event {
                        libp2p::swarm::SwarmEvent::NewListenAddr { address, .. } => {
                            let address = address.to_string();
                            task_listen_addresses.lock().await.push(address.clone());
                            if let Some(sender) = listen_tx.take() {
                                let _ = sender.send(address);
                            }
                        }
                        libp2p::swarm::SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            task_connected_peers.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                            let _ = swarm
                                .behaviour_mut()
                                .kademlia
                                .get_providers(task_provider_key.clone());
                        }
                        libp2p::swarm::SwarmEvent::ConnectionClosed { peer_id, .. } => {
                            task_connected_peers.fetch_update(
                                std::sync::atomic::Ordering::Relaxed,
                                std::sync::atomic::Ordering::Relaxed,
                                |count| Some(count.saturating_sub(1)),
                            ).ok();
                            swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                        }
                        libp2p::swarm::SwarmEvent::OutgoingConnectionError { .. } => {
                            task_failed_bootstrap_dials.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        libp2p::swarm::SwarmEvent::ListenerError { error, .. } => {
                            IpfsPubsubProviderRoom::record_health_fault(
                                &task_inbox,
                                ipfs_swarm_provider_unhealthy_error("listener_error", error),
                            )
                            .await;
                        }
                        libp2p::swarm::SwarmEvent::ListenerClosed {
                            reason: Err(error),
                            addresses,
                            ..
                        } => {
                            IpfsPubsubProviderRoom::record_health_fault(
                                &task_inbox,
                                ipfs_swarm_provider_unhealthy_error(
                                    "listener_closed",
                                    format!("addresses={addresses:?} error={error}"),
                                ),
                            )
                            .await;
                        }
                        libp2p::swarm::SwarmEvent::Behaviour(IpfsPubsubBehaviourEvent::Gossipsub(libp2p::gossipsub::Event::Message { message, .. })) => {
                            let _ = IpfsPubsubProviderRoom::record_message(
                                &task_inbox,
                                &task_local_peer_id,
                                message.data,
                            )
                            .await;
                        }
                        libp2p::swarm::SwarmEvent::Behaviour(IpfsPubsubBehaviourEvent::Kademlia(
                            libp2p::kad::Event::OutboundQueryProgressed {
                                result:
                                    libp2p::kad::QueryResult::GetProviders(Ok(
                                        libp2p::kad::GetProvidersOk::FoundProviders { providers, .. },
                                    )),
                                ..
                            },
                        )) => {
                            for peer_id in providers {
                                if peer_id != *swarm.local_peer_id() {
                                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                    let _ = swarm.dial(peer_id);
                                }
                            }
                        }
                        libp2p::swarm::SwarmEvent::Behaviour(IpfsPubsubBehaviourEvent::Identify(
                            libp2p::identify::Event::Received { peer_id, info, .. },
                        )) => {
                            for addr in info.listen_addrs {
                                swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    timeout(Duration::from_secs(5), listen_rx)
        .await
        .map_err(|_| {
            TransportError::SignalingAdapter("ipfs_pubsub listen address timed out".to_owned())
        })?
        .map_err(|_| {
            TransportError::SignalingAdapter("ipfs_pubsub listen address dropped".to_owned())
        })?;

    if bootstrap_dial_count > 0 {
        let deadline =
            Instant::now() + Duration::from_secs(IPFS_PUBSUB_BOOTSTRAP_CONNECT_TIMEOUT_SECS);
        loop {
            if connected_peers.load(std::sync::atomic::Ordering::Relaxed) > 0 {
                break;
            }
            if failed_bootstrap_dials.load(std::sync::atomic::Ordering::Relaxed)
                >= bootstrap_dial_count
            {
                return Err(ipfs_typed_error(
                    "bootstrap_connect",
                    AdapterReadinessState::ProviderUnhealthy,
                    format!(
                        "all configured bootstrap dials failed before a libp2p peer connection was established (bootstrap_dials={bootstrap_dial_count})"
                    ),
                ));
            }
            if Instant::now() >= deadline {
                return Err(ipfs_typed_error(
                    "bootstrap_connect",
                    AdapterReadinessState::ProviderUnhealthy,
                    format!(
                        "timed out waiting for a reachable Discrypt/IPFS pubsub peer (bootstrap_dials={bootstrap_dial_count})"
                    ),
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    Ok(IpfsPubsubProviderRoom {
        local_peer_id,
        local_libp2p_peer_id,
        commands: command_tx,
        inbox,
        listen_addresses,
    })
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[async_trait]
impl SignalingAdapter for IpfsPubsubProviderAdapter {
    type Session = IpfsPubsubProviderSession;

    async fn connect(
        &self,
        profile: SignalingAdapterProfile,
    ) -> Result<Self::Session, TransportError> {
        profile.validate()?;
        if profile.kind != SignalingAdapterKind::IpfsPubsub {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "adapter profile kind {} does not match ipfs_pubsub adapter",
                profile.kind.canonical_name()
            )));
        }
        for endpoint in &profile.endpoints {
            let _ = ipfs_multiaddr_from_endpoint(endpoint)?;
        }
        Ok(IpfsPubsubProviderSession { profile })
    }

    fn capabilities(&self) -> SignalingAdapterCapabilities {
        SignalingAdapterCapabilities::production_required()
    }

    fn observability_redacted(&self) -> SignalingObservability {
        SignalingObservability {
            adapter_kind: SignalingAdapterKind::IpfsPubsub,
            endpoint_label: "ipfs_pubsub#configured_profile".to_owned(),
            health: SignalingHealthState::Healthy,
            trust_label: AdapterTrustLabel {
                label: "ipfs_pubsub".to_owned(),
                posture: "real rust-libp2p gossipsub client; peers see topic hashes and opaque Discrypt envelopes".to_owned(),
            },
        }
    }
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[async_trait]
impl AdapterSession for IpfsPubsubProviderSession {
    type Room = IpfsPubsubProviderRoom;

    async fn join(
        &self,
        scope: ConversationScope,
        rendezvous: RendezvousCapability,
        local_peer_id: SignalingPeerId,
    ) -> Result<Self::Room, TransportError> {
        scope.validate()?;
        if rendezvous.scope != scope {
            return Err(TransportError::SignalingAdapter(
                "rendezvous capability scope mismatch".to_owned(),
            ));
        }
        if rendezvous.adapter_kind != SignalingAdapterKind::IpfsPubsub {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "rendezvous capability kind {} does not match ipfs_pubsub adapter",
                rendezvous.adapter_kind.canonical_name()
            )));
        }
        spawn_ipfs_pubsub_room(self.profile.clone(), rendezvous, local_peer_id).await
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn health(&self) -> SignalingHealth {
        SignalingHealth {
            adapter_kind: SignalingAdapterKind::IpfsPubsub,
            state: SignalingHealthState::Healthy,
            latency_bucket: "unknown".to_owned(),
            failure_class: None,
        }
    }
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[async_trait]
impl RendezvousRoom for IpfsPubsubProviderRoom {
    async fn publish_presence(
        &self,
        encrypted_presence: OpaqueSignalingPayload,
        ttl_seconds: u32,
    ) -> Result<(), TransportError> {
        if ttl_seconds == 0 {
            return Err(TransportError::SignalingAdapter(
                "presence ttl must be non-zero".to_owned(),
            ));
        }
        reject_forbidden_plaintext(&encrypted_presence.bytes)?;
        let payload = ipfs_encode_envelope(&IpfsPubsubWireEnvelope::Presence {
            schema: IPFS_PUBSUB_EVENT_SCHEMA,
            from_peer: self.local_peer_id.clone(),
            payload: encrypted_presence,
            ttl_seconds,
        })?;
        ipfs_command(&self.commands, payload).await
    }

    async fn subscribe_presence(&self) -> Result<Vec<PresenceEvent>, TransportError> {
        tokio::time::sleep(Duration::from_millis(500)).await;
        self.take_health_fault().await?;
        let mut inbox = self.inbox.lock().await;
        Ok(std::mem::take(&mut inbox.presence))
    }

    async fn send_signal(
        &self,
        to_peer: SignalingPeerId,
        payload: SealedWebRtcNegotiationPayload,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&payload.ciphertext)?;
        let payload = ipfs_encode_envelope(&IpfsPubsubWireEnvelope::Signal {
            schema: IPFS_PUBSUB_EVENT_SCHEMA,
            from_peer: self.local_peer_id.clone(),
            to_peer,
            payload,
        })?;
        ipfs_command(&self.commands, payload).await
    }

    async fn take_signals(&self) -> Result<Vec<PeerSignal>, TransportError> {
        tokio::time::sleep(Duration::from_millis(500)).await;
        self.take_health_fault().await?;
        let mut inbox = self.inbox.lock().await;
        Ok(std::mem::take(&mut inbox.signals))
    }

    async fn broadcast_control(
        &self,
        sealed_payload: OpaqueSignalingPayload,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&sealed_payload.bytes)?;
        let payload = ipfs_encode_envelope(&IpfsPubsubWireEnvelope::Control {
            schema: IPFS_PUBSUB_EVENT_SCHEMA,
            from_peer: self.local_peer_id.clone(),
            payload: sealed_payload,
        })?;
        ipfs_command(&self.commands, payload).await
    }

    async fn take_control_payloads(&self) -> Result<Vec<ControlBroadcast>, TransportError> {
        tokio::time::sleep(Duration::from_millis(500)).await;
        self.take_health_fault().await?;
        let mut inbox = self.inbox.lock().await;
        Ok(std::mem::take(&mut inbox.controls))
    }

    async fn leave(&self) -> Result<(), TransportError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.commands
            .send(IpfsPubsubCommand::Leave { result: tx })
            .await
            .map_err(|_| {
                TransportError::SignalingAdapter("ipfs_pubsub swarm task stopped".to_owned())
            })?;
        rx.await.map_err(|_| {
            TransportError::SignalingAdapter("ipfs_pubsub leave result dropped".to_owned())
        })?
    }
}

#[cfg(feature = "nostr-adapter")]
impl NostrProviderRoom {
    async fn publish_envelope(&self, envelope: NostrWireEnvelope) -> Result<(), TransportError> {
        let bytes =
            serde_json::to_vec(&envelope).map_err(|err| nostr_err("wire envelope encode", err))?;
        reject_forbidden_plaintext(&bytes)?;
        let content =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD_NO_PAD, bytes);
        let builder = nostr_sdk::EventBuilder::new(
            nostr_sdk::Kind::Custom(NOSTR_DISCRYPT_EVENT_KIND),
            content,
        )
        .tag(nostr_discrypt_tag(&self.topic)?);
        let output = self
            .client
            .send_event_builder_to(self.relay_urls.iter().map(String::as_str), builder)
            .await
            .map_err(|err| nostr_err("publish", err))?;
        if output.success.is_empty() {
            let failed = format!("{:?}", output.failed);
            let readiness = AdapterReadinessState::classify_provider_failure(&failed);
            return Err(TransportError::SignalingAdapter(format!(
                "nostr publish failed on all relays: failure_class={} health_state={:?} details={failed}",
                readiness.failure_class(),
                readiness.to_health_state()
            )));
        }
        self.drain_network_for(Duration::from_secs(2)).await
    }

    async fn drain_network_for(&self, duration: Duration) -> Result<(), TransportError> {
        let deadline = Instant::now() + duration;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            let notification = {
                let mut notifications = self.notifications.lock().await;
                timeout(remaining, notifications.recv()).await
            };
            match notification {
                Ok(Ok(nostr_sdk::RelayPoolNotification::Event { event, .. })) => {
                    self.record_event(&event).await?;
                }
                Ok(Ok(nostr_sdk::RelayPoolNotification::Message { relay_url, message })) => {
                    if let Some(error) = nostr_relay_message_error(&relay_url, &message) {
                        return Err(error);
                    }
                    if let nostr_sdk::RelayMessage::Event { event, .. } = message {
                        self.record_event(&event).await?;
                    }
                }
                Ok(Ok(nostr_sdk::RelayPoolNotification::Shutdown)) => {
                    return Err(TransportError::SignalingAdapter(
                        "nostr relay pool shut down".to_owned(),
                    ));
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                    return Err(TransportError::SignalingAdapter(
                        "nostr notification stream closed".to_owned(),
                    ));
                }
                Err(_) => continue,
            }
        }
    }

    async fn record_event(&self, event: &nostr_sdk::Event) -> Result<(), TransportError> {
        if event.kind != nostr_sdk::Kind::Custom(NOSTR_DISCRYPT_EVENT_KIND) {
            return Ok(());
        }
        let has_topic = event
            .tags
            .iter()
            .any(|tag| tag.kind().to_string() == "d" && tag.content() == Some(self.topic.as_str()));
        if !has_topic {
            return Ok(());
        }
        reject_forbidden_plaintext(event.content.as_bytes())?;
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD_NO_PAD,
            event.content.as_bytes(),
        )
        .map_err(|err| nostr_err("wire envelope base64 decode", err))?;
        reject_forbidden_plaintext(&bytes)?;
        let envelope: NostrWireEnvelope =
            serde_json::from_slice(&bytes).map_err(|err| nostr_err("wire envelope decode", err))?;
        let mut inbox = self.inbox.lock().await;
        match envelope {
            NostrWireEnvelope::Presence {
                schema,
                from_peer,
                payload,
                ttl_seconds,
            } if schema == NOSTR_EVENT_SCHEMA && from_peer != self.local_peer_id => {
                inbox.presence.push(PresenceEvent {
                    peer_id: from_peer,
                    encrypted_presence: payload,
                    ttl_seconds,
                });
            }
            NostrWireEnvelope::Signal {
                schema,
                from_peer,
                to_peer,
                payload,
            } if schema == NOSTR_EVENT_SCHEMA && to_peer == self.local_peer_id => {
                inbox.signals.push(PeerSignal {
                    from_peer,
                    to_peer,
                    payload,
                });
            }
            NostrWireEnvelope::Control {
                schema,
                from_peer,
                payload,
            } if schema == NOSTR_EVENT_SCHEMA && from_peer != self.local_peer_id => {
                inbox.controls.push(ControlBroadcast { from_peer, payload });
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(feature = "nostr-adapter")]
#[async_trait]
impl SignalingAdapter for NostrProviderAdapter {
    type Session = NostrProviderSession;

    async fn connect(
        &self,
        profile: SignalingAdapterProfile,
    ) -> Result<Self::Session, TransportError> {
        profile.validate()?;
        if profile.kind != SignalingAdapterKind::Nostr {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "adapter profile kind {} does not match nostr adapter",
                profile.kind.canonical_name()
            )));
        }
        Ok(NostrProviderSession { profile })
    }

    fn capabilities(&self) -> SignalingAdapterCapabilities {
        SignalingAdapterCapabilities::production_required()
    }

    fn observability_redacted(&self) -> SignalingObservability {
        SignalingObservability {
            adapter_kind: SignalingAdapterKind::Nostr,
            endpoint_label: "nostr#configured_profile".to_owned(),
            health: SignalingHealthState::Healthy,
            trust_label: AdapterTrustLabel {
                label: "nostr".to_owned(),
                posture: "real Nostr relay client; relay sees signed event metadata and opaque Discrypt envelopes".to_owned(),
            },
        }
    }
}

#[cfg(feature = "nostr-adapter")]
#[async_trait]
impl AdapterSession for NostrProviderSession {
    type Room = NostrProviderRoom;

    async fn join(
        &self,
        scope: ConversationScope,
        rendezvous: RendezvousCapability,
        local_peer_id: SignalingPeerId,
    ) -> Result<Self::Room, TransportError> {
        scope.validate()?;
        if rendezvous.scope != scope {
            return Err(TransportError::SignalingAdapter(
                "rendezvous capability scope mismatch".to_owned(),
            ));
        }
        if rendezvous.adapter_kind != SignalingAdapterKind::Nostr {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "rendezvous capability kind {} does not match nostr adapter",
                rendezvous.adapter_kind.canonical_name()
            )));
        }
        let relay_urls = nostr_endpoints_for_profile(&self.profile)?;
        let keys = nostr_client_secret(&rendezvous.topic, &local_peer_id)?;
        let client = nostr_sdk::Client::new(keys);
        for relay_url in &relay_urls {
            client
                .add_relay(relay_url.as_str())
                .await
                .map_err(|err| nostr_err("add relay", err))?;
        }
        client.connect().await;
        let subscription_id = nostr_subscription_id(&rendezvous.topic, &local_peer_id);
        let output = client
            .subscribe_with_id_to(
                relay_urls.iter().map(String::as_str),
                subscription_id.clone(),
                nostr_filter(&rendezvous.topic),
                None,
            )
            .await
            .map_err(|err| nostr_err("subscribe", err))?;
        if output.success.is_empty() && !output.failed.is_empty() {
            let failed = format!("{:?}", output.failed);
            let readiness = AdapterReadinessState::classify_provider_failure(&failed);
            return Err(TransportError::SignalingAdapter(format!(
                "nostr subscribe failed on all relays: failure_class={} health_state={:?} details={failed}",
                readiness.failure_class(),
                readiness.to_health_state()
            )));
        }
        let room = NostrProviderRoom {
            local_peer_id,
            client: client.clone(),
            relay_urls,
            subscription_id,
            topic: rendezvous.topic,
            notifications: AsyncMutex::new(client.notifications()),
            inbox: AsyncMutex::new(NostrInbox::default()),
        };
        room.drain_network_for(Duration::from_millis(700)).await?;
        Ok(room)
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn health(&self) -> SignalingHealth {
        SignalingHealth {
            adapter_kind: SignalingAdapterKind::Nostr,
            state: SignalingHealthState::Healthy,
            latency_bucket: "unknown".to_owned(),
            failure_class: None,
        }
    }
}

#[cfg(feature = "nostr-adapter")]
#[async_trait]
impl RendezvousRoom for NostrProviderRoom {
    async fn publish_presence(
        &self,
        encrypted_presence: OpaqueSignalingPayload,
        ttl_seconds: u32,
    ) -> Result<(), TransportError> {
        if ttl_seconds == 0 {
            return Err(TransportError::SignalingAdapter(
                "presence ttl must be non-zero".to_owned(),
            ));
        }
        reject_forbidden_plaintext(&encrypted_presence.bytes)?;
        self.publish_envelope(NostrWireEnvelope::Presence {
            schema: NOSTR_EVENT_SCHEMA,
            from_peer: self.local_peer_id.clone(),
            payload: encrypted_presence,
            ttl_seconds,
        })
        .await
    }

    async fn subscribe_presence(&self) -> Result<Vec<PresenceEvent>, TransportError> {
        self.drain_network_for(Duration::from_millis(500)).await?;
        let mut inbox = self.inbox.lock().await;
        Ok(std::mem::take(&mut inbox.presence))
    }

    async fn send_signal(
        &self,
        to_peer: SignalingPeerId,
        payload: SealedWebRtcNegotiationPayload,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&payload.ciphertext)?;
        self.publish_envelope(NostrWireEnvelope::Signal {
            schema: NOSTR_EVENT_SCHEMA,
            from_peer: self.local_peer_id.clone(),
            to_peer,
            payload,
        })
        .await
    }

    async fn take_signals(&self) -> Result<Vec<PeerSignal>, TransportError> {
        self.drain_network_for(Duration::from_millis(500)).await?;
        let mut inbox = self.inbox.lock().await;
        Ok(std::mem::take(&mut inbox.signals))
    }

    async fn broadcast_control(
        &self,
        sealed_payload: OpaqueSignalingPayload,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&sealed_payload.bytes)?;
        self.publish_envelope(NostrWireEnvelope::Control {
            schema: NOSTR_EVENT_SCHEMA,
            from_peer: self.local_peer_id.clone(),
            payload: sealed_payload,
        })
        .await
    }

    async fn take_control_payloads(&self) -> Result<Vec<ControlBroadcast>, TransportError> {
        self.drain_network_for(Duration::from_millis(500)).await?;
        let mut inbox = self.inbox.lock().await;
        Ok(std::mem::take(&mut inbox.controls))
    }

    async fn leave(&self) -> Result<(), TransportError> {
        self.client.unsubscribe(&self.subscription_id).await;
        self.client.disconnect().await;
        Ok(())
    }
}

#[cfg(feature = "mqtt-adapter")]
impl MqttProviderRoom {
    async fn publish_envelope(
        &self,
        topic: String,
        envelope: MqttWireEnvelope,
    ) -> Result<(), TransportError> {
        let bytes =
            serde_json::to_vec(&envelope).map_err(|err| mqtt_err("wire envelope encode", err))?;
        reject_forbidden_plaintext(&bytes)?;
        self.client
            .publish(topic, rumqttc::QoS::AtLeastOnce, false, bytes)
            .await
            .map_err(|err| mqtt_err("publish", err))?;
        self.drain_network_for(Duration::from_secs(1)).await
    }

    async fn wait_for_subscription_acks(
        &self,
        expected: usize,
        duration: Duration,
    ) -> Result<(), TransportError> {
        let deadline = Instant::now() + duration;
        let mut acked = 0;
        while acked < expected {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(TransportError::SignalingAdapter(format!(
                    "mqtt subscribe ack timeout: observed {acked}/{expected}"
                )));
            }
            let event = {
                let mut events = self.events.lock().await;
                timeout(remaining, events.recv()).await
            };
            match event {
                Ok(Some(MqttProviderEvent::SubAck)) => {
                    acked += 1;
                    if std::env::var("DISCRYPT_SIGNALING_TRACE").as_deref() == Ok("1") {
                        eprintln!("mqtt subscribe ack {acked}/{expected}");
                    }
                }
                Ok(Some(MqttProviderEvent::Publish { topic, payload })) => {
                    self.record_publish(topic, payload).await?;
                }
                Ok(Some(MqttProviderEvent::Error(err))) => {
                    if std::env::var("DISCRYPT_SIGNALING_TRACE").as_deref() == Ok("1") {
                        eprintln!("mqtt event error {err}");
                    }
                    return Err(mqtt_err("event loop", err));
                }
                Ok(None) => {
                    return Err(TransportError::SignalingAdapter(
                        "mqtt event loop stopped".to_owned(),
                    ));
                }
                Err(_) => continue,
            }
        }
        Ok(())
    }

    async fn drain_network_for(&self, duration: Duration) -> Result<(), TransportError> {
        let deadline = Instant::now() + duration;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            let event = {
                let mut events = self.events.lock().await;
                timeout(remaining, events.recv()).await
            };
            match event {
                Ok(Some(MqttProviderEvent::Publish { topic, payload })) => {
                    if std::env::var("DISCRYPT_SIGNALING_TRACE").as_deref() == Ok("1") {
                        eprintln!("mqtt incoming publish {topic}");
                    }
                    self.record_publish(topic, payload).await?;
                }
                Ok(Some(MqttProviderEvent::SubAck)) => {
                    if std::env::var("DISCRYPT_SIGNALING_TRACE").as_deref() == Ok("1") {
                        eprintln!("mqtt late subscribe ack");
                    }
                }
                Ok(Some(MqttProviderEvent::Error(err))) => {
                    if std::env::var("DISCRYPT_SIGNALING_TRACE").as_deref() == Ok("1") {
                        eprintln!("mqtt event error {err}");
                    }
                    return Err(mqtt_err("event loop", err));
                }
                Ok(None) => {
                    return Err(TransportError::SignalingAdapter(
                        "mqtt event loop stopped".to_owned(),
                    ));
                }
                Err(_) => continue,
            }
        }
    }

    async fn record_publish(&self, _topic: String, bytes: Vec<u8>) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&bytes)?;
        let envelope: MqttWireEnvelope =
            serde_json::from_slice(&bytes).map_err(|err| mqtt_err("wire envelope decode", err))?;
        let mut inbox = self.inbox.lock().await;
        match envelope {
            MqttWireEnvelope::Presence {
                schema,
                from_peer,
                payload,
                ttl_seconds,
            } if schema == 1 && from_peer != self.local_peer_id => {
                inbox.presence.push(PresenceEvent {
                    peer_id: from_peer,
                    encrypted_presence: payload,
                    ttl_seconds,
                });
            }
            MqttWireEnvelope::Signal {
                schema,
                from_peer,
                to_peer,
                payload,
            } if schema == 1 && to_peer == self.local_peer_id => {
                inbox.signals.push(PeerSignal {
                    from_peer,
                    to_peer,
                    payload,
                });
            }
            MqttWireEnvelope::Control {
                schema,
                from_peer,
                payload,
            } if schema == 1 && from_peer != self.local_peer_id => {
                inbox.controls.push(ControlBroadcast { from_peer, payload });
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[async_trait]
impl SignalingAdapter for DiscryptQuicRendezvousProviderAdapter {
    type Session = DiscryptQuicRendezvousProviderSession;

    async fn connect(
        &self,
        profile: SignalingAdapterProfile,
    ) -> Result<Self::Session, TransportError> {
        profile.validate()?;
        if profile.kind != SignalingAdapterKind::DiscryptQuicRendezvous {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "adapter profile kind {} does not match discrypt_quic_rendezvous adapter",
                profile.kind.canonical_name()
            )));
        }
        let endpoint_base = discrypt_rendezvous_endpoint_base(&profile)?;
        discrypt_rendezvous_validate_profile_trust(&profile)?;
        let endpoint = profile.endpoints.first().ok_or_else(|| {
            TransportError::InvalidConnectivityPolicy(
                "discrypt rendezvous profile requires one endpoint".to_owned(),
            )
        })?;
        let endpoint_security = endpoint.security;
        let endpoint_trust_fingerprint = endpoint.trust_fingerprint.as_deref();
        let health_url = format!("{endpoint_base}/healthz");
        let health = tokio::task::spawn_blocking(move || {
            let mut response = ureq::get(&health_url).call().map_err(|error| {
                TransportError::SignalingAdapter(format!(
                    "discrypt rendezvous service health check failed: {error}"
                ))
            })?;
            response
                .body_mut()
                .read_json::<DiscryptRendezvousHealthResponse>()
                .map_err(|error| {
                    TransportError::SignalingAdapter(format!(
                        "discrypt rendezvous health response decode failed: {error}"
                    ))
                })
        })
        .await
        .map_err(|error| {
            TransportError::SignalingAdapter(format!(
                "discrypt rendezvous health task failed: {error}"
            ))
        })??;
        discrypt_rendezvous_validate_health(
            &endpoint_base,
            endpoint_security,
            endpoint_trust_fingerprint,
            &health,
        )?;
        Ok(Self::Session {
            profile,
            endpoint_base,
        })
    }

    fn capabilities(&self) -> SignalingAdapterCapabilities {
        SignalingAdapterCapabilities::production_required()
    }

    fn observability_redacted(&self) -> SignalingObservability {
        SignalingObservability {
            adapter_kind: SignalingAdapterKind::DiscryptQuicRendezvous,
            endpoint_label: "discrypt_rendezvous#configured_service".to_owned(),
            health: SignalingHealthState::Healthy,
            trust_label: AdapterTrustLabel {
                label: "discrypt_quic_rendezvous".to_owned(),
                posture: "separate content-blind Discrypt rendezvous service API; native quic:// transport remains reserved until audited".to_owned(),
            },
        }
    }
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[async_trait]
impl AdapterSession for DiscryptQuicRendezvousProviderSession {
    type Room = DiscryptQuicRendezvousProviderRoom;

    async fn join(
        &self,
        scope: ConversationScope,
        rendezvous: RendezvousCapability,
        local_peer_id: SignalingPeerId,
    ) -> Result<Self::Room, TransportError> {
        scope.validate()?;
        if rendezvous.scope != scope {
            return Err(TransportError::SignalingAdapter(
                "rendezvous capability scope mismatch".to_owned(),
            ));
        }
        if rendezvous.adapter_kind != SignalingAdapterKind::DiscryptQuicRendezvous {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "rendezvous capability kind {} does not match discrypt_quic_rendezvous adapter",
                rendezvous.adapter_kind.canonical_name()
            )));
        }
        let _ = &self.profile;
        Ok(DiscryptQuicRendezvousProviderRoom {
            endpoint_base: self.endpoint_base.clone(),
            local_peer_id,
            topic: rendezvous.topic,
            nonce_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        })
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn health(&self) -> SignalingHealth {
        SignalingHealth {
            adapter_kind: SignalingAdapterKind::DiscryptQuicRendezvous,
            state: SignalingHealthState::Healthy,
            latency_bucket: "unknown".to_owned(),
            failure_class: None,
        }
    }
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[async_trait]
impl RendezvousRoom for DiscryptQuicRendezvousProviderRoom {
    async fn publish_presence(
        &self,
        encrypted_presence: OpaqueSignalingPayload,
        ttl_seconds: u32,
    ) -> Result<(), TransportError> {
        if ttl_seconds == 0 {
            return Err(TransportError::SignalingAdapter(
                "presence ttl must be non-zero".to_owned(),
            ));
        }
        reject_forbidden_plaintext(&encrypted_presence.bytes)?;
        self.publish_envelope(
            DiscryptRendezvousSignalKind::Rendezvous,
            discrypt_rendezvous_key(&self.topic, "presence", None),
            DiscryptRendezvousWireEnvelope::Presence {
                schema: 1,
                from_peer: self.local_peer_id.clone(),
                payload: encrypted_presence,
                ttl_seconds,
            },
            ttl_seconds,
        )
        .await
    }

    async fn subscribe_presence(&self) -> Result<Vec<PresenceEvent>, TransportError> {
        let envelopes = self
            .take_envelopes(
                DiscryptRendezvousSignalKind::Rendezvous,
                discrypt_rendezvous_key(&self.topic, "presence", None),
            )
            .await?;
        Ok(envelopes
            .into_iter()
            .filter_map(|envelope| match envelope {
                DiscryptRendezvousWireEnvelope::Presence {
                    schema: 1,
                    from_peer,
                    payload,
                    ttl_seconds,
                } if from_peer != self.local_peer_id => Some(PresenceEvent {
                    peer_id: from_peer,
                    encrypted_presence: payload,
                    ttl_seconds,
                }),
                _ => None,
            })
            .collect())
    }

    async fn send_signal(
        &self,
        to_peer: SignalingPeerId,
        payload: SealedWebRtcNegotiationPayload,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&payload.ciphertext)?;
        let kind = discrypt_rendezvous_signal_kind(&payload.kind);
        self.publish_envelope(
            kind,
            discrypt_rendezvous_key(&self.topic, "signal", Some(&to_peer)),
            DiscryptRendezvousWireEnvelope::Signal {
                schema: 1,
                from_peer: self.local_peer_id.clone(),
                to_peer,
                payload,
            },
            120,
        )
        .await
    }

    async fn take_signals(&self) -> Result<Vec<PeerSignal>, TransportError> {
        let mut signals = Vec::new();
        for kind in [
            DiscryptRendezvousSignalKind::Offer,
            DiscryptRendezvousSignalKind::Answer,
            DiscryptRendezvousSignalKind::Candidate,
        ] {
            let envelopes = self
                .take_envelopes(
                    kind,
                    discrypt_rendezvous_key(&self.topic, "signal", Some(&self.local_peer_id)),
                )
                .await?;
            for envelope in envelopes {
                if let DiscryptRendezvousWireEnvelope::Signal {
                    schema: 1,
                    from_peer,
                    to_peer,
                    payload,
                } = envelope
                {
                    if to_peer == self.local_peer_id {
                        signals.push(PeerSignal {
                            from_peer,
                            to_peer,
                            payload,
                        });
                    }
                }
            }
        }
        Ok(signals)
    }

    async fn broadcast_control(
        &self,
        sealed_payload: OpaqueSignalingPayload,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&sealed_payload.bytes)?;
        self.publish_envelope(
            DiscryptRendezvousSignalKind::AdmissionHelper,
            discrypt_rendezvous_key(&self.topic, "control", None),
            DiscryptRendezvousWireEnvelope::Control {
                schema: 1,
                from_peer: self.local_peer_id.clone(),
                payload: sealed_payload,
            },
            120,
        )
        .await
    }

    async fn take_control_payloads(&self) -> Result<Vec<ControlBroadcast>, TransportError> {
        let envelopes = self
            .take_envelopes(
                DiscryptRendezvousSignalKind::AdmissionHelper,
                discrypt_rendezvous_key(&self.topic, "control", None),
            )
            .await?;
        Ok(envelopes
            .into_iter()
            .filter_map(|envelope| match envelope {
                DiscryptRendezvousWireEnvelope::Control {
                    schema: 1,
                    from_peer,
                    payload,
                } if from_peer != self.local_peer_id => {
                    Some(ControlBroadcast { from_peer, payload })
                }
                _ => None,
            })
            .collect())
    }

    async fn leave(&self) -> Result<(), TransportError> {
        Ok(())
    }
}

#[cfg(feature = "mqtt-adapter")]
#[async_trait]
impl SignalingAdapter for MqttProviderAdapter {
    type Session = MqttProviderSession;

    async fn connect(
        &self,
        profile: SignalingAdapterProfile,
    ) -> Result<Self::Session, TransportError> {
        profile.validate()?;
        if profile.kind != SignalingAdapterKind::Mqtt {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "adapter profile kind {} does not match mqtt adapter",
                profile.kind.canonical_name()
            )));
        }
        Ok(MqttProviderSession { profile })
    }

    fn capabilities(&self) -> SignalingAdapterCapabilities {
        SignalingAdapterCapabilities::production_required()
    }

    fn observability_redacted(&self) -> SignalingObservability {
        SignalingObservability {
            adapter_kind: SignalingAdapterKind::Mqtt,
            endpoint_label: "mqtt#configured_profile".to_owned(),
            health: SignalingHealthState::Healthy,
            trust_label: AdapterTrustLabel {
                label: "mqtt".to_owned(),
                posture: "real provider client; broker sees hashed topic and opaque envelopes"
                    .to_owned(),
            },
        }
    }
}

#[cfg(feature = "mqtt-adapter")]
#[async_trait]
impl AdapterSession for MqttProviderSession {
    type Room = MqttProviderRoom;

    async fn join(
        &self,
        scope: ConversationScope,
        rendezvous: RendezvousCapability,
        local_peer_id: SignalingPeerId,
    ) -> Result<Self::Room, TransportError> {
        scope.validate()?;
        if rendezvous.scope != scope {
            return Err(TransportError::SignalingAdapter(
                "rendezvous capability scope mismatch".to_owned(),
            ));
        }
        if rendezvous.adapter_kind != SignalingAdapterKind::Mqtt {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "rendezvous capability kind {} does not match mqtt adapter",
                rendezvous.adapter_kind.canonical_name()
            )));
        }
        let endpoint = mqtt_endpoint_for_profile(&self.profile)?;
        let client_id = mqtt_client_id(&rendezvous.topic, &local_peer_id);
        let options = mqtt_options_from_endpoint(&endpoint.endpoint.0, &client_id)?;
        let topics = mqtt_topics(&rendezvous, &local_peer_id);
        let (client, mut eventloop) = rumqttc::AsyncClient::builder(options).capacity(64).build();
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(128);
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(publish))) => {
                        if event_tx
                            .send(MqttProviderEvent::Publish {
                                topic: String::from_utf8_lossy(publish.topic.as_ref()).into_owned(),
                                payload: publish.payload.to_vec(),
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::SubAck(_))) => {
                        if event_tx.send(MqttProviderEvent::SubAck).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        let _ = event_tx
                            .send(MqttProviderEvent::Error(err.to_string()))
                            .await;
                        break;
                    }
                }
            }
        });
        client
            .subscribe(topics.presence.clone(), rumqttc::QoS::AtLeastOnce)
            .await
            .map_err(|err| mqtt_err("subscribe presence", err))?;
        client
            .subscribe(topics.control.clone(), rumqttc::QoS::AtLeastOnce)
            .await
            .map_err(|err| mqtt_err("subscribe control", err))?;
        client
            .subscribe(
                topics.signal_for_local_peer.clone(),
                rumqttc::QoS::AtLeastOnce,
            )
            .await
            .map_err(|err| mqtt_err("subscribe peer signal", err))?;
        let room = MqttProviderRoom {
            local_peer_id,
            client,
            events: AsyncMutex::new(event_rx),
            rendezvous_topic: rendezvous.topic,
            topics,
            inbox: AsyncMutex::new(MqttInbox::default()),
        };
        room.wait_for_subscription_acks(3, Duration::from_secs(5))
            .await?;
        room.drain_network_for(Duration::from_millis(500)).await?;
        Ok(room)
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn health(&self) -> SignalingHealth {
        SignalingHealth {
            adapter_kind: SignalingAdapterKind::Mqtt,
            state: SignalingHealthState::Healthy,
            latency_bucket: "unknown".to_owned(),
            failure_class: None,
        }
    }
}

#[cfg(feature = "mqtt-adapter")]
#[async_trait]
impl RendezvousRoom for MqttProviderRoom {
    async fn publish_presence(
        &self,
        encrypted_presence: OpaqueSignalingPayload,
        ttl_seconds: u32,
    ) -> Result<(), TransportError> {
        if ttl_seconds == 0 {
            return Err(TransportError::SignalingAdapter(
                "presence ttl must be non-zero".to_owned(),
            ));
        }
        reject_forbidden_plaintext(&encrypted_presence.bytes)?;
        self.publish_envelope(
            self.topics.presence.clone(),
            MqttWireEnvelope::Presence {
                schema: 1,
                from_peer: self.local_peer_id.clone(),
                payload: encrypted_presence,
                ttl_seconds,
            },
        )
        .await
    }

    async fn subscribe_presence(&self) -> Result<Vec<PresenceEvent>, TransportError> {
        self.drain_network_for(Duration::from_millis(300)).await?;
        let mut inbox = self.inbox.lock().await;
        Ok(std::mem::take(&mut inbox.presence))
    }

    async fn send_signal(
        &self,
        to_peer: SignalingPeerId,
        payload: SealedWebRtcNegotiationPayload,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&payload.ciphertext)?;
        let topic = mqtt_signal_topic(&self.rendezvous_topic, &to_peer);
        self.publish_envelope(
            topic,
            MqttWireEnvelope::Signal {
                schema: 1,
                from_peer: self.local_peer_id.clone(),
                to_peer,
                payload,
            },
        )
        .await
    }

    async fn take_signals(&self) -> Result<Vec<PeerSignal>, TransportError> {
        self.drain_network_for(Duration::from_millis(300)).await?;
        let mut inbox = self.inbox.lock().await;
        Ok(std::mem::take(&mut inbox.signals))
    }

    async fn broadcast_control(
        &self,
        sealed_payload: OpaqueSignalingPayload,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&sealed_payload.bytes)?;
        self.publish_envelope(
            self.topics.control.clone(),
            MqttWireEnvelope::Control {
                schema: 1,
                from_peer: self.local_peer_id.clone(),
                payload: sealed_payload,
            },
        )
        .await
    }

    async fn take_control_payloads(&self) -> Result<Vec<ControlBroadcast>, TransportError> {
        self.drain_network_for(Duration::from_millis(300)).await?;
        let mut inbox = self.inbox.lock().await;
        Ok(std::mem::take(&mut inbox.controls))
    }

    async fn leave(&self) -> Result<(), TransportError> {
        self.client
            .disconnect()
            .await
            .map_err(|err| mqtt_err("disconnect", err))
    }
}

#[async_trait]
impl SignalingAdapter for LocalConformanceProviderAdapter {
    type Session = LocalConformanceProviderSession;

    async fn connect(
        &self,
        profile: SignalingAdapterProfile,
    ) -> Result<Self::Session, TransportError> {
        profile.validate()?;
        if profile.kind != self.kind {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "adapter profile kind {} does not match local conformance kind {}",
                profile.kind.canonical_name(),
                self.kind.canonical_name()
            )));
        }
        if profile
            .endpoints
            .iter()
            .any(|endpoint| endpoint.security != SignalingEndpointSecurity::LocalDevLoopback)
        {
            return Err(TransportError::InvalidConnectivityPolicy(
                "local conformance adapters require local-dev loopback endpoints".to_owned(),
            ));
        }
        Ok(LocalConformanceProviderSession {
            kind: self.kind,
            bus: self.bus.clone(),
        })
    }

    fn capabilities(&self) -> SignalingAdapterCapabilities {
        SignalingAdapterCapabilities::production_required()
    }

    fn observability_redacted(&self) -> SignalingObservability {
        SignalingObservability {
            adapter_kind: self.kind,
            endpoint_label: format!("local-conformance:{}#redacted", self.kind.canonical_name()),
            health: SignalingHealthState::Healthy,
            trust_label: AdapterTrustLabel {
                label: self.kind.canonical_name().to_owned(),
                posture: "deterministic local conformance only".to_owned(),
            },
        }
    }
}

#[async_trait]
impl AdapterSession for LocalConformanceProviderSession {
    type Room = LocalConformanceProviderRoom;

    async fn join(
        &self,
        scope: ConversationScope,
        rendezvous: RendezvousCapability,
        local_peer_id: SignalingPeerId,
    ) -> Result<Self::Room, TransportError> {
        scope.validate()?;
        if rendezvous.scope != scope {
            return Err(TransportError::SignalingAdapter(
                "rendezvous capability scope mismatch".to_owned(),
            ));
        }
        if rendezvous.adapter_kind != self.kind {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "rendezvous capability kind {} does not match local conformance kind {}",
                rendezvous.adapter_kind.canonical_name(),
                self.kind.canonical_name()
            )));
        }
        let key = LocalRoomKey {
            kind: self.kind,
            topic: rendezvous.topic,
        };
        self.bus.ensure_room(key.clone())?;
        Ok(LocalConformanceProviderRoom {
            bus: self.bus.clone(),
            key,
            local_peer_id,
        })
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn health(&self) -> SignalingHealth {
        SignalingHealth {
            adapter_kind: self.kind,
            state: SignalingHealthState::Healthy,
            latency_bucket: "local".to_owned(),
            failure_class: None,
        }
    }
}

#[async_trait]
impl RendezvousRoom for LocalConformanceProviderRoom {
    async fn publish_presence(
        &self,
        encrypted_presence: OpaqueSignalingPayload,
        ttl_seconds: u32,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&encrypted_presence.bytes)?;
        if ttl_seconds == 0 {
            return Err(TransportError::SignalingAdapter(
                "presence ttl must be non-zero".to_owned(),
            ));
        }
        self.bus.with_state(|state| {
            state
                .rooms
                .entry(self.key.clone())
                .or_default()
                .presence
                .push(PresenceEvent {
                    peer_id: self.local_peer_id.clone(),
                    encrypted_presence,
                    ttl_seconds,
                });
        })
    }

    async fn subscribe_presence(&self) -> Result<Vec<PresenceEvent>, TransportError> {
        self.bus.with_state(|state| {
            state
                .rooms
                .get(&self.key)
                .map(|room| room.presence.clone())
                .unwrap_or_default()
        })
    }

    async fn send_signal(
        &self,
        to_peer: SignalingPeerId,
        payload: SealedWebRtcNegotiationPayload,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&payload.ciphertext)?;
        self.bus.with_state(|state| {
            state
                .rooms
                .entry(self.key.clone())
                .or_default()
                .signals
                .push(PeerSignal {
                    from_peer: self.local_peer_id.clone(),
                    to_peer,
                    payload,
                });
        })
    }

    async fn take_signals(&self) -> Result<Vec<PeerSignal>, TransportError> {
        self.bus.with_state(|state| {
            let Some(room) = state.rooms.get_mut(&self.key) else {
                return Vec::new();
            };
            let mut delivered = Vec::new();
            room.signals.retain(|signal| {
                if signal.to_peer == self.local_peer_id {
                    delivered.push(signal.clone());
                    false
                } else {
                    true
                }
            });
            delivered
        })
    }

    async fn broadcast_control(
        &self,
        sealed_payload: OpaqueSignalingPayload,
    ) -> Result<(), TransportError> {
        reject_forbidden_plaintext(&sealed_payload.bytes)?;
        self.bus.with_state(|state| {
            state
                .rooms
                .entry(self.key.clone())
                .or_default()
                .controls
                .push(ControlBroadcast {
                    from_peer: self.local_peer_id.clone(),
                    payload: sealed_payload,
                });
        })
    }

    async fn take_control_payloads(&self) -> Result<Vec<ControlBroadcast>, TransportError> {
        self.bus.with_state(|state| {
            let Some(room) = state.rooms.get_mut(&self.key) else {
                return Vec::new();
            };
            let mut delivered = Vec::new();
            room.controls.retain(|control| {
                if control.from_peer != self.local_peer_id {
                    delivered.push(control.clone());
                    false
                } else {
                    true
                }
            });
            delivered
        })
    }

    async fn leave(&self) -> Result<(), TransportError> {
        self.bus.with_state(|state| {
            if let Some(room) = state.rooms.get_mut(&self.key) {
                room.presence
                    .retain(|event| event.peer_id != self.local_peer_id);
                room.signals.retain(|signal| {
                    signal.from_peer != self.local_peer_id && signal.to_peer != self.local_peer_id
                });
                room.controls
                    .retain(|control| control.from_peer != self.local_peer_id);
            }
        })
    }
}

#[async_trait]
impl SignalingAdapter for FeatureGatedProviderAdapter {
    type Session = FeatureGatedProviderSession;

    async fn connect(
        &self,
        profile: SignalingAdapterProfile,
    ) -> Result<Self::Session, TransportError> {
        profile.validate()?;
        if profile.kind != self.boundary.kind {
            return Err(TransportError::InvalidConnectivityPolicy(format!(
                "adapter profile kind {} does not match boundary kind {}",
                profile.kind.canonical_name(),
                self.boundary.kind.canonical_name()
            )));
        }
        Err(self.boundary.unavailable_error())
    }

    fn capabilities(&self) -> SignalingAdapterCapabilities {
        SignalingAdapterCapabilities::production_required()
    }

    fn observability_redacted(&self) -> SignalingObservability {
        SignalingObservability {
            adapter_kind: self.boundary.kind,
            endpoint_label: format!("{}#not_connected", self.boundary.kind.canonical_name()),
            health: SignalingHealthState::ProviderUnhealthy,
            trust_label: AdapterTrustLabel {
                label: self.boundary.kind.canonical_name().to_owned(),
                posture: format!(
                    "{}; {}",
                    self.boundary.cargo_feature,
                    self.boundary.failure_class()
                ),
            },
        }
    }
}

#[async_trait]
impl AdapterSession for FeatureGatedProviderSession {
    type Room = FeatureGatedProviderRoom;

    async fn join(
        &self,
        _scope: ConversationScope,
        _rendezvous: RendezvousCapability,
        _local_peer_id: SignalingPeerId,
    ) -> Result<Self::Room, TransportError> {
        Err(self.boundary.unavailable_error())
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn health(&self) -> SignalingHealth {
        SignalingHealth {
            adapter_kind: self.boundary.kind,
            state: SignalingHealthState::ProviderUnhealthy,
            latency_bucket: "not_connected".to_owned(),
            failure_class: Some(self.boundary.failure_class().to_owned()),
        }
    }
}

#[async_trait]
impl RendezvousRoom for FeatureGatedProviderRoom {
    async fn publish_presence(
        &self,
        _encrypted_presence: OpaqueSignalingPayload,
        _ttl_seconds: u32,
    ) -> Result<(), TransportError> {
        Err(self.boundary.unavailable_error())
    }

    async fn subscribe_presence(&self) -> Result<Vec<PresenceEvent>, TransportError> {
        Err(self.boundary.unavailable_error())
    }

    async fn send_signal(
        &self,
        _to_peer: SignalingPeerId,
        _payload: SealedWebRtcNegotiationPayload,
    ) -> Result<(), TransportError> {
        Err(self.boundary.unavailable_error())
    }

    async fn take_signals(&self) -> Result<Vec<PeerSignal>, TransportError> {
        Err(self.boundary.unavailable_error())
    }

    async fn broadcast_control(
        &self,
        _sealed_payload: OpaqueSignalingPayload,
    ) -> Result<(), TransportError> {
        Err(self.boundary.unavailable_error())
    }

    async fn take_control_payloads(&self) -> Result<Vec<ControlBroadcast>, TransportError> {
        Err(self.boundary.unavailable_error())
    }

    async fn leave(&self) -> Result<(), TransportError> {
        Err(self.boundary.unavailable_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        derive_scope_commitment, ConnectivityScopeLevel, Endpoint, ProviderMetadataPosture,
        SignalingEndpointSecurity, SignalingProviderEndpoint, WebRtcIceCandidate,
        WebRtcNegotiationPayloadKind, WebRtcNegotiationSealer, WebRtcSdpType,
        WebRtcSessionDescription,
    };

    fn valid_endpoint(kind: SignalingAdapterKind) -> &'static str {
        match kind {
            SignalingAdapterKind::Mqtt => "wss://mqtt.example.invalid",
            SignalingAdapterKind::Nostr => "wss://nostr.example.invalid",
            SignalingAdapterKind::IpfsPubsub => {
                "/ip4/203.0.113.10/tcp/4001/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"
            }
            SignalingAdapterKind::DiscryptQuicRendezvous => "quic://signal.example.invalid",
        }
    }

    fn local_endpoint(kind: SignalingAdapterKind) -> &'static str {
        match kind {
            SignalingAdapterKind::Mqtt => "mqtt://127.0.0.1:1883",
            SignalingAdapterKind::Nostr => "ws://127.0.0.1:8080",
            SignalingAdapterKind::IpfsPubsub => "http://127.0.0.1:5001",
            SignalingAdapterKind::DiscryptQuicRendezvous => "http://127.0.0.1:9443",
        }
    }

    fn valid_profile(
        kind: SignalingAdapterKind,
    ) -> Result<SignalingAdapterProfile, TransportError> {
        Ok(SignalingAdapterProfile {
            profile_id: format!("profile-{}", kind.canonical_name()),
            kind,
            endpoints: vec![SignalingProviderEndpoint::new(
                Endpoint::new(valid_endpoint(kind)),
                SignalingEndpointSecurity::ProductionTls,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new(kind.canonical_name(), "redacted boundary")?,
        })
    }

    fn local_profile(
        kind: SignalingAdapterKind,
    ) -> Result<SignalingAdapterProfile, TransportError> {
        Ok(SignalingAdapterProfile {
            profile_id: format!("local-{}", kind.canonical_name()),
            kind,
            endpoints: vec![SignalingProviderEndpoint::new(
                Endpoint::new(local_endpoint(kind)),
                SignalingEndpointSecurity::LocalDevLoopback,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new(kind.canonical_name(), "local conformance")?,
        })
    }

    fn expected_readiness(boundary: ProviderAdapterBoundary) -> ProviderAdapterReadiness {
        let feature_enabled = match boundary.kind {
            SignalingAdapterKind::Mqtt => cfg!(feature = "mqtt-adapter"),
            SignalingAdapterKind::Nostr => cfg!(feature = "nostr-adapter"),
            SignalingAdapterKind::IpfsPubsub => cfg!(feature = "ipfs-pubsub-adapter"),
            SignalingAdapterKind::DiscryptQuicRendezvous => {
                cfg!(feature = "discrypt-quic-rendezvous-adapter")
            }
        };
        if matches!(
            boundary.kind,
            SignalingAdapterKind::Mqtt
                | SignalingAdapterKind::Nostr
                | SignalingAdapterKind::IpfsPubsub
        ) && feature_enabled
        {
            ProviderAdapterReadiness::ImplementationAvailable
        } else if feature_enabled {
            ProviderAdapterReadiness::ImplementationUnavailable
        } else {
            ProviderAdapterReadiness::FeatureDisabled
        }
    }

    fn expected_adapter_readiness_state(
        readiness: ProviderAdapterReadiness,
    ) -> AdapterReadinessState {
        match readiness {
            ProviderAdapterReadiness::FeatureDisabled => AdapterReadinessState::FeatureDisabled,
            ProviderAdapterReadiness::ImplementationUnavailable => {
                AdapterReadinessState::ImplementationUnavailable
            }
            ProviderAdapterReadiness::ImplementationAvailable => AdapterReadinessState::Available,
        }
    }

    fn dedup_requested_kinds() -> [SignalingAdapterKind; 7] {
        [
            SignalingAdapterKind::Nostr,
            SignalingAdapterKind::Mqtt,
            SignalingAdapterKind::Nostr,
            SignalingAdapterKind::IpfsPubsub,
            SignalingAdapterKind::DiscryptQuicRendezvous,
            SignalingAdapterKind::IpfsPubsub,
            SignalingAdapterKind::Mqtt,
        ]
    }

    #[test]
    fn required_provider_adapter_registry_is_stable_and_matches_boundaries_and_factory() {
        let registry = required_provider_adapter_registry();
        let boundaries = required_provider_adapter_boundaries();
        assert_eq!(registry.len(), boundaries.len());

        for entry in registry {
            let boundary = adapter_boundary_for_kind(entry.kind);
            assert_eq!(entry.boundary, boundary);
            assert_eq!(
                entry.kind, boundary.kind,
                "kind and boundary kind must remain aligned"
            );
            let factory = SignalingAdapterFactory::for_kind(entry.kind);
            assert_eq!(factory.boundary(), boundary);
        }
    }

    #[test]
    fn signaling_adapter_registry_readiness_is_consistent_with_boundary_definition() {
        let registry = required_provider_adapter_registry();
        let expected = required_provider_adapter_boundaries();

        for (entry, expected_boundary) in registry.iter().zip(expected.iter()) {
            assert_eq!(entry.boundary, *expected_boundary);
            assert_eq!(
                entry.readiness_state(),
                expected_adapter_readiness_state(expected_boundary.readiness)
            );
        }
    }

    #[test]
    fn plan_signaling_adapter_fallback_try_all_deduplicates_and_marks_selected() {
        let requested = dedup_requested_kinds();
        let _registry = required_provider_adapter_boundaries();
        let plan =
            plan_signaling_adapter_fallback(&requested, AdapterFallbackBehavior::TryAll, None);

        assert_eq!(plan.behavior, AdapterFallbackBehavior::TryAll);
        let mut expected = Vec::new();
        for kind in requested {
            if !expected.contains(&kind) {
                expected.push(kind);
            }
        }
        assert_eq!(
            plan.attempts
                .iter()
                .map(|attempt| attempt.kind)
                .collect::<Vec<_>>(),
            expected
        );
        assert!(plan.attempts.iter().all(|attempt| attempt.attempted));

        let first_selectable = expected
            .iter()
            .find(|kind| {
                SignalingAdapterFactory::for_kind(**kind)
                    .readiness_state()
                    .selectable()
            })
            .copied();
        assert_eq!(plan.selected, first_selectable);

        if let Some(selected) = first_selectable {
            assert_eq!(
                plan.attempts
                    .iter()
                    .find(|attempt| attempt.kind == selected)
                    .map(|attempt| attempt.selected),
                Some(true)
            );
            assert_eq!(
                plan.attempts
                    .iter()
                    .filter(|attempt| attempt.selected && attempt.kind == selected)
                    .count(),
                1
            );
        } else {
            assert_eq!(plan.selected, None);
            assert!(plan.all_unavailable());
            assert!(plan.attempts.iter().all(|attempt| !attempt.selected));
        }
    }

    #[test]
    fn plan_signaling_adapter_fallback_first_healthy_stops_after_selected() {
        let requested = dedup_requested_kinds();
        let plan = plan_signaling_adapter_fallback(
            &requested,
            AdapterFallbackBehavior::FirstHealthy,
            None,
        );

        let expected_dedup: Vec<_> = {
            let mut kinds = Vec::new();
            for kind in requested {
                if !kinds.contains(&kind) {
                    kinds.push(kind);
                }
            }
            kinds
        };
        let selected = expected_dedup
            .iter()
            .find(|kind| {
                SignalingAdapterFactory::for_kind(**kind)
                    .readiness_state()
                    .selectable()
            })
            .copied();
        let expected_attempts = match selected {
            Some(_) => plan
                .selected
                .as_ref()
                .and_then(|selected| expected_dedup.iter().position(|kind| kind == selected))
                .map(|index| index + 1)
                .unwrap_or(expected_dedup.len()),
            None => expected_dedup.len(),
        };
        assert_eq!(plan.attempts.len(), expected_attempts);
        assert!(plan.attempts.iter().all(|attempt| attempt.attempted));
        assert_eq!(plan.selected, selected);
        if let Some(selected) = selected {
            let mut last_selected = None;
            for attempt in plan.attempts.iter().rev() {
                if attempt.selected {
                    last_selected = Some(attempt.kind);
                    break;
                }
            }
            assert_eq!(
                last_selected,
                Some(selected),
                "plan should include selected kind and mark it selected"
            );
        } else {
            assert!(plan.all_unavailable());
        }
    }

    #[test]
    fn plan_signaling_adapter_fallback_manual_only_uses_only_requested_manual_adapter() {
        let requested = dedup_requested_kinds();
        let manual = SignalingAdapterKind::IpfsPubsub;
        let plan = plan_signaling_adapter_fallback(
            &requested,
            AdapterFallbackBehavior::ManualOnly,
            Some(manual),
        );
        assert_eq!(plan.behavior, AdapterFallbackBehavior::ManualOnly);
        assert_eq!(plan.attempts.len(), 1);
        let attempt = &plan.attempts[0];
        assert_eq!(attempt.kind, manual);
        let readiness = SignalingAdapterFactory::for_kind(manual).readiness_state();
        assert_eq!(attempt.readiness, readiness);
        assert!(attempt.attempted);
        assert_eq!(attempt.selected, readiness.selectable());
        assert_eq!(
            plan.selected,
            if readiness.selectable() {
                Some(manual)
            } else {
                None
            }
        );

        let absent =
            plan_signaling_adapter_fallback(&requested, AdapterFallbackBehavior::ManualOnly, None);
        assert!(absent.attempts.is_empty());
        assert_eq!(absent.selected, None);
    }

    #[tokio::test]
    async fn all_required_provider_boundaries_fail_closed_without_real_clients(
    ) -> Result<(), TransportError> {
        let boundaries = required_provider_adapter_boundaries();
        assert_eq!(boundaries.len(), 4);
        assert_eq!(boundaries[0].kind, SignalingAdapterKind::Mqtt);
        assert_eq!(boundaries[1].kind, SignalingAdapterKind::Nostr);
        assert_eq!(boundaries[2].kind, SignalingAdapterKind::IpfsPubsub);
        assert_eq!(
            boundaries[3].kind,
            SignalingAdapterKind::DiscryptQuicRendezvous
        );

        for boundary in boundaries {
            assert_eq!(boundary.readiness, expected_readiness(boundary));
            assert_eq!(
                boundary.implementation_available(),
                boundary.readiness == ProviderAdapterReadiness::ImplementationAvailable
            );
            assert_eq!(
                boundary.failure_class(),
                match boundary.readiness {
                    ProviderAdapterReadiness::FeatureDisabled => "feature_disabled",
                    ProviderAdapterReadiness::ImplementationUnavailable =>
                        "implementation_unavailable",
                    ProviderAdapterReadiness::ImplementationAvailable => "implementation_available",
                }
            );
            let adapter = FeatureGatedProviderAdapter::new(boundary.kind);
            assert_eq!(adapter.boundary().cargo_feature, boundary.cargo_feature);
            assert!(adapter.capabilities().satisfies_production_contract());
            let observability = adapter.observability_redacted();
            assert_eq!(observability.adapter_kind, boundary.kind);
            assert_eq!(
                observability.health,
                SignalingHealthState::ProviderUnhealthy
            );
            assert!(!observability.endpoint_label.contains("example.invalid"));

            let error = adapter.connect(valid_profile(boundary.kind)?).await;
            assert!(matches!(error, Err(TransportError::SignalingAdapter(_))));
            let message = error
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default();
            assert!(message.contains(boundary.kind.canonical_name()));
            assert!(message.contains(boundary.cargo_feature));
        }
        Ok(())
    }

    #[tokio::test]
    async fn provider_boundary_rejects_wrong_adapter_profile_kind() -> Result<(), TransportError> {
        let adapter = FeatureGatedProviderAdapter::new(SignalingAdapterKind::Mqtt);
        let wrong_profile = valid_profile(SignalingAdapterKind::Nostr)?;

        assert!(matches!(
            adapter.connect(wrong_profile).await,
            Err(TransportError::InvalidConnectivityPolicy(_))
        ));
        Ok(())
    }

    #[test]
    #[cfg(feature = "nostr-adapter")]
    fn nostr_structured_relay_messages_map_to_typed_failures() {
        use std::borrow::Cow;

        let rate_limited = nostr_sdk::RelayMessage::Notice(Cow::Borrowed(
            "rate-limited: slow down before publishing more events",
        ));
        assert_eq!(
            nostr_relay_message_failure(&rate_limited),
            Some(AdapterReadinessState::ProviderRateLimited)
        );

        let auth_required = nostr_sdk::RelayMessage::Closed {
            subscription_id: Cow::Owned(nostr_sdk::SubscriptionId::new("dc-test")),
            message: Cow::Borrowed("auth-required: restricted relay"),
        };
        assert_eq!(
            nostr_relay_message_failure(&auth_required),
            Some(AdapterReadinessState::ProviderAuthRequired)
        );

        let benign_notice = nostr_sdk::RelayMessage::Notice(Cow::Borrowed("welcome"));
        assert_eq!(nostr_relay_message_failure(&benign_notice), None);
    }

    #[test]
    fn provider_failure_classes_map_to_typed_health_states() {
        let cases = [
            (
                "rate-limited: you are noting too much",
                AdapterReadinessState::ProviderRateLimited,
                SignalingHealthState::ProviderRateLimited,
            ),
            (
                "relay requires auth",
                AdapterReadinessState::ProviderAuthRequired,
                SignalingHealthState::ProviderAuthRequired,
            ),
            (
                "max message size exceeded",
                AdapterReadinessState::ProviderMessageTooLarge,
                SignalingHealthState::ProviderMessageTooLarge,
            ),
            (
                "certificate fingerprint mismatch",
                AdapterReadinessState::TrustMismatch,
                SignalingHealthState::TrustMismatch,
            ),
        ];
        for (message, readiness, health) in cases {
            let classified = AdapterReadinessState::classify_provider_failure(message);
            assert_eq!(classified, readiness);
            assert_eq!(classified.to_health_state(), health);
        }
        assert_eq!(
            AdapterReadinessState::classify_provider_failure("connection closed"),
            AdapterReadinessState::ProviderUnhealthy
        );
    }

    #[tokio::test]
    #[cfg(feature = "nostr-adapter")]
    async fn nostr_adapter_feature_is_selectable_with_real_relay_client(
    ) -> Result<(), TransportError> {
        let boundary = adapter_boundary_for_kind(SignalingAdapterKind::Nostr);
        assert_eq!(
            boundary.readiness,
            ProviderAdapterReadiness::ImplementationAvailable
        );
        assert_eq!(boundary.failure_class(), "implementation_available");
        assert!(SignalingAdapterFactory::for_kind(SignalingAdapterKind::Nostr).selectable());

        let plan = plan_signaling_adapter_fallback(
            &[SignalingAdapterKind::Nostr],
            AdapterFallbackBehavior::ManualOnly,
            Some(SignalingAdapterKind::Nostr),
        );
        assert_eq!(plan.selected, Some(SignalingAdapterKind::Nostr));
        assert_eq!(plan.attempts.len(), 1);
        assert_eq!(plan.attempts[0].readiness, AdapterReadinessState::Available);
        assert!(plan.attempts[0].selected);

        let adapter = NostrProviderAdapter;
        let session = adapter
            .connect(valid_profile(SignalingAdapterKind::Nostr)?)
            .await?;
        assert_eq!(session.health().await.state, SignalingHealthState::Healthy);
        let observability = adapter.observability_redacted();
        assert_eq!(observability.adapter_kind, SignalingAdapterKind::Nostr);
        assert!(!observability.endpoint_label.contains("example.invalid"));
        Ok(())
    }

    #[test]
    #[cfg(feature = "nostr-adapter")]
    fn nostr_profile_preserves_all_configured_relays_for_room_join() -> Result<(), TransportError> {
        let mut profile = valid_profile(SignalingAdapterKind::Nostr)?;
        profile.endpoints.push(SignalingProviderEndpoint::new(
            Endpoint::new("wss://nostr-backup.example.invalid"),
            SignalingEndpointSecurity::ProductionTls,
        ));

        let relays = nostr_endpoints_for_profile(&profile)?;
        assert_eq!(
            relays,
            vec![
                "wss://nostr.example.invalid".to_owned(),
                "wss://nostr-backup.example.invalid".to_owned(),
            ]
        );
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    async fn ipfs_pubsub_adapter_feature_is_selectable_with_real_libp2p_client(
    ) -> Result<(), TransportError> {
        let boundary = adapter_boundary_for_kind(SignalingAdapterKind::IpfsPubsub);
        assert_eq!(
            boundary.readiness,
            ProviderAdapterReadiness::ImplementationAvailable
        );
        assert_eq!(boundary.failure_class(), "implementation_available");
        assert!(SignalingAdapterFactory::for_kind(SignalingAdapterKind::IpfsPubsub).selectable());

        let plan = plan_signaling_adapter_fallback(
            &[SignalingAdapterKind::IpfsPubsub],
            AdapterFallbackBehavior::ManualOnly,
            Some(SignalingAdapterKind::IpfsPubsub),
        );
        assert_eq!(plan.selected, Some(SignalingAdapterKind::IpfsPubsub));
        assert_eq!(plan.attempts.len(), 1);
        assert_eq!(plan.attempts[0].readiness, AdapterReadinessState::Available);
        assert!(plan.attempts[0].selected);

        let adapter = IpfsPubsubProviderAdapter;
        let session = adapter
            .connect(valid_profile(SignalingAdapterKind::IpfsPubsub)?)
            .await?;
        assert_eq!(session.health().await.state, SignalingHealthState::Healthy);
        let observability = adapter.observability_redacted();
        assert_eq!(observability.adapter_kind, SignalingAdapterKind::IpfsPubsub);
        assert!(!observability.endpoint_label.contains("example.invalid"));
        Ok(())
    }

    #[test]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    fn ipfs_pubsub_resource_policy_is_bounded_and_default_bootstrap_is_parseable() {
        assert_eq!(IPFS_PUBSUB_BOOTSTRAP_POLICY_VERSION, 1);
        assert!(IPFS_PUBSUB_MAX_MESSAGE_BYTES <= 64 * 1024);
        assert!(IPFS_PUBSUB_MAX_BOOTSTRAP_ENDPOINTS <= 16);
        assert!(IPFS_PUBSUB_COMMAND_QUEUE_DEPTH <= 128);

        let config = ipfs_pubsub_gossipsub_config().expect("gossipsub config");
        assert_eq!(config.max_transmit_size(), IPFS_PUBSUB_MAX_MESSAGE_BYTES);
        assert_eq!(config.history_length(), IPFS_PUBSUB_HISTORY_LENGTH);
        assert_eq!(config.history_gossip(), IPFS_PUBSUB_HISTORY_GOSSIP);
        assert_eq!(config.mesh_n_low(), IPFS_PUBSUB_MESH_N_LOW);
        assert_eq!(config.mesh_n(), IPFS_PUBSUB_MESH_N);
        assert_eq!(config.mesh_n_high(), IPFS_PUBSUB_MESH_N_HIGH);
        assert_eq!(
            config.duplicate_cache_time(),
            Duration::from_secs(IPFS_PUBSUB_DUPLICATE_CACHE_SECS)
        );
        assert!(matches!(
            config.validation_mode(),
            libp2p::gossipsub::ValidationMode::Strict
        ));
        assert!(!config.flood_publish());

        let defaults = ipfs_pubsub_default_bootstrap_addrs().expect("default bootstrap addrs");
        assert!(
            defaults.is_empty(),
            "default bootstrap is disabled until the libp2p DNS audit blocker is remediated"
        );
        ipfs_validate_bootstrap_policy(&defaults).expect("empty default bootstrap policy");
    }

    #[test]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    fn ipfs_pubsub_bootstrap_policy_rejects_duplicates_and_overflow() {
        let duplicate =
            "/ip4/127.0.0.1/tcp/4001/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"
                .parse::<libp2p::Multiaddr>()
                .expect("multiaddr");
        let error = ipfs_validate_bootstrap_policy(&[duplicate.clone(), duplicate])
            .expect_err("duplicate bootstrap endpoint must be rejected");
        assert!(format!("{error}").contains("duplicate bootstrap endpoint"));

        let mut too_many = Vec::new();
        for port in 0..=IPFS_PUBSUB_MAX_BOOTSTRAP_ENDPOINTS {
            too_many.push(
                format!(
                    "/ip4/127.0.0.1/tcp/{}/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
                    4000 + port
                )
                .parse::<libp2p::Multiaddr>()
                .expect("multiaddr"),
            );
        }
        let error = ipfs_validate_bootstrap_policy(&too_many)
            .expect_err("too many bootstrap endpoints must be rejected");
        assert!(format!("{error}").contains("resource policy limit"));
    }

    #[test]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    fn ipfs_pubsub_bootstrap_policy_rejects_dns_or_non_topic_peer_endpoints() {
        let dns_bootstrap: libp2p::Multiaddr =
            "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"
                .parse()
                .expect("dns multiaddr");
        let dns_error = ipfs_validate_bootstrap_policy(&[dns_bootstrap])
            .expect_err("DNS bootstrap remains audit-blocked");
        assert!(format!("{dns_error}").contains("DNS bootstrap endpoint rejected"));

        let missing_peer: libp2p::Multiaddr =
            "/ip4/203.0.113.10/tcp/4001".parse().expect("ip4 multiaddr");
        let peer_error = ipfs_validate_bootstrap_policy(&[missing_peer])
            .expect_err("dialable public endpoint must identify the topic peer");
        assert!(format!("{peer_error}").contains("/p2p/<peer-id>"));
    }

    #[test]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    fn ipfs_pubsub_bootstrap_policy_accepts_direct_topic_peer_multiaddrs() {
        let direct_topic_peer: libp2p::Multiaddr =
            "/ip4/203.0.113.10/tcp/4001/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"
                .parse()
                .expect("direct topic peer multiaddr");
        ipfs_validate_bootstrap_policy(&[direct_topic_peer])
            .expect("explicit direct topic peer multiaddr should satisfy production policy");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    async fn ipfs_pubsub_local_two_peer_presence_signal_and_control_roundtrip(
    ) -> Result<(), TransportError> {
        let adapter = IpfsPubsubProviderAdapter;
        let alice = SignalingPeerId::new("alice-device")?;
        let bob = SignalingPeerId::new("bob-device")?;
        let scope = crate::ConversationScope::new(
            ConnectivityScopeLevel::Dm,
            derive_scope_commitment(ConnectivityScopeLevel::Dm, b"ipfs local dm", "test"),
        )?;
        let capability = RendezvousCapability::derive(
            scope.clone(),
            SignalingAdapterKind::IpfsPubsub,
            b"bootstrap secret with more than thirty two bytes",
            b"random entropy bytes",
            120,
            ProviderMetadataPosture::HashedTopic,
            AdapterTrustLabel::new("ipfs_pubsub", "local rust-libp2p gossipsub")?,
        )?;

        let alice_profile = SignalingAdapterProfile {
            profile_id: "ipfs-alice-local".to_owned(),
            kind: SignalingAdapterKind::IpfsPubsub,
            endpoints: vec![SignalingProviderEndpoint::new(
                Endpoint::new("/ip4/127.0.0.1/tcp/0"),
                SignalingEndpointSecurity::LocalDevLoopback,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new("ipfs_pubsub", "local rust-libp2p gossipsub")?,
        };
        let alice_room = adapter
            .connect(alice_profile)
            .await?
            .join(scope.clone(), capability.clone(), alice.clone())
            .await?;
        let alice_addr = alice_room
            .direct_topic_peer_multiaddrs_for_tests()
            .into_iter()
            .next()
            .ok_or_else(|| {
                TransportError::SignalingAdapter("missing alice /p2p topic peer address".to_owned())
            })?;
        let bob_profile = SignalingAdapterProfile {
            profile_id: "ipfs-bob-local".to_owned(),
            kind: SignalingAdapterKind::IpfsPubsub,
            endpoints: vec![SignalingProviderEndpoint::new(
                Endpoint::new(alice_addr),
                SignalingEndpointSecurity::LocalDevLoopback,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new("ipfs_pubsub", "local rust-libp2p gossipsub")?,
        };
        let bob_room = adapter
            .connect(bob_profile)
            .await?
            .join(scope, capability, bob.clone())
            .await?;

        tokio::time::sleep(Duration::from_secs(2)).await;
        alice_room
            .publish_presence(
                OpaqueSignalingPayload::new(b"sealed-presence-alice".to_vec())?,
                120,
            )
            .await?;
        let bob_presence = bob_room.subscribe_presence().await?;
        assert!(bob_presence.iter().any(|event| event.peer_id == alice));

        let offer = SealedWebRtcNegotiationPayload {
            version: 1,
            kind: WebRtcNegotiationPayloadKind::Offer,
            nonce: [7; 12],
            ciphertext: b"sealed-ipfs-offer".to_vec(),
        };
        alice_room.send_signal(bob.clone(), offer.clone()).await?;
        let bob_signals = bob_room.take_signals().await?;
        assert!(bob_signals.iter().any(|signal| signal.payload == offer));

        bob_room
            .broadcast_control(OpaqueSignalingPayload::new(b"sealed-control-bob".to_vec())?)
            .await?;
        let alice_controls = alice_room.take_control_payloads().await?;
        assert!(alice_controls
            .iter()
            .any(|control| control.from_peer == bob));

        alice_room.leave().await?;
        bob_room.leave().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    async fn ipfs_pubsub_direct_topic_peer_multiaddr_roundtrip() -> Result<(), TransportError> {
        let adapter = IpfsPubsubProviderAdapter;
        let alice = SignalingPeerId::new("alice-device")?;
        let bob = SignalingPeerId::new("bob-device")?;
        let scope = crate::ConversationScope::new(
            ConnectivityScopeLevel::Group,
            derive_scope_commitment(
                ConnectivityScopeLevel::Group,
                b"ipfs direct topic peer",
                "test",
            ),
        )?;
        let capability = RendezvousCapability::derive(
            scope.clone(),
            SignalingAdapterKind::IpfsPubsub,
            b"bootstrap secret with more than thirty two bytes",
            b"random entropy bytes",
            120,
            ProviderMetadataPosture::HashedTopic,
            AdapterTrustLabel::new("ipfs_pubsub", "direct topic-peer rust-libp2p gossipsub")?,
        )?;

        let alice_profile = SignalingAdapterProfile {
            profile_id: "ipfs-alice-direct-topic-peer".to_owned(),
            kind: SignalingAdapterKind::IpfsPubsub,
            endpoints: vec![SignalingProviderEndpoint::new(
                Endpoint::new("/ip4/127.0.0.1/tcp/0"),
                SignalingEndpointSecurity::LocalDevLoopback,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new(
                "ipfs_pubsub",
                "local listener for direct topic peer",
            )?,
        };
        let alice_room = adapter
            .connect(alice_profile)
            .await?
            .join(scope.clone(), capability.clone(), alice.clone())
            .await?;
        let alice_topic_peer = alice_room
            .direct_topic_peer_multiaddrs_for_tests()
            .into_iter()
            .next()
            .ok_or_else(|| {
                TransportError::SignalingAdapter(
                    "missing alice /p2p topic peer multiaddr".to_owned(),
                )
            })?;
        assert!(alice_topic_peer.contains("/p2p/"));

        let bob_profile = SignalingAdapterProfile {
            profile_id: "ipfs-bob-direct-topic-peer".to_owned(),
            kind: SignalingAdapterKind::IpfsPubsub,
            endpoints: vec![SignalingProviderEndpoint::new(
                Endpoint::new(alice_topic_peer),
                SignalingEndpointSecurity::SelfHostedExplicit,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new("ipfs_pubsub", "explicit direct topic peer")?,
        };
        let bob_room = adapter
            .connect(bob_profile)
            .await?
            .join(scope, capability, bob.clone())
            .await?;

        tokio::time::sleep(Duration::from_secs(2)).await;
        bob_room
            .publish_presence(
                OpaqueSignalingPayload::new(b"sealed-presence-bob-direct-topic-peer".to_vec())?,
                120,
            )
            .await?;
        let alice_presence = alice_room.subscribe_presence().await?;
        assert!(alice_presence.iter().any(|event| event.peer_id == bob));

        let offer = SealedWebRtcNegotiationPayload {
            version: 1,
            kind: WebRtcNegotiationPayloadKind::Offer,
            nonce: [3; 12],
            ciphertext: b"sealed-ipfs-direct-topic-peer-offer".to_vec(),
        };
        alice_room.send_signal(bob.clone(), offer.clone()).await?;
        let bob_signals = bob_room.take_signals().await?;
        assert!(bob_signals.iter().any(|signal| signal.payload == offer));

        alice_room.leave().await?;
        bob_room.leave().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    async fn ipfs_pubsub_unreachable_bootstrap_maps_to_typed_health() -> Result<(), TransportError>
    {
        let adapter = IpfsPubsubProviderAdapter;
        let peer = SignalingPeerId::new("alice-device")?;
        let scope = crate::ConversationScope::new(
            ConnectivityScopeLevel::Dm,
            derive_scope_commitment(
                ConnectivityScopeLevel::Dm,
                b"ipfs unreachable bootstrap",
                "test",
            ),
        )?;
        let capability = RendezvousCapability::derive(
            scope.clone(),
            SignalingAdapterKind::IpfsPubsub,
            b"bootstrap secret with more than thirty two bytes",
            b"random entropy bytes",
            120,
            ProviderMetadataPosture::HashedTopic,
            AdapterTrustLabel::new("ipfs_pubsub", "unreachable bootstrap test")?,
        )?;
        let profile = SignalingAdapterProfile {
            profile_id: "ipfs-unreachable-bootstrap".to_owned(),
            kind: SignalingAdapterKind::IpfsPubsub,
            endpoints: vec![SignalingProviderEndpoint::new(
                Endpoint::new(
                    "/ip4/127.0.0.1/tcp/9/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
                ),
                SignalingEndpointSecurity::LocalDevLoopback,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new("ipfs_pubsub", "unreachable bootstrap test")?,
        };

        let error = match adapter
            .connect(profile)
            .await?
            .join(scope, capability, peer)
            .await
        {
            Ok(_) => panic!("unreachable bootstrap must fail with typed health"),
            Err(error) => error,
        };
        let TransportError::SignalingAdapter(message) = error else {
            panic!("expected signaling adapter error");
        };
        assert!(message.contains("bootstrap_connect"));
        assert!(message.contains("failure_class=provider_unhealthy"));
        assert!(message.contains("health_state=ProviderUnhealthy"));
        Ok(())
    }

    #[test]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    fn ipfs_pubsub_insufficient_peers_reports_actionable_topic_mesh_error() {
        let error = ipfs_pubsub_no_topic_mesh_error(
            libp2p::gossipsub::PublishError::NoPeersSubscribedToTopic,
            1,
            0,
            0,
        );

        let TransportError::SignalingAdapter(message) = error else {
            panic!("expected signaling adapter error");
        };
        assert!(message.contains("topic_mesh_unavailable"));
        assert!(message.contains("failure_class=provider_unhealthy"));
        assert!(message.contains("health_state=ProviderUnhealthy"));
        assert!(message.contains("connected_peers=1"));
        assert!(message.contains("subscribed_topic_peers=0"));
        assert!(message.contains("generic libp2p bootstrap peers do not relay"));
        assert!(message.contains("reachable Discrypt/IPFS pubsub peer"));
    }

    #[test]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    fn ipfs_pubsub_oversized_envelope_maps_to_typed_health() {
        let envelope = IpfsPubsubWireEnvelope::Control {
            schema: IPFS_PUBSUB_EVENT_SCHEMA,
            from_peer: SignalingPeerId::new("alice-device").expect("peer id"),
            payload: OpaqueSignalingPayload::new(vec![b'x'; IPFS_PUBSUB_MAX_MESSAGE_BYTES + 1])
                .expect("opaque payload"),
        };
        let error = ipfs_encode_envelope(&envelope).expect_err("oversized envelope must fail");
        let TransportError::SignalingAdapter(message) = error else {
            panic!("expected signaling adapter error");
        };
        assert!(message.contains("envelope_size"));
        assert!(message.contains("failure_class=provider_message_too_large"));
        assert!(message.contains("health_state=ProviderMessageTooLarge"));
        assert!(message.contains("max=65536"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    async fn ipfs_pubsub_duplicate_storm_maps_to_typed_health() -> Result<(), TransportError> {
        let local_peer_id = SignalingPeerId::new("bob-device")?;
        let envelope = IpfsPubsubWireEnvelope::Signal {
            schema: IPFS_PUBSUB_EVENT_SCHEMA,
            from_peer: SignalingPeerId::new("alice-device")?,
            to_peer: local_peer_id.clone(),
            payload: SealedWebRtcNegotiationPayload {
                version: 1,
                kind: WebRtcNegotiationPayloadKind::Offer,
                nonce: [9; 12],
                ciphertext: b"sealed-ipfs-duplicate-storm-offer".to_vec(),
            },
        };
        let bytes = ipfs_encode_envelope(&envelope)?;
        let inbox = Arc::new(AsyncMutex::new(IpfsPubsubInbox::default()));

        IpfsPubsubProviderRoom::record_message(&inbox, &local_peer_id, bytes.clone()).await?;
        {
            let inbox = inbox.lock().await;
            assert_eq!(inbox.signals.len(), 1);
            assert!(inbox.health_faults.is_empty());
        }

        for _ in 1..IPFS_PUBSUB_DUPLICATE_STORM_THRESHOLD {
            IpfsPubsubProviderRoom::record_message(&inbox, &local_peer_id, bytes.clone()).await?;
        }

        let error = IpfsPubsubProviderRoom::record_message(&inbox, &local_peer_id, bytes)
            .await
            .expect_err("duplicate storm must fail with typed provider health");
        let TransportError::SignalingAdapter(message) = error else {
            panic!("expected signaling adapter error");
        };
        assert!(message.contains("duplicate_storm"));
        assert!(message.contains("failure_class=provider_unhealthy"));
        assert!(message.contains("health_state=ProviderUnhealthy"));
        assert!(message.contains(&format!(
            "threshold={IPFS_PUBSUB_DUPLICATE_STORM_THRESHOLD}"
        )));

        let inbox = inbox.lock().await;
        assert_eq!(inbox.signals.len(), 1, "duplicates must not redeliver");
        assert_eq!(inbox.health_faults.len(), 1);
        Ok(())
    }

    #[test]
    #[cfg(feature = "ipfs-pubsub-adapter")]
    fn ipfs_pubsub_swarm_runtime_errors_map_to_typed_health() {
        let error =
            ipfs_swarm_provider_unhealthy_error("listener_error", "permission denied on listener");
        let TransportError::SignalingAdapter(message) = error else {
            panic!("expected signaling adapter error");
        };
        assert!(message.contains("listener_error"));
        assert!(message.contains("failure_class=provider_unhealthy"));
        assert!(message.contains("health_state=ProviderUnhealthy"));
        assert!(message.contains("libp2p swarm reported provider/runtime failure"));
    }

    #[tokio::test]
    #[cfg(not(feature = "discrypt-quic-rendezvous-adapter"))]
    async fn quic_rendezvous_feature_gate_remains_fail_closed_until_sibling_client_is_wired(
    ) -> Result<(), TransportError> {
        let boundary = adapter_boundary_for_kind(SignalingAdapterKind::DiscryptQuicRendezvous);
        assert_eq!(
            boundary.readiness,
            ProviderAdapterReadiness::ImplementationUnavailable
        );
        assert_eq!(boundary.failure_class(), "implementation_unavailable");
        assert!(
            !SignalingAdapterFactory::for_kind(SignalingAdapterKind::DiscryptQuicRendezvous)
                .selectable()
        );

        let plan = plan_signaling_adapter_fallback(
            &[SignalingAdapterKind::DiscryptQuicRendezvous],
            AdapterFallbackBehavior::ManualOnly,
            Some(SignalingAdapterKind::DiscryptQuicRendezvous),
        );
        assert_eq!(plan.selected, None);
        assert_eq!(plan.attempts.len(), 1);
        assert_eq!(
            plan.attempts[0].readiness,
            AdapterReadinessState::ImplementationUnavailable
        );
        assert!(!plan.attempts[0].selected);

        let adapter =
            FeatureGatedProviderAdapter::new(SignalingAdapterKind::DiscryptQuicRendezvous);
        let error = adapter
            .connect(valid_profile(SignalingAdapterKind::DiscryptQuicRendezvous)?)
            .await;
        assert!(matches!(error, Err(TransportError::SignalingAdapter(_))));
        let message = error
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(message.contains("discrypt_quic_rendezvous"));
        assert!(message.contains("no audited production provider client is wired"));
        Ok(())
    }

    #[tokio::test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    async fn quic_rendezvous_feature_gate_is_selectable_but_rejects_reserved_native_quic_scheme(
    ) -> Result<(), TransportError> {
        let boundary = adapter_boundary_for_kind(SignalingAdapterKind::DiscryptQuicRendezvous);
        assert_eq!(
            boundary.readiness,
            ProviderAdapterReadiness::ImplementationAvailable
        );
        assert_eq!(boundary.failure_class(), "implementation_available");
        assert!(
            SignalingAdapterFactory::for_kind(SignalingAdapterKind::DiscryptQuicRendezvous)
                .selectable()
        );

        let plan = plan_signaling_adapter_fallback(
            &[SignalingAdapterKind::DiscryptQuicRendezvous],
            AdapterFallbackBehavior::ManualOnly,
            Some(SignalingAdapterKind::DiscryptQuicRendezvous),
        );
        assert_eq!(
            plan.selected,
            Some(SignalingAdapterKind::DiscryptQuicRendezvous)
        );
        assert_eq!(plan.attempts[0].readiness, AdapterReadinessState::Available);
        assert!(plan.attempts[0].selected);

        let adapter = DiscryptQuicRendezvousProviderAdapter;
        let error = adapter
            .connect(valid_profile(SignalingAdapterKind::DiscryptQuicRendezvous)?)
            .await
            .expect_err("native quic:// is still reserved by the sibling service ADR");
        assert!(error
            .to_string()
            .contains("native quic:// transport remains reserved"));
        Ok(())
    }

    #[tokio::test]
    #[cfg(not(feature = "discrypt-quic-rendezvous-adapter"))]
    async fn provider_adapter_roundtrip_probe_quic_fails_closed() -> Result<(), TransportError> {
        let scope = crate::ConversationScope::new(
            ConnectivityScopeLevel::Dm,
            derive_scope_commitment(ConnectivityScopeLevel::Dm, b"quic probe dm", "test"),
        )?;
        let error = probe_provider_adapter_roundtrip(
            valid_profile(SignalingAdapterKind::DiscryptQuicRendezvous)?,
            scope,
            b"bootstrap secret with more than thirty two bytes",
            b"random entropy bytes",
        )
        .await
        .expect_err("QUIC provider probe must fail closed until sibling client is wired");
        assert!(error
            .to_string()
            .contains("discrypt_quic_rendezvous adapter"));
        Ok(())
    }

    #[tokio::test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    async fn provider_adapter_roundtrip_probe_quic_rejects_reserved_native_scheme(
    ) -> Result<(), TransportError> {
        let scope = crate::ConversationScope::new(
            ConnectivityScopeLevel::Dm,
            derive_scope_commitment(ConnectivityScopeLevel::Dm, b"quic probe dm", "test"),
        )?;
        let error = probe_provider_adapter_roundtrip(
            valid_profile(SignalingAdapterKind::DiscryptQuicRendezvous)?,
            scope,
            b"bootstrap secret with more than thirty two bytes",
            b"random entropy bytes",
        )
        .await
        .expect_err("native quic:// provider probe must stay reserved until native QUIC lands");
        assert!(error
            .to_string()
            .contains("native quic:// transport remains reserved"));
        Ok(())
    }

    #[tokio::test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    async fn quic_rendezvous_rejects_https_endpoint_without_signed_trust_fingerprint(
    ) -> Result<(), TransportError> {
        let adapter = DiscryptQuicRendezvousProviderAdapter;
        let profile = SignalingAdapterProfile {
            profile_id: "production-rendezvous-no-trust".to_owned(),
            kind: SignalingAdapterKind::DiscryptQuicRendezvous,
            endpoints: vec![SignalingProviderEndpoint::new(
                Endpoint::new("https://rendezvous.example.invalid"),
                SignalingEndpointSecurity::ProductionTls,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new(
                "discrypt_quic_rendezvous",
                "production service without signed trust",
            )?,
        };

        let error = adapter
            .connect(profile)
            .await
            .expect_err("production rendezvous endpoint must require signed trust");
        assert!(error
            .to_string()
            .contains("requires signed endpoint trust fingerprint"));
        Ok(())
    }

    #[tokio::test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    async fn quic_rendezvous_rejects_mismatched_signed_trust_fingerprint(
    ) -> Result<(), TransportError> {
        let adapter = DiscryptQuicRendezvousProviderAdapter;
        let endpoint = "https://rendezvous.example.invalid";
        let mut provider_endpoint = SignalingProviderEndpoint::new(
            Endpoint::new(endpoint),
            SignalingEndpointSecurity::ProductionTls,
        );
        provider_endpoint.trust_fingerprint = Some(
            discrypt_rendezvous_trust_fingerprint_for_endpoint("https://evil.example.invalid"),
        );
        let profile = SignalingAdapterProfile {
            profile_id: "production-rendezvous-wrong-trust".to_owned(),
            kind: SignalingAdapterKind::DiscryptQuicRendezvous,
            endpoints: vec![provider_endpoint],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new(
                "discrypt_quic_rendezvous",
                "production service with mismatched trust",
            )?,
        };

        let error = adapter
            .connect(profile)
            .await
            .expect_err("mismatched rendezvous trust must fail before health probe");
        assert!(error
            .to_string()
            .contains("endpoint trust fingerprint mismatch"));
        Ok(())
    }

    #[test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    fn quic_rendezvous_health_requires_matching_public_base_for_production() {
        let trust_fingerprint = discrypt_rendezvous_trust_fingerprint_for_endpoint(
            "https://rendezvous.example.invalid",
        );
        let health = DiscryptRendezvousHealthResponse {
            schema_version: Some(DISCRYPT_RENDEZVOUS_HEALTH_SCHEMA_VERSION),
            protocol_version: Some(DISCRYPT_RENDEZVOUS_HEALTH_PROTOCOL_VERSION.to_owned()),
            status: "ok".to_owned(),
            service: "discrypt-rendezvous".to_owned(),
            public_base_url: "https://other.example.invalid".to_owned(),
            max_body_bytes: Some(64 * 1024),
            rate_limit_window_seconds: Some(60),
            rate_limit_max_requests: Some(120),
            service_identity_fingerprint: Some(trust_fingerprint.clone()),
            tls_alpn_protocols: vec!["h2".to_owned()],
            service_expires_at: Some(chrono::Utc::now() + chrono::Duration::days(1)),
            rotation_policy: Some("rotate before service_expires_at".to_owned()),
            endpoint_allowlist_commitment: Some(trust_fingerprint.clone()),
            at_rest_records: 0,
        };
        let error = discrypt_rendezvous_validate_health(
            "https://rendezvous.example.invalid",
            SignalingEndpointSecurity::ProductionTls,
            Some(&trust_fingerprint),
            &health,
        )
        .expect_err("production health must match selected endpoint");
        assert!(error.to_string().contains("public_base_url mismatch"));
    }

    #[test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    fn quic_rendezvous_health_accepts_signed_identity_and_rotation_metadata(
    ) -> Result<(), TransportError> {
        let endpoint = "https://rendezvous.example.invalid";
        let trust_fingerprint = discrypt_rendezvous_trust_fingerprint_for_endpoint(endpoint);
        let health = DiscryptRendezvousHealthResponse {
            schema_version: Some(DISCRYPT_RENDEZVOUS_HEALTH_SCHEMA_VERSION),
            protocol_version: Some(DISCRYPT_RENDEZVOUS_HEALTH_PROTOCOL_VERSION.to_owned()),
            status: "ok".to_owned(),
            service: "discrypt-rendezvous".to_owned(),
            public_base_url: endpoint.to_owned(),
            max_body_bytes: Some(64 * 1024),
            rate_limit_window_seconds: Some(60),
            rate_limit_max_requests: Some(120),
            service_identity_fingerprint: Some(trust_fingerprint.clone()),
            tls_alpn_protocols: vec!["h2".to_owned()],
            service_expires_at: Some(chrono::Utc::now() + chrono::Duration::days(1)),
            rotation_policy: Some("rotate before service_expires_at".to_owned()),
            endpoint_allowlist_commitment: Some(trust_fingerprint.clone()),
            at_rest_records: 0,
        };
        discrypt_rendezvous_validate_health(
            endpoint,
            SignalingEndpointSecurity::ProductionTls,
            Some(&trust_fingerprint),
            &health,
        )
    }

    #[test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    fn quic_rendezvous_health_allows_local_loopback_public_base_mismatch(
    ) -> Result<(), TransportError> {
        let health = DiscryptRendezvousHealthResponse {
            schema_version: None,
            protocol_version: None,
            status: "ok".to_owned(),
            service: "discrypt-rendezvous".to_owned(),
            public_base_url: "https://127.0.0.1/rendezvous-test".to_owned(),
            max_body_bytes: None,
            rate_limit_window_seconds: None,
            rate_limit_max_requests: None,
            service_identity_fingerprint: None,
            tls_alpn_protocols: Vec::new(),
            service_expires_at: None,
            rotation_policy: None,
            endpoint_allowlist_commitment: None,
            at_rest_records: 0,
        };
        discrypt_rendezvous_validate_health(
            "http://127.0.0.1:18787",
            SignalingEndpointSecurity::LocalDevLoopback,
            None,
            &health,
        )
    }

    #[test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    fn quic_rendezvous_health_rejects_non_ok_status() {
        let trust_fingerprint = discrypt_rendezvous_trust_fingerprint_for_endpoint(
            "https://rendezvous.example.invalid",
        );
        let health = DiscryptRendezvousHealthResponse {
            schema_version: Some(DISCRYPT_RENDEZVOUS_HEALTH_SCHEMA_VERSION),
            protocol_version: Some(DISCRYPT_RENDEZVOUS_HEALTH_PROTOCOL_VERSION.to_owned()),
            status: "degraded".to_owned(),
            service: "discrypt-rendezvous".to_owned(),
            public_base_url: "https://rendezvous.example.invalid".to_owned(),
            max_body_bytes: Some(64 * 1024),
            rate_limit_window_seconds: Some(60),
            rate_limit_max_requests: Some(120),
            service_identity_fingerprint: Some(trust_fingerprint.clone()),
            tls_alpn_protocols: vec!["h2".to_owned()],
            service_expires_at: Some(chrono::Utc::now() + chrono::Duration::days(1)),
            rotation_policy: Some("rotate before service_expires_at".to_owned()),
            endpoint_allowlist_commitment: Some(trust_fingerprint.clone()),
            at_rest_records: 0,
        };
        let error = discrypt_rendezvous_validate_health(
            "https://rendezvous.example.invalid",
            SignalingEndpointSecurity::ProductionTls,
            Some(&trust_fingerprint),
            &health,
        )
        .expect_err("non-ok health must fail");
        assert!(error.to_string().contains("health is not ok"));
    }

    #[test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    fn quic_rendezvous_health_requires_production_protocol_metadata() {
        let trust_fingerprint = discrypt_rendezvous_trust_fingerprint_for_endpoint(
            "https://rendezvous.example.invalid",
        );
        let health = DiscryptRendezvousHealthResponse {
            schema_version: None,
            protocol_version: None,
            status: "ok".to_owned(),
            service: "discrypt-rendezvous".to_owned(),
            public_base_url: "https://rendezvous.example.invalid".to_owned(),
            max_body_bytes: None,
            rate_limit_window_seconds: None,
            rate_limit_max_requests: None,
            service_identity_fingerprint: Some(trust_fingerprint.clone()),
            tls_alpn_protocols: vec!["h2".to_owned()],
            service_expires_at: Some(chrono::Utc::now() + chrono::Duration::days(1)),
            rotation_policy: Some("rotate before service_expires_at".to_owned()),
            endpoint_allowlist_commitment: Some(trust_fingerprint.clone()),
            at_rest_records: 0,
        };
        let error = discrypt_rendezvous_validate_health(
            "https://rendezvous.example.invalid",
            SignalingEndpointSecurity::ProductionTls,
            Some(&trust_fingerprint),
            &health,
        )
        .expect_err("production health must include schema metadata");
        assert!(error.to_string().contains("schema_version"));
    }

    #[test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    fn quic_rendezvous_health_rejects_unsafe_max_body_policy() {
        let trust_fingerprint = discrypt_rendezvous_trust_fingerprint_for_endpoint(
            "https://rendezvous.example.invalid",
        );
        let health = DiscryptRendezvousHealthResponse {
            schema_version: Some(DISCRYPT_RENDEZVOUS_HEALTH_SCHEMA_VERSION),
            protocol_version: Some(DISCRYPT_RENDEZVOUS_HEALTH_PROTOCOL_VERSION.to_owned()),
            status: "ok".to_owned(),
            service: "discrypt-rendezvous".to_owned(),
            public_base_url: "https://rendezvous.example.invalid".to_owned(),
            max_body_bytes: Some(DISCRYPT_RENDEZVOUS_MAX_MAX_BODY_BYTES + 1),
            rate_limit_window_seconds: Some(60),
            rate_limit_max_requests: Some(120),
            service_identity_fingerprint: Some(trust_fingerprint.clone()),
            tls_alpn_protocols: vec!["h2".to_owned()],
            service_expires_at: Some(chrono::Utc::now() + chrono::Duration::days(1)),
            rotation_policy: Some("rotate before service_expires_at".to_owned()),
            endpoint_allowlist_commitment: Some(trust_fingerprint.clone()),
            at_rest_records: 0,
        };
        let error = discrypt_rendezvous_validate_health(
            "https://rendezvous.example.invalid",
            SignalingEndpointSecurity::ProductionTls,
            Some(&trust_fingerprint),
            &health,
        )
        .expect_err("production health must keep max body bounded");
        assert!(error.to_string().contains("max_body_bytes"));
    }

    #[tokio::test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
    async fn discrypt_rendezvous_sibling_service_roundtrip_when_binary_is_available(
    ) -> Result<(), TransportError> {
        let server_bin = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join("discrypt-signaling/target/debug/discrypt-signaling-server");
        if !server_bin.exists() {
            eprintln!(
                "skipping sibling service roundtrip: {} is not built",
                server_bin.display()
            );
            return Ok(());
        }
        let probe_socket = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|error| TransportError::Io(error.to_string()))?;
        let port = probe_socket
            .local_addr()
            .map_err(|error| TransportError::Io(error.to_string()))?
            .port();
        drop(probe_socket);
        let endpoint = format!("http://127.0.0.1:{port}");
        let mut child = std::process::Command::new(&server_bin)
            .args([
                "--bind",
                &format!("127.0.0.1:{port}"),
                "--public-base-url",
                "https://127.0.0.1/rendezvous-test",
                "--name",
                "discrypt-rendezvous-adapter-test",
            ])
            .spawn()
            .map_err(|error| TransportError::Io(error.to_string()))?;

        let health_url = format!("{endpoint}/healthz");
        let mut healthy = false;
        for _ in 0..30 {
            if ureq::get(&health_url).call().is_ok() {
                healthy = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if !healthy {
            let _ = child.kill();
            let _ = child.wait();
            return Err(TransportError::SignalingAdapter(
                "sibling signaling service did not become healthy".to_owned(),
            ));
        }

        let scope = crate::ConversationScope::new(
            ConnectivityScopeLevel::Dm,
            derive_scope_commitment(
                ConnectivityScopeLevel::Dm,
                b"quic sibling service dm",
                "test",
            ),
        )?;
        let profile = SignalingAdapterProfile {
            profile_id: "local-discrypt-rendezvous-service".to_owned(),
            kind: SignalingAdapterKind::DiscryptQuicRendezvous,
            endpoints: vec![SignalingProviderEndpoint::new(
                Endpoint::new(endpoint),
                SignalingEndpointSecurity::LocalDevLoopback,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new(
                "discrypt_quic_rendezvous",
                "local sibling service binary",
            )?,
        };
        let probe = probe_provider_adapter_roundtrip(
            profile,
            scope,
            b"bootstrap secret with more than thirty two bytes",
            b"random entropy bytes",
        )
        .await;
        let _ = child.kill();
        let _ = child.wait();
        let probe = probe?;
        assert!(probe.presence_roundtrip);
        assert!(probe.signal_roundtrip);
        assert!(probe.control_roundtrip);
        Ok(())
    }

    #[tokio::test]
    async fn feature_gated_session_and_room_methods_fail_closed() -> Result<(), TransportError> {
        let boundary = adapter_boundary_for_kind(SignalingAdapterKind::Mqtt);
        let session = FeatureGatedProviderSession { boundary };
        let room = FeatureGatedProviderRoom { boundary };
        let scope = crate::ConversationScope::new(
            ConnectivityScopeLevel::Dm,
            derive_scope_commitment(ConnectivityScopeLevel::Dm, b"feature gated dm", "test"),
        )?;
        let capability = RendezvousCapability::derive(
            scope.clone(),
            SignalingAdapterKind::Mqtt,
            b"bootstrap secret with more than thirty two bytes",
            b"random entropy bytes",
            120,
            ProviderMetadataPosture::HashedTopic,
            AdapterTrustLabel::new("mqtt", "redacted")?,
        )?;
        let peer = SignalingPeerId::new("alice-device")?;

        assert!(matches!(
            session.join(scope, capability, peer.clone()).await,
            Err(TransportError::SignalingAdapter(_))
        ));
        assert_eq!(
            session.health().await.failure_class,
            Some(boundary.failure_class().to_owned())
        );
        session.close().await?;

        let opaque = OpaqueSignalingPayload::new(b"opaque sealed payload".to_vec())?;
        assert!(matches!(
            room.publish_presence(opaque.clone(), 120).await,
            Err(TransportError::SignalingAdapter(_))
        ));
        assert!(matches!(
            room.subscribe_presence().await,
            Err(TransportError::SignalingAdapter(_))
        ));
        assert!(matches!(
            room.send_signal(
                peer,
                SealedWebRtcNegotiationPayload {
                    version: 1,
                    kind: WebRtcNegotiationPayloadKind::Offer,
                    nonce: [9; 12],
                    ciphertext: b"ciphertext".to_vec(),
                },
            )
            .await,
            Err(TransportError::SignalingAdapter(_))
        ));
        assert!(matches!(
            room.take_signals().await,
            Err(TransportError::SignalingAdapter(_))
        ));
        assert!(matches!(
            room.broadcast_control(opaque).await,
            Err(TransportError::SignalingAdapter(_))
        ));
        assert!(matches!(
            room.take_control_payloads().await,
            Err(TransportError::SignalingAdapter(_))
        ));
        assert!(matches!(
            room.leave().await,
            Err(TransportError::SignalingAdapter(_))
        ));
        Ok(())
    }

    #[test]
    fn text_control_runtime_attachment_seam_reports_missing_implementation() {
        let attach_request = ProviderTextControlRuntimeAttachment {
            adapter_kind: "mqtt".to_owned(),
            profile_id: "unit-test-profile".to_owned(),
            endpoint_label: "unit-test-endpoint".to_owned(),
            rendezvous_topic: "unit-test-topic".to_owned(),
            scope_commitment: "unit-test-scope".to_owned(),
            runtime_spec: None,
        };
        let error = match resume_text_control_runtime_from_probe(attach_request) {
            Ok(_) => panic!("runtime attachment path must remain unimplemented"),
            Err(error) => error,
        };
        assert!(error
            .to_string()
            .contains(TEXT_CONTROL_RUNTIME_SPEC_MISSING_MESSAGE));
    }

    #[test]
    fn runtime_spec_from_probe_serializes_and_stays_fail_closed_without_handoff_material() {
        let probe = ProviderWebRtcDataChannelProbe {
            kind: SignalingAdapterKind::Mqtt,
            profile_id: "unit-test-profile".to_owned(),
            endpoint_label: "mqtt://redacted".to_owned(),
            scope_commitment: "scope".to_owned(),
            rendezvous_topic: "topic".to_owned(),
            offerer_direct_path_ready: true,
            answerer_direct_path_ready: true,
            offerer_turn_fallback_ready: false,
            answerer_turn_fallback_ready: false,
            offerer_configured_turn_servers: 0,
            answerer_configured_turn_servers: 0,
            offerer_local_relay_candidates_gathered: 0,
            answerer_local_relay_candidates_gathered: 0,
            offerer_remote_relay_candidates_applied: 0,
            answerer_remote_relay_candidates_applied: 0,
            offerer_data_channel_open: true,
            answerer_data_channel_open: true,
            text_control_frame_roundtrip: true,
            text_control_frame_sha256: "text-sha".to_owned(),
            receipt_frame_roundtrip: true,
            receipt_frame_sha256: "receipt-sha".to_owned(),
            runtime_spec: None,
        };
        let spec = ProviderTextControlRuntimeSpec::from_probe_without_negotiation_material(
            &probe, 100, 60,
        );
        let encoded = serde_json::to_string(&spec).expect("serialize runtime spec");
        assert!(encoded.contains("unit-test-profile"));
        assert!(!encoded.contains("text-sha"));
        let decoded: ProviderTextControlRuntimeSpec =
            serde_json::from_str(&encoded).expect("decode runtime spec");
        assert_eq!(
            decoded.schema_version,
            PROVIDER_TEXT_CONTROL_RUNTIME_SPEC_SCHEMA_VERSION
        );
        assert_eq!(decoded.attachment.adapter_kind, "mqtt");
        assert_eq!(decoded.expires_at_unix_seconds, 160);
        let error = decoded
            .validate_for_runtime_attach(120, &decoded.attachment)
            .expect_err("missing offer/answer/ICE must fail closed");
        assert!(error.to_string().contains("missing negotiated material"));
    }

    #[test]
    fn runtime_spec_rejects_stale_handoff_before_runtime_attach() {
        let spec = ProviderTextControlRuntimeSpec {
            schema_version: PROVIDER_TEXT_CONTROL_RUNTIME_SPEC_SCHEMA_VERSION,
            attachment: ProviderTextControlRuntimeAttachment {
                adapter_kind: "mqtt".to_owned(),
                profile_id: "unit-test-profile".to_owned(),
                endpoint_label: "endpoint".to_owned(),
                rendezvous_topic: "topic".to_owned(),
                scope_commitment: "unit-scope".to_owned(),
                runtime_spec: None,
            },
            created_at_unix_seconds: 10,
            expires_at_unix_seconds: 20,
            sealed_offer: None,
            sealed_answer: None,
            sealed_ice_candidates: Vec::new(),
            missing_material: Vec::new(),
        };
        let error = spec
            .validate_for_runtime_attach(21, &spec.attachment)
            .expect_err("expired spec must fail before runtime attach");
        assert!(error.to_string().contains("stale"));
    }

    #[test]
    fn runtime_spec_rejects_incompatible_attachment() {
        let mut attachment = ProviderTextControlRuntimeAttachment {
            adapter_kind: "mqtt".to_owned(),
            profile_id: "unit-test-profile".to_owned(),
            endpoint_label: "endpoint".to_owned(),
            rendezvous_topic: "topic".to_owned(),
            scope_commitment: "unit-scope".to_owned(),
            runtime_spec: None,
        };
        let now_unix_seconds = Utc::now().timestamp();
        let spec = ProviderTextControlRuntimeSpec {
            schema_version: PROVIDER_TEXT_CONTROL_RUNTIME_SPEC_SCHEMA_VERSION,
            attachment: attachment.clone(),
            created_at_unix_seconds: now_unix_seconds,
            expires_at_unix_seconds: now_unix_seconds.saturating_add(3600),
            sealed_offer: Some(SealedWebRtcNegotiationPayload {
                version: 1,
                kind: WebRtcNegotiationPayloadKind::Offer,
                nonce: [7; 12],
                ciphertext: b"offer".to_vec(),
            }),
            sealed_answer: Some(SealedWebRtcNegotiationPayload {
                version: 1,
                kind: WebRtcNegotiationPayloadKind::Answer,
                nonce: [8; 12],
                ciphertext: b"answer".to_vec(),
            }),
            sealed_ice_candidates: Vec::new(),
            missing_material: Vec::new(),
        };
        attachment.adapter_kind = "nostr".to_owned();
        let error = spec
            .validate_for_runtime_attach(150, &attachment)
            .expect_err("incompatible adapter must fail before runtime attach");
        assert!(error
            .to_string()
            .contains(TEXT_CONTROL_RUNTIME_SPEC_INCOMPATIBLE_MESSAGE));
    }

    #[test]
    fn valid_runtime_spec_still_fails_closed_until_runtime_factory_exists() {
        let attachment = ProviderTextControlRuntimeAttachment {
            adapter_kind: "mqtt".to_owned(),
            profile_id: "unit-test-profile".to_owned(),
            endpoint_label: "endpoint".to_owned(),
            rendezvous_topic: "topic".to_owned(),
            scope_commitment: "unit-scope".to_owned(),
            runtime_spec: None,
        };
        let now_unix_seconds = Utc::now().timestamp();
        let spec = ProviderTextControlRuntimeSpec {
            schema_version: PROVIDER_TEXT_CONTROL_RUNTIME_SPEC_SCHEMA_VERSION,
            attachment: attachment.clone(),
            created_at_unix_seconds: now_unix_seconds,
            expires_at_unix_seconds: now_unix_seconds.saturating_add(3600),
            sealed_offer: Some(SealedWebRtcNegotiationPayload {
                version: 1,
                kind: WebRtcNegotiationPayloadKind::Offer,
                nonce: [7; 12],
                ciphertext: b"offer".to_vec(),
            }),
            sealed_answer: Some(SealedWebRtcNegotiationPayload {
                version: 1,
                kind: WebRtcNegotiationPayloadKind::Answer,
                nonce: [8; 12],
                ciphertext: b"answer".to_vec(),
            }),
            sealed_ice_candidates: vec![SealedWebRtcNegotiationPayload {
                version: 1,
                kind: WebRtcNegotiationPayloadKind::Candidate,
                nonce: [9; 12],
                ciphertext: b"candidate".to_vec(),
            }],
            missing_material: Vec::new(),
        };
        let error =
            match resume_text_control_runtime_from_spec(&spec, &attachment, now_unix_seconds + 1) {
                Ok(_) => panic!("validated handoff must not fake a live runtime"),
                Err(error) => error,
            };
        assert!(error
            .to_string()
            .contains(TEXT_CONTROL_RUNTIME_NOT_IMPLEMENTED_MESSAGE));

        let attachment_with_spec = ProviderTextControlRuntimeAttachment {
            runtime_spec: Some(Box::new(spec)),
            ..attachment
        };
        let error = match resume_text_control_runtime_from_probe(attachment_with_spec) {
            Ok(_) => panic!("attachment seam must still fail closed without factory"),
            Err(error) => error,
        };
        assert!(error
            .to_string()
            .contains(TEXT_CONTROL_RUNTIME_NOT_IMPLEMENTED_MESSAGE));
    }

    #[tokio::test]
    async fn local_conformance_adapters_deliver_opaque_dm_payloads_without_plaintext_leaks(
    ) -> Result<(), TransportError> {
        for kind in [
            SignalingAdapterKind::Mqtt,
            SignalingAdapterKind::Nostr,
            SignalingAdapterKind::IpfsPubsub,
            SignalingAdapterKind::DiscryptQuicRendezvous,
        ] {
            let bus = LocalConformanceProviderBus::default();
            let adapter = LocalConformanceProviderAdapter::new(kind, bus.clone());
            let profile = local_profile(kind)?;
            let alice = SignalingPeerId::new("alice-device")?;
            let bob = SignalingPeerId::new("bob-device")?;
            let scope = crate::ConversationScope::new(
                ConnectivityScopeLevel::Dm,
                derive_scope_commitment(
                    ConnectivityScopeLevel::Dm,
                    b"Alice Display and Bob Display private family voice",
                    "local conformance",
                ),
            )?;
            let capability = RendezvousCapability::derive(
                scope.clone(),
                kind,
                b"bootstrap secret with more than thirty two bytes",
                b"random entropy bytes",
                120,
                ProviderMetadataPosture::HashedTopic,
                AdapterTrustLabel::new(kind.canonical_name(), "local conformance")?,
            )?;
            let alice_room = adapter
                .connect(profile.clone())
                .await?
                .join(scope.clone(), capability.clone(), alice.clone())
                .await?;
            let bob_room = adapter
                .connect(profile)
                .await?
                .join(scope, capability, bob.clone())
                .await?;

            alice_room
                .publish_presence(
                    OpaqueSignalingPayload::new(b"sealed-presence-alice".to_vec())?,
                    120,
                )
                .await?;
            bob_room
                .publish_presence(
                    OpaqueSignalingPayload::new(b"sealed-presence-bob".to_vec())?,
                    120,
                )
                .await?;
            let bob_presence = bob_room.subscribe_presence().await?;
            assert!(bob_presence.iter().any(|event| event.peer_id == alice));
            let alice_presence = alice_room.subscribe_presence().await?;
            assert!(alice_presence.iter().any(|event| event.peer_id == bob));

            let sealer = WebRtcNegotiationSealer::new([kind as u8 + 1; 32]);
            let offer = sealer.seal_description(&WebRtcSessionDescription {
                sdp_type: WebRtcSdpType::Offer,
                sdp: "v=0\r\na=ice-ufrag:aliceRaw\r\na=ice-pwd:alicePwd\r\n".to_owned(),
            })?;
            let answer = sealer.seal_description(&WebRtcSessionDescription {
                sdp_type: WebRtcSdpType::Answer,
                sdp: "v=0\r\na=ice-ufrag:bobRaw\r\na=ice-pwd:bobPwd\r\n".to_owned(),
            })?;
            let alice_candidate = sealer.seal_candidate(&WebRtcIceCandidate {
                candidate: "candidate:1 1 UDP 2130706431 192.0.2.10 54400 typ host".to_owned(),
                sdp_mid: Some("0".to_owned()),
                sdp_mline_index: Some(0),
                username_fragment: Some("aliceRaw".to_owned()),
                url: None,
            })?;
            let bob_candidate = sealer.seal_candidate(&WebRtcIceCandidate {
                candidate: "candidate:2 1 UDP 2130706431 192.0.2.11 54401 typ host".to_owned(),
                sdp_mid: Some("0".to_owned()),
                sdp_mline_index: Some(0),
                username_fragment: Some("bobRaw".to_owned()),
                url: None,
            })?;

            alice_room.send_signal(bob.clone(), offer.clone()).await?;
            alice_room
                .send_signal(bob.clone(), alice_candidate.clone())
                .await?;
            bob_room.send_signal(alice.clone(), answer.clone()).await?;
            bob_room
                .send_signal(alice.clone(), bob_candidate.clone())
                .await?;

            let bob_signals = bob_room.take_signals().await?;
            assert_eq!(
                bob_signals
                    .iter()
                    .map(|signal| signal.payload.kind)
                    .collect::<Vec<_>>(),
                vec![
                    WebRtcNegotiationPayloadKind::Offer,
                    WebRtcNegotiationPayloadKind::Candidate,
                ]
            );
            assert_eq!(bob_signals[0].payload, offer);
            assert_eq!(bob_signals[1].payload, alice_candidate);

            let alice_signals = alice_room.take_signals().await?;
            assert_eq!(
                alice_signals
                    .iter()
                    .map(|signal| signal.payload.kind)
                    .collect::<Vec<_>>(),
                vec![
                    WebRtcNegotiationPayloadKind::Answer,
                    WebRtcNegotiationPayloadKind::Candidate,
                ]
            );
            assert_eq!(alice_signals[0].payload, answer);
            assert_eq!(alice_signals[1].payload, bob_candidate);

            alice_room
                .broadcast_control(OpaqueSignalingPayload::new(
                    b"sealed-control-alice".to_vec(),
                )?)
                .await?;
            let bob_controls = bob_room.take_control_payloads().await?;
            assert_eq!(bob_controls.len(), 1);
            assert_eq!(bob_controls[0].from_peer, alice);
            assert_eq!(bob_controls[0].payload.bytes, b"sealed-control-alice");

            assert_no_forbidden_plaintext(&bus.relay_visible_material_for_tests());
            let observability = format!(
                "{:?}{:?}",
                adapter.observability_redacted(),
                adapter.connect(local_profile(kind)?).await?.health().await
            );
            assert_no_forbidden_text(&observability);
        }
        Ok(())
    }

    #[tokio::test]
    async fn local_conformance_adapter_rejects_plaintext_sdp_and_ice_markers(
    ) -> Result<(), TransportError> {
        let bus = LocalConformanceProviderBus::default();
        let adapter = LocalConformanceProviderAdapter::new(SignalingAdapterKind::Mqtt, bus);
        let scope = crate::ConversationScope::new(
            ConnectivityScopeLevel::Dm,
            derive_scope_commitment(
                ConnectivityScopeLevel::Dm,
                b"plaintext rejection dm",
                "test",
            ),
        )?;
        let capability = RendezvousCapability::derive(
            scope.clone(),
            SignalingAdapterKind::Mqtt,
            b"bootstrap secret with more than thirty two bytes",
            b"random entropy bytes",
            120,
            ProviderMetadataPosture::HashedTopic,
            AdapterTrustLabel::new("mqtt", "local conformance")?,
        )?;
        let room = adapter
            .connect(local_profile(SignalingAdapterKind::Mqtt)?)
            .await?
            .join(scope, capability, SignalingPeerId::new("alice-device")?)
            .await?;

        assert!(matches!(
            room.publish_presence(OpaqueSignalingPayload::new(b"Alice Display".to_vec())?, 120)
                .await,
            Err(TransportError::PlaintextLeak)
        ));
        assert!(matches!(
            room.send_signal(
                SignalingPeerId::new("bob-device")?,
                SealedWebRtcNegotiationPayload {
                    version: 1,
                    kind: WebRtcNegotiationPayloadKind::Offer,
                    nonce: [1; 12],
                    ciphertext: b"v=0\r\na=ice-ufrag:raw\r\ncandidate:raw".to_vec(),
                },
            )
            .await,
            Err(TransportError::PlaintextLeak)
        ));
        assert!(matches!(
            room.broadcast_control(OpaqueSignalingPayload::new(b"raw sdp control".to_vec())?)
                .await,
            Err(TransportError::PlaintextLeak)
        ));
        Ok(())
    }

    #[tokio::test]
    async fn live_provider_text_control_runtime_pair_carries_multiple_opaque_frames(
    ) -> Result<(), TransportError> {
        let bus = LocalConformanceProviderBus::default();
        let adapter = LocalConformanceProviderAdapter::new(SignalingAdapterKind::Mqtt, bus.clone());
        let scope = ConversationScope::new(
            ConnectivityScopeLevel::Dm,
            derive_scope_commitment(
                ConnectivityScopeLevel::Dm,
                b"alice bob live runtime pair",
                "runtime pair",
            ),
        )?;
        let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
        let answerer_received = received.clone();
        let runtime = start_provider_webrtc_text_control_runtime_pair_with_adapter(
            adapter,
            local_profile(SignalingAdapterKind::Mqtt)?,
            scope,
            b"runtime pair bootstrap secret with thirty two bytes",
            b"runtime pair entropy",
            WebRtcNegotiationConfig::new(IceServerConfig::new(
                vec![Endpoint::new("stun:127.0.0.1:3478")],
                vec![],
            )?),
            move |frame| {
                answerer_received
                    .lock()
                    .map_err(|_| {
                        TransportError::Unavailable(
                            "runtime pair answerer receipt lock poisoned".to_owned(),
                        )
                    })?
                    .push(frame.clone());
                Ok(format!("ciphertext:runtime-pair-receipt:{}", sha256_hex(&frame)).into_bytes())
            },
        )
        .await?;

        assert!(runtime.evidence().offerer_direct_path_ready);
        assert!(runtime.evidence().answerer_direct_path_ready);
        assert!(runtime.evidence().offerer_data_channel_open);
        assert!(runtime.evidence().answerer_data_channel_open);
        runtime
            .evidence()
            .runtime_spec
            .validate_for_runtime_attach(
                Utc::now().timestamp(),
                &runtime.evidence().runtime_spec.attachment,
            )?;

        let transport = runtime.transport();
        for index in 0..2 {
            let frame = format!("ciphertext:runtime-pair-frame:{index}").into_bytes();
            let expected_receipt =
                format!("ciphertext:runtime-pair-receipt:{}", sha256_hex(&frame)).into_bytes();
            transport.send_text_control_frame(frame).await?;
            let receipt = timeout(Duration::from_secs(5), transport.recv_text_control_frame())
                .await
                .map_err(|_| {
                    TransportError::Unavailable(
                        "timed out receiving runtime pair receipt".to_owned(),
                    )
                })??;
            assert_eq!(receipt, expected_receipt);
        }

        let metrics = transport.text_control_transport_metrics().await;
        assert!(metrics.open);
        assert_eq!(metrics.frames_sent, 2);
        assert_eq!(metrics.frames_received, 2);
        assert_eq!(
            received
                .lock()
                .map_err(|_| TransportError::Unavailable("receipt lock poisoned".to_owned()))?
                .len(),
            2
        );
        for material in bus.relay_visible_material_for_tests() {
            assert!(!material.windows(3).any(|window| window == b"v=0"));
            assert!(!material
                .windows(b"runtime-pair-frame".len())
                .any(|window| window == b"runtime-pair-frame"));
        }

        runtime.close().await?;
        Ok(())
    }

    fn assert_no_forbidden_plaintext(material: &[Vec<u8>]) {
        for bytes in material {
            if let Ok(text) = std::str::from_utf8(bytes) {
                assert_no_forbidden_text(text);
            }
        }
    }

    fn assert_no_forbidden_text(text: &str) {
        let lower = text.to_ascii_lowercase();
        for marker in [
            "alice display",
            "bob display",
            "family voice",
            "v=0",
            "a=ice-ufrag",
            "a=ice-pwd",
            "candidate:",
            "aliceraw",
            "bobraw",
        ] {
            assert!(
                !lower.contains(marker),
                "provider-visible material leaked forbidden marker {marker}: {text}"
            );
        }
    }
}

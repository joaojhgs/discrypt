//! Feature-gated production boundaries for required signaling providers.
//!
//! Each required provider has a concrete adapter boundary that validates
//! profiles, exposes redacted health, and fails closed unless an audited
//! provider client is compiled behind its explicit Cargo feature. MQTT, Nostr,
//! and IPFS/libp2p PubSub now have real provider clients behind explicit
//! adapter features; the Rust QUIC rendezvous adapter remains an explicit
//! fail-closed boundary until its sibling-service client lands.

#[cfg(any(feature = "mqtt-adapter", feature = "ipfs-pubsub-adapter"))]
use crate::SignalingProviderEndpoint;
use crate::{
    AdapterFallbackBehavior, AdapterSession, AdapterTrustLabel, ControlBroadcast,
    ConversationScope, IceServerConfig, OpaqueSignalingPayload, PeerSignal, PresenceEvent,
    RendezvousCapability, RendezvousRoom, SealedWebRtcNegotiationPayload, SignalingAdapter,
    SignalingAdapterCapabilities, SignalingAdapterKind, SignalingAdapterProfile,
    SignalingEndpointSecurity, SignalingHealth, SignalingHealthState, SignalingObservability,
    SignalingPeerId, TransportError,
};
#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter"
))]
use crate::{
    TextControlDataTransport, WebRtcNegotiationConfig, WebRtcNegotiationPayloadKind,
    WebRtcNegotiationSealer, WebRtcNegotiator,
};
use async_trait::async_trait;
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
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter"
))]
use tokio::time::Instant;
#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter"
))]
use tokio::time::{timeout, Duration};

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
    /// Rust QUIC rendezvous boundary until real client is wired.
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
                Self::DiscryptQuicRendezvous(FeatureGatedProviderAdapter::new(kind))
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
        feature = "ipfs-pubsub-adapter"
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
        feature = "ipfs-pubsub-adapter"
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
            Err(adapter.boundary().unavailable_error())
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
    if text_control_frame.is_empty() {
        return Err(TransportError::Unavailable(
            "text/control proof frame must be non-empty opaque bytes".to_owned(),
        ));
    }
    if receipt_control_frame.is_empty() {
        return Err(TransportError::Unavailable(
            "receipt/control proof frame must be non-empty opaque bytes".to_owned(),
        ));
    }
    profile.validate()?;
    scope.validate()?;
    ice_servers.validate_credentials_at(chrono::Utc::now())?;
    let factory = SignalingAdapterFactory::for_kind(profile.kind);
    #[cfg(not(any(
        feature = "mqtt-adapter",
        feature = "nostr-adapter",
        feature = "ipfs-pubsub-adapter"
    )))]
    let _ = (
        bootstrap_secret,
        random_entropy,
        &ice_servers,
        &text_control_frame,
        &receipt_control_frame,
    );
    probe_provider_webrtc_datachannel_request_response_with_factory(
        factory,
        profile,
        scope,
        bootstrap_secret,
        random_entropy,
        ice_servers,
        text_control_frame,
        receipt_control_frame,
    )
    .await
}

async fn probe_provider_webrtc_datachannel_request_response_with_factory(
    factory: SignalingAdapterFactory,
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    ice_servers: IceServerConfig,
    text_control_frame: Vec<u8>,
    receipt_control_frame: Vec<u8>,
) -> Result<ProviderWebRtcDataChannelProbe, TransportError> {
    #[cfg(not(any(
        feature = "mqtt-adapter",
        feature = "nostr-adapter",
        feature = "ipfs-pubsub-adapter"
    )))]
    let _ = (
        profile,
        scope,
        bootstrap_secret,
        random_entropy,
        ice_servers,
        text_control_frame,
        receipt_control_frame,
    );
    match factory {
        SignalingAdapterFactory::Mqtt(adapter) => {
            #[cfg(feature = "mqtt-adapter")]
            {
                probe_webrtc_with_adapter(
                    adapter,
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    ice_servers,
                    text_control_frame,
                    receipt_control_frame,
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
                probe_webrtc_with_adapter(
                    adapter,
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    ice_servers,
                    text_control_frame,
                    receipt_control_frame,
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
                probe_webrtc_with_adapter(
                    adapter,
                    profile,
                    scope,
                    bootstrap_secret,
                    random_entropy,
                    ice_servers,
                    text_control_frame,
                    receipt_control_frame,
                )
                .await
            }
            #[cfg(not(feature = "ipfs-pubsub-adapter"))]
            {
                Err(adapter.boundary().unavailable_error())
            }
        }
        SignalingAdapterFactory::DiscryptQuicRendezvous(adapter) => {
            Err(adapter.boundary().unavailable_error())
        }
    }
}

#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter"
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
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter"
))]
async fn probe_webrtc_with_adapter<A>(
    adapter: A,
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
    ice_servers: IceServerConfig,
    text_control_frame: Vec<u8>,
    receipt_control_frame: Vec<u8>,
) -> Result<ProviderWebRtcDataChannelProbe, TransportError>
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

    let mut alice_config = WebRtcNegotiationConfig::new(ice_servers.clone());
    alice_config.udp_addrs = vec!["0.0.0.0:0".to_owned()];
    let mut bob_config = WebRtcNegotiationConfig::new(ice_servers);
    bob_config.udp_addrs = vec!["0.0.0.0:0".to_owned()];
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
                    bob_room
                        .send_signal(alice.clone(), sealer.seal_description(&answer)?)
                        .await?;
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
            alice_room
                .send_signal(bob.clone(), sealer.seal_candidate(&candidate)?)
                .await?;
        }
        for candidate in bob_webrtc.drain_local_candidates().await {
            bob_room
                .send_signal(alice.clone(), sealer.seal_candidate(&candidate)?)
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

    let receipt_frame = receipt_control_frame;
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
        offerer_data_channel_open: offerer_data.open,
        answerer_data_channel_open: answerer_data.open,
        text_control_frame_roundtrip: frame_roundtrip,
        text_control_frame_sha256: frame_sha256,
        receipt_frame_roundtrip,
        receipt_frame_sha256,
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
    feature = "ipfs-pubsub-adapter"
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
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter"
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
            readiness: feature_readiness(cfg!(feature = "discrypt-quic-rendezvous-adapter")),
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

const fn feature_readiness(enabled: bool) -> ProviderAdapterReadiness {
    if enabled {
        ProviderAdapterReadiness::ImplementationUnavailable
    } else {
        ProviderAdapterReadiness::FeatureDisabled
    }
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
const IPFS_PUBSUB_KAD_QUERY_TIMEOUT_SECS: u64 = 20;

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
        if !inbox.seen_messages.insert(fingerprint) {
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

    let (command_tx, mut command_rx) = tokio::sync::mpsc::channel(IPFS_PUBSUB_COMMAND_QUEUE_DEPTH);
    let (listen_tx, listen_rx) = tokio::sync::oneshot::channel();
    let inbox = Arc::new(AsyncMutex::new(IpfsPubsubInbox::default()));
    let task_inbox = inbox.clone();
    let listen_addresses = Arc::new(AsyncMutex::new(Vec::<String>::new()));
    let task_listen_addresses = listen_addresses.clone();
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
                            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                            let _ = swarm
                                .behaviour_mut()
                                .kademlia
                                .get_providers(task_provider_key.clone());
                        }
                        libp2p::swarm::SwarmEvent::ConnectionClosed { peer_id, .. } => {
                            swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
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

    Ok(IpfsPubsubProviderRoom {
        local_peer_id,
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
            SignalingAdapterKind::IpfsPubsub => "/dns/bootstrap.example.invalid/tcp/4001",
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
        let duplicate = "/ip4/127.0.0.1/tcp/4001"
            .parse::<libp2p::Multiaddr>()
            .expect("multiaddr");
        let error = ipfs_validate_bootstrap_policy(&[duplicate.clone(), duplicate])
            .expect_err("duplicate bootstrap endpoint must be rejected");
        assert!(format!("{error}").contains("duplicate bootstrap endpoint"));

        let mut too_many = Vec::new();
        for port in 0..=IPFS_PUBSUB_MAX_BOOTSTRAP_ENDPOINTS {
            too_many.push(
                format!("/ip4/127.0.0.1/tcp/{}", 4000 + port)
                    .parse::<libp2p::Multiaddr>()
                    .expect("multiaddr"),
            );
        }
        let error = ipfs_validate_bootstrap_policy(&too_many)
            .expect_err("too many bootstrap endpoints must be rejected");
        assert!(format!("{error}").contains("resource policy limit"));
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
            .listen_addresses_for_tests()
            .into_iter()
            .next()
            .ok_or_else(|| {
                TransportError::SignalingAdapter("missing alice listen address".to_owned())
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

    #[tokio::test]
    #[cfg(feature = "discrypt-quic-rendezvous-adapter")]
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

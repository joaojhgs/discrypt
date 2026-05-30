//! Feature-gated production boundaries for required signaling providers.
//!
//! Each required provider has a concrete adapter boundary that validates
//! profiles, exposes redacted health, and fails closed unless an audited
//! provider client is compiled behind its explicit Cargo feature. MQTT now has a
//! real provider client behind `mqtt-adapter`; Nostr, IPFS/libp2p PubSub, and
//! the Rust QUIC rendezvous adapter remain explicit fail-closed boundaries until
//! their real clients land.

#[cfg(feature = "mqtt-adapter")]
use crate::SignalingProviderEndpoint;
use crate::{
    AdapterFallbackBehavior, AdapterSession, AdapterTrustLabel, ControlBroadcast,
    ConversationScope, OpaqueSignalingPayload, PeerSignal, PresenceEvent, RendezvousCapability,
    RendezvousRoom, SealedWebRtcNegotiationPayload, SignalingAdapter, SignalingAdapterCapabilities,
    SignalingAdapterKind, SignalingAdapterProfile, SignalingEndpointSecurity, SignalingHealth,
    SignalingHealthState, SignalingObservability, SignalingPeerId, TransportError,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
#[cfg(feature = "mqtt-adapter")]
use tokio::sync::Mutex as AsyncMutex;
#[cfg(feature = "mqtt-adapter")]
use tokio::time::{timeout, Duration, Instant};

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
            Self::FeatureDisabled
            | Self::ImplementationUnavailable
            | Self::ProviderUnhealthy
            | Self::ProviderRateLimited
            | Self::ProviderAuthRequired
            | Self::ProviderMessageTooLarge
            | Self::TrustMismatch
            | Self::IceFailedRequiresTurn => SignalingHealthState::ProviderUnhealthy,
        }
    }
}

/// One required adapter in the ordered registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    /// Nostr boundary until real client is wired.
    Nostr(FeatureGatedProviderAdapter),
    /// IPFS/libp2p PubSub boundary until real client is wired.
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
            SignalingAdapterKind::Nostr => Self::Nostr(FeatureGatedProviderAdapter::new(kind)),
            SignalingAdapterKind::IpfsPubsub => {
                Self::IpfsPubsub(FeatureGatedProviderAdapter::new(kind))
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
            Self::Nostr(adapter) => adapter.boundary(),
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
            readiness: feature_readiness(cfg!(feature = "nostr-adapter")),
        },
        SignalingAdapterKind::IpfsPubsub => ProviderAdapterBoundary {
            kind,
            cargo_feature: "ipfs-pubsub-adapter",
            readiness: feature_readiness(cfg!(feature = "ipfs-pubsub-adapter")),
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
#[derive(Clone, Debug, Default)]
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
type MqttEventReceiver = tokio::sync::mpsc::Receiver<Result<(String, Vec<u8>), String>>;

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
    let separator = if endpoint.contains('?') { '&' } else { '?' };
    let url = format!("{endpoint}{separator}client_id={client_id}");
    let mut options =
        rumqttc::MqttOptions::parse_url(url).map_err(|err| mqtt_err("endpoint parse", err))?;
    options.set_keep_alive(std::time::Duration::from_secs(10));
    options.set_clean_session(true);
    options.set_max_packet_size(64 * 1024, 64 * 1024);
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
                Ok(Some(Ok((topic, payload)))) => {
                    if std::env::var("DISCRYPT_SIGNALING_TRACE").as_deref() == Ok("1") {
                        eprintln!("mqtt incoming publish {topic}");
                    }
                    self.record_publish(topic, payload).await?;
                }
                Ok(Some(Err(err))) => {
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
        let (client, mut eventloop) = rumqttc::AsyncClient::new(options, 64);
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(128);
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(publish))) => {
                        if event_tx
                            .send(Ok((publish.topic, publish.payload.to_vec())))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        let _ = event_tx.send(Err(err.to_string())).await;
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
                "/dns/bootstrap.example.invalid/tcp/4001/p2p/12D3KooWBootstrap"
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
        if boundary.kind == SignalingAdapterKind::Mqtt && feature_enabled {
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
                plan.attempts.last().map(|attempt| attempt.selected),
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
            let last = plan
                .attempts
                .last()
                .expect("plan should not be empty when selected");
            assert_eq!(last.kind, selected);
            assert!(last.selected);
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
        assert_eq!(attempt.attempted, true);
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

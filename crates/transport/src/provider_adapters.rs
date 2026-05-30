//! Feature-gated production boundaries for required signaling providers.
//!
//! The real MQTT, Nostr, IPFS/libp2p PubSub, and Rust QUIC provider clients are
//! intentionally not emulated here. This module gives each required provider a
//! concrete adapter boundary that validates profiles, exposes redacted health,
//! and fails closed unless a future production implementation replaces the
//! boundary behind its explicit Cargo feature.

use crate::{
    AdapterSession, AdapterTrustLabel, ControlBroadcast, ConversationScope, OpaqueSignalingPayload,
    PeerSignal, PresenceEvent, RendezvousCapability, RendezvousRoom,
    SealedWebRtcNegotiationPayload, SignalingAdapter, SignalingAdapterCapabilities,
    SignalingAdapterKind, SignalingAdapterProfile, SignalingEndpointSecurity, SignalingHealth,
    SignalingHealthState, SignalingObservability, SignalingPeerId, TransportError,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// Production readiness for a provider adapter boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAdapterReadiness {
    /// The provider adapter Cargo feature is not enabled in this build.
    FeatureDisabled,
    /// The Cargo feature is enabled but no audited provider client is wired yet.
    ImplementationUnavailable,
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
    /// True when a real provider client is available in this build.
    #[must_use]
    pub const fn implementation_available(self) -> bool {
        false
    }

    /// Redacted failure label for health/observability.
    #[must_use]
    pub const fn failure_class(self) -> &'static str {
        match self.readiness {
            ProviderAdapterReadiness::FeatureDisabled => "feature_disabled",
            ProviderAdapterReadiness::ImplementationUnavailable => "implementation_unavailable",
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
            readiness: feature_readiness(cfg!(feature = "mqtt-adapter")),
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
        if feature_enabled {
            ProviderAdapterReadiness::ImplementationUnavailable
        } else {
            ProviderAdapterReadiness::FeatureDisabled
        }
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
            assert!(!boundary.implementation_available());
            assert_eq!(
                boundary.failure_class(),
                match boundary.readiness {
                    ProviderAdapterReadiness::FeatureDisabled => "feature_disabled",
                    ProviderAdapterReadiness::ImplementationUnavailable =>
                        "implementation_unavailable",
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

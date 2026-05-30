//! Feature-gated production boundaries for required signaling providers.
//!
//! The real MQTT, Nostr, IPFS/libp2p PubSub, and Rust QUIC provider clients are
//! intentionally not emulated here. This module gives each required provider a
//! concrete adapter boundary that validates profiles, exposes redacted health,
//! and fails closed unless a future production implementation replaces the
//! boundary behind its explicit Cargo feature.

use crate::{
    AdapterSession, AdapterTrustLabel, ConversationScope, OpaqueSignalingPayload, PeerSignal,
    PresenceEvent, RendezvousCapability, RendezvousRoom, SealedWebRtcNegotiationPayload,
    SignalingAdapter, SignalingAdapterCapabilities, SignalingAdapterKind, SignalingAdapterProfile,
    SignalingHealth, SignalingHealthState, SignalingObservability, SignalingPeerId, TransportError,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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

    async fn leave(&self) -> Result<(), TransportError> {
        Err(self.boundary.unavailable_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        derive_scope_commitment, ConnectivityScopeLevel, Endpoint, ProviderMetadataPosture,
        SignalingEndpointSecurity, SignalingProviderEndpoint, WebRtcNegotiationPayloadKind,
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
            room.leave().await,
            Err(TransportError::SignalingAdapter(_))
        ));
        Ok(())
    }
}

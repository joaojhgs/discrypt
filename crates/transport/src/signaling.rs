//! Signaling adapter contract for serverless rendezvous providers.
//!
//! Adapters exchange pre-derived rendezvous capabilities and already-sealed
//! WebRTC negotiation/control payloads. They do not receive invite secrets,
//! MLS/SFrame/content keys, raw SDP, raw ICE credentials, TURN secrets, group
//! names, channel names, or message/audio plaintext.

use crate::{
    AdapterTrustLabel, ConversationScope, RendezvousCapability, SealedWebRtcNegotiationPayload,
    SignalingAdapterCapabilities, SignalingAdapterKind, SignalingAdapterProfile, TransportError,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
#[cfg(any(test, feature = "harness", feature = "local-dev"))]
use std::collections::HashMap;
use std::fmt;
#[cfg(any(test, feature = "harness", feature = "local-dev"))]
use std::sync::{Arc, Mutex};
#[cfg(any(test, feature = "harness", feature = "local-dev"))]
use std::time::{Duration, Instant};

/// Opaque peer/device identifier safe for adapter routing.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SignalingPeerId(pub String);

impl SignalingPeerId {
    /// Construct a peer id from an already-redacted stable device/peer id.
    pub fn new(value: impl Into<String>) -> Result<Self, TransportError> {
        let peer = Self(value.into());
        if peer.0.is_empty()
            || peer.0.len() > 128
            || peer.0.trim() != peer.0
            || !peer.0.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
        {
            return Err(TransportError::InvalidConnectivityPolicy(
                "signaling peer ids must be trimmed ASCII token strings".to_owned(),
            ));
        }
        Ok(peer)
    }
}

/// Opaque encrypted presence or control bytes.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpaqueSignalingPayload {
    /// Ciphertext/opaque payload bytes.
    pub bytes: Vec<u8>,
}

impl fmt::Debug for OpaqueSignalingPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpaqueSignalingPayload")
            .field("bytes", &format_args!("{} bytes", self.bytes.len()))
            .finish()
    }
}

impl OpaqueSignalingPayload {
    /// Construct an opaque payload from non-empty sealed bytes.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, TransportError> {
        let payload = Self {
            bytes: bytes.into(),
        };
        if payload.bytes.is_empty() {
            return Err(TransportError::SignalingAdapter(
                "opaque signaling payload must not be empty".to_owned(),
            ));
        }
        Ok(payload)
    }
}

/// Presence event delivered by an adapter room subscription.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresenceEvent {
    /// Peer that published the event.
    pub peer_id: SignalingPeerId,
    /// Encrypted presence payload.
    pub encrypted_presence: OpaqueSignalingPayload,
    /// Provider-side TTL remaining/declared by the adapter.
    pub ttl_seconds: u32,
}

/// Peer-targeted sealed signal delivered by an adapter room subscription.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerSignal {
    /// Sender peer id.
    pub from_peer: SignalingPeerId,
    /// Recipient peer id.
    pub to_peer: SignalingPeerId,
    /// Already-sealed WebRTC negotiation payload.
    pub payload: SealedWebRtcNegotiationPayload,
}

/// Typed provider/adapter health states used for fallback and UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalingHealthState {
    /// Adapter/provider is healthy.
    Healthy,
    /// Provider returned or implied rate limiting.
    ProviderRateLimited,
    /// Provider requires authentication not present in this profile.
    ProviderAuthRequired,
    /// Provider is unreachable or failed health checks.
    ProviderUnhealthy,
    /// Provider rejected the message size.
    ProviderMessageTooLarge,
    /// Endpoint trust/identity check failed before sending.
    TrustMismatch,
}

/// Redacted health report from an adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingHealth {
    /// Adapter kind.
    pub adapter_kind: SignalingAdapterKind,
    /// Current health state.
    pub state: SignalingHealthState,
    /// Redacted latency bucket such as `lt_100ms`, `1s_5s`, or `unknown`.
    pub latency_bucket: String,
    /// Redacted failure label, if any.
    #[serde(default)]
    pub failure_class: Option<String>,
}

/// Redacted adapter observability. Never include raw endpoints, topics, payloads, or secrets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingObservability {
    /// Adapter kind.
    pub adapter_kind: SignalingAdapterKind,
    /// Hash/label for the endpoint, not the raw endpoint unless already public UI policy.
    pub endpoint_label: String,
    /// Health state.
    pub health: SignalingHealthState,
    /// Redacted trust posture.
    pub trust_label: AdapterTrustLabel,
}

/// Connected adapter session for one adapter profile.
#[async_trait]
pub trait AdapterSession: Send + Sync {
    /// Room/session type returned after joining a scope.
    type Room: RendezvousRoom;

    /// Join a pre-derived rendezvous capability for a committed scope.
    async fn join(
        &self,
        scope: ConversationScope,
        rendezvous: RendezvousCapability,
        local_peer_id: SignalingPeerId,
    ) -> Result<Self::Room, TransportError>;

    /// Close provider resources for this adapter session.
    async fn close(&self) -> Result<(), TransportError>;

    /// Return redacted health for fallback selection.
    async fn health(&self) -> SignalingHealth;
}

/// Joined rendezvous room over one signaling adapter.
#[async_trait]
pub trait RendezvousRoom: Send + Sync {
    /// Publish encrypted/opaque presence with an adapter-enforced TTL.
    async fn publish_presence(
        &self,
        encrypted_presence: OpaqueSignalingPayload,
        ttl_seconds: u32,
    ) -> Result<(), TransportError>;

    /// Read available encrypted presence events.
    async fn subscribe_presence(&self) -> Result<Vec<PresenceEvent>, TransportError>;

    /// Send one already-sealed WebRTC offer/answer/candidate to a peer.
    async fn send_signal(
        &self,
        to_peer: SignalingPeerId,
        payload: SealedWebRtcNegotiationPayload,
    ) -> Result<(), TransportError>;

    /// Read available peer-targeted sealed WebRTC negotiation signals.
    async fn take_signals(&self) -> Result<Vec<PeerSignal>, TransportError>;

    /// Broadcast an already-sealed room control payload.
    async fn broadcast_control(
        &self,
        sealed_payload: OpaqueSignalingPayload,
    ) -> Result<(), TransportError>;

    /// Leave the room and clear retained presence where supported.
    async fn leave(&self) -> Result<(), TransportError>;
}

/// Signaling adapter boundary implemented by MQTT, Nostr, IPFS/libp2p, and Rust QUIC providers.
#[async_trait]
pub trait SignalingAdapter: Send + Sync {
    /// Connected session type.
    type Session: AdapterSession;

    /// Connect to an adapter profile after policy validation.
    async fn connect(
        &self,
        profile: SignalingAdapterProfile,
    ) -> Result<Self::Session, TransportError>;

    /// Adapter capabilities used by policy validation and conformance tests.
    fn capabilities(&self) -> SignalingAdapterCapabilities;

    /// Redacted observability safe for logs/UI.
    fn observability_redacted(&self) -> SignalingObservability;
}

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
#[derive(Clone, Default)]
pub struct LocalConformanceSignalingBus {
    state: Arc<Mutex<LocalConformanceBusState>>,
}

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
#[derive(Default)]
struct LocalConformanceBusState {
    rooms: HashMap<String, LocalConformanceRoomState>,
}

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
#[derive(Default)]
struct LocalConformanceRoomState {
    presence: Vec<LocalConformancePresenceRecord>,
    signals: Vec<PeerSignal>,
    controls: Vec<OpaqueSignalingPayload>,
}

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
#[derive(Clone)]
struct LocalConformancePresenceRecord {
    event: PresenceEvent,
    expires_at: Instant,
}

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
#[derive(Clone)]
pub struct LocalConformanceSignalingAdapter {
    kind: SignalingAdapterKind,
    bus: LocalConformanceSignalingBus,
}

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
impl LocalConformanceSignalingAdapter {
    pub fn new(kind: SignalingAdapterKind, bus: LocalConformanceSignalingBus) -> Self {
        Self { kind, bus }
    }
}

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
pub struct LocalConformanceSignalingSession {
    kind: SignalingAdapterKind,
    bus: LocalConformanceSignalingBus,
}

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
pub struct LocalConformanceRendezvousRoom {
    key: String,
    local_peer_id: SignalingPeerId,
    bus: LocalConformanceSignalingBus,
}

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
#[async_trait]
impl SignalingAdapter for LocalConformanceSignalingAdapter {
    type Session = LocalConformanceSignalingSession;

    async fn connect(
        &self,
        profile: SignalingAdapterProfile,
    ) -> Result<Self::Session, TransportError> {
        profile.validate()?;
        if profile.kind != self.kind {
            return Err(TransportError::SignalingAdapter(format!(
                "profile kind {} does not match local conformance adapter {}",
                profile.kind.canonical_name(),
                self.kind.canonical_name()
            )));
        }
        Ok(LocalConformanceSignalingSession {
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
            endpoint_label: format!("local-conformance#{}", self.kind.canonical_name()),
            health: SignalingHealthState::Healthy,
            trust_label: AdapterTrustLabel {
                label: format!("{} local conformance", self.kind.canonical_name()),
                posture: "deterministic harness; not a public provider".to_owned(),
            },
        }
    }
}

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
#[async_trait]
impl AdapterSession for LocalConformanceSignalingSession {
    type Room = LocalConformanceRendezvousRoom;

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
            return Err(TransportError::SignalingAdapter(format!(
                "rendezvous capability kind {} does not match adapter {}",
                rendezvous.adapter_kind.canonical_name(),
                self.kind.canonical_name()
            )));
        }
        let key = room_key(self.kind, &rendezvous.topic);
        self.bus
            .state
            .lock()
            .map_err(|_| TransportError::SignalingAdapter("local bus lock poisoned".to_owned()))?
            .rooms
            .entry(key.clone())
            .or_default();
        Ok(LocalConformanceRendezvousRoom {
            key,
            local_peer_id,
            bus: self.bus.clone(),
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

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
#[async_trait]
impl RendezvousRoom for LocalConformanceRendezvousRoom {
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
        let mut state =
            self.bus.state.lock().map_err(|_| {
                TransportError::SignalingAdapter("local bus lock poisoned".to_owned())
            })?;
        let room = state.rooms.entry(self.key.clone()).or_default();
        room.presence
            .retain(|record| record.expires_at > Instant::now());
        room.presence.push(LocalConformancePresenceRecord {
            event: PresenceEvent {
                peer_id: self.local_peer_id.clone(),
                encrypted_presence,
                ttl_seconds,
            },
            expires_at: Instant::now() + Duration::from_secs(u64::from(ttl_seconds)),
        });
        Ok(())
    }

    async fn subscribe_presence(&self) -> Result<Vec<PresenceEvent>, TransportError> {
        let mut state =
            self.bus.state.lock().map_err(|_| {
                TransportError::SignalingAdapter("local bus lock poisoned".to_owned())
            })?;
        let room = state.rooms.entry(self.key.clone()).or_default();
        room.presence
            .retain(|record| record.expires_at > Instant::now());
        Ok(room
            .presence
            .iter()
            .map(|record| record.event.clone())
            .collect())
    }

    async fn send_signal(
        &self,
        to_peer: SignalingPeerId,
        payload: SealedWebRtcNegotiationPayload,
    ) -> Result<(), TransportError> {
        if payload.ciphertext.is_empty() {
            return Err(TransportError::SignalingAdapter(
                "sealed signaling payload must not be empty".to_owned(),
            ));
        }
        self.bus
            .state
            .lock()
            .map_err(|_| TransportError::SignalingAdapter("local bus lock poisoned".to_owned()))?
            .rooms
            .entry(self.key.clone())
            .or_default()
            .signals
            .push(PeerSignal {
                from_peer: self.local_peer_id.clone(),
                to_peer,
                payload,
            });
        Ok(())
    }

    async fn take_signals(&self) -> Result<Vec<PeerSignal>, TransportError> {
        let mut state =
            self.bus.state.lock().map_err(|_| {
                TransportError::SignalingAdapter("local bus lock poisoned".to_owned())
            })?;
        let room = state.rooms.entry(self.key.clone()).or_default();
        let mut mine = Vec::new();
        let mut remaining = Vec::new();
        for signal in room.signals.drain(..) {
            if signal.to_peer == self.local_peer_id {
                mine.push(signal);
            } else {
                remaining.push(signal);
            }
        }
        room.signals = remaining;
        Ok(mine)
    }

    async fn broadcast_control(
        &self,
        sealed_payload: OpaqueSignalingPayload,
    ) -> Result<(), TransportError> {
        self.bus
            .state
            .lock()
            .map_err(|_| TransportError::SignalingAdapter("local bus lock poisoned".to_owned()))?
            .rooms
            .entry(self.key.clone())
            .or_default()
            .controls
            .push(sealed_payload);
        Ok(())
    }

    async fn leave(&self) -> Result<(), TransportError> {
        let mut state =
            self.bus.state.lock().map_err(|_| {
                TransportError::SignalingAdapter("local bus lock poisoned".to_owned())
            })?;
        if let Some(room) = state.rooms.get_mut(&self.key) {
            room.presence
                .retain(|record| record.event.peer_id != self.local_peer_id);
        }
        Ok(())
    }
}

#[cfg(any(test, feature = "harness", feature = "local-dev"))]
fn room_key(kind: SignalingAdapterKind, topic: &str) -> String {
    format!("{}:{topic}", kind.canonical_name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AdapterTrustLabel, ConnectivityScopeLevel, ProviderMetadataPosture, RendezvousCapability,
        SignalingAdapterKind, WebRtcNegotiationPayloadKind,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct TestAdapter {
        state: Arc<Mutex<TestState>>,
    }

    #[derive(Default)]
    struct TestState {
        joined_topic: Option<String>,
        presence: Vec<PresenceEvent>,
        signals: Vec<PeerSignal>,
        left: bool,
    }

    struct TestSession {
        state: Arc<Mutex<TestState>>,
    }

    struct TestRoom {
        state: Arc<Mutex<TestState>>,
        local_peer_id: SignalingPeerId,
    }

    #[async_trait]
    impl SignalingAdapter for TestAdapter {
        type Session = TestSession;

        async fn connect(
            &self,
            profile: crate::SignalingAdapterProfile,
        ) -> Result<Self::Session, TransportError> {
            profile.validate()?;
            Ok(TestSession {
                state: self.state.clone(),
            })
        }

        fn capabilities(&self) -> SignalingAdapterCapabilities {
            SignalingAdapterCapabilities::production_required()
        }

        fn observability_redacted(&self) -> SignalingObservability {
            SignalingObservability {
                adapter_kind: SignalingAdapterKind::Mqtt,
                endpoint_label: "endpoint#redacted".to_owned(),
                health: SignalingHealthState::Healthy,
                trust_label: AdapterTrustLabel {
                    label: "test".to_owned(),
                    posture: "redacted".to_owned(),
                },
            }
        }
    }

    #[async_trait]
    impl AdapterSession for TestSession {
        type Room = TestRoom;

        async fn join(
            &self,
            scope: crate::ConversationScope,
            rendezvous: RendezvousCapability,
            local_peer_id: SignalingPeerId,
        ) -> Result<Self::Room, TransportError> {
            scope.validate()?;
            if rendezvous.scope != scope {
                return Err(TransportError::SignalingAdapter(
                    "rendezvous capability scope mismatch".to_owned(),
                ));
            }
            self.state
                .lock()
                .map_err(|_| TransportError::SignalingAdapter("test lock poisoned".to_owned()))?
                .joined_topic = Some(rendezvous.topic);
            Ok(TestRoom {
                state: self.state.clone(),
                local_peer_id,
            })
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

    #[async_trait]
    impl RendezvousRoom for TestRoom {
        async fn publish_presence(
            &self,
            encrypted_presence: OpaqueSignalingPayload,
            ttl_seconds: u32,
        ) -> Result<(), TransportError> {
            self.state
                .lock()
                .map_err(|_| TransportError::SignalingAdapter("test lock poisoned".to_owned()))?
                .presence
                .push(PresenceEvent {
                    peer_id: self.local_peer_id.clone(),
                    encrypted_presence,
                    ttl_seconds,
                });
            Ok(())
        }

        async fn subscribe_presence(&self) -> Result<Vec<PresenceEvent>, TransportError> {
            Ok(self
                .state
                .lock()
                .map_err(|_| TransportError::SignalingAdapter("test lock poisoned".to_owned()))?
                .presence
                .clone())
        }

        async fn send_signal(
            &self,
            to_peer: SignalingPeerId,
            payload: SealedWebRtcNegotiationPayload,
        ) -> Result<(), TransportError> {
            self.state
                .lock()
                .map_err(|_| TransportError::SignalingAdapter("test lock poisoned".to_owned()))?
                .signals
                .push(PeerSignal {
                    from_peer: self.local_peer_id.clone(),
                    to_peer,
                    payload,
                });
            Ok(())
        }

        async fn take_signals(&self) -> Result<Vec<PeerSignal>, TransportError> {
            Ok(self
                .state
                .lock()
                .map_err(|_| TransportError::SignalingAdapter("test lock poisoned".to_owned()))?
                .signals
                .clone())
        }

        async fn broadcast_control(
            &self,
            sealed_payload: OpaqueSignalingPayload,
        ) -> Result<(), TransportError> {
            if sealed_payload
                .bytes
                .windows(3)
                .any(|window| window == b"sdp")
            {
                return Err(TransportError::SignalingAdapter(
                    "raw negotiation marker rejected by test adapter".to_owned(),
                ));
            }
            Ok(())
        }

        async fn leave(&self) -> Result<(), TransportError> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| TransportError::SignalingAdapter("test lock poisoned".to_owned()))?;
            state.presence.clear();
            state.left = true;
            Ok(())
        }
    }

    fn profile() -> Result<crate::SignalingAdapterProfile, TransportError> {
        Ok(crate::SignalingAdapterProfile {
            profile_id: "mqtt-default".to_owned(),
            kind: SignalingAdapterKind::Mqtt,
            endpoints: vec![crate::SignalingProviderEndpoint::new(
                crate::Endpoint::new("wss://mqtt.example.invalid"),
                crate::SignalingEndpointSecurity::ProductionTls,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new("mqtt", "redacted")?,
        })
    }

    fn local_profile(
        kind: SignalingAdapterKind,
    ) -> Result<crate::SignalingAdapterProfile, TransportError> {
        let endpoint = match kind {
            SignalingAdapterKind::Mqtt => "mqtt://127.0.0.1:1883",
            SignalingAdapterKind::Nostr => "ws://127.0.0.1:7777",
            SignalingAdapterKind::IpfsPubsub => "ws://127.0.0.1:7788",
            SignalingAdapterKind::DiscryptQuicRendezvous => "http://127.0.0.1:7799",
        };
        Ok(crate::SignalingAdapterProfile {
            profile_id: format!("{}-local-conformance", kind.canonical_name()),
            kind,
            endpoints: vec![crate::SignalingProviderEndpoint::new(
                crate::Endpoint::new(endpoint),
                crate::SignalingEndpointSecurity::LocalDevLoopback,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new(kind.canonical_name(), "local conformance")?,
        })
    }

    #[tokio::test]
    async fn adapter_contract_uses_prederived_capability_and_sealed_payloads(
    ) -> Result<(), TransportError> {
        let adapter = TestAdapter {
            state: Arc::new(Mutex::new(TestState::default())),
        };
        let scope = crate::ConversationScope::new(
            ConnectivityScopeLevel::Dm,
            crate::derive_scope_commitment(ConnectivityScopeLevel::Dm, b"alice bob", "test"),
        )?;
        let capability = RendezvousCapability::derive(
            scope.clone(),
            SignalingAdapterKind::Mqtt,
            b"bootstrap secret with enough bytes for tests",
            b"random entropy bytes",
            60,
            ProviderMetadataPosture::HashedTopic,
            AdapterTrustLabel::new("mqtt", "redacted")?,
        )?;
        let session = adapter.connect(profile()?).await?;
        let room = session
            .join(scope, capability, SignalingPeerId::new("alice-device")?)
            .await?;

        room.publish_presence(
            OpaqueSignalingPayload::new(b"sealed presence".to_vec())?,
            60,
        )
        .await?;
        let presence = room.subscribe_presence().await?;
        assert_eq!(presence.len(), 1);

        let sealed = SealedWebRtcNegotiationPayload {
            version: 1,
            kind: WebRtcNegotiationPayloadKind::Offer,
            nonce: [7; 12],
            ciphertext: b"opaque ciphertext".to_vec(),
        };
        room.send_signal(SignalingPeerId::new("bob-device")?, sealed.clone())
            .await?;
        assert_eq!(room.take_signals().await?[0].payload, sealed);

        room.leave().await?;
        assert!(room.subscribe_presence().await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn local_conformance_adapters_exchange_sealed_dm_signals_for_every_required_kind(
    ) -> Result<(), TransportError> {
        let kinds = [
            SignalingAdapterKind::Mqtt,
            SignalingAdapterKind::Nostr,
            SignalingAdapterKind::IpfsPubsub,
            SignalingAdapterKind::DiscryptQuicRendezvous,
        ];
        for kind in kinds {
            let bus = LocalConformanceSignalingBus::default();
            let adapter = LocalConformanceSignalingAdapter::new(kind, bus);
            let scope = crate::ConversationScope::new(
                ConnectivityScopeLevel::Dm,
                crate::derive_scope_commitment(
                    ConnectivityScopeLevel::Dm,
                    format!("alice-bob-{}", kind.canonical_name()).as_bytes(),
                    "local-conformance",
                ),
            )?;
            let capability = RendezvousCapability::derive(
                scope.clone(),
                kind,
                b"bootstrap secret with enough bytes for all local adapters",
                format!("local-conformance-random-entropy-{}", kind.canonical_name()).as_bytes(),
                120,
                ProviderMetadataPosture::HashedTopic,
                AdapterTrustLabel::new(kind.canonical_name(), "local conformance")?,
            )?;
            assert!(!capability.topic.contains("alice"));
            assert!(!capability.topic.contains("bob"));

            let alice = adapter.connect(local_profile(kind)?).await?;
            let bob = adapter.connect(local_profile(kind)?).await?;
            let alice_room = alice
                .join(
                    scope.clone(),
                    capability.clone(),
                    SignalingPeerId::new("alice-device")?,
                )
                .await?;
            let bob_room = bob
                .join(scope, capability, SignalingPeerId::new("bob-device")?)
                .await?;

            alice_room
                .publish_presence(OpaqueSignalingPayload::new(b"sealed-alice-presence")?, 120)
                .await?;
            let bob_presence = bob_room.subscribe_presence().await?;
            assert_eq!(bob_presence.len(), 1);
            assert_eq!(
                bob_presence[0].peer_id,
                SignalingPeerId::new("alice-device")?
            );

            let sealed_dm_offer = SealedWebRtcNegotiationPayload {
                version: 1,
                kind: WebRtcNegotiationPayloadKind::Offer,
                nonce: [kind as u8; 12],
                ciphertext: format!("sealed-dm-offer-{}", kind.canonical_name()).into_bytes(),
            };
            alice_room
                .send_signal(SignalingPeerId::new("bob-device")?, sealed_dm_offer.clone())
                .await?;
            let bob_signals = bob_room.take_signals().await?;
            assert_eq!(bob_signals.len(), 1);
            assert_eq!(
                bob_signals[0].from_peer,
                SignalingPeerId::new("alice-device")?
            );
            assert_eq!(bob_signals[0].payload, sealed_dm_offer);
            assert!(alice_room.take_signals().await?.is_empty());

            bob_room.leave().await?;
            alice_room.leave().await?;
        }
        Ok(())
    }
}

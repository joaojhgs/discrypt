//! Signaling/rendezvous adapter contracts for DM, group, and channel bootstrap.
//!
//! This module deliberately separates provider-visible rendezvous from WebRTC
//! media/data transport. Adapters exchange opaque, already-sealed negotiation
//! payloads only; no raw SDP, ICE credentials, display names, or TURN secrets are
//! accepted at this boundary.

use crate::{Endpoint, TransportError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Required production signaling adapter families.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalingAdapterKind {
    /// MQTT broker/WebSocket publish-subscribe rendezvous.
    Mqtt,
    /// Nostr relay event rendezvous.
    Nostr,
    /// IPFS/libp2p PubSub rendezvous.
    IpfsPubsub,
    /// Separate Rust QUIC signaling service adapter.
    DiscryptQuicRendezvous,
}

impl SignalingAdapterKind {
    /// Stable adapter id used in invites, policy storage, and tests.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mqtt => "mqtt",
            Self::Nostr => "nostr",
            Self::IpfsPubsub => "ipfs_pubsub",
            Self::DiscryptQuicRendezvous => "discrypt_quic_rendezvous",
        }
    }

    /// Endpoint schemes allowed for this adapter kind.
    #[must_use]
    pub const fn allowed_schemes(self) -> &'static [&'static str] {
        match self {
            Self::Mqtt => &["mqtts://", "wss://"],
            Self::Nostr => &["wss://"],
            Self::IpfsPubsub => &["ipfs://", "libp2p://"],
            Self::DiscryptQuicRendezvous => &["quic://", "https://"],
        }
    }
}

/// Product scope whose policy selects signaling and ICE profiles.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterScope {
    /// App/global default policy.
    AppDefault,
    /// Pairwise DM conversation policy.
    Dm { dm_id: String },
    /// Group/server policy inherited by channels.
    Group { group_id: String },
    /// Channel-level override policy.
    Channel {
        group_id: String,
        channel_id: String,
    },
    /// Initial invite bootstrap snapshot before persisted scope policy exists.
    InviteBootstrap { invite_id: String },
}

impl AdapterScope {
    /// Non-PII stable material used when deriving rendezvous topics.
    #[must_use]
    pub fn canonical_scope(&self) -> String {
        match self {
            Self::AppDefault => "app:default".to_owned(),
            Self::Dm { dm_id } => format!("dm:{dm_id}"),
            Self::Group { group_id } => format!("group:{group_id}"),
            Self::Channel {
                group_id,
                channel_id,
            } => format!("channel:{group_id}:{channel_id}"),
            Self::InviteBootstrap { invite_id } => format!("invite:{invite_id}"),
        }
    }
}

/// Endpoint plus trust metadata for one provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingProviderEndpoint {
    /// Broker, relay, bootstrap peer, or QUIC/HTTPS endpoint.
    pub endpoint: Endpoint,
    /// Service fingerprint or public-key commitment expected by the invite/policy.
    pub trust_fingerprint: String,
}

impl SignalingProviderEndpoint {
    /// Build and validate an endpoint for an adapter kind.
    pub fn new(
        kind: SignalingAdapterKind,
        endpoint: impl Into<String>,
        trust_fingerprint: impl Into<String>,
    ) -> Result<Self, TransportError> {
        let provider = Self {
            endpoint: Endpoint::new(endpoint.into()),
            trust_fingerprint: trust_fingerprint.into(),
        };
        provider.validate(kind)?;
        Ok(provider)
    }

    /// Validate scheme and fingerprint shape.
    pub fn validate(&self, kind: SignalingAdapterKind) -> Result<(), TransportError> {
        let endpoint = self.endpoint.0.as_str();
        if endpoint.trim() != endpoint
            || !kind
                .allowed_schemes()
                .iter()
                .any(|scheme| endpoint.starts_with(scheme))
        {
            return Err(TransportError::Unavailable(format!(
                "{} endpoint has unsupported scheme: {}",
                kind.as_str(),
                endpoint
            )));
        }
        if !is_commitment(&self.trust_fingerprint) {
            return Err(TransportError::Unavailable(format!(
                "{} endpoint trust fingerprint is invalid",
                kind.as_str()
            )));
        }
        Ok(())
    }
}

/// Ordered adapter profile persisted for an app, DM, group, channel, or invite.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingAdapterProfile {
    /// Stable profile id.
    pub profile_id: String,
    /// Adapter family.
    pub kind: SignalingAdapterKind,
    /// Ordered provider endpoints for the adapter.
    pub endpoints: Vec<SignalingProviderEndpoint>,
    /// Publish/subscribe TTL.
    pub ttl: Duration,
    /// Provider-visible metadata caveat.
    pub metadata_posture: String,
}

impl SignalingAdapterProfile {
    /// Construct and validate one profile.
    pub fn new(
        profile_id: impl Into<String>,
        kind: SignalingAdapterKind,
        endpoints: Vec<SignalingProviderEndpoint>,
        ttl: Duration,
        metadata_posture: impl Into<String>,
    ) -> Result<Self, TransportError> {
        let profile = Self {
            profile_id: profile_id.into(),
            kind,
            endpoints,
            ttl,
            metadata_posture: metadata_posture.into(),
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Validate endpoint set and UI caveat.
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.profile_id.trim().is_empty()
            || self.endpoints.is_empty()
            || self.ttl.is_zero()
            || self.metadata_posture.trim().is_empty()
        {
            return Err(TransportError::Unavailable(
                "signaling adapter profile is incomplete".to_owned(),
            ));
        }
        for endpoint in &self.endpoints {
            endpoint.validate(self.kind)?;
        }
        Ok(())
    }
}

/// ICE profile selected independently from signaling adapters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IceProfile {
    /// Stable ICE profile id.
    pub profile_id: String,
    /// STUN endpoints; default policy starts with Google's public STUN.
    pub stun_servers: Vec<Endpoint>,
    /// Redacted TURN endpoints. Empty by default.
    pub turn_servers: Vec<Endpoint>,
}

impl Default for IceProfile {
    fn default() -> Self {
        Self {
            profile_id: "default-public-stun-no-turn".to_owned(),
            stun_servers: vec![Endpoint::new("stun:stun.l.google.com:19302")],
            turn_servers: Vec::new(),
        }
    }
}

impl IceProfile {
    /// Human-readable default TURN status for UI and docs.
    #[must_use]
    pub fn turn_status_copy(&self) -> &'static str {
        if self.turn_servers.is_empty() {
            "TURN not configured"
        } else {
            "TURN configured"
        }
    }
}

/// Full connectivity policy selected by product scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectivityPolicy {
    /// Scope this policy belongs to.
    pub scope: AdapterScope,
    /// Ordered signaling adapters to try.
    pub signaling_profiles: Vec<SignalingAdapterProfile>,
    /// ICE/STUN/TURN configuration independent from signaling.
    pub ice_profile: IceProfile,
}

impl ConnectivityPolicy {
    /// Build and validate a policy.
    pub fn new(
        scope: AdapterScope,
        signaling_profiles: Vec<SignalingAdapterProfile>,
        ice_profile: IceProfile,
    ) -> Result<Self, TransportError> {
        let policy = Self {
            scope,
            signaling_profiles,
            ice_profile,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Validate adapter and ICE shape.
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.signaling_profiles.is_empty() || self.ice_profile.stun_servers.is_empty() {
            return Err(TransportError::Unavailable(
                "connectivity policy needs at least one signaling profile and STUN endpoint"
                    .to_owned(),
            ));
        }
        for profile in &self.signaling_profiles {
            profile.validate()?;
        }
        Ok(())
    }

    /// Select the first currently healthy adapter profile in policy order.
    pub fn select<'a>(
        &'a self,
        health: &BTreeMap<String, AdapterHealth>,
    ) -> Result<AdapterSelection<'a>, AdapterSelectionError> {
        for profile in &self.signaling_profiles {
            let status = health
                .get(&profile.profile_id)
                .copied()
                .unwrap_or(AdapterHealth::Healthy);
            if status == AdapterHealth::Healthy {
                return Ok(AdapterSelection {
                    profile,
                    reason: "first healthy adapter in scoped policy order",
                });
            }
        }
        Err(AdapterSelectionError::NoHealthyAdapter)
    }
}

/// Health status used for ordered fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterHealth {
    /// Provider is usable.
    Healthy,
    /// Provider failed recently and should be skipped temporarily.
    BackingOff,
    /// Provider is not configured or trust failed.
    Unavailable,
}

/// Selected adapter profile and explanation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterSelection<'a> {
    /// Selected profile.
    pub profile: &'a SignalingAdapterProfile,
    /// Selection explanation.
    pub reason: &'static str,
}

/// Selection failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterSelectionError {
    /// No adapter profile is healthy.
    NoHealthyAdapter,
}

/// Derived rendezvous topic/capability. The raw scope secret is not stored.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct RendezvousCapability {
    /// Adapter family for which this topic is valid.
    pub adapter_kind: SignalingAdapterKind,
    /// Provider-visible derived topic commitment.
    pub topic_commitment: String,
}

impl RendezvousCapability {
    /// Derive a provider-visible topic commitment from sealed, non-PII inputs.
    #[must_use]
    pub fn derive(adapter_kind: SignalingAdapterKind, scope: &AdapterScope, secret: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"discrypt-rendezvous-capability-v1");
        hasher.update(adapter_kind.as_str().as_bytes());
        hasher.update(scope.canonical_scope().as_bytes());
        hasher.update(secret);
        Self {
            adapter_kind,
            topic_commitment: hex::encode(hasher.finalize()),
        }
    }
}

/// Opaque signaling payload. `sealed_payload` must already be encrypted/sealed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterMessage {
    /// Message kind: offer, answer, candidate, or adapter-specific health probe.
    pub kind: String,
    /// Sealed payload only.
    pub sealed_payload: Vec<u8>,
}

/// Provider failure categories surfaced by adapters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterFailure {
    /// Endpoint was unreachable.
    Unreachable,
    /// Provider rejected due rate limit.
    RateLimited,
    /// Message exceeded provider limits.
    MessageTooLarge,
    /// Endpoint trust/fingerprint failed.
    TrustFailed,
}

/// Async signaling adapter contract.
#[async_trait::async_trait]
pub trait SignalingAdapter: Send + Sync {
    /// Adapter kind.
    fn kind(&self) -> SignalingAdapterKind;

    /// Publish one sealed negotiation payload.
    async fn publish(
        &self,
        capability: &RendezvousCapability,
        message: AdapterMessage,
    ) -> Result<(), AdapterFailure>;

    /// Take all currently queued sealed negotiation payloads.
    async fn take(
        &self,
        capability: &RendezvousCapability,
    ) -> Result<Vec<AdapterMessage>, AdapterFailure>;
}

/// Local deterministic adapter used by conformance and two-profile harnesses.
#[derive(Clone, Debug)]
pub struct InMemorySignalingAdapter {
    kind: SignalingAdapterKind,
    queue: Arc<Mutex<BTreeMap<String, VecDeque<AdapterMessage>>>>,
}

impl InMemorySignalingAdapter {
    /// Construct an in-memory adapter for one required adapter family.
    #[must_use]
    pub fn new(kind: SignalingAdapterKind) -> Self {
        Self {
            kind,
            queue: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl SignalingAdapter for InMemorySignalingAdapter {
    fn kind(&self) -> SignalingAdapterKind {
        self.kind
    }

    async fn publish(
        &self,
        capability: &RendezvousCapability,
        message: AdapterMessage,
    ) -> Result<(), AdapterFailure> {
        if message.sealed_payload.is_empty() {
            return Err(AdapterFailure::MessageTooLarge);
        }
        let mut queue = self.queue.lock().map_err(|_| AdapterFailure::Unreachable)?;
        queue
            .entry(capability.topic_commitment.clone())
            .or_default()
            .push_back(message);
        Ok(())
    }

    async fn take(
        &self,
        capability: &RendezvousCapability,
    ) -> Result<Vec<AdapterMessage>, AdapterFailure> {
        let mut queue = self.queue.lock().map_err(|_| AdapterFailure::Unreachable)?;
        Ok(queue
            .remove(&capability.topic_commitment)
            .unwrap_or_default()
            .into_iter()
            .collect())
    }
}

/// Result from the required shared adapter conformance suite.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterConformanceReport {
    /// Adapter kind tested.
    pub adapter_kind: SignalingAdapterKind,
    /// Topic is derived and does not expose display names.
    pub derived_topic_used: bool,
    /// Opaque payload published/taken byte-for-byte.
    pub sealed_payload_round_tripped: bool,
    /// Empty/unsealed payload was rejected.
    pub invalid_payload_rejected: bool,
    /// Report limitation.
    pub limitation: String,
}

impl AdapterConformanceReport {
    /// True when the deterministic shared adapter contract passed.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.derived_topic_used
            && self.sealed_payload_round_tripped
            && self.invalid_payload_rejected
            && self.limitation.contains("deterministic")
    }
}

/// Run a deterministic adapter conformance check without provider-specific shims.
pub async fn run_adapter_conformance<A: SignalingAdapter>(
    adapter: &A,
    scope: &AdapterScope,
) -> AdapterConformanceReport {
    let capability = RendezvousCapability::derive(adapter.kind(), scope, b"test capability secret");
    let message = AdapterMessage {
        kind: "offer".to_owned(),
        sealed_payload: b"sealed-offer-bytes".to_vec(),
    };
    let publish_ok = adapter.publish(&capability, message.clone()).await.is_ok();
    let received = adapter.take(&capability).await.unwrap_or_default();
    let invalid_payload_rejected = adapter
        .publish(
            &capability,
            AdapterMessage {
                kind: "offer".to_owned(),
                sealed_payload: Vec::new(),
            },
        )
        .await
        .is_err();

    AdapterConformanceReport {
        adapter_kind: adapter.kind(),
        derived_topic_used: !capability.topic_commitment.contains(&scope.canonical_scope()),
        sealed_payload_round_tripped: publish_ok && received == vec![message],
        invalid_payload_rejected,
        limitation: "deterministic local adapter conformance; production provider tests must run against MQTT/Nostr/IPFS/Rust-QUIC endpoints".to_owned(),
    }
}

fn is_commitment(value: &str) -> bool {
    value.len() >= 32 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(seed: &str) -> String {
        hex::encode(Sha256::digest(seed.as_bytes()))
    }

    fn profile(kind: SignalingAdapterKind) -> SignalingAdapterProfile {
        let endpoint = match kind {
            SignalingAdapterKind::Mqtt => "wss://mqtt.example.invalid/mqtt",
            SignalingAdapterKind::Nostr => "wss://nostr.example.invalid",
            SignalingAdapterKind::IpfsPubsub => "libp2p://12D3KooWBootstrapPeer",
            SignalingAdapterKind::DiscryptQuicRendezvous => "quic://signal.example.invalid:443",
        };
        SignalingAdapterProfile::new(
            kind.as_str(),
            kind,
            vec![SignalingProviderEndpoint::new(kind, endpoint, fingerprint(endpoint)).unwrap()],
            Duration::from_secs(60),
            "provider sees random topic and timing only",
        )
        .unwrap()
    }

    #[test]
    fn default_ice_profile_uses_public_stun_and_no_turn() {
        let profile = IceProfile::default();
        assert_eq!(
            profile.stun_servers,
            vec![Endpoint::new("stun:stun.l.google.com:19302")]
        );
        assert!(profile.turn_servers.is_empty());
        assert_eq!(profile.turn_status_copy(), "TURN not configured");
    }

    #[test]
    fn policy_selects_first_healthy_adapter_by_scope() {
        let policy = ConnectivityPolicy::new(
            AdapterScope::Dm {
                dm_id: "dm-a".to_owned(),
            },
            vec![
                profile(SignalingAdapterKind::Mqtt),
                profile(SignalingAdapterKind::Nostr),
            ],
            IceProfile::default(),
        )
        .unwrap();
        let mut health = BTreeMap::new();
        health.insert("mqtt".to_owned(), AdapterHealth::BackingOff);
        let selected = policy.select(&health).unwrap();
        assert_eq!(selected.profile.kind, SignalingAdapterKind::Nostr);
    }

    #[test]
    fn every_required_adapter_has_distinct_valid_endpoint_policy() {
        for kind in [
            SignalingAdapterKind::Mqtt,
            SignalingAdapterKind::Nostr,
            SignalingAdapterKind::IpfsPubsub,
            SignalingAdapterKind::DiscryptQuicRendezvous,
        ] {
            let profile = profile(kind);
            assert_eq!(profile.kind, kind);
            assert!(profile.validate().is_ok());
        }
    }

    #[tokio::test]
    async fn every_required_adapter_passes_shared_local_conformance() {
        for kind in [
            SignalingAdapterKind::Mqtt,
            SignalingAdapterKind::Nostr,
            SignalingAdapterKind::IpfsPubsub,
            SignalingAdapterKind::DiscryptQuicRendezvous,
        ] {
            let adapter = InMemorySignalingAdapter::new(kind);
            let report = run_adapter_conformance(
                &adapter,
                &AdapterScope::Dm {
                    dm_id: "alice-bob".to_owned(),
                },
            )
            .await;
            assert_eq!(report.adapter_kind, kind);
            assert!(report.ready(), "{report:?}");
        }
    }

    #[test]
    fn rendezvous_capability_hides_scope_names_and_differs_per_scope() {
        let dm = AdapterScope::Dm {
            dm_id: "alice-bob".to_owned(),
        };
        let group = AdapterScope::Group {
            group_id: "private-lab".to_owned(),
        };
        let dm_cap = RendezvousCapability::derive(SignalingAdapterKind::Mqtt, &dm, b"secret");
        let group_cap = RendezvousCapability::derive(SignalingAdapterKind::Mqtt, &group, b"secret");
        assert_ne!(dm_cap.topic_commitment, group_cap.topic_commitment);
        assert!(!dm_cap.topic_commitment.contains("alice"));
        assert!(!group_cap.topic_commitment.contains("private"));
    }
}

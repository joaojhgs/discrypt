//! Scoped signaling/ICE connectivity policy.
//!
//! This module is deliberately policy-only. It derives adapter-facing
//! rendezvous capabilities from committed scope identifiers and sealed
//! bootstrap material, but it does not connect to MQTT, Nostr, IPFS/libp2p, or
//! the Rust QUIC rendezvous service.

use crate::{Endpoint, IceEndpointPolicy, TransportError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Required production signaling adapter kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalingAdapterKind {
    /// MQTT over TLS/WebSocket public or self-hosted broker.
    Mqtt,
    /// Nostr relay signaling adapter.
    Nostr,
    /// IPFS/libp2p PubSub rendezvous adapter.
    IpfsPubsub,
    /// Separate Rust QUIC signaling/rendezvous service.
    DiscryptQuicRendezvous,
}

impl SignalingAdapterKind {
    /// Stable adapter kind encoded into policy and rendezvous derivation.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Mqtt => "mqtt",
            Self::Nostr => "nostr",
            Self::IpfsPubsub => "ipfs_pubsub",
            Self::DiscryptQuicRendezvous => "discrypt_quic_rendezvous",
        }
    }
}

/// Connectivity scope where an adapter/ICE policy can be selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectivityScopeLevel {
    /// Global application default.
    AppDefault,
    /// Pairwise DM conversation policy.
    Dm,
    /// Group/server default policy.
    Group,
    /// Channel policy inherited from a group unless overridden.
    Channel,
    /// Invite bootstrap snapshot for initial group join or first-contact DM.
    InviteBootstrap,
}

/// Committed scope id. Display names never belong in this value.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ConversationScope {
    /// Scope level.
    pub level: ConnectivityScopeLevel,
    /// Stable commitment for the group, DM, channel, invite, or app default.
    pub scope_id_commitment: String,
    /// Optional parent group commitment for channel inheritance.
    #[serde(default)]
    pub parent_scope_commitment: Option<String>,
}

impl ConversationScope {
    /// Construct and validate a committed conversation scope.
    pub fn new(
        level: ConnectivityScopeLevel,
        scope_id_commitment: impl Into<String>,
    ) -> Result<Self, TransportError> {
        let scope = Self {
            level,
            scope_id_commitment: scope_id_commitment.into(),
            parent_scope_commitment: None,
        };
        scope.validate()?;
        Ok(scope)
    }

    /// Return a copy with the parent group commitment used by channel policy inheritance.
    pub fn with_parent_scope_commitment(
        mut self,
        parent_scope_commitment: impl Into<String>,
    ) -> Result<Self, TransportError> {
        self.parent_scope_commitment = Some(parent_scope_commitment.into());
        self.validate()?;
        Ok(self)
    }

    /// Validate that the scope uses commitments rather than display names.
    pub fn validate(&self) -> Result<(), TransportError> {
        match self.level {
            ConnectivityScopeLevel::AppDefault => {
                if self.scope_id_commitment != "app_default" {
                    return Err(TransportError::InvalidConnectivityPolicy(
                        "app default scope must use the app_default commitment label".to_owned(),
                    ));
                }
            }
            ConnectivityScopeLevel::Dm
            | ConnectivityScopeLevel::Group
            | ConnectivityScopeLevel::Channel
            | ConnectivityScopeLevel::InviteBootstrap => {
                validate_commitment(&self.scope_id_commitment)?;
            }
        }
        if let Some(parent) = &self.parent_scope_commitment {
            validate_commitment(parent)?;
        }
        Ok(())
    }
}

/// Public-provider metadata posture for derived topics/tags.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMetadataPosture {
    /// Topic/tag is a deterministic hash commitment.
    HashedTopic,
    /// Topic/tag is random and signed in the invite/bootstrap snapshot.
    RandomTopic,
    /// Topic/tag rotates by epoch while preserving scope commitment.
    EpochRotatingTopic,
}

/// Ordered adapter fallback behavior for one scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterFallbackBehavior {
    /// Try every configured adapter until one is usable.
    TryAll,
    /// Select the first currently healthy adapter.
    FirstHealthy,
    /// Use only the manually selected adapter.
    ManualOnly,
}

/// Endpoint security class selected by policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalingEndpointSecurity {
    /// Production endpoints must use TLS/WSS/QUIC identity validation.
    ProductionTls,
    /// Cleartext loopback is permitted only for local development tests.
    LocalDevLoopback,
    /// Explicit self-hosted endpoint supplied by user/group/DM policy.
    SelfHostedExplicit,
}

/// Redacted trust label surfaced to UI/diagnostic surfaces.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterTrustLabel {
    /// Human-readable provider/operator label.
    pub label: String,
    /// Honest trust posture shown to users.
    pub posture: String,
}

impl AdapterTrustLabel {
    /// Construct a non-empty trust label.
    pub fn new(
        label: impl Into<String>,
        posture: impl Into<String>,
    ) -> Result<Self, TransportError> {
        let trust = Self {
            label: label.into(),
            posture: posture.into(),
        };
        if trust.label.trim().is_empty()
            || trust.label.trim() != trust.label
            || trust.posture.trim().is_empty()
            || trust.posture.trim() != trust.posture
        {
            return Err(TransportError::InvalidConnectivityPolicy(
                "trust labels must be non-empty and trimmed".to_owned(),
            ));
        }
        Ok(trust)
    }
}

/// Provider endpoint metadata safe to store in connectivity policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingProviderEndpoint {
    /// Adapter endpoint URL or multiaddr.
    pub endpoint: Endpoint,
    /// Endpoint security class.
    pub security: SignalingEndpointSecurity,
    /// Optional operator/region label. This is not a trust root.
    #[serde(default)]
    pub operator_label: Option<String>,
    /// Maximum provider message size, when known.
    #[serde(default)]
    pub max_message_bytes: Option<u32>,
    /// Whether retained/ephemeral presence is supported by the provider.
    pub retained_presence: bool,
    /// Optional endpoint identity/fingerprint commitment.
    #[serde(default)]
    pub trust_fingerprint: Option<String>,
}

impl SignalingProviderEndpoint {
    /// Construct provider endpoint metadata.
    #[must_use]
    pub fn new(endpoint: Endpoint, security: SignalingEndpointSecurity) -> Self {
        Self {
            endpoint,
            security,
            operator_label: None,
            max_message_bytes: None,
            retained_presence: false,
            trust_fingerprint: None,
        }
    }

    /// Validate endpoint scheme, TLS posture, and redacted labels.
    pub fn validate_for_kind(&self, kind: SignalingAdapterKind) -> Result<(), TransportError> {
        let value = self.endpoint.0.as_str();
        validate_endpoint_text(value)?;
        if let Some(label) = &self.operator_label {
            if label.trim().is_empty() || label.trim() != label {
                return Err(TransportError::InvalidConnectivityPolicy(
                    "operator label must be non-empty and trimmed".to_owned(),
                ));
            }
        }
        if let Some(fingerprint) = &self.trust_fingerprint {
            validate_commitment(fingerprint)?;
        }

        match self.security {
            SignalingEndpointSecurity::ProductionTls => {
                let valid = match kind {
                    SignalingAdapterKind::Mqtt => {
                        value.starts_with("mqtts://") || value.starts_with("wss://")
                    }
                    SignalingAdapterKind::Nostr => value.starts_with("wss://"),
                    SignalingAdapterKind::IpfsPubsub => is_ipfs_public_direct_peer_multiaddr(value),
                    SignalingAdapterKind::DiscryptQuicRendezvous => {
                        value.starts_with("quic://")
                            || value.starts_with("https://")
                            || value.starts_with("wss://")
                    }
                };
                if !valid {
                    return Err(TransportError::InvalidConnectivityPolicy(format!(
                        "production endpoint scheme is invalid for {}",
                        kind.canonical_name()
                    )));
                }
            }
            SignalingEndpointSecurity::LocalDevLoopback => {
                if !is_loopback_cleartext(value) {
                    return Err(TransportError::InvalidConnectivityPolicy(
                        "local-dev signaling endpoints must be loopback cleartext only".to_owned(),
                    ));
                }
            }
            SignalingEndpointSecurity::SelfHostedExplicit => {
                let valid = value.starts_with("mqtts://")
                    || value.starts_with("wss://")
                    || value.starts_with("quic://")
                    || value.starts_with("https://")
                    || is_ipfs_public_direct_peer_multiaddr(value);
                if !valid {
                    return Err(TransportError::InvalidConnectivityPolicy(
                        "self-hosted signaling endpoint must still use an explicit supported scheme"
                            .to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Adapter capabilities used by policy selection and conformance tests.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingAdapterCapabilities {
    /// Supports expiring presence publication.
    pub presence_ttl: bool,
    /// Supports trickled ICE candidate delivery.
    pub trickle_ice: bool,
    /// Supports room-wide sealed control broadcast.
    pub broadcast_control: bool,
    /// Supports retained presence that can be explicitly cleared on leave.
    pub retained_presence: bool,
    /// Supports redacted health telemetry.
    pub health_telemetry: bool,
}

impl SignalingAdapterCapabilities {
    /// Capabilities required by the shared production adapter conformance suite.
    #[must_use]
    pub const fn production_required() -> Self {
        Self {
            presence_ttl: true,
            trickle_ice: true,
            broadcast_control: true,
            retained_presence: false,
            health_telemetry: true,
        }
    }

    /// True when the adapter can satisfy the common production contract.
    #[must_use]
    pub const fn satisfies_production_contract(&self) -> bool {
        self.presence_ttl && self.trickle_ice && self.broadcast_control && self.health_telemetry
    }
}

/// One configured signaling adapter profile entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingAdapterProfile {
    /// Stable adapter profile id.
    pub profile_id: String,
    /// Adapter kind.
    pub kind: SignalingAdapterKind,
    /// Ordered provider endpoints for this adapter kind.
    pub endpoints: Vec<SignalingProviderEndpoint>,
    /// Metadata posture for provider-visible topics/tags.
    pub metadata_posture: ProviderMetadataPosture,
    /// Provider capabilities used by conformance and fallback.
    pub capabilities: SignalingAdapterCapabilities,
    /// UI/diagnostic trust label.
    pub trust_label: AdapterTrustLabel,
}

impl SignalingAdapterProfile {
    /// Validate profile id, endpoints, and production capability shape.
    pub fn validate(&self) -> Result<(), TransportError> {
        validate_profile_id(&self.profile_id)?;
        if self.endpoints.is_empty() {
            return Err(TransportError::InvalidConnectivityPolicy(
                "adapter profile must contain at least one endpoint".to_owned(),
            ));
        }
        if !self.capabilities.satisfies_production_contract() {
            return Err(TransportError::InvalidConnectivityPolicy(
                "adapter profile does not satisfy production signaling capabilities".to_owned(),
            ));
        }
        for endpoint in &self.endpoints {
            endpoint.validate_for_kind(self.kind)?;
        }
        Ok(())
    }
}

/// ICE profile separated from signaling adapter selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IceProfile {
    /// Stable ICE profile id.
    pub profile_id: String,
    /// Signed/profiled ICE endpoint policy.
    pub policy: IceEndpointPolicy,
    /// Honest UI label for this profile.
    pub label: String,
}

impl IceProfile {
    /// Construct and validate an ICE profile.
    pub fn new(
        profile_id: impl Into<String>,
        policy: IceEndpointPolicy,
        label: impl Into<String>,
    ) -> Result<Self, TransportError> {
        let profile = Self {
            profile_id: profile_id.into(),
            policy,
            label: label.into(),
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Validate profile metadata and ICE endpoint policy.
    pub fn validate(&self) -> Result<(), TransportError> {
        validate_profile_id(&self.profile_id)?;
        if self.label.trim().is_empty() || self.label.trim() != self.label {
            return Err(TransportError::InvalidConnectivityPolicy(
                "ICE profile label must be non-empty and trimmed".to_owned(),
            ));
        }
        self.policy.validate()
    }
}

/// Scoped connectivity profile selected by app, DM, group, channel, or invite bootstrap.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectivityPolicy {
    /// Scope this policy applies to.
    pub scope: ConversationScope,
    /// Stable signaling profile id.
    pub signaling_profile_id: String,
    /// Ordered signaling adapters for this scope.
    pub signaling_adapters: Vec<SignalingAdapterProfile>,
    /// ICE profile id used independently of signaling.
    pub ice_profile_id: String,
    /// ICE profile snapshot for invite/bootstrap or persisted scope.
    pub ice_profile: IceProfile,
    /// Adapter fallback behavior.
    pub fallback_behavior: AdapterFallbackBehavior,
    /// UI/diagnostic trust label for the selected policy.
    pub trust_label: AdapterTrustLabel,
}

impl ConnectivityPolicy {
    /// Validate scope, profile ids, adapter order, and ICE policy.
    pub fn validate(&self) -> Result<(), TransportError> {
        self.scope.validate()?;
        validate_profile_id(&self.signaling_profile_id)?;
        validate_profile_id(&self.ice_profile_id)?;
        if self.ice_profile.profile_id != self.ice_profile_id {
            return Err(TransportError::InvalidConnectivityPolicy(
                "connectivity policy ice_profile_id must match embedded ICE profile".to_owned(),
            ));
        }
        if self.signaling_adapters.is_empty() {
            return Err(TransportError::InvalidConnectivityPolicy(
                "connectivity policy requires at least one signaling adapter".to_owned(),
            ));
        }
        for adapter in &self.signaling_adapters {
            adapter.validate()?;
        }
        Ok(())
    }

    /// Return the ordered production adapter kinds configured for this scope.
    #[must_use]
    pub fn adapter_kinds(&self) -> Vec<SignalingAdapterKind> {
        self.signaling_adapters
            .iter()
            .map(|adapter| adapter.kind)
            .collect()
    }
}

/// Policy source selected after applying precedence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectivityPolicySource {
    /// Invite bootstrap snapshot wins for initial join/contact.
    InviteBootstrap,
    /// Channel-specific override.
    ChannelOverride,
    /// Group/server default.
    GroupDefault,
    /// DM-specific default/override.
    DmOverride,
    /// Global app default.
    AppDefault,
}

/// Resolved policy plus the source that won precedence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EffectiveConnectivityPolicy {
    /// Winning source.
    pub source: ConnectivityPolicySource,
    /// Winning policy snapshot.
    pub policy: ConnectivityPolicy,
}

/// Persisted connectivity policies by scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectivityPolicyStore {
    /// Global app default policy.
    pub app_default: ConnectivityPolicy,
    /// Per-DM policies keyed by committed DM scope id.
    #[serde(default)]
    pub dm_policies: BTreeMap<String, ConnectivityPolicy>,
    /// Group defaults keyed by committed group scope id.
    #[serde(default)]
    pub group_policies: BTreeMap<String, ConnectivityPolicy>,
    /// Channel overrides keyed by committed channel scope id.
    #[serde(default)]
    pub channel_policies: BTreeMap<String, ConnectivityPolicy>,
}

impl ConnectivityPolicyStore {
    /// Resolve policy precedence for a target scope, optionally with an invite bootstrap snapshot.
    pub fn resolve(
        &self,
        target: &ConversationScope,
        invite_bootstrap: Option<&ConnectivityPolicy>,
    ) -> Result<EffectiveConnectivityPolicy, TransportError> {
        self.app_default.validate()?;
        target.validate()?;
        if let Some(invite) = invite_bootstrap {
            invite.validate()?;
            return Ok(EffectiveConnectivityPolicy {
                source: ConnectivityPolicySource::InviteBootstrap,
                policy: invite.clone(),
            });
        }

        match target.level {
            ConnectivityScopeLevel::Channel => {
                if let Some(policy) = self.channel_policies.get(&target.scope_id_commitment) {
                    policy.validate()?;
                    return Ok(EffectiveConnectivityPolicy {
                        source: ConnectivityPolicySource::ChannelOverride,
                        policy: policy.clone(),
                    });
                }
                if let Some(parent) = &target.parent_scope_commitment {
                    if let Some(policy) = self.group_policies.get(parent) {
                        policy.validate()?;
                        return Ok(EffectiveConnectivityPolicy {
                            source: ConnectivityPolicySource::GroupDefault,
                            policy: policy.clone(),
                        });
                    }
                }
            }
            ConnectivityScopeLevel::Group => {
                if let Some(policy) = self.group_policies.get(&target.scope_id_commitment) {
                    policy.validate()?;
                    return Ok(EffectiveConnectivityPolicy {
                        source: ConnectivityPolicySource::GroupDefault,
                        policy: policy.clone(),
                    });
                }
            }
            ConnectivityScopeLevel::Dm => {
                if let Some(policy) = self.dm_policies.get(&target.scope_id_commitment) {
                    policy.validate()?;
                    return Ok(EffectiveConnectivityPolicy {
                        source: ConnectivityPolicySource::DmOverride,
                        policy: policy.clone(),
                    });
                }
            }
            ConnectivityScopeLevel::AppDefault | ConnectivityScopeLevel::InviteBootstrap => {}
        }

        Ok(EffectiveConnectivityPolicy {
            source: ConnectivityPolicySource::AppDefault,
            policy: self.app_default.clone(),
        })
    }
}

/// Adapter-facing rendezvous capability derived by policy code before adapter use.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RendezvousCapability {
    /// Scope commitment, not display name.
    pub scope: ConversationScope,
    /// Adapter kind this capability is valid for.
    pub adapter_kind: SignalingAdapterKind,
    /// Provider-visible derived topic/tag/room id.
    pub topic: String,
    /// Optional publish token commitment.
    #[serde(default)]
    pub publish_token_commitment: Option<String>,
    /// Optional subscribe token commitment.
    #[serde(default)]
    pub subscribe_token_commitment: Option<String>,
    /// Message/presence TTL in seconds.
    pub ttl_seconds: u32,
    /// Provider metadata posture.
    pub metadata_posture: ProviderMetadataPosture,
    /// Redacted trust label.
    pub trust_label: AdapterTrustLabel,
}

impl RendezvousCapability {
    /// Derive adapter-facing topic and token commitments from bootstrap material.
    pub fn derive(
        scope: ConversationScope,
        adapter_kind: SignalingAdapterKind,
        bootstrap_secret: &[u8],
        random_entropy: &[u8],
        ttl_seconds: u32,
        metadata_posture: ProviderMetadataPosture,
        trust_label: AdapterTrustLabel,
    ) -> Result<Self, TransportError> {
        scope.validate()?;
        if bootstrap_secret.len() < 32 || random_entropy.len() < 16 {
            return Err(TransportError::InvalidConnectivityPolicy(
                "rendezvous derivation requires a bootstrap secret and random entropy".to_owned(),
            ));
        }
        if ttl_seconds == 0 || ttl_seconds > 86_400 {
            return Err(TransportError::InvalidConnectivityPolicy(
                "rendezvous TTL must be between 1 and 86400 seconds".to_owned(),
            ));
        }

        let topic_commitment = derive_hex(
            b"discrypt-rendezvous-topic-v1",
            &scope,
            adapter_kind,
            bootstrap_secret,
            random_entropy,
        );
        let publish_token_commitment = derive_hex(
            b"discrypt-rendezvous-publish-token-v1",
            &scope,
            adapter_kind,
            bootstrap_secret,
            random_entropy,
        );
        let subscribe_token_commitment = derive_hex(
            b"discrypt-rendezvous-subscribe-token-v1",
            &scope,
            adapter_kind,
            bootstrap_secret,
            random_entropy,
        );

        Ok(Self {
            scope,
            adapter_kind,
            topic: format!(
                "discrypt-rv1-{}-{topic_commitment}",
                adapter_kind.canonical_name()
            ),
            publish_token_commitment: Some(publish_token_commitment),
            subscribe_token_commitment: Some(subscribe_token_commitment),
            ttl_seconds,
            metadata_posture,
            trust_label,
        })
    }
}

/// Deterministically derive a committed scope id for tests, invites, and persisted policies.
#[must_use]
pub fn derive_scope_commitment(
    level: ConnectivityScopeLevel,
    stable_scope_secret: &[u8],
    domain_separator: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"discrypt-connectivity-scope-v1");
    hasher.update(level_name(level).as_bytes());
    hasher.update(domain_separator.as_bytes());
    hasher.update(stable_scope_secret);
    lower_hex(&hasher.finalize())
}

fn derive_hex(
    domain_separator: &[u8],
    scope: &ConversationScope,
    adapter_kind: SignalingAdapterKind,
    bootstrap_secret: &[u8],
    random_entropy: &[u8],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain_separator);
    hasher.update(level_name(scope.level).as_bytes());
    hasher.update(scope.scope_id_commitment.as_bytes());
    if let Some(parent) = &scope.parent_scope_commitment {
        hasher.update(parent.as_bytes());
    }
    hasher.update(adapter_kind.canonical_name().as_bytes());
    hasher.update(bootstrap_secret);
    hasher.update(random_entropy);
    lower_hex(&hasher.finalize())
}

fn level_name(level: ConnectivityScopeLevel) -> &'static str {
    match level {
        ConnectivityScopeLevel::AppDefault => "app_default",
        ConnectivityScopeLevel::Dm => "dm",
        ConnectivityScopeLevel::Group => "group",
        ConnectivityScopeLevel::Channel => "channel",
        ConnectivityScopeLevel::InviteBootstrap => "invite_bootstrap",
    }
}

fn validate_commitment(value: &str) -> Result<(), TransportError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(TransportError::InvalidConnectivityPolicy(
            "scope and endpoint commitments must be 64 hex characters".to_owned(),
        ));
    }
    Ok(())
}

fn validate_profile_id(value: &str) -> Result<(), TransportError> {
    if value.is_empty()
        || value.len() > 96
        || value.trim() != value
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(TransportError::InvalidConnectivityPolicy(
            "profile ids must be trimmed ASCII token strings".to_owned(),
        ));
    }
    Ok(())
}

fn validate_endpoint_text(value: &str) -> Result<(), TransportError> {
    if value.is_empty()
        || value.len() > 1024
        || value.trim() != value
        || value.chars().any(char::is_whitespace)
    {
        return Err(TransportError::InvalidConnectivityPolicy(
            "signaling endpoints must be non-empty, trimmed, and whitespace-free".to_owned(),
        ));
    }
    Ok(())
}

fn is_loopback_cleartext(value: &str) -> bool {
    ["http://127.0.0.1:", "ws://127.0.0.1:", "mqtt://127.0.0.1:"]
        .iter()
        .any(|prefix| value.starts_with(prefix))
        || ["http://[::1]:", "ws://[::1]:", "mqtt://[::1]:"]
            .iter()
            .any(|prefix| value.starts_with(prefix))
        || value.starts_with("/ip4/127.0.0.1/")
        || value.starts_with("/ip6/::1/")
}

fn is_ipfs_public_direct_peer_multiaddr(value: &str) -> bool {
    let value = value.strip_prefix("libp2p://").unwrap_or(value);
    (value.starts_with("/ip4/") || value.starts_with("/ip6/"))
        && value.contains("/tcp/")
        && value.contains("/p2p/")
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TurnServerConfig;

    fn trust(label: &str) -> Result<AdapterTrustLabel, TransportError> {
        AdapterTrustLabel::new(label, "redacted provider posture")
    }

    fn endpoint(url: &str, security: SignalingEndpointSecurity) -> SignalingProviderEndpoint {
        SignalingProviderEndpoint::new(Endpoint::new(url), security)
    }

    fn adapter(
        kind: SignalingAdapterKind,
        url: &str,
    ) -> Result<SignalingAdapterProfile, TransportError> {
        let security = if url.contains("127.0.0.1") {
            SignalingEndpointSecurity::LocalDevLoopback
        } else {
            SignalingEndpointSecurity::ProductionTls
        };
        Ok(SignalingAdapterProfile {
            profile_id: format!("profile-{}", kind.canonical_name()),
            kind,
            endpoints: vec![endpoint(url, security)],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: trust(kind.canonical_name())?,
        })
    }

    fn ice_profile(id: &str) -> Result<IceProfile, TransportError> {
        IceProfile::new(
            id,
            IceEndpointPolicy::new(
                vec![Endpoint::new("stun:stun.l.google.com:19302")],
                vec![TurnServerConfig::new(
                    Endpoint::new("turns:turn.example.invalid:5349"),
                    None,
                    None,
                    None,
                )],
            )?,
            id,
        )
    }

    fn scope(
        level: ConnectivityScopeLevel,
        seed: &[u8],
    ) -> Result<ConversationScope, TransportError> {
        ConversationScope::new(level, derive_scope_commitment(level, seed, "test"))
    }

    fn policy(
        scope: ConversationScope,
        adapter_kind: SignalingAdapterKind,
        endpoint_url: &str,
        profile_id: &str,
    ) -> Result<ConnectivityPolicy, TransportError> {
        Ok(ConnectivityPolicy {
            scope,
            signaling_profile_id: profile_id.to_owned(),
            signaling_adapters: vec![adapter(adapter_kind, endpoint_url)?],
            ice_profile_id: format!("{profile_id}.ice"),
            ice_profile: ice_profile(&format!("{profile_id}.ice"))?,
            fallback_behavior: AdapterFallbackBehavior::TryAll,
            trust_label: trust(profile_id)?,
        })
    }

    #[test]
    fn policy_store_resolves_invite_channel_group_dm_then_app_precedence(
    ) -> Result<(), TransportError> {
        let app = policy(
            ConversationScope::new(ConnectivityScopeLevel::AppDefault, "app_default")?,
            SignalingAdapterKind::Mqtt,
            "wss://mqtt.example.invalid",
            "app",
        )?;
        let dm_scope = scope(ConnectivityScopeLevel::Dm, b"dm pair")?;
        let group_scope = scope(ConnectivityScopeLevel::Group, b"group")?;
        let channel_scope = scope(ConnectivityScopeLevel::Channel, b"channel")?
            .with_parent_scope_commitment(group_scope.scope_id_commitment.clone())?;
        let invite_scope = scope(ConnectivityScopeLevel::InviteBootstrap, b"invite")?;

        let dm = policy(
            dm_scope.clone(),
            SignalingAdapterKind::Nostr,
            "wss://relay.example.invalid",
            "dm",
        )?;
        let group = policy(
            group_scope.clone(),
            SignalingAdapterKind::Mqtt,
            "wss://group-mqtt.example.invalid",
            "group",
        )?;
        let channel = policy(
            channel_scope.clone(),
            SignalingAdapterKind::DiscryptQuicRendezvous,
            "quic://signal.example.invalid",
            "channel",
        )?;
        let invite = policy(
            invite_scope,
            SignalingAdapterKind::IpfsPubsub,
            "/dns/bootstrap.example.invalid/tcp/4001/p2p/12D3KooWBootstrap",
            "invite",
        )?;

        let mut store = ConnectivityPolicyStore {
            app_default: app,
            dm_policies: BTreeMap::new(),
            group_policies: BTreeMap::new(),
            channel_policies: BTreeMap::new(),
        };
        store
            .dm_policies
            .insert(dm_scope.scope_id_commitment.clone(), dm);
        store
            .group_policies
            .insert(group_scope.scope_id_commitment.clone(), group);
        store
            .channel_policies
            .insert(channel_scope.scope_id_commitment.clone(), channel);

        let invite_result = store.resolve(&dm_scope, Some(&invite))?;
        assert_eq!(
            invite_result.source,
            ConnectivityPolicySource::InviteBootstrap
        );
        assert_eq!(
            invite_result.policy.adapter_kinds(),
            vec![SignalingAdapterKind::IpfsPubsub]
        );

        let channel_result = store.resolve(&channel_scope, None)?;
        assert_eq!(
            channel_result.source,
            ConnectivityPolicySource::ChannelOverride
        );
        assert_eq!(
            channel_result.policy.adapter_kinds(),
            vec![SignalingAdapterKind::DiscryptQuicRendezvous]
        );

        let inherited_channel = scope(ConnectivityScopeLevel::Channel, b"other channel")?
            .with_parent_scope_commitment(group_scope.scope_id_commitment)?;
        let inherited_result = store.resolve(&inherited_channel, None)?;
        assert_eq!(
            inherited_result.source,
            ConnectivityPolicySource::GroupDefault
        );

        let dm_result = store.resolve(&dm_scope, None)?;
        assert_eq!(dm_result.source, ConnectivityPolicySource::DmOverride);
        Ok(())
    }

    #[test]
    fn rendezvous_capability_uses_commitments_and_excludes_display_names(
    ) -> Result<(), TransportError> {
        let display_name = "private family voice channel";
        let scope = scope(ConnectivityScopeLevel::Channel, display_name.as_bytes())?;
        let capability = RendezvousCapability::derive(
            scope.clone(),
            SignalingAdapterKind::Mqtt,
            b"bootstrap secret with at least thirty two bytes",
            b"random-topic-entropy",
            300,
            ProviderMetadataPosture::HashedTopic,
            trust("mqtt")?,
        )?;

        assert!(capability.topic.starts_with("discrypt-rv1-mqtt-"));
        assert!(!capability.topic.contains("private"));
        assert!(!capability.topic.contains("family"));
        assert_ne!(capability.scope.scope_id_commitment, display_name);

        let nostr = RendezvousCapability::derive(
            scope,
            SignalingAdapterKind::Nostr,
            b"bootstrap secret with at least thirty two bytes",
            b"random-topic-entropy",
            300,
            ProviderMetadataPosture::HashedTopic,
            trust("nostr")?,
        )?;
        assert_ne!(capability.topic, nostr.topic);
        Ok(())
    }

    #[test]
    fn ipfs_profile_accepts_direct_topic_peer_multiaddr() -> Result<(), TransportError> {
        let profile = adapter(
            SignalingAdapterKind::IpfsPubsub,
            "/ip4/203.0.113.10/tcp/4001/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
        )?;

        assert!(profile.validate().is_ok());
        Ok(())
    }

    #[test]
    fn ipfs_profile_rejects_dns_bootstrap_until_topic_peer_discovery_is_audited(
    ) -> Result<(), TransportError> {
        let profile = adapter(
            SignalingAdapterKind::IpfsPubsub,
            "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
        )?;

        let error = profile
            .validate()
            .expect_err("DNS bootstrap is not an accepted production IPFS default");
        assert!(format!("{error}").contains("production endpoint scheme is invalid"));
        Ok(())
    }

    #[test]
    fn adapter_profile_rejects_plaintext_public_provider_urls() -> Result<(), TransportError> {
        let mut public_mqtt =
            adapter(SignalingAdapterKind::Mqtt, "mqtts://broker.example.invalid")?;
        public_mqtt.endpoints = vec![endpoint(
            "mqtt://broker.example.invalid",
            SignalingEndpointSecurity::ProductionTls,
        )];
        assert!(matches!(
            public_mqtt.validate(),
            Err(TransportError::InvalidConnectivityPolicy(_))
        ));

        let local_mqtt = adapter(SignalingAdapterKind::Mqtt, "mqtt://127.0.0.1:1883")?;
        assert!(local_mqtt.validate().is_ok());
        Ok(())
    }
}

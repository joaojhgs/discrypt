//! Invite, password-admission, and final MLS Welcome abstractions.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use chrono::{DateTime, Utc};
use discrypt_transport::{IceEndpointPolicy, IceServerConfig};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

/// ADR-005 selected password/admission design.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AdmissionPasswordProtocol {
    /// Current production path: an online helper verifies the password and signs a short-lived proof.
    OnlineAuthorizedHelper,
    /// Reserved future path for a concrete OPAQUE/PAKE dependency after dependency review.
    OpaquePakeReserved,
}

/// ADR-005 decision record for password-gated admission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdmissionPasswordDecision {
    /// Selected launch protocol.
    pub selected_protocol: AdmissionPasswordProtocol,
    /// Why no offline verifier is allowed in invites or storage.
    pub no_offline_verifier: &'static str,
    /// Rate-limit proof source.
    pub rate_limit_proof: &'static str,
    /// Final admission requirement after password/helper success.
    pub final_admission_gate: &'static str,
    /// UI/error states that frontend and command surfaces must expose.
    pub ux_error_states: &'static [&'static str],
}

impl AdmissionPasswordDecision {
    /// Return true when the code-level decision covers every ADR-005 axis.
    #[must_use]
    pub fn covers_adr_005(&self) -> bool {
        self.selected_protocol == AdmissionPasswordProtocol::OnlineAuthorizedHelper
            && self.no_offline_verifier.contains("OfflineVerifierRejected")
            && self.rate_limit_proof.contains("OnlineAdmissionHelper")
            && self.rate_limit_proof.contains("max_attempts")
            && self.final_admission_gate.contains("AuthorizedWelcome")
            && self.final_admission_gate.contains("MLS Welcome/add")
            && self.ux_error_states.contains(&"password_rejected")
            && self.ux_error_states.contains(&"helper_proof_expired")
            && self.ux_error_states.contains(&"welcome_required")
            && self.ux_error_states.contains(&"offline_verifier_rejected")
    }
}

/// Current ADR-005 password/admission decision.
#[must_use]
pub fn admission_password_decision() -> AdmissionPasswordDecision {
    AdmissionPasswordDecision {
        selected_protocol: AdmissionPasswordProtocol::OnlineAuthorizedHelper,
        no_offline_verifier: "PasswordGate::OfflineVerifier is rejected with InviteError::OfflineVerifierRejected; invite descriptors carry no offline-copyable verifier material.",
        rate_limit_proof: "OnlineAdmissionHelper stores a private password commitment server-side, counts attempts per subject with max_attempts, and returns the same PasswordRejected error for wrong password and over-limit cases.",
        final_admission_gate: "AdmissionController requires a valid AuthorizedHelperProof or PAKE result plus exact AuthorizedWelcome MLS Welcome/add authorization before invite consumption.",
        ux_error_states: &[
            "password_rejected",
            "helper_mismatch",
            "helper_proof_expired",
            "welcome_required",
            "welcome_invalid",
            "offline_verifier_rejected",
        ],
    }
}

/// Invite object with expiry/revoke/max-use controls.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Invite {
    /// Stable invite id.
    pub id: Uuid,
    /// Hash of the room secret; raw link secret is not stored.
    pub room_secret_hash: [u8; 32],
    /// Expiry timestamp.
    pub expires_at: DateTime<Utc>,
    /// Maximum uses.
    pub max_uses: u32,
    /// Current uses.
    pub uses: u32,
    /// Revocation flag.
    pub revoked: bool,
}

/// Invite/admission errors.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum InviteError {
    /// Invite expired.
    #[error("invite expired")]
    Expired,
    /// Invite revoked.
    #[error("invite revoked")]
    Revoked,
    /// Invite max uses exhausted.
    #[error("invite exhausted")]
    Exhausted,
    /// Invite id was not found in the production invite store.
    #[error("invite not found")]
    NotFound,
    /// Invite issuer signature is malformed or invalid.
    #[error("invite issuer signature invalid")]
    InvalidIssuerSignature,
    /// Signaling endpoint is malformed or violates its endpoint policy.
    #[error("invite signaling endpoint invalid")]
    InvalidSignalingEndpoint,
    /// Signaling trust metadata is malformed.
    #[error("invite signaling trust metadata invalid")]
    InvalidTrustMetadata,
    /// Signaling endpoint policy is malformed or unsupported.
    #[error("invite signaling endpoint policy invalid")]
    InvalidEndpointPolicy,
    /// Password gate is not backed by PAKE/OPAQUE/helper rate limiting.
    #[error("offline verifier cannot enforce rate limits")]
    OfflineVerifierRejected,
    /// Password proof failed or exceeded rate limits.
    #[error("password gate rejected")]
    PasswordRejected,
    /// Online helper proof does not match this admission gate.
    #[error("helper proof does not match admission gate")]
    HelperMismatch,
    /// Online helper proof expired.
    #[error("helper proof expired")]
    HelperProofExpired,
    /// Online helper proof signature is malformed or invalid.
    #[error("helper proof signature invalid")]
    InvalidHelperProofSignature,
    /// Final MLS add/Welcome authorization is absent.
    #[error("authorized MLS welcome required")]
    WelcomeRequired,
    /// Final MLS Welcome authorization expired.
    #[error("authorized MLS welcome expired")]
    WelcomeExpired,
    /// Final MLS Welcome authorization targets a different invite.
    #[error("authorized MLS welcome invite mismatch")]
    WelcomeInviteMismatch,
    /// Final MLS Welcome authorization signature/hash is invalid.
    #[error("authorized MLS welcome invalid")]
    InvalidWelcomeAuthorization,
}

/// Policy that tells joiners which endpoint classes are allowed for rendezvous.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteEndpointPolicy {
    /// Production endpoint: TLS Web/HTTP or QUIC rendezvous only.
    ProductionTls,
    /// Local development endpoint: loopback-only cleartext is allowed.
    LocalDevLoopback,
}

impl InviteEndpointPolicy {
    /// Stable string encoded into signed descriptors and invite links.
    #[must_use]
    pub fn canonical_name(&self) -> &'static str {
        match self {
            Self::ProductionTls => "production_tls",
            Self::LocalDevLoopback => "local_dev_loopback",
        }
    }

    fn validates_endpoint(&self, endpoint: &str) -> bool {
        match self {
            Self::ProductionTls => {
                endpoint.starts_with("https://")
                    || endpoint.starts_with("wss://")
                    || endpoint.starts_with("quic://")
            }
            Self::LocalDevLoopback => {
                endpoint.starts_with("http://127.0.0.1:")
                    || endpoint.starts_with("ws://127.0.0.1:")
                    || endpoint.starts_with("http://[::1]:")
                    || endpoint.starts_with("ws://[::1]:")
            }
        }
    }
}

/// Joiner-visible signaling trust metadata pinned by the signed invite descriptor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InviteTrustMetadata {
    /// Hex fingerprint for the signaling server key/certificate expected by the joiner.
    pub signaling_fingerprint: String,
    /// Human-readable trust posture; does not grant identity/group trust by itself.
    pub trust_status: String,
}

impl InviteTrustMetadata {
    /// Construct signaling trust metadata after validating the fingerprint shape.
    pub fn new(
        signaling_fingerprint: impl Into<String>,
        trust_status: impl Into<String>,
    ) -> Result<Self, InviteError> {
        let metadata = Self {
            signaling_fingerprint: signaling_fingerprint.into(),
            trust_status: trust_status.into(),
        };
        metadata.validate()?;
        Ok(metadata)
    }

    fn validate(&self) -> Result<(), InviteError> {
        if !is_hex_fingerprint(&self.signaling_fingerprint) || self.trust_status.trim().is_empty() {
            return Err(InviteError::InvalidTrustMetadata);
        }
        Ok(())
    }
}

/// Production invite metadata required to locate and validate the rendezvous endpoint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InviteSignalingMetadata {
    /// Public signaling rendezvous endpoint. It is signed but carries no room/group secret.
    pub signaling_endpoint: String,
    /// Endpoint policy joiners must enforce before using the endpoint.
    pub endpoint_policy: InviteEndpointPolicy,
    /// Joiner-visible endpoint trust material.
    pub trust: InviteTrustMetadata,
    /// Signed ICE/STUN/TURN endpoint policy used to build typed transport config.
    #[serde(default)]
    pub ice_endpoint_policy: IceEndpointPolicy,
}

impl InviteSignalingMetadata {
    /// Construct and validate invite signaling metadata.
    pub fn new(
        signaling_endpoint: impl Into<String>,
        endpoint_policy: InviteEndpointPolicy,
        trust: InviteTrustMetadata,
    ) -> Result<Self, InviteError> {
        let metadata = Self {
            signaling_endpoint: signaling_endpoint.into(),
            endpoint_policy,
            trust,
            ice_endpoint_policy: IceEndpointPolicy::default_production(),
        };
        metadata.validate()?;
        Ok(metadata)
    }

    /// Deterministic safe default used by local command surfaces and tests.
    #[must_use]
    pub fn default_production() -> Self {
        let endpoint = "https://signaling.discrypt.invalid/v1/rendezvous".to_owned();
        let fingerprint = signaling_fingerprint_for_endpoint(&endpoint);
        Self {
            signaling_endpoint: endpoint,
            endpoint_policy: InviteEndpointPolicy::ProductionTls,
            trust: InviteTrustMetadata {
                signaling_fingerprint: fingerprint,
                trust_status: "signed endpoint fingerprint; verify before MLS Welcome".to_owned(),
            },
            ice_endpoint_policy: IceEndpointPolicy::default_production(),
        }
    }

    /// Return this signaling metadata with an explicit signed ICE endpoint policy.
    pub fn with_ice_endpoint_policy(
        mut self,
        ice_endpoint_policy: IceEndpointPolicy,
    ) -> Result<Self, InviteError> {
        ice_endpoint_policy
            .validate()
            .map_err(|_| InviteError::InvalidEndpointPolicy)?;
        self.ice_endpoint_policy = ice_endpoint_policy;
        self.validate()?;
        Ok(self)
    }

    /// Validate endpoint, policy, trust metadata, and ICE endpoint policy without exposing invite secrets.
    pub fn validate(&self) -> Result<(), InviteError> {
        if self.signaling_endpoint.trim() != self.signaling_endpoint
            || self.signaling_endpoint.is_empty()
            || self.signaling_endpoint.len() > 512
        {
            return Err(InviteError::InvalidSignalingEndpoint);
        }
        if !self
            .endpoint_policy
            .validates_endpoint(&self.signaling_endpoint)
        {
            return Err(InviteError::InvalidSignalingEndpoint);
        }
        self.trust.validate()?;
        self.ice_endpoint_policy
            .validate()
            .map_err(|_| InviteError::InvalidEndpointPolicy)
    }
}

/// Production invite descriptor stored and exchanged without exposing the raw room secret.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoredInvite {
    /// Opaque random invite id; not derived from room/group names or counters.
    pub invite_id: String,
    /// Domain-separated commitment to the room secret.
    pub room_secret_commitment: [u8; 32],
    /// Issuer device verification key.
    pub issuer_public_key: Vec<u8>,
    /// Issuer signature over the canonical invite descriptor.
    pub issuer_signature: Vec<u8>,
    /// Signed signaling endpoint and trust metadata for joiner rendezvous.
    pub signaling_metadata: InviteSignalingMetadata,
    /// Expiry timestamp.
    pub expires_at: DateTime<Utc>,
    /// Maximum accepted uses.
    pub max_uses: u32,
    /// Consumed uses.
    pub consumed_uses: u32,
    /// Governance event id that revoked this invite, if any.
    pub revocation_event_id: Option<String>,
}

impl StoredInvite {
    /// Verify the issuer signature on this invite descriptor.
    pub fn verify_issuer_signature(&self) -> Result<(), InviteError> {
        self.signaling_metadata.validate()?;
        let verifying_key = VerifyingKey::from_bytes(
            &self
                .issuer_public_key
                .as_slice()
                .try_into()
                .map_err(|_| InviteError::InvalidIssuerSignature)?,
        )
        .map_err(|_| InviteError::InvalidIssuerSignature)?;
        let signature = Signature::from_slice(&self.issuer_signature)
            .map_err(|_| InviteError::InvalidIssuerSignature)?;
        verifying_key
            .verify(&self.signing_bytes(), &signature)
            .map_err(|_| InviteError::InvalidIssuerSignature)
    }

    /// Verify the signed invite descriptor and resolve its ICE endpoint policy into typed transport config.
    pub fn ice_server_config(
        &self,
        group_policy: Option<&IceEndpointPolicy>,
    ) -> Result<IceServerConfig, InviteError> {
        self.verify_issuer_signature()?;
        IceEndpointPolicy::resolve(
            Some(&self.signaling_metadata.ice_endpoint_policy),
            group_policy,
        )
        .map_err(|_| InviteError::InvalidEndpointPolicy)
    }

    /// Verify the signed invite descriptor and reject expired/incomplete TURN credentials at `now`.
    pub fn ice_server_config_at(
        &self,
        group_policy: Option<&IceEndpointPolicy>,
        now: DateTime<Utc>,
    ) -> Result<IceServerConfig, InviteError> {
        self.verify_issuer_signature()?;
        IceEndpointPolicy::resolve_at(
            Some(&self.signaling_metadata.ice_endpoint_policy),
            group_policy,
            now,
        )
        .map_err(|_| InviteError::InvalidEndpointPolicy)
    }

    /// True when the invite has a revocation governance event.
    #[must_use]
    pub fn revoked(&self) -> bool {
        self.revocation_event_id.is_some()
    }

    fn sign(
        invite_id: String,
        room_secret_commitment: [u8; 32],
        expires_at: DateTime<Utc>,
        max_uses: u32,
        signaling_metadata: InviteSignalingMetadata,
        issuer: &SigningKey,
    ) -> Self {
        let issuer_public_key = issuer.verifying_key().to_bytes().to_vec();
        let mut invite = Self {
            invite_id,
            room_secret_commitment,
            issuer_public_key,
            issuer_signature: Vec::new(),
            signaling_metadata,
            expires_at,
            max_uses,
            consumed_uses: 0,
            revocation_event_id: None,
        };
        invite.issuer_signature = issuer.sign(&invite.signing_bytes()).to_bytes().to_vec();
        invite
    }

    fn signing_bytes(&self) -> Vec<u8> {
        canonical_invite_signing_bytes(
            &self.invite_id,
            &self.room_secret_commitment,
            &self.issuer_public_key,
            &self.signaling_metadata,
            self.expires_at,
            self.max_uses,
        )
    }
}

/// Production invite store enforcing opaque ids, commitments, issuer signatures,
/// revocation, expiry, max-use, and consumed-use accounting.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct InviteStore {
    invites: BTreeMap<String, StoredInvite>,
}

impl InviteStore {
    /// Create an empty invite store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Issue and persist a signed invite descriptor.
    pub fn issue_invite(
        &mut self,
        room_secret: &[u8],
        expires_at: DateTime<Utc>,
        max_uses: u32,
        issuer: &SigningKey,
    ) -> StoredInvite {
        let invite = StoredInvite::sign(
            opaque_invite_id(),
            room_secret_commitment(room_secret),
            expires_at,
            max_uses.max(1),
            InviteSignalingMetadata::default_production(),
            issuer,
        );
        self.invites
            .insert(invite.invite_id.clone(), invite.clone());
        invite
    }

    /// Issue and persist a signed invite descriptor with explicit production signaling metadata.
    pub fn issue_invite_with_metadata(
        &mut self,
        room_secret: &[u8],
        expires_at: DateTime<Utc>,
        max_uses: u32,
        signaling_metadata: InviteSignalingMetadata,
        issuer: &SigningKey,
    ) -> Result<StoredInvite, InviteError> {
        signaling_metadata.validate()?;
        let invite = StoredInvite::sign(
            opaque_invite_id(),
            room_secret_commitment(room_secret),
            expires_at,
            max_uses.max(1),
            signaling_metadata,
            issuer,
        );
        self.invites
            .insert(invite.invite_id.clone(), invite.clone());
        Ok(invite)
    }

    /// Return a stored invite by opaque id.
    #[must_use]
    pub fn get(&self, invite_id: &str) -> Option<&StoredInvite> {
        self.invites.get(invite_id)
    }

    /// Revoke an invite with the governance event id that authorized revocation.
    pub fn revoke(
        &mut self,
        invite_id: &str,
        revocation_event_id: impl Into<String>,
    ) -> Result<(), InviteError> {
        let invite = self
            .invites
            .get_mut(invite_id)
            .ok_or(InviteError::NotFound)?;
        invite.revocation_event_id = Some(revocation_event_id.into());
        Ok(())
    }

    /// Consume one use after validating signature, revocation, expiry, and max-use.
    pub fn consume(&mut self, invite_id: &str, now: DateTime<Utc>) -> Result<(), InviteError> {
        let invite = self
            .invites
            .get_mut(invite_id)
            .ok_or(InviteError::NotFound)?;
        invite.verify_issuer_signature()?;
        if invite.revoked() {
            return Err(InviteError::Revoked);
        }
        if now > invite.expires_at {
            return Err(InviteError::Expired);
        }
        if invite.consumed_uses >= invite.max_uses {
            return Err(InviteError::Exhausted);
        }
        invite.consumed_uses = invite.consumed_uses.saturating_add(1);
        Ok(())
    }
}

/// Domain-separated commitment for invite room secrets.
#[must_use]
pub fn room_secret_commitment(room_secret: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"discrypt-room-secret-commitment-v1");
    hasher.update(room_secret);
    hasher.finalize().into()
}

fn opaque_invite_id() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Deterministic hex fingerprint for endpoint trust pinning in local command surfaces.
#[must_use]
pub fn signaling_fingerprint_for_endpoint(endpoint: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"external-signaling-endpoint-fingerprint-v1");
    hasher.update(endpoint.as_bytes());
    hex::encode(hasher.finalize())
}

fn is_hex_fingerprint(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn canonical_invite_signing_bytes(
    invite_id: &str,
    room_secret_commitment: &[u8; 32],
    issuer_public_key: &[u8],
    signaling_metadata: &InviteSignalingMetadata,
    expires_at: DateTime<Utc>,
    max_uses: u32,
) -> Vec<u8> {
    let mut bytes = b"discrypt-invite-descriptor".to_vec();
    bytes.push(1);
    bytes.extend_from_slice(&(invite_id.len() as u64).to_le_bytes());
    bytes.extend_from_slice(invite_id.as_bytes());
    bytes.extend_from_slice(room_secret_commitment);
    bytes.extend_from_slice(&(issuer_public_key.len() as u64).to_le_bytes());
    bytes.extend_from_slice(issuer_public_key);
    bytes.extend_from_slice(&(signaling_metadata.signaling_endpoint.len() as u64).to_le_bytes());
    bytes.extend_from_slice(signaling_metadata.signaling_endpoint.as_bytes());
    let policy = signaling_metadata.endpoint_policy.canonical_name();
    bytes.extend_from_slice(&(policy.len() as u64).to_le_bytes());
    bytes.extend_from_slice(policy.as_bytes());
    bytes.extend_from_slice(
        &(signaling_metadata.trust.signaling_fingerprint.len() as u64).to_le_bytes(),
    );
    bytes.extend_from_slice(signaling_metadata.trust.signaling_fingerprint.as_bytes());
    bytes.extend_from_slice(&(signaling_metadata.trust.trust_status.len() as u64).to_le_bytes());
    bytes.extend_from_slice(signaling_metadata.trust.trust_status.as_bytes());
    let ice_policy = signaling_metadata.ice_endpoint_policy.signing_bytes();
    bytes.extend_from_slice(&(ice_policy.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&ice_policy);
    bytes.extend_from_slice(&expires_at.timestamp_millis().to_le_bytes());
    bytes.extend_from_slice(&max_uses.to_le_bytes());
    bytes
}

impl Invite {
    /// Create an invite from a room secret.
    #[must_use]
    pub fn new(room_secret: &[u8], expires_at: DateTime<Utc>, max_uses: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            room_secret_hash: room_secret_commitment(room_secret),
            expires_at,
            max_uses,
            uses: 0,
            revoked: false,
        }
    }

    /// Revoke this invite.
    pub fn revoke(&mut self) {
        self.revoked = true;
    }

    /// Consume one invite use.
    pub fn consume(&mut self, now: DateTime<Utc>) -> Result<(), InviteError> {
        if self.revoked {
            return Err(InviteError::Revoked);
        }
        if now > self.expires_at {
            return Err(InviteError::Expired);
        }
        if self.uses >= self.max_uses {
            return Err(InviteError::Exhausted);
        }
        self.uses = self.uses.saturating_add(1);
        Ok(())
    }
}

/// Signed authorization that binds final admission to an MLS Welcome/add payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthorizedWelcome {
    /// Invite id this Welcome satisfies.
    pub invite_id: String,
    /// Application/group id or OpenMLS group id bytes.
    pub group_id: Vec<u8>,
    /// Hash of the exact Welcome/add payload the joiner must process.
    pub welcome_payload_hash: [u8; 32],
    /// Authorization expiry.
    pub expires_at: DateTime<Utc>,
    /// Issuer device verification key.
    pub issuer_public_key: Vec<u8>,
    /// Issuer signature over the canonical Welcome authorization.
    pub issuer_signature: Vec<u8>,
}

impl AuthorizedWelcome {
    /// Create a signed Welcome authorization for an invite and concrete Welcome payload.
    #[must_use]
    pub fn sign(
        invite_id: impl Into<String>,
        group_id: impl Into<Vec<u8>>,
        welcome_payload: &[u8],
        expires_at: DateTime<Utc>,
        issuer: &SigningKey,
    ) -> Self {
        let issuer_public_key = issuer.verifying_key().to_bytes().to_vec();
        let mut authorization = Self {
            invite_id: invite_id.into(),
            group_id: group_id.into(),
            welcome_payload_hash: welcome_payload_hash(welcome_payload),
            expires_at,
            issuer_public_key,
            issuer_signature: Vec::new(),
        };
        authorization.issuer_signature = issuer
            .sign(&authorization.signing_bytes())
            .to_bytes()
            .to_vec();
        authorization
    }

    /// Verify this authorization against the invite id and exact Welcome payload.
    pub fn verify(
        &self,
        expected_invite_id: &str,
        welcome_payload: &[u8],
        now: DateTime<Utc>,
    ) -> Result<(), InviteError> {
        if self.invite_id != expected_invite_id {
            return Err(InviteError::WelcomeInviteMismatch);
        }
        if now > self.expires_at {
            return Err(InviteError::WelcomeExpired);
        }
        if self.welcome_payload_hash != welcome_payload_hash(welcome_payload) {
            return Err(InviteError::InvalidWelcomeAuthorization);
        }
        let verifying_key = VerifyingKey::from_bytes(
            &self
                .issuer_public_key
                .as_slice()
                .try_into()
                .map_err(|_| InviteError::InvalidWelcomeAuthorization)?,
        )
        .map_err(|_| InviteError::InvalidWelcomeAuthorization)?;
        let signature = Signature::from_slice(&self.issuer_signature)
            .map_err(|_| InviteError::InvalidWelcomeAuthorization)?;
        verifying_key
            .verify(&self.signing_bytes(), &signature)
            .map_err(|_| InviteError::InvalidWelcomeAuthorization)
    }

    fn signing_bytes(&self) -> Vec<u8> {
        canonical_welcome_authorization_bytes(
            &self.invite_id,
            &self.group_id,
            &self.welcome_payload_hash,
            self.expires_at,
            &self.issuer_public_key,
        )
    }
}

/// Domain-separated hash of an MLS Welcome/add payload.
#[must_use]
pub fn welcome_payload_hash(welcome_payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"discrypt-authorized-welcome-payload-v1");
    hasher.update(welcome_payload);
    hasher.finalize().into()
}

fn canonical_welcome_authorization_bytes(
    invite_id: &str,
    group_id: &[u8],
    welcome_payload_hash: &[u8; 32],
    expires_at: DateTime<Utc>,
    issuer_public_key: &[u8],
) -> Vec<u8> {
    let mut bytes = b"discrypt-authorized-welcome".to_vec();
    bytes.push(1);
    bytes.extend_from_slice(&(invite_id.len() as u64).to_le_bytes());
    bytes.extend_from_slice(invite_id.as_bytes());
    bytes.extend_from_slice(&(group_id.len() as u64).to_le_bytes());
    bytes.extend_from_slice(group_id);
    bytes.extend_from_slice(welcome_payload_hash);
    bytes.extend_from_slice(&expires_at.timestamp_millis().to_le_bytes());
    bytes.extend_from_slice(&(issuer_public_key.len() as u64).to_le_bytes());
    bytes.extend_from_slice(issuer_public_key);
    bytes
}

/// Signed proof from an online authorized helper that a subject passed the
/// password/admission challenge without exposing an offline verifier in invites.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthorizedHelperProof {
    /// Helper id expected by the invite's password gate.
    pub helper_id: String,
    /// Joining subject/device/session id.
    pub subject: String,
    /// Fresh challenge id supplied by the online helper.
    pub challenge_id: Uuid,
    /// Proof expiry.
    pub expires_at: DateTime<Utc>,
    /// Helper verification key.
    pub helper_public_key: Vec<u8>,
    /// Helper signature over the canonical proof payload.
    pub signature: Vec<u8>,
}

impl AuthorizedHelperProof {
    /// Verify helper id, subject, expiry, and Ed25519 signature.
    pub fn verify(
        &self,
        expected_helper_id: &str,
        expected_subject: &str,
        now: DateTime<Utc>,
    ) -> Result<(), InviteError> {
        if self.helper_id != expected_helper_id || self.subject != expected_subject {
            return Err(InviteError::HelperMismatch);
        }
        if now > self.expires_at {
            return Err(InviteError::HelperProofExpired);
        }
        let verifying_key = VerifyingKey::from_bytes(
            &self
                .helper_public_key
                .as_slice()
                .try_into()
                .map_err(|_| InviteError::InvalidHelperProofSignature)?,
        )
        .map_err(|_| InviteError::InvalidHelperProofSignature)?;
        let signature = Signature::from_slice(&self.signature)
            .map_err(|_| InviteError::InvalidHelperProofSignature)?;
        verifying_key
            .verify(&self.signing_bytes(), &signature)
            .map_err(|_| InviteError::InvalidHelperProofSignature)
    }

    fn sign(
        helper_id: String,
        subject: String,
        challenge_id: Uuid,
        expires_at: DateTime<Utc>,
        helper_key: &SigningKey,
    ) -> Self {
        let helper_public_key = helper_key.verifying_key().to_bytes().to_vec();
        let mut proof = Self {
            helper_id,
            subject,
            challenge_id,
            expires_at,
            helper_public_key,
            signature: Vec::new(),
        };
        proof.signature = helper_key.sign(&proof.signing_bytes()).to_bytes().to_vec();
        proof
    }

    fn signing_bytes(&self) -> Vec<u8> {
        canonical_helper_proof_bytes(
            &self.helper_id,
            &self.subject,
            self.challenge_id,
            self.expires_at,
            &self.helper_public_key,
        )
    }
}

/// Online helper that rate-limits password attempts and returns short-lived
/// signed admission proofs. It models the production server/helper side of the
/// allowed non-OPAQUE path; invites still carry no offline verifier material.
#[derive(Clone, Debug)]
pub struct OnlineAdmissionHelper {
    helper_id: String,
    password_commitment: [u8; 32],
    signing_key: SigningKey,
    max_attempts: u32,
    proof_ttl_seconds: i64,
    attempts_by_subject: BTreeMap<String, u32>,
}

impl OnlineAdmissionHelper {
    /// Create an online helper with a private password commitment.
    #[must_use]
    pub fn new(
        helper_id: impl Into<String>,
        password_secret: &[u8],
        signing_key: SigningKey,
        max_attempts: u32,
        proof_ttl_seconds: i64,
    ) -> Self {
        Self {
            helper_id: helper_id.into(),
            password_commitment: password_secret_commitment(password_secret),
            signing_key,
            max_attempts: max_attempts.max(1),
            proof_ttl_seconds: proof_ttl_seconds.max(1),
            attempts_by_subject: BTreeMap::new(),
        }
    }

    /// Return the helper id referenced by password gates.
    #[must_use]
    pub fn helper_id(&self) -> &str {
        &self.helper_id
    }

    /// Return helper public key for pinning/trust metadata.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Attempt the online helper password flow and receive a short-lived proof.
    pub fn authorize(
        &mut self,
        subject: impl Into<String>,
        password_attempt: &[u8],
        now: DateTime<Utc>,
    ) -> Result<AuthorizedHelperProof, InviteError> {
        let subject = subject.into();
        let attempts = self.attempts_by_subject.entry(subject.clone()).or_default();
        *attempts = attempts.saturating_add(1);
        if *attempts > self.max_attempts
            || password_secret_commitment(password_attempt) != self.password_commitment
        {
            return Err(InviteError::PasswordRejected);
        }
        Ok(AuthorizedHelperProof::sign(
            self.helper_id.clone(),
            subject,
            Uuid::new_v4(),
            now + chrono::Duration::seconds(self.proof_ttl_seconds),
            &self.signing_key,
        ))
    }
}

/// Domain-separated password commitment held by the online helper only.
#[must_use]
pub fn password_secret_commitment(password_secret: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"discrypt-online-helper-password-v1");
    hasher.update(password_secret);
    hasher.finalize().into()
}

fn canonical_helper_proof_bytes(
    helper_id: &str,
    subject: &str,
    challenge_id: Uuid,
    expires_at: DateTime<Utc>,
    helper_public_key: &[u8],
) -> Vec<u8> {
    let mut bytes = b"discrypt-online-helper-proof".to_vec();
    bytes.push(1);
    bytes.extend_from_slice(&(helper_id.len() as u64).to_le_bytes());
    bytes.extend_from_slice(helper_id.as_bytes());
    bytes.extend_from_slice(&(subject.len() as u64).to_le_bytes());
    bytes.extend_from_slice(subject.as_bytes());
    bytes.extend_from_slice(challenge_id.as_bytes());
    bytes.extend_from_slice(&expires_at.timestamp_millis().to_le_bytes());
    bytes.extend_from_slice(&(helper_public_key.len() as u64).to_le_bytes());
    bytes.extend_from_slice(helper_public_key);
    bytes
}

/// Password admission mode; offline-copyable rate limits are forbidden by design.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PasswordGate {
    /// No password gate.
    None,
    /// OPAQUE/PAKE-backed gate.
    OpaquePake { server_id: String },
    /// Online authorized admission helper.
    OnlineAuthorizedHelper { helper_id: String },
    /// Explicitly rejected shape: an offline verifier cannot enforce attempts.
    OfflineVerifier { verifier_id: String },
}

impl PasswordGate {
    /// True when this gate can enforce online/rate-limited attempts.
    #[must_use]
    pub fn supports_real_rate_limit(&self) -> bool {
        matches!(
            self,
            Self::None | Self::OpaquePake { .. } | Self::OnlineAuthorizedHelper { .. }
        )
    }

    /// True when a password proof is required.
    #[must_use]
    pub fn requires_password(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Password attempt controller.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdmissionController {
    gate: PasswordGate,
    max_attempts: u32,
    attempts_by_subject: BTreeMap<String, u32>,
}

impl AdmissionController {
    /// Create a controller.
    #[must_use]
    pub fn new(gate: PasswordGate, max_attempts: u32) -> Self {
        Self {
            gate,
            max_attempts: max_attempts.max(1),
            attempts_by_subject: BTreeMap::new(),
        }
    }

    /// Check whether the configured gate is admissible for v1.
    pub fn validate_gate(&self) -> Result<(), InviteError> {
        if self.gate.supports_real_rate_limit() {
            Ok(())
        } else {
            Err(InviteError::OfflineVerifierRejected)
        }
    }

    /// Attempt password admission. The boundary treats `proof_ok` as PAKE/helper result.
    pub fn attempt_password(
        &mut self,
        subject: impl Into<String>,
        proof_ok: bool,
    ) -> Result<(), InviteError> {
        self.validate_gate()?;
        if !self.gate.requires_password() {
            return Ok(());
        }
        let subject = subject.into();
        let attempts = self.attempts_by_subject.entry(subject).or_default();
        *attempts = attempts.saturating_add(1);
        if *attempts > self.max_attempts || !proof_ok {
            return Err(InviteError::PasswordRejected);
        }
        Ok(())
    }

    /// Attempt admission with a signed online-helper proof.
    pub fn attempt_online_helper(
        &mut self,
        subject: impl Into<String>,
        proof: &AuthorizedHelperProof,
        now: DateTime<Utc>,
    ) -> Result<(), InviteError> {
        self.validate_gate()?;
        let subject = subject.into();
        let PasswordGate::OnlineAuthorizedHelper { helper_id } = &self.gate else {
            return Err(InviteError::HelperMismatch);
        };
        proof.verify(helper_id, &subject, now)?;
        Ok(())
    }

    /// Final admission through the online-helper path requires invite, helper proof, and Welcome/add.
    pub fn finalize_helper_admission(
        &mut self,
        invite: &mut Invite,
        now: DateTime<Utc>,
        subject: impl Into<String>,
        helper_proof: &AuthorizedHelperProof,
        authorized_welcome: Option<&AuthorizedWelcome>,
        welcome_payload: &[u8],
    ) -> Result<(), InviteError> {
        let welcome = authorized_welcome.ok_or(InviteError::WelcomeRequired)?;
        welcome.verify(&invite.id.to_string(), welcome_payload, now)?;
        self.attempt_online_helper(subject, helper_proof, now)?;
        invite.consume(now)
    }

    /// Final admission requires invite, password gate success, and signed Welcome/add authorization.
    pub fn finalize_admission(
        &mut self,
        invite: &mut Invite,
        now: DateTime<Utc>,
        subject: impl Into<String>,
        password_proof_ok: bool,
        authorized_welcome: Option<&AuthorizedWelcome>,
        welcome_payload: &[u8],
    ) -> Result<(), InviteError> {
        let welcome = authorized_welcome.ok_or(InviteError::WelcomeRequired)?;
        welcome.verify(&invite.id.to_string(), welcome_payload, now)?;
        self.attempt_password(subject, password_proof_ok)?;
        invite.consume(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn invite_honors_expiry_revoke_and_max_use() {
        let now = Utc::now();
        let mut i = Invite::new(b"secret", now + Duration::minutes(1), 1);
        assert!(i.consume(now).is_ok());
        assert_eq!(i.consume(now), Err(InviteError::Exhausted));
        let mut expired = Invite::new(b"secret", now - Duration::seconds(1), 1);
        assert_eq!(expired.consume(now), Err(InviteError::Expired));
        let mut revoked = Invite::new(b"secret", now + Duration::minutes(1), 1);
        revoked.revoke();
        assert_eq!(revoked.consume(now), Err(InviteError::Revoked));
    }

    #[test]
    fn invite_store_uses_opaque_signed_commitments_and_counts_uses() {
        let issuer = SigningKey::generate(&mut OsRng);
        let now = Utc::now();
        let mut store = InviteStore::new();
        let invite = store.issue_invite(b"room secret", now + Duration::minutes(5), 2, &issuer);

        assert_eq!(invite.invite_id.len(), 64);
        assert!(invite
            .invite_id
            .chars()
            .all(|character| character.is_ascii_hexdigit()));
        let raw_hash: [u8; 32] = Sha256::digest(b"room secret").into();
        assert_ne!(invite.room_secret_commitment, raw_hash);
        assert!(invite.verify_issuer_signature().is_ok());
        assert_eq!(invite.consumed_uses, 0);
        assert_eq!(store.consume(&invite.invite_id, now), Ok(()));
        assert_eq!(
            store
                .get(&invite.invite_id)
                .map(|stored| stored.consumed_uses),
            Some(1)
        );
        assert_eq!(store.consume(&invite.invite_id, now), Ok(()));
        assert_eq!(
            store.consume(&invite.invite_id, now),
            Err(InviteError::Exhausted)
        );
    }

    #[test]
    fn invite_store_rejects_tampering_revocation_expiry_and_unknown_ids() {
        let issuer = SigningKey::generate(&mut OsRng);
        let now = Utc::now();
        let mut store = InviteStore::new();
        let invite = store.issue_invite(b"room secret", now + Duration::minutes(5), 1, &issuer);

        let mut tampered = invite.clone();
        tampered.max_uses = 9;
        assert_eq!(
            tampered.verify_issuer_signature(),
            Err(InviteError::InvalidIssuerSignature)
        );

        assert_eq!(store.revoke(&invite.invite_id, "gov-event-1"), Ok(()));
        assert_eq!(
            store.consume(&invite.invite_id, now),
            Err(InviteError::Revoked)
        );
        assert_eq!(
            store
                .get(&invite.invite_id)
                .and_then(|stored| stored.revocation_event_id.as_deref()),
            Some("gov-event-1")
        );

        let expired = store.issue_invite(b"other secret", now - Duration::seconds(1), 1, &issuer);
        assert_eq!(
            store.consume(&expired.invite_id, now),
            Err(InviteError::Expired)
        );
        assert_eq!(
            store.consume("not-present", now),
            Err(InviteError::NotFound)
        );
    }

    #[test]
    fn invite_descriptor_signs_signaling_metadata_and_rejects_invalid_values() {
        let issuer = SigningKey::generate(&mut OsRng);
        let now = Utc::now();
        let endpoint = "https://signal.example.invalid/v1/rendezvous";
        let trust = InviteTrustMetadata::new(
            signaling_fingerprint_for_endpoint(endpoint),
            "signed endpoint fingerprint; verify before MLS Welcome",
        );
        assert!(trust.is_ok());
        let Ok(trust) = trust else {
            return;
        };
        let metadata =
            InviteSignalingMetadata::new(endpoint, InviteEndpointPolicy::ProductionTls, trust);
        assert!(metadata.is_ok());
        let Ok(metadata) = metadata else {
            return;
        };
        let mut store = InviteStore::new();
        let invite_result = store.issue_invite_with_metadata(
            b"room secret",
            now + Duration::minutes(5),
            2,
            metadata,
            &issuer,
        );
        assert!(invite_result.is_ok());
        let Ok(invite) = invite_result else {
            return;
        };
        assert!(invite.verify_issuer_signature().is_ok());

        let mut tampered_endpoint = invite.clone();
        tampered_endpoint.signaling_metadata.signaling_endpoint =
            "https://evil.example.invalid/v1/rendezvous".to_owned();
        assert_eq!(
            tampered_endpoint.verify_issuer_signature(),
            Err(InviteError::InvalidIssuerSignature)
        );

        let mut tampered_fingerprint = invite.clone();
        tampered_fingerprint
            .signaling_metadata
            .trust
            .signaling_fingerprint = signaling_fingerprint_for_endpoint("https://evil.example");
        assert_eq!(
            tampered_fingerprint.verify_issuer_signature(),
            Err(InviteError::InvalidIssuerSignature)
        );

        let invalid_endpoint = InviteSignalingMetadata::new(
            "http://example.invalid/rendezvous",
            InviteEndpointPolicy::ProductionTls,
            InviteTrustMetadata::new(
                signaling_fingerprint_for_endpoint("http://example.invalid/rendezvous"),
                "signed endpoint fingerprint",
            )
            .unwrap_or_else(|_| InviteTrustMetadata {
                signaling_fingerprint: signaling_fingerprint_for_endpoint("fallback"),
                trust_status: "fallback".to_owned(),
            }),
        );
        assert_eq!(invalid_endpoint, Err(InviteError::InvalidSignalingEndpoint));

        assert_eq!(
            InviteTrustMetadata::new("not-a-fingerprint", "signed endpoint fingerprint"),
            Err(InviteError::InvalidTrustMetadata)
        );
    }

    #[test]
    fn invite_descriptor_serialization_does_not_leak_raw_room_secret() {
        let issuer = SigningKey::generate(&mut OsRng);
        let now = Utc::now();
        let raw_secret = b"raw-room-secret-never-in-descriptor";
        let mut store = InviteStore::new();
        let invite = store.issue_invite(raw_secret, now + Duration::minutes(5), 1, &issuer);

        let serialized = serde_json::to_string(&invite);
        assert!(serialized.is_ok());
        let Ok(serialized) = serialized else {
            return;
        };
        assert!(!serialized.contains("raw-room-secret-never-in-descriptor"));
        assert!(!format!("{invite:?}").contains("raw-room-secret-never-in-descriptor"));
        assert!(serialized.contains("signaling_endpoint"));
        assert!(serialized.contains("signaling_fingerprint"));
        assert!(serialized.contains("endpoint_policy"));
    }

    #[test]
    fn online_helper_flow_rate_limits_and_signs_expiring_proofs() {
        let now = Utc::now();
        let helper_key = SigningKey::generate(&mut OsRng);
        let mut helper =
            OnlineAdmissionHelper::new("helper-a", b"correct horse", helper_key, 2, 60);
        let proof_result = helper.authorize("alice-device", b"correct horse", now);
        assert!(proof_result.is_ok());
        let Ok(proof) = proof_result else {
            return;
        };
        assert!(proof.verify("helper-a", "alice-device", now).is_ok());
        assert_eq!(
            proof.verify("helper-b", "alice-device", now),
            Err(InviteError::HelperMismatch)
        );
        assert_eq!(
            proof.verify("helper-a", "bob-device", now),
            Err(InviteError::HelperMismatch)
        );
        assert_eq!(
            proof.verify("helper-a", "alice-device", now + Duration::seconds(61)),
            Err(InviteError::HelperProofExpired)
        );

        assert_eq!(
            helper.authorize("mallory", b"wrong", now),
            Err(InviteError::PasswordRejected)
        );
        assert_eq!(
            helper.authorize("mallory", b"wrong", now),
            Err(InviteError::PasswordRejected)
        );
        assert_eq!(
            helper.authorize("mallory", b"correct horse", now),
            Err(InviteError::PasswordRejected)
        );
    }

    #[test]
    fn helper_admission_requires_matching_gate_and_welcome() {
        let now = Utc::now();
        let helper_key = SigningKey::generate(&mut OsRng);
        let mut helper = OnlineAdmissionHelper::new("helper-a", b"secret", helper_key, 2, 60);
        let proof_result = helper.authorize("alice", b"secret", now);
        assert!(proof_result.is_ok());
        let Ok(proof) = proof_result else {
            return;
        };
        let mut invite = Invite::new(b"room", now + Duration::minutes(1), 1);
        let mut controller = AdmissionController::new(
            PasswordGate::OnlineAuthorizedHelper {
                helper_id: "helper-a".into(),
            },
            3,
        );
        let welcome_payload = b"openmls-welcome";
        let welcome_issuer = SigningKey::generate(&mut OsRng);
        let welcome = AuthorizedWelcome::sign(
            invite.id.to_string(),
            b"group-a".to_vec(),
            welcome_payload,
            now + Duration::minutes(1),
            &welcome_issuer,
        );
        assert_eq!(
            controller.finalize_helper_admission(
                &mut invite,
                now,
                "alice",
                &proof,
                None,
                welcome_payload
            ),
            Err(InviteError::WelcomeRequired)
        );
        assert_eq!(
            controller.finalize_helper_admission(
                &mut invite,
                now,
                "alice",
                &proof,
                Some(&welcome),
                welcome_payload,
            ),
            Ok(())
        );

        let mut wrong_controller = AdmissionController::new(
            PasswordGate::OnlineAuthorizedHelper {
                helper_id: "helper-b".into(),
            },
            3,
        );
        let mut invite = Invite::new(b"room", now + Duration::minutes(1), 1);
        let wrong_gate_welcome = AuthorizedWelcome::sign(
            invite.id.to_string(),
            b"group-a".to_vec(),
            welcome_payload,
            now + Duration::minutes(1),
            &welcome_issuer,
        );
        assert_eq!(
            wrong_controller.finalize_helper_admission(
                &mut invite,
                now,
                "alice",
                &proof,
                Some(&wrong_gate_welcome),
                welcome_payload,
            ),
            Err(InviteError::HelperMismatch)
        );
    }

    #[test]
    fn admission_rejects_offline_verifier_and_requires_welcome() {
        let now = Utc::now();
        let mut invite = Invite::new(b"secret", now + Duration::minutes(1), 2);
        let mut offline = AdmissionController::new(
            PasswordGate::OfflineVerifier {
                verifier_id: "copyable".into(),
            },
            1,
        );
        let welcome_payload = b"welcome";
        let welcome = AuthorizedWelcome::sign(
            invite.id.to_string(),
            b"group".to_vec(),
            welcome_payload,
            now + Duration::minutes(1),
            &SigningKey::generate(&mut OsRng),
        );
        assert_eq!(
            offline.finalize_admission(
                &mut invite,
                now,
                "alice",
                true,
                Some(&welcome),
                welcome_payload,
            ),
            Err(InviteError::OfflineVerifierRejected)
        );
        let mut pake = AdmissionController::new(
            PasswordGate::OpaquePake {
                server_id: "helper".into(),
            },
            1,
        );
        assert_eq!(
            pake.finalize_admission(&mut invite, now, "alice", true, None, welcome_payload),
            Err(InviteError::WelcomeRequired)
        );
        assert_eq!(
            pake.finalize_admission(
                &mut invite,
                now,
                "alice",
                true,
                Some(&welcome),
                b"tampered-welcome",
            ),
            Err(InviteError::InvalidWelcomeAuthorization)
        );
        assert_eq!(
            pake.finalize_admission(
                &mut invite,
                now,
                "alice",
                true,
                Some(&welcome),
                welcome_payload,
            ),
            Ok(())
        );
        assert_eq!(
            pake.finalize_admission(
                &mut invite,
                now,
                "alice",
                true,
                Some(&welcome),
                welcome_payload,
            ),
            Err(InviteError::PasswordRejected)
        );
    }

    #[test]
    fn admission_password_decision_covers_adr_005() {
        let decision = admission_password_decision();

        assert!(decision.covers_adr_005());
        assert_eq!(
            decision.selected_protocol,
            AdmissionPasswordProtocol::OnlineAuthorizedHelper
        );
        assert!(decision.no_offline_verifier.contains("OfflineVerifier"));
        assert!(decision.rate_limit_proof.contains("PasswordRejected"));
        assert!(decision.final_admission_gate.contains("AuthorizedWelcome"));
    }

    #[test]
    fn online_helper_failure_privacy_uses_uniform_rejection() {
        let now = Utc::now();
        let helper_key = SigningKey::generate(&mut OsRng);
        let mut helper = OnlineAdmissionHelper::new("helper-a", b"correct", helper_key, 1, 60);

        assert_eq!(
            helper.authorize("mallory", b"wrong", now),
            Err(InviteError::PasswordRejected)
        );
        assert_eq!(
            helper.authorize("mallory", b"correct", now),
            Err(InviteError::PasswordRejected)
        );
    }
}

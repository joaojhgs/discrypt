//! Invite, password-admission, and final MLS Welcome abstractions.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

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
    /// Password gate is not backed by PAKE/OPAQUE/helper rate limiting.
    #[error("offline verifier cannot enforce rate limits")]
    OfflineVerifierRejected,
    /// Password proof failed or exceeded rate limits.
    #[error("password gate rejected")]
    PasswordRejected,
    /// Final MLS add/Welcome authorization is absent.
    #[error("authorized MLS welcome required")]
    WelcomeRequired,
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
        issuer: &SigningKey,
    ) -> Self {
        let issuer_public_key = issuer.verifying_key().to_bytes().to_vec();
        let mut invite = Self {
            invite_id,
            room_secret_commitment,
            issuer_public_key,
            issuer_signature: Vec::new(),
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
            issuer,
        );
        self.invites
            .insert(invite.invite_id.clone(), invite.clone());
        invite
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

fn canonical_invite_signing_bytes(
    invite_id: &str,
    room_secret_commitment: &[u8; 32],
    issuer_public_key: &[u8],
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

    /// Attempt password admission. The facade treats `proof_ok` as PAKE/helper result.
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

    /// Final admission requires invite, password gate success, and authorized Welcome/add.
    pub fn finalize_admission(
        &mut self,
        invite: &mut Invite,
        now: DateTime<Utc>,
        subject: impl Into<String>,
        password_proof_ok: bool,
        authorized_welcome: bool,
    ) -> Result<(), InviteError> {
        if !authorized_welcome {
            return Err(InviteError::WelcomeRequired);
        }
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
    fn admission_rejects_offline_verifier_and_requires_welcome() {
        let now = Utc::now();
        let mut invite = Invite::new(b"secret", now + Duration::minutes(1), 2);
        let mut offline = AdmissionController::new(
            PasswordGate::OfflineVerifier {
                verifier_id: "copyable".into(),
            },
            1,
        );
        assert_eq!(
            offline.finalize_admission(&mut invite, now, "alice", true, true),
            Err(InviteError::OfflineVerifierRejected)
        );
        let mut pake = AdmissionController::new(
            PasswordGate::OpaquePake {
                server_id: "helper".into(),
            },
            1,
        );
        assert_eq!(
            pake.finalize_admission(&mut invite, now, "alice", true, false),
            Err(InviteError::WelcomeRequired)
        );
        assert_eq!(
            pake.finalize_admission(&mut invite, now, "alice", true, true),
            Ok(())
        );
        assert_eq!(
            pake.finalize_admission(&mut invite, now, "alice", true, true),
            Err(InviteError::PasswordRejected)
        );
    }
}

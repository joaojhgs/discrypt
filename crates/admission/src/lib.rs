//! Invite, password-admission, and final MLS Welcome abstractions.
use chrono::{DateTime, Utc};
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

impl Invite {
    /// Create an invite from a room secret.
    #[must_use]
    pub fn new(room_secret: &[u8], expires_at: DateTime<Utc>, max_uses: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            room_secret_hash: Sha256::digest(room_secret).into(),
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

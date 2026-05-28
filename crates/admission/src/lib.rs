//! Invite and password-admission abstractions.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// Invite object with expiry/revoke/max-use controls.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Invite {
    pub id: Uuid,
    pub room_secret_hash: [u8; 32],
    pub expires_at: DateTime<Utc>,
    pub max_uses: u32,
    pub uses: u32,
    pub revoked: bool,
}
#[derive(Debug, Error, Eq, PartialEq)]
pub enum InviteError {
    #[error("invite expired")]
    Expired,
    #[error("invite revoked")]
    Revoked,
    #[error("invite exhausted")]
    Exhausted,
}
impl Invite {
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
        self.uses += 1;
        Ok(())
    }
}

/// Password admission mode; offline-copyable rate limits are forbidden by design.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PasswordGate {
    None,
    OpaquePake { server_id: String },
    OnlineAuthorizedHelper { helper_id: String },
}
impl PasswordGate {
    #[must_use]
    pub fn supports_real_rate_limit(&self) -> bool {
        !matches!(self, Self::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    #[test]
    fn invite_honors_max_use() {
        let mut i = Invite::new(b"secret", Utc::now() + Duration::minutes(1), 1);
        assert!(i.consume(Utc::now()).is_ok());
        assert_eq!(i.consume(Utc::now()), Err(InviteError::Exhausted));
    }
}

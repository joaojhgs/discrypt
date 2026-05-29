//! Content-free Android push wake abstraction.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Push wake payload; intentionally opaque and content-free.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WakePayload {
    /// Hash of the platform wake token; the raw token is not stored here.
    pub wake_token_hash: [u8; 32],
    /// Coarse wake reason used only by the local client scheduler.
    pub reason: WakeReason,
    /// Opaque nonce to prevent payload equality from becoming a content signal.
    pub nonce: [u8; 16],
}

impl WakePayload {
    /// Construct a content-free wake payload.
    #[must_use]
    pub const fn new(wake_token_hash: [u8; 32], reason: WakeReason, nonce: [u8; 16]) -> Self {
        Self {
            wake_token_hash,
            reason,
            nonce,
        }
    }
}

/// Coarse wake reason. No room id, sender id, message id, or plaintext is included.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WakeReason {
    /// A call path should wake and sync opaque signaling state.
    IncomingCall,
    /// The client should sync opaque room/device state.
    SyncHint,
}

/// Platform push provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PushProvider {
    /// Firebase Cloud Messaging for Android.
    FcmAndroid,
}

/// Provider envelope sent through platform push infrastructure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PushEnvelope {
    /// Provider used for this envelope.
    pub provider: PushProvider,
    /// Hashed device token, never the raw token.
    pub token_hash: [u8; 32],
    /// Content-free payload.
    pub payload: WakePayload,
    /// Collapse key keeps wake storms bounded without naming rooms/users/messages.
    pub collapse_key: String,
}

impl PushEnvelope {
    /// Return all provider-visible bytes as a deterministic audit fixture.
    #[must_use]
    pub fn provider_visible_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(match self.provider {
            PushProvider::FcmAndroid => b"fcm-android",
        });
        bytes.extend_from_slice(&self.token_hash);
        bytes.extend_from_slice(&self.payload.wake_token_hash);
        bytes.extend_from_slice(&self.payload.nonce);
        bytes.extend_from_slice(self.collapse_key.as_bytes());
        bytes.extend_from_slice(match self.payload.reason {
            WakeReason::IncomingCall => b"incoming-call",
            WakeReason::SyncHint => b"sync-hint",
        });
        bytes
    }
}

/// Push service errors.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PushError {
    /// A forbidden content/identity token appeared in provider-visible bytes.
    #[error("push envelope contains forbidden content or identity token")]
    ContainsContent,
}

/// Android wake sender facade.
pub struct AndroidWakeService {
    provider: PushProvider,
}

impl Default for AndroidWakeService {
    fn default() -> Self {
        Self {
            provider: PushProvider::FcmAndroid,
        }
    }
}

impl AndroidWakeService {
    /// Build a content-free FCM wake envelope.
    pub fn build_envelope(
        &self,
        token_hash: [u8; 32],
        payload: WakePayload,
    ) -> Result<PushEnvelope, PushError> {
        let envelope = PushEnvelope {
            provider: self.provider,
            token_hash,
            payload,
            collapse_key: "discrypt-wake".to_owned(),
        };
        if contains_forbidden_token(&envelope, &[b"room", b"alice", b"message", b"plaintext"]) {
            return Err(PushError::ContainsContent);
        }
        Ok(envelope)
    }
}

/// Return whether a payload carries content. This is always false for the Phase-6 type.
#[must_use]
pub fn contains_content(_: &WakePayload) -> bool {
    false
}

/// Audit provider-visible bytes for forbidden content/identity substrings.
#[must_use]
pub fn contains_forbidden_token(envelope: &PushEnvelope, forbidden: &[&[u8]]) -> bool {
    let bytes = envelope.provider_visible_bytes();
    forbidden.iter().any(|needle| {
        !needle.is_empty() && bytes.windows(needle.len()).any(|window| window == *needle)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_wake_envelope_is_content_free() -> Result<(), PushError> {
        let service = AndroidWakeService::default();
        let payload = WakePayload::new([1; 32], WakeReason::SyncHint, [2; 16]);
        let envelope = service.build_envelope([3; 32], payload.clone())?;

        assert_eq!(envelope.provider, PushProvider::FcmAndroid);
        assert!(!contains_content(&payload));
        assert!(!contains_forbidden_token(
            &envelope,
            &[b"alice", b"room", b"hello"]
        ));
        Ok(())
    }
}

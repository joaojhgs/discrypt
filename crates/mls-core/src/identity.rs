//! Local identity keys, friend codes, and safety-number verification.

use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

/// Human-shareable short fingerprint used for out-of-band discovery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FriendCode(String);

impl FriendCode {
    /// Return the encoded friend code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A longer comparison string displayed during explicit MITM verification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SafetyNumber(String);

impl SafetyNumber {
    /// Return the safety number text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Local account identity. The signing key never leaves local storage except via sealed backup.
#[derive(Clone)]
pub struct Identity {
    display_name: String,
    signing_key: SigningKey,
}

impl core::fmt::Debug for Identity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Identity")
            .field("display_name", &self.display_name)
            .field(
                "verifying_key",
                &hex::encode(self.verifying_key().as_bytes()),
            )
            .finish_non_exhaustive()
    }
}

impl Identity {
    /// Generate a new local identity keypair.
    #[must_use]
    pub fn generate(display_name: impl Into<String>) -> Self {
        Self {
            display_name: display_name.into(),
            signing_key: SigningKey::generate(&mut OsRng),
        }
    }

    /// Rehydrate from sealed key bytes.
    #[must_use]
    pub fn from_signing_key(display_name: impl Into<String>, key_bytes: &[u8; 32]) -> Self {
        Self {
            display_name: display_name.into(),
            signing_key: SigningKey::from_bytes(key_bytes),
        }
    }

    /// Return display name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Return public verification key.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Export signing key bytes for sealed backup code paths.
    #[must_use]
    pub fn sealed_backup_material(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Destroy a mutable copy of exported key bytes.
    pub fn zeroize_exported_material(bytes: &mut [u8; 32]) {
        bytes.zeroize();
    }

    /// Friend code: short public-key hash prefix.
    #[must_use]
    pub fn friend_code(&self) -> FriendCode {
        let digest = Sha256::digest(self.verifying_key().as_bytes());
        FriendCode(hex::encode(&digest[..10]))
    }

    /// Safety number: pairwise sorted public-key fingerprint.
    #[must_use]
    pub fn safety_number(&self, peer: &VerifyingKey) -> SafetyNumber {
        let ours = self.verifying_key();
        let mut keys = [ours.as_bytes().to_vec(), peer.as_bytes().to_vec()];
        keys.sort();
        let mut hasher = Sha256::new();
        hasher.update(&keys[0]);
        hasher.update(&keys[1]);
        let digest = hex::encode(hasher.finalize());
        let grouped = digest
            .as_bytes()
            .chunks(4)
            .take(12)
            .map(|c| core::str::from_utf8(c).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" ");
        SafetyNumber(grouped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_number_is_pairwise_symmetric() {
        let alice = Identity::generate("alice");
        let bob = Identity::generate("bob");
        assert_eq!(
            alice.safety_number(&bob.verifying_key()),
            bob.safety_number(&alice.verifying_key())
        );
        assert!(!alice.friend_code().as_str().is_empty());
    }

    #[test]
    fn exported_key_material_round_trips_and_can_be_zeroized() {
        let alice = Identity::generate("alice");
        let mut material = alice.sealed_backup_material();
        let restored = Identity::from_signing_key("alice", &material);
        assert_eq!(alice.verifying_key(), restored.verifying_key());
        Identity::zeroize_exported_material(&mut material);
        assert_eq!(material, [0u8; 32]);
    }
}

//! Local identity keys, friend codes, and safety-number verification.

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

/// Human-shareable friend-code/QR payload used for out-of-band discovery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FriendCode(String);

impl FriendCode {
    /// Wrap an externally supplied friend-code/QR payload for validation.
    #[must_use]
    pub fn from_payload(payload: impl Into<String>) -> Self {
        Self(payload.into())
    }

    /// Return the encoded friend code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Extract the public identity key embedded in a v1 friend-code/QR payload.
    #[must_use]
    pub fn verifying_key(&self) -> Option<VerifyingKey> {
        let identity_key_hex = self
            .0
            .split_once("?ik=")
            .map(|(_, tail)| tail)
            .and_then(|tail| tail.split('&').next())?;
        let decoded = hex::decode(identity_key_hex).ok()?;
        let key_bytes: [u8; 32] = decoded.try_into().ok()?;
        VerifyingKey::from_bytes(&key_bytes).ok()
    }

    /// Build a friend-code/QR payload from a display label and identity key.
    #[must_use]
    pub fn from_verifying_key(display_name: &str, verifying_key: &VerifyingKey) -> Self {
        let public_key = hex::encode(verifying_key.as_bytes());
        let fingerprint = Sha256::digest(verifying_key.as_bytes());
        Self(format!(
            "discrypt://friend/v1/{}?ik={public_key}&fp={}",
            slugify(display_name),
            hex::encode(&fingerprint[..10])
        ))
    }
}

/// A longer comparison string displayed during explicit MITM verification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SafetyNumber(String);

/// Parse a hex-encoded Ed25519 identity key.
#[must_use]
pub fn verifying_key_from_hex(identity_key_hex: &str) -> Option<VerifyingKey> {
    let decoded = hex::decode(identity_key_hex).ok()?;
    let key_bytes: [u8; 32] = decoded.try_into().ok()?;
    VerifyingKey::from_bytes(&key_bytes).ok()
}

impl SafetyNumber {
    /// Derive a pairwise safety number directly from two public identity keys.
    #[must_use]
    pub fn from_identity_keys(ours: &VerifyingKey, peer: &VerifyingKey) -> Self {
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
        Self(grouped)
    }
}

#[cfg(test)]
impl FriendCode {
    #[must_use]
    fn legacy_unchecked_verifying_key_for_tests(&self) -> Option<VerifyingKey> {
        let identity_key_hex = self
            .0
            .split_once("?ik=")
            .map(|(_, tail)| tail)
            .and_then(|tail| tail.split('&').next())?;
        let decoded = hex::decode(identity_key_hex).ok()?;
        let key_bytes: [u8; 32] = decoded.try_into().ok()?;
        VerifyingKey::from_bytes(&key_bytes).ok()
    }
}

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

    /// Sign a device-pairing authorization message with the local account key.
    #[must_use]
    pub(crate) fn sign_pairing_authorization(&self, message: &[u8]) -> [u8; 64] {
        self.signing_key.sign(message).to_bytes()
    }

    /// Destroy a mutable copy of exported key bytes.
    pub fn zeroize_exported_material(bytes: &mut [u8; 32]) {
        bytes.zeroize();
    }

    /// Friend code: QR-friendly payload containing the public identity key.
    #[must_use]
    pub fn friend_code(&self) -> FriendCode {
        FriendCode::from_verifying_key(self.display_name(), &self.verifying_key())
    }

    /// Safety number: pairwise sorted public-key fingerprint.
    #[must_use]
    pub fn safety_number(&self, peer: &VerifyingKey) -> SafetyNumber {
        SafetyNumber::from_identity_keys(&self.verifying_key(), peer)
    }

    /// Safety number derived from the identity key embedded in a friend-code/QR payload.
    #[must_use]
    pub fn safety_number_from_friend_code(&self, peer: &FriendCode) -> Option<SafetyNumber> {
        let peer_key = peer.verifying_key()?;
        Some(self.safety_number(&peer_key))
    }
}

fn slugify(label: &str) -> String {
    let slug = label
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else if character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "contact".to_owned()
    } else {
        slug
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
        assert!(alice
            .friend_code()
            .as_str()
            .starts_with("discrypt://friend/v1/alice?ik="));
        assert!(alice.friend_code().as_str().contains("&fp="));
    }

    #[test]
    fn friend_code_carries_identity_key_used_for_safety_number() {
        let alice = Identity::generate("alice");
        let bob = Identity::generate("bob");
        let bob_code = bob.friend_code();

        assert_eq!(bob_code.verifying_key(), Some(bob.verifying_key()));
        assert_eq!(
            alice.safety_number_from_friend_code(&bob_code),
            Some(alice.safety_number(&bob.verifying_key()))
        );
        assert_eq!(FriendCode("not-a-code".to_owned()).verifying_key(), None);
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

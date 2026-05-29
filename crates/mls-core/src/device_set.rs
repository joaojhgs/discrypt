//! Multi-device leaf tracking and transparency events.

use crate::identity::Identity;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

const PAIRING_PAYLOAD_VERSION: u8 = 1;

/// Status of a device leaf.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DeviceStatus {
    Active,
    Removed,
}

/// A per-device MLS leaf under one identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeviceLeaf {
    /// Stable device id.
    pub device_id: Uuid,
    /// Monotonic leaf index assigned by the local group wrapper.
    pub leaf_index: u32,
    /// User identity verification key bytes.
    pub identity_key: [u8; 32],
    /// Device signing/credential key bytes.
    pub device_key: [u8; 32],
    /// Human platform label.
    pub label: String,
    /// Leaf status.
    pub status: DeviceStatus,
}

/// Transparency event shown to peers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransparencyEvent {
    /// Device id affected.
    pub device_id: Uuid,
    /// Event label.
    pub kind: String,
    /// Group epoch when observed.
    pub epoch: u64,
}

/// Existing-device authorization payload for adding another own device.
///
/// The serialized payload is intentionally pasteable so UI shells can represent a QR code
/// as a string until camera scanning exists. The signature covers a canonical message built
/// from all public fields except `signature`, so a joining device cannot rewrite the label,
/// authorizing device, account identity, or expiry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DevicePairingPayload {
    /// Pairing payload schema version.
    pub version: u8,
    /// Active existing device that authorized the pairing.
    pub authorizing_device_id: Uuid,
    /// Account identity verifying key, hex encoded.
    pub identity_key: String,
    /// Human label requested for the new device.
    pub requested_label: String,
    /// Fresh challenge preventing accidental payload reuse collisions.
    pub challenge: Uuid,
    /// Last group/device epoch where this payload is accepted.
    pub expires_epoch: u64,
    /// Ed25519 signature over the canonical payload message, hex encoded.
    pub signature: String,
}

/// Pairing authorization failures.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DevicePairingError {
    /// The existing device id is unknown, removed, or not under the supplied identity.
    #[error("authorizing device is not an active device for this identity")]
    UnauthorizedAuthorizingDevice,
    /// The payload cannot be decoded.
    #[error("invalid pairing payload: {0}")]
    InvalidPayload(String),
    /// The payload was created for a different account identity.
    #[error("pairing payload identity does not match local identity")]
    IdentityMismatch,
    /// The payload is no longer valid for the supplied epoch.
    #[error("pairing payload expired")]
    Expired,
    /// The signature does not verify against the account identity.
    #[error("pairing payload signature verification failed")]
    SignatureVerificationFailed,
    /// The joining device key is already active in this device set.
    #[error("device key is already active in this device set")]
    DuplicateDeviceKey,
}

/// Own device set for one account identity.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeviceSet {
    next_leaf: u32,
    devices: BTreeMap<Uuid, DeviceLeaf>,
    transparency: Vec<TransparencyEvent>,
}

impl DeviceSet {
    /// Create an empty device set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a device leaf after existing-device authorization has succeeded.
    pub fn add_authorized_device(
        &mut self,
        identity: &Identity,
        device_key: VerifyingKey,
        label: impl Into<String>,
        epoch: u64,
    ) -> DeviceLeaf {
        self.insert_authorized_device(identity, device_key, label, epoch, "device-added")
    }

    /// Create a pasteable pairing payload authorized by an existing active device.
    pub fn create_pairing_payload(
        &self,
        identity: &Identity,
        authorizing_device_id: Uuid,
        requested_label: impl Into<String>,
        current_epoch: u64,
        valid_for_epochs: u64,
    ) -> Result<String, DevicePairingError> {
        self.require_authorizing_device(identity, authorizing_device_id)?;
        let requested_label = normalize_device_label(requested_label);
        let identity_key = hex::encode(identity.verifying_key().to_bytes());
        let challenge = Uuid::new_v4();
        let expires_epoch = current_epoch.saturating_add(valid_for_epochs.max(1));
        let message = canonical_pairing_message(
            PAIRING_PAYLOAD_VERSION,
            authorizing_device_id,
            &identity_key,
            &requested_label,
            challenge,
            expires_epoch,
        );
        let signature = hex::encode(identity.sign_pairing_authorization(message.as_bytes()));
        serde_json::to_string(&DevicePairingPayload {
            version: PAIRING_PAYLOAD_VERSION,
            authorizing_device_id,
            identity_key,
            requested_label,
            challenge,
            expires_epoch,
            signature,
        })
        .map_err(|error| DevicePairingError::InvalidPayload(error.to_string()))
    }

    /// Add a new device only after verifying an existing-device-authorized payload.
    pub fn add_device_from_pairing_payload(
        &mut self,
        identity: &Identity,
        payload: &str,
        new_device_key: VerifyingKey,
        current_epoch: u64,
    ) -> Result<DeviceLeaf, DevicePairingError> {
        let payload: DevicePairingPayload = serde_json::from_str(payload)
            .map_err(|error| DevicePairingError::InvalidPayload(error.to_string()))?;
        if payload.version != PAIRING_PAYLOAD_VERSION {
            return Err(DevicePairingError::InvalidPayload(format!(
                "unsupported version {}",
                payload.version
            )));
        }
        if current_epoch > payload.expires_epoch {
            return Err(DevicePairingError::Expired);
        }

        let identity_key = identity.verifying_key().to_bytes();
        let payload_identity = decode_32_byte_hex(&payload.identity_key)?;
        if payload_identity != identity_key {
            return Err(DevicePairingError::IdentityMismatch);
        }
        self.require_authorizing_device(identity, payload.authorizing_device_id)?;
        if self.devices.values().any(|device| {
            device.status == DeviceStatus::Active && device.device_key == new_device_key.to_bytes()
        }) {
            return Err(DevicePairingError::DuplicateDeviceKey);
        }

        let message = canonical_pairing_message(
            payload.version,
            payload.authorizing_device_id,
            &payload.identity_key,
            &payload.requested_label,
            payload.challenge,
            payload.expires_epoch,
        );
        let signature = decode_signature(&payload.signature)?;
        identity
            .verifying_key()
            .verify(message.as_bytes(), &signature)
            .map_err(|_| DevicePairingError::SignatureVerificationFailed)?;

        Ok(self.insert_authorized_device(
            identity,
            new_device_key,
            payload.requested_label,
            current_epoch,
            "device-paired",
        ))
    }

    fn insert_authorized_device(
        &mut self,
        identity: &Identity,
        device_key: VerifyingKey,
        label: impl Into<String>,
        epoch: u64,
        event_kind: impl Into<String>,
    ) -> DeviceLeaf {
        let leaf = DeviceLeaf {
            device_id: Uuid::new_v4(),
            leaf_index: self.next_leaf,
            identity_key: identity.verifying_key().to_bytes(),
            device_key: device_key.to_bytes(),
            label: normalize_device_label(label),
            status: DeviceStatus::Active,
        };
        self.next_leaf = self.next_leaf.saturating_add(1);
        self.transparency.push(TransparencyEvent {
            device_id: leaf.device_id,
            kind: event_kind.into(),
            epoch,
        });
        self.devices.insert(leaf.device_id, leaf.clone());
        leaf
    }

    fn require_authorizing_device(
        &self,
        identity: &Identity,
        authorizing_device_id: Uuid,
    ) -> Result<&DeviceLeaf, DevicePairingError> {
        let expected_identity = identity.verifying_key().to_bytes();
        self.devices
            .get(&authorizing_device_id)
            .filter(|device| {
                device.status == DeviceStatus::Active && device.identity_key == expected_identity
            })
            .ok_or(DevicePairingError::UnauthorizedAuthorizingDevice)
    }

    /// Remove a device and emit a transparency event. Returns true if an active device changed.
    pub fn remove_device(&mut self, device_id: Uuid, epoch: u64) -> bool {
        let Some(device) = self.devices.get_mut(&device_id) else {
            return false;
        };
        if device.status == DeviceStatus::Removed {
            return false;
        }
        device.status = DeviceStatus::Removed;
        self.transparency.push(TransparencyEvent {
            device_id,
            kind: "device-removed".into(),
            epoch,
        });
        true
    }

    /// Active device leaves.
    #[must_use]
    pub fn active_devices(&self) -> Vec<&DeviceLeaf> {
        self.devices
            .values()
            .filter(|d| d.status == DeviceStatus::Active)
            .collect()
    }

    /// Transparency event stream.
    #[must_use]
    pub fn transparency_events(&self) -> &[TransparencyEvent] {
        &self.transparency
    }
}

fn canonical_pairing_message(
    version: u8,
    authorizing_device_id: Uuid,
    identity_key: &str,
    requested_label: &str,
    challenge: Uuid,
    expires_epoch: u64,
) -> String {
    format!(
        "discrypt-device-pairing-v{version}|authorizer={authorizing_device_id}|identity={identity_key}|label={requested_label}|challenge={challenge}|expires_epoch={expires_epoch}"
    )
}

fn normalize_device_label(label: impl Into<String>) -> String {
    let label = label.into();
    let trimmed = label.trim();
    if trimmed.is_empty() {
        "paired device".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn decode_32_byte_hex(value: &str) -> Result<[u8; 32], DevicePairingError> {
    let decoded =
        hex::decode(value).map_err(|error| DevicePairingError::InvalidPayload(error.to_string()))?;
    decoded
        .try_into()
        .map_err(|_| DevicePairingError::InvalidPayload("expected 32-byte key".to_owned()))
}

fn decode_signature(value: &str) -> Result<Signature, DevicePairingError> {
    let decoded =
        hex::decode(value).map_err(|error| DevicePairingError::InvalidPayload(error.to_string()))?;
    Signature::from_slice(&decoded)
        .map_err(|error| DevicePairingError::InvalidPayload(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;

    #[test]
    fn add_and_remove_device_emits_transparency() {
        let identity = Identity::generate("alice");
        let device_key = SigningKey::generate(&mut OsRng).verifying_key();
        let mut set = DeviceSet::new();
        let leaf = set.add_authorized_device(&identity, device_key, "laptop", 1);
        assert_eq!(leaf.leaf_index, 0);
        assert_eq!(set.active_devices().len(), 1);
        assert!(set.remove_device(leaf.device_id, 2));
        assert_eq!(set.active_devices().len(), 0);
        assert_eq!(set.transparency_events().len(), 2);
    }

    #[test]
    fn pairing_payload_adds_second_device_after_existing_device_authorization() {
        let identity = Identity::generate("alice");
        let laptop_key = SigningKey::generate(&mut OsRng).verifying_key();
        let phone_key = SigningKey::generate(&mut OsRng).verifying_key();
        let mut set = DeviceSet::new();
        let laptop = set.add_authorized_device(&identity, laptop_key, "laptop", 1);

        let payload = set
            .create_pairing_payload(&identity, laptop.device_id, "phone", 2, 3)
            .unwrap_or_else(|error| panic!("payload created: {error}"));
        let phone = set
            .add_device_from_pairing_payload(&identity, &payload, phone_key, 2)
            .unwrap_or_else(|error| panic!("payload accepted: {error}"));

        assert_eq!(phone.leaf_index, 1);
        assert_eq!(phone.label, "phone");
        assert_eq!(set.active_devices().len(), 2);
        assert_eq!(set.transparency_events()[1].kind, "device-paired");
        assert_eq!(laptop.identity_key, phone.identity_key);
        assert_ne!(laptop.device_key, phone.device_key);
    }

    #[test]
    fn pairing_rejects_tampering_expiry_and_missing_authorizer() {
        let identity = Identity::generate("alice");
        let other_identity = Identity::generate("mallory");
        let laptop_key = SigningKey::generate(&mut OsRng).verifying_key();
        let phone_key = SigningKey::generate(&mut OsRng).verifying_key();
        let mut set = DeviceSet::new();
        let laptop = set.add_authorized_device(&identity, laptop_key, "laptop", 1);
        let payload = set
            .create_pairing_payload(&identity, laptop.device_id, "phone", 2, 1)
            .unwrap_or_else(|error| panic!("payload created: {error}"));

        assert_eq!(
            set.add_device_from_pairing_payload(&identity, &payload, phone_key, 4),
            Err(DevicePairingError::Expired)
        );
        assert_eq!(
            set.add_device_from_pairing_payload(&other_identity, &payload, phone_key, 2),
            Err(DevicePairingError::IdentityMismatch)
        );

        let mut tampered: DevicePairingPayload =
            serde_json::from_str(&payload).unwrap_or_else(|error| panic!("{error}"));
        tampered.requested_label = "attacker-phone".to_owned();
        let tampered = serde_json::to_string(&tampered).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            set.add_device_from_pairing_payload(&identity, &tampered, phone_key, 2),
            Err(DevicePairingError::SignatureVerificationFailed)
        );

        let mut empty = DeviceSet::new();
        assert_eq!(
            empty.create_pairing_payload(&identity, laptop.device_id, "phone", 2, 1),
            Err(DevicePairingError::UnauthorizedAuthorizingDevice)
        );
    }
}

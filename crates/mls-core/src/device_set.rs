//! Multi-device leaf tracking and transparency events.

use crate::identity::Identity;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

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
        let leaf = DeviceLeaf {
            device_id: Uuid::new_v4(),
            leaf_index: self.next_leaf,
            identity_key: identity.verifying_key().to_bytes(),
            device_key: device_key.to_bytes(),
            label: label.into(),
            status: DeviceStatus::Active,
        };
        self.next_leaf = self.next_leaf.saturating_add(1);
        self.transparency.push(TransparencyEvent {
            device_id: leaf.device_id,
            kind: "device-added".into(),
            epoch,
        });
        self.devices.insert(leaf.device_id, leaf.clone());
        leaf
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
}

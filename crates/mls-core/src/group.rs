//! Minimal group state facade. Later phases replace internals with OpenMLS.

use crate::{derive_epoch_secret, DeviceLeaf, ExportLabel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

/// MLS leaf index type.
pub type LeafIndex = u32;

/// Errors from group state transitions.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum MlsCoreError {
    /// Leaf already exists.
    #[error("leaf {0} already exists")]
    LeafAlreadyExists(LeafIndex),
    /// Leaf not found.
    #[error("leaf {0} not found")]
    LeafNotFound(LeafIndex),
}

/// Phase-0 group wrapper preserving epoch/exporter contracts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupState {
    /// Group id.
    pub group_id: String,
    /// Current epoch.
    pub epoch: u64,
    members: BTreeMap<LeafIndex, DeviceLeaf>,
    epoch_secret: [u8; 32],
}

impl GroupState {
    /// Create an empty group.
    #[must_use]
    pub fn new(group_id: impl Into<String>) -> Self {
        let group_id = group_id.into();
        let digest: [u8; 32] = Sha256::digest(group_id.as_bytes()).into();
        Self {
            group_id,
            epoch: 0,
            members: BTreeMap::new(),
            epoch_secret: digest,
        }
    }

    /// Add a device leaf and advance epoch.
    pub fn add_leaf(&mut self, leaf: DeviceLeaf) -> Result<(), MlsCoreError> {
        if self.members.contains_key(&leaf.leaf_index) {
            return Err(MlsCoreError::LeafAlreadyExists(leaf.leaf_index));
        }
        self.members.insert(leaf.leaf_index, leaf);
        self.advance_epoch(b"add");
        Ok(())
    }

    /// Remove a leaf and advance epoch.
    pub fn remove_leaf(&mut self, leaf: LeafIndex) -> Result<(), MlsCoreError> {
        self.members
            .remove(&leaf)
            .ok_or(MlsCoreError::LeafNotFound(leaf))?;
        self.advance_epoch(b"remove");
        Ok(())
    }

    /// Current active members.
    #[must_use]
    pub fn members(&self) -> &BTreeMap<LeafIndex, DeviceLeaf> {
        &self.members
    }

    /// Export a secret for another subsystem.
    #[must_use]
    pub fn export(&self, label: ExportLabel, context: &[u8]) -> [u8; 32] {
        derive_epoch_secret(&self.epoch_secret, label, context)
    }

    fn advance_epoch(&mut self, reason: &[u8]) {
        self.epoch = self.epoch.saturating_add(1);
        let mut h = Sha256::new();
        h.update(self.epoch_secret);
        h.update(self.epoch.to_be_bytes());
        h.update(reason);
        self.epoch_secret = h.finalize().into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeviceSet, Identity};
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;

    #[test]
    fn add_remove_advances_epoch_and_rotates_exporter() {
        let identity = Identity::generate("alice");
        let mut devices = DeviceSet::new();
        let leaf = devices.add_authorized_device(
            &identity,
            SigningKey::generate(&mut OsRng).verifying_key(),
            "laptop",
            0,
        );
        let mut group = GroupState::new("room");
        let before = group.export(ExportLabel::SFrame, b"call");
        assert_eq!(group.add_leaf(leaf.clone()), Ok(()));
        let after_add = group.export(ExportLabel::SFrame, b"call");
        assert_ne!(before, after_add);
        assert_eq!(group.remove_leaf(leaf.leaf_index), Ok(()));
        assert_eq!(group.epoch, 2);
    }

    #[test]
    fn single_identity_can_participate_with_two_device_leaves() {
        let identity = Identity::generate("alice");
        let mut devices = DeviceSet::new();
        let laptop = devices.add_authorized_device(
            &identity,
            SigningKey::generate(&mut OsRng).verifying_key(),
            "laptop",
            0,
        );
        let phone = devices.add_authorized_device(
            &identity,
            SigningKey::generate(&mut OsRng).verifying_key(),
            "phone",
            1,
        );

        let mut group = GroupState::new("room");
        assert_eq!(group.add_leaf(laptop.clone()), Ok(()));
        assert_eq!(group.add_leaf(phone.clone()), Ok(()));

        assert_eq!(group.members().len(), 2);
        assert_eq!(laptop.identity_key, phone.identity_key);
        assert_ne!(laptop.device_key, phone.device_key);
        assert_ne!(laptop.leaf_index, phone.leaf_index);
    }

    #[test]
    fn sixteen_member_group_add_remove_preserves_stable_exporter_contract() {
        let mut group = GroupState::new("room-16");
        let mut leaves = Vec::new();

        for idx in 0..16 {
            let identity = Identity::generate(format!("member-{idx}"));
            let mut devices = DeviceSet::new();
            let mut leaf = devices.add_authorized_device(
                &identity,
                SigningKey::generate(&mut OsRng).verifying_key(),
                "primary",
                idx,
            );
            // Phase-0 group facade models the group-assigned leaf index explicitly.
            leaf.leaf_index = idx as u32;
            assert_eq!(group.add_leaf(leaf.clone()), Ok(()));
            leaves.push(leaf);
        }

        assert_eq!(group.members().len(), 16);
        assert_eq!(group.epoch, 16);
        let first = group.export(ExportLabel::SFrame, b"call");
        let second = group.export(ExportLabel::SFrame, b"call");
        assert_eq!(first, second);

        for leaf in leaves.into_iter().take(4) {
            assert_eq!(group.remove_leaf(leaf.leaf_index), Ok(()));
        }

        assert_eq!(group.members().len(), 12);
        assert_eq!(group.epoch, 20);
        assert_ne!(first, group.export(ExportLabel::SFrame, b"call"));
    }
}

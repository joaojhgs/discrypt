//! Core identity, device-set, governance, and MLS-adjacent primitives for discrypt v1.
//!
//! This crate intentionally wraps protocol state behind small, testable types. Phase 0
//! provides deterministic primitives and test surfaces; later phases swap the group engine
//! behind these boundaries for OpenMLS without changing higher-level contracts.

pub mod device_set;
pub mod exporter;
pub mod governance;
pub mod group;
pub mod identity;
pub mod provider;

pub use device_set::{DeviceLeaf, DeviceSet, DeviceStatus, TransparencyEvent};
pub use exporter::{derive_epoch_secret, ExportLabel};
pub use governance::{CanonicalEventRef, GovernanceAction, GovernanceEvent, GovernanceLog, Role};
pub use group::{GroupState, LeafIndex, MlsCoreError};
pub use identity::{FriendCode, Identity, SafetyNumber};

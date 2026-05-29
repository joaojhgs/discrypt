//! Core identity, device-set, governance, and MLS-adjacent primitives for discrypt v1.
//!
//! This crate intentionally wraps protocol state behind small, testable types. Phase 0
//! provides deterministic primitives and test surfaces; later phases swap the group engine
//! behind these boundaries for OpenMLS without changing higher-level contracts.

//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod device_set;
pub mod exporter;
pub mod governance;
pub mod group;
pub mod identity;
pub mod openmls_engine;
pub mod production_status;
pub mod provider;

pub use device_set::{
    DeviceLeaf, DevicePairingError, DevicePairingPayload, DeviceRotation, DeviceSet,
    DeviceSetError, DeviceStatus, TransparencyEvent,
};
pub use exporter::{derive_epoch_secret, ExportLabel};
pub use governance::{CanonicalEventRef, GovernanceAction, GovernanceEvent, GovernanceLog, Role};
pub use group::{GroupState, LeafIndex, MlsCoreError};
pub use identity::{verifying_key_from_hex, FriendCode, Identity, SafetyNumber};
pub use openmls_engine::{
    DiscryptOpenMlsProvider, JsonOpenMlsCodec, OpenMlsGroupEngine, OpenMlsGroupError,
    OpenMlsGroupOperationResult, OpenMlsGroupSnapshot, OpenMlsMemberPackage,
};

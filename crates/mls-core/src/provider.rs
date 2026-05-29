//! Provider metadata for the OpenMLS crypto/storage integration.
//!
//! ## ProductionStatus
//! Phase D wires upstream OpenMLS with the RustCrypto provider and OpenMLS SQLite
//! storage behind the group service boundary. Release audit status remains
//! explicit metadata rather than an implicit production-readiness claim.

/// Provider marker describing the current cryptographic backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderInfo {
    /// Provider name.
    pub name: &'static str,
    /// Whether this provider is production-audited for release.
    pub production_audited: bool,
}

/// OpenMLS provider metadata.
#[must_use]
pub fn provider_info() -> ProviderInfo {
    ProviderInfo {
        name: "openmls-rustcrypto-sqlite",
        production_audited: false,
    }
}

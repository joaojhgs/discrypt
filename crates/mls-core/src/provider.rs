//! Provider boundary for future OpenMLS crypto/storage integration.
//!
//! ## ProductionStatus
//! The current provider is an unaudited Phase-0 facade. Production builds must
//! replace it with an audited OpenMLS provider/storage integration before making
//! MLS production-readiness claims.

/// Provider marker describing the current cryptographic backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderInfo {
    /// Provider name.
    pub name: &'static str,
    /// Whether this provider is production-audited for release.
    pub production_audited: bool,
}

/// Phase-0 provider metadata.
#[must_use]
pub fn provider_info() -> ProviderInfo {
    ProviderInfo {
        name: "phase0-rustcrypto-facade",
        production_audited: false,
    }
}

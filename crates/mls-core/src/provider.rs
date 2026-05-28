//! Provider boundary for future OpenMLS crypto/storage integration.

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

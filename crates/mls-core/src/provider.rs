//! OpenMLS provider metadata.
//!
//! The production-facing OpenMLS engine lives in [`crate::openmls_engine`]. This
//! module intentionally exposes only backend metadata so there is a single group
//! operation implementation path: RustCrypto plus the OpenMLS SQLite
//! `StorageProvider` behind `OpenMlsGroupEngine`.

/// Provider marker describing the current cryptographic backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderInfo {
    /// Provider name.
    pub name: &'static str,
    /// Whether this provider is production-audited for release.
    pub production_audited: bool,
}

/// OpenMLS provider metadata for the active Discrypt group engine.
#[must_use]
pub fn provider_info() -> ProviderInfo {
    ProviderInfo {
        name: "openmls-rustcrypto-sqlite-storage",
        production_audited: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_info_reports_storage_backed_openmls_path() {
        let info = provider_info();
        assert_eq!(info.name, "openmls-rustcrypto-sqlite-storage");
        assert!(!info.production_audited);
    }
}

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

/// ADR-002 OpenMLS provider/storage decision surfaced for release gates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenMlsProviderDecision {
    /// Selected upstream OpenMLS crate version requirement.
    pub openmls_version: &'static str,
    /// Selected OpenMLS cryptographic provider.
    pub crypto_provider: &'static str,
    /// Selected persistent OpenMLS storage provider.
    pub storage_provider: &'static str,
    /// Selected storage serialization codec.
    pub storage_codec: &'static str,
    /// Selected MLS ciphersuite.
    pub ciphersuite: &'static str,
    /// Selected signature scheme for member credentials.
    pub signature_scheme: &'static str,
    /// Persistent group-store design used by Discrypt services.
    pub group_store_design: &'static str,
    /// Exporter handling policy across Rust service boundaries.
    pub exporter_handling: &'static str,
    /// Commit and Welcome persistence policy.
    pub commit_welcome_persistence: &'static str,
    /// Fork-repair integration policy.
    pub fork_repair_integration: &'static str,
}

impl OpenMlsProviderDecision {
    /// True when the decision covers every ADR-002 launch-hint dimension.
    #[must_use]
    pub fn covers_launch_hint(&self) -> bool {
        [
            self.openmls_version,
            self.crypto_provider,
            self.storage_provider,
            self.storage_codec,
            self.ciphersuite,
            self.signature_scheme,
            self.group_store_design,
            self.exporter_handling,
            self.commit_welcome_persistence,
            self.fork_repair_integration,
        ]
        .iter()
        .all(|field| !field.trim().is_empty())
    }

    /// True when raw exporter material is explicitly constrained to Rust service owners.
    #[must_use]
    pub fn exporter_is_rust_only(&self) -> bool {
        self.exporter_handling.contains("Rust service labels only")
            && self.exporter_handling.contains("never to UI")
    }
}

/// OpenMLS provider metadata for the active Discrypt group engine.
#[must_use]
pub fn provider_info() -> ProviderInfo {
    ProviderInfo {
        name: "openmls-rustcrypto-sqlite-storage",
        production_audited: false,
    }
}

/// ADR-002 provider/storage decision for the active Discrypt group engine.
#[must_use]
pub fn provider_decision() -> OpenMlsProviderDecision {
    OpenMlsProviderDecision {
        openmls_version: "0.8.1",
        crypto_provider: "openmls_rust_crypto::RustCrypto",
        storage_provider: "openmls_sqlite_storage::SqliteStorageProvider<JsonOpenMlsCodec, Connection>",
        storage_codec: "JsonOpenMlsCodec using serde_json for OpenMLS provider records",
        ciphersuite: "MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519",
        signature_scheme: "ED25519",
        group_store_design: "OpenMlsGroupEngine owns MlsGroup instances backed by a per-profile SQLite provider path and reloads groups through MlsGroup::load",
        exporter_handling: "OpenMLS export_secret output is routed to approved Rust service labels only and never to UI, commands, signaling, relay, or logs",
        commit_welcome_persistence: "pending commits are staged, compared by bytes before merge, merged through OpenMLS storage, and Welcome/GroupInfo bytes are serialized for authorized joiners only",
        fork_repair_integration: "mls-delivery detects fork/replay/downgrade states and repair plans re-add/rejoin members instead of replaying divergent MLS commits",
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

    #[test]
    fn provider_decision_covers_adr_002_launch_hint() {
        let decision = provider_decision();
        assert_eq!(decision.openmls_version, "0.8.1");
        assert_eq!(decision.crypto_provider, "openmls_rust_crypto::RustCrypto");
        assert!(decision.storage_provider.contains("SqliteStorageProvider"));
        assert_eq!(decision.signature_scheme, "ED25519");
        assert!(decision.covers_launch_hint());
        assert!(decision.exporter_is_rust_only());
        assert!(decision
            .commit_welcome_persistence
            .contains("Welcome/GroupInfo"));
        assert!(decision.fork_repair_integration.contains("mls-delivery"));
    }
}

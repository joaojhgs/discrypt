//! Build-time production/harness status for this crate.
//!
//! `ProductionStatus` is intentionally compile-time only: it records which of
//! the release-control features Cargo enabled for this crate so callers and
//! static checks can distinguish production paths from harness/local-dev paths
//! without inferring production readiness from deterministic test adapters.

/// Cargo feature status for this crate's production-readiness gates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductionStatus {
    /// Cargo package name whose gates were evaluated.
    pub crate_name: &'static str,
    /// Deterministic harness-only adapters/tests are compiled in.
    pub harness: bool,
    /// Local developer fallbacks are compiled in.
    pub local_dev: bool,
    /// Real network adapters may be compiled in; does not by itself prove runtime configuration.
    pub production_network: bool,
    /// Real media adapters may be compiled in; does not by itself prove runtime configuration.
    pub production_media: bool,
    /// Real storage/keychain adapters may be compiled in; does not by itself prove runtime configuration.
    pub production_storage: bool,
}

impl ProductionStatus {
    /// Capture this crate's Cargo feature gates.
    #[must_use]
    pub const fn current() -> Self {
        Self {
            crate_name: env!("CARGO_PKG_NAME"),
            harness: cfg!(feature = "harness"),
            local_dev: cfg!(feature = "local-dev"),
            production_network: cfg!(feature = "production-network"),
            production_media: cfg!(feature = "production-media"),
            production_storage: cfg!(feature = "production-storage"),
        }
    }

    /// True when any explicit production capability feature is enabled.
    #[must_use]
    pub const fn has_production_feature(self) -> bool {
        self.production_network || self.production_media || self.production_storage
    }

    /// True when harness/local-dev code is part of this build and must be labelled non-production.
    #[must_use]
    pub const fn requires_non_production_label(self) -> bool {
        self.harness || self.local_dev
    }
}

/// Current crate build status.
pub const CURRENT: ProductionStatus = ProductionStatus::current();

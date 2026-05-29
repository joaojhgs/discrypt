//! Exporter derivation facade used by Rust text/media/content-key service layers.

use sha2::{Digest, Sha256};

/// Rust service labels allowed to receive exporter-derived secrets.
///
/// Keep this enum restricted to services that own encrypted payload handling in
/// Rust. Command/UI, governance, admission, signaling, transport, relay, and
/// keychain boundaries must not request arbitrary exporter labels or raw bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportLabel {
    /// Text encryption/history delivery service material.
    Text,
    /// Media/SFrame service material.
    Media,
    /// Message content-key service material.
    ContentKey,
}

impl ExportLabel {
    fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Text => b"discrypt/v1/text",
            Self::Media => b"discrypt/v1/media",
            Self::ContentKey => b"discrypt/v1/content-key",
        }
    }
}

/// Derive a deterministic phase-0 secret from an epoch secret, approved Rust
/// service label, and service-owned context.
#[must_use]
pub fn derive_epoch_secret(epoch_secret: &[u8], label: ExportLabel, context: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"discrypt-exporter-v1");
    h.update(label.as_bytes());
    h.update((context.len() as u64).to_be_bytes());
    h.update(context);
    h.update(epoch_secret);
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_domain_separate() {
        let seed = [7u8; 32];
        assert_ne!(
            derive_epoch_secret(&seed, ExportLabel::Media, b"room"),
            derive_epoch_secret(&seed, ExportLabel::ContentKey, b"room")
        );
    }

    #[test]
    fn media_exporter_is_bound_to_sender_device_context() {
        let seed = [7u8; 32];
        assert_ne!(
            derive_epoch_secret(&seed, ExportLabel::Media, b"leaf=1;device=laptop"),
            derive_epoch_secret(&seed, ExportLabel::Media, b"leaf=1;device=phone")
        );
    }
}

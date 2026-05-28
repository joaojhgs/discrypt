//! Exporter derivation facade used by media/content-key layers.

use sha2::{Digest, Sha256};

/// Labels for exporter-derived secrets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportLabel {
    SFrame,
    Content,
    Governance,
}

impl ExportLabel {
    fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::SFrame => b"discrypt/v1/sframe",
            Self::Content => b"discrypt/v1/content",
            Self::Governance => b"discrypt/v1/governance",
        }
    }
}

/// Derive a deterministic phase-0 secret from an epoch secret, label, and context.
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
            derive_epoch_secret(&seed, ExportLabel::SFrame, b"room"),
            derive_epoch_secret(&seed, ExportLabel::Content, b"room")
        );
    }

    #[test]
    fn sframe_exporter_is_bound_to_sender_device_context() {
        let seed = [7u8; 32];
        assert_ne!(
            derive_epoch_secret(&seed, ExportLabel::SFrame, b"leaf=1;device=laptop"),
            derive_epoch_secret(&seed, ExportLabel::SFrame, b"leaf=1;device=phone")
        );
    }
}

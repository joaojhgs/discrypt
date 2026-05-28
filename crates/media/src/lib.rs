//! Media encryption and sender-binding facade.
use mls_core::{derive_epoch_secret, ExportLabel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use thiserror::Error;

/// Media sender binding from KID to MLS leaf/device.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SenderBinding {
    pub kid: Vec<u8>,
    pub leaf_index: u32,
    pub device_id: String,
}
#[derive(Debug, Error, Eq, PartialEq)]
pub enum MediaError {
    #[error("unknown sender binding")]
    UnknownSender,
    #[error("replay detected")]
    Replay,
}
/// Receiver anti-replay window.
#[derive(Default)]
pub struct ReplayWindow {
    seen: BTreeSet<(Vec<u8>, u64)>,
}
impl ReplayWindow {
    pub fn accept(&mut self, kid: &[u8], counter: u64) -> Result<(), MediaError> {
        if !self.seen.insert((kid.to_vec(), counter)) {
            return Err(MediaError::Replay);
        }
        Ok(())
    }
}
#[must_use]
pub fn derive_media_key(epoch_secret: &[u8], binding: &SenderBinding) -> [u8; 32] {
    let ctx = serde_json::to_vec(binding).unwrap_or_default();
    derive_epoch_secret(epoch_secret, ExportLabel::SFrame, &ctx)
}
#[must_use]
pub fn protect_frame(key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
    let pad: [u8; 32] = Sha256::digest(key).into();
    plaintext
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ pad[i % pad.len()])
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_replay_and_roundtrips_facade_ciphertext() {
        let b = SenderBinding {
            kid: b"k".to_vec(),
            leaf_index: 1,
            device_id: "d".into(),
        };
        let key = derive_media_key(&[1; 32], &b);
        let ct = protect_frame(&key, b"hi");
        assert_eq!(protect_frame(&key, &ct), b"hi");
        let mut w = ReplayWindow::default();
        assert!(w.accept(&b.kid, 1).is_ok());
        assert_eq!(w.accept(&b.kid, 1), Err(MediaError::Replay));
    }
}

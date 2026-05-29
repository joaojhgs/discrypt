//! Insertable-Streams ↔ Rust SFrame bridge.
//!
//! JS owns encoded frame bytes and opaque KIDs/counters only. Raw media keys stay
//! in Rust `SFrameSender` / `SFrameReceiver` state.

use crate::{
    MediaError, ProtectedFrame, SFrameReceiver, SFrameSender, SenderBinding, VerifiedFrame,
};
use serde::{Deserialize, Serialize};

/// Clear encoded frame payload received from WebRTC Insertable Streams.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BridgeClearFrame {
    /// Encoded audio/video frame bytes before SFrame protection.
    pub bytes: Vec<u8>,
}

/// Verified clear encoded frame plus authenticated sender identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BridgeVerifiedFrame {
    /// Authenticated MLS sender binding.
    pub sender: SenderBinding,
    /// Accepted sender counter.
    pub counter: u64,
    /// Opened encoded media frame.
    pub clear: BridgeClearFrame,
}

/// Protected encoded frame that can be passed through JS, relays, and WebRTC.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BridgeProtectedFrame {
    /// KID copied from Rust sender binding; not a secret.
    pub kid: Vec<u8>,
    /// Sender counter copied from Rust sender state; not a secret.
    pub counter: u64,
    /// Authenticated ciphertext bytes.
    pub bytes: Vec<u8>,
}

impl From<ProtectedFrame> for BridgeProtectedFrame {
    fn from(frame: ProtectedFrame) -> Self {
        Self {
            kid: frame.kid,
            counter: frame.counter,
            bytes: frame.ciphertext,
        }
    }
}

impl From<BridgeProtectedFrame> for ProtectedFrame {
    fn from(frame: BridgeProtectedFrame) -> Self {
        Self {
            kid: frame.kid,
            counter: frame.counter,
            ciphertext: frame.bytes,
        }
    }
}

impl From<VerifiedFrame> for BridgeClearFrame {
    fn from(frame: VerifiedFrame) -> Self {
        Self {
            bytes: frame.plaintext,
        }
    }
}

/// Rust-owned transform bridge state for one sender/receiver media direction.
#[derive(Debug)]
pub struct RustTransformBridge {
    sender: SFrameSender,
    receiver: SFrameReceiver,
}

impl RustTransformBridge {
    /// Construct a bridge from Rust-owned media crypto states.
    #[must_use]
    pub fn new(sender: SFrameSender, receiver: SFrameReceiver) -> Self {
        Self { sender, receiver }
    }

    /// Protect an encoded frame from JS without exposing key material to JS.
    pub fn protect_encoded(
        &mut self,
        frame: BridgeClearFrame,
    ) -> Result<BridgeProtectedFrame, MediaError> {
        self.sender.protect(&frame.bytes).map(Into::into)
    }

    /// Open an encoded frame and return plaintext bytes only after verification.
    pub fn open_encoded(
        &mut self,
        frame: BridgeProtectedFrame,
    ) -> Result<BridgeClearFrame, MediaError> {
        self.open_protected_frame(frame)
            .map(|verified| verified.clear)
    }

    /// Open an encoded frame and return the authenticated sender binding and counter.
    pub fn open_protected_frame(
        &mut self,
        frame: BridgeProtectedFrame,
    ) -> Result<BridgeVerifiedFrame, MediaError> {
        let verified = self.receiver.open(&frame.into())?;
        Ok(BridgeVerifiedFrame {
            sender: verified.binding,
            counter: verified.counter,
            clear: BridgeClearFrame {
                bytes: verified.plaintext,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MediaKeyRegistry, ReplayWindow, SenderBinding};

    #[test]
    fn bridge_roundtrip_exposes_only_frame_metadata_to_js() -> Result<(), MediaError> {
        let binding = SenderBinding::derive_for_epoch(&[7; 32], "bridge-group", 7, 7, "desktop")?;
        let sender = SFrameSender::new(&[7; 32], binding.clone())?;
        let mut registry = MediaKeyRegistry::new();
        registry.register_sender(&[7; 32], binding)?;
        let receiver = SFrameReceiver::new(registry, ReplayWindow::default());
        let mut bridge = RustTransformBridge::new(sender, receiver);

        let protected = bridge.protect_encoded(BridgeClearFrame {
            bytes: b"encoded opus".to_vec(),
        })?;
        assert!(!protected.kid.is_empty());
        assert_ne!(protected.bytes, b"encoded opus");
        let clear = bridge.open_encoded(protected)?;
        assert_eq!(clear.bytes, b"encoded opus");
        Ok(())
    }
}

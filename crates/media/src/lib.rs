//! Media encryption, transform bridge, and sender-binding facade.
//!
//! The media crate is the Phase-1 boundary between MLS exporter state and the
//! WebRTC frame path. Rust owns SFrame-like keys; callers only exchange KIDs,
//! counters, plaintext frames, and protected frames.

pub mod sframe;
pub mod transform_bridge;
pub mod transport;

pub use sframe::{
    MediaError, MediaKeyRegistry, ProtectedFrame, ReplayWindow, SFrameKey, SFrameReceiver,
    SFrameSender, SenderBinding, VerifiedFrame,
};
pub use transform_bridge::{BridgeClearFrame, BridgeProtectedFrame, RustTransformBridge};
pub use transport::{AndroidVoiceContingency, MediaTransportPath, NativeWebRtcRsSkeleton};

//! Media encryption, transform bridge, and sender-binding boundary.
//!
//! The media crate is the Phase-1 boundary between MLS exporter state and the
//! WebRTC frame path. Rust owns SFrame-like keys; callers only exchange KIDs,
//! counters, plaintext frames, and protected frames.

//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod capture;
pub mod production_status;
pub mod sframe;
pub mod transform_bridge;
pub mod transport;

pub use capture::{
    apply_app_output_volume_percent, apply_microphone_gain_percent, AudioCaptureFormat,
    CapturedAudioFrame, DecodedAudioFrame, EncodedOpusFrame, OpusAudioDecoder, OpusAudioEncoder,
    PlaybackAudioSink, PlaybackVolumeMixer, ProtectedMediaFrameSink, SpeakerPlaybackKey,
    VoiceActivityDetector, VoiceActivityLevel, VoiceActivitySource, VoiceCaptureSFramePipeline,
    VoiceCaptureSendOutcome, VoiceCaptureSendReport, VoiceJitterBuffer, VoiceReceiveSFramePipeline,
    APP_AUDIO_GAIN_UNITY_PERCENT, APP_OUTPUT_VOLUME_MAX_PERCENT, MIC_GAIN_MAX_PERCENT,
};
pub use sframe::{
    MediaError, MediaKeyRegistry, ProtectedFrame, ReplayWindow, SFrameKey, SFrameReceiver,
    SFrameSender, SenderBinding, VerifiedFrame,
};
pub use transform_bridge::{
    BridgeClearFrame, BridgeProtectedFrame, BridgeVerifiedFrame, RustTransformBridge,
};
pub use transport::{
    build_voice_overlay_fanout, AndroidVoiceContingency, MediaTransportPath,
    MicrophonePermissionState, NativeWebRtcRsContingency, NativeWebRtcRsContingencyError,
    VoiceDeviceDescriptor, VoiceDeviceKind, VoiceDeviceSelection, VoiceFanoutDelivery,
    VoiceFanoutRouteKind, VoiceOverlayFanoutInput, VoiceOverlayFanoutPlan,
};

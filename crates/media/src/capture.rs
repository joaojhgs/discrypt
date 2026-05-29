//! Voice capture → Opus encode → Rust SFrame protection pipeline.
//!
//! WebRTC captures and plays audio at 48 kHz. This module keeps that media path
//! honest: captured PCM is validated, encoded with a real Rust Opus encoder,
//! passed to the Rust-owned transform bridge, and only protected SFrame bytes are
//! handed to the transport sink.

use crate::{BridgeClearFrame, BridgeProtectedFrame, MediaError, RustTransformBridge};
use libopus_rs::{Application, Encoder};
use serde::{Deserialize, Serialize};

const WEBRTC_OPUS_SAMPLE_RATE_HZ: u32 = 48_000;
const MIN_OPUS_PACKET_BYTES: usize = 3;
const MAX_OPUS_PACKET_BYTES: usize = 1_275;
const MAX_OPUS_FRAME_DATA_BYTES: usize = 1_274;

/// PCM capture format accepted by the production voice send path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioCaptureFormat {
    /// Capture sample rate in Hz. WebRTC Opus capture is normalized to 48 kHz.
    pub sample_rate_hz: u32,
    /// Number of interleaved PCM channels.
    pub channels: u8,
    /// Frame duration in milliseconds.
    pub frame_duration_ms: u16,
}

impl AudioCaptureFormat {
    /// Construct and validate the canonical WebRTC Opus capture format.
    pub fn new(
        sample_rate_hz: u32,
        channels: u8,
        frame_duration_ms: u16,
    ) -> Result<Self, MediaError> {
        let format = Self {
            sample_rate_hz,
            channels,
            frame_duration_ms,
        };
        format.validate()?;
        Ok(format)
    }

    /// Canonical mono 20 ms WebRTC voice frame format.
    #[must_use]
    pub const fn mono_20ms_48khz() -> Self {
        Self {
            sample_rate_hz: WEBRTC_OPUS_SAMPLE_RATE_HZ,
            channels: 1,
            frame_duration_ms: 20,
        }
    }

    /// Number of samples per channel in one frame.
    #[must_use]
    pub fn samples_per_channel(self) -> usize {
        (self.sample_rate_hz as usize * self.frame_duration_ms as usize) / 1_000
    }

    /// Number of interleaved PCM samples in one captured frame.
    #[must_use]
    pub fn interleaved_samples_per_frame(self) -> usize {
        self.samples_per_channel() * self.channels as usize
    }

    /// Validate that the format can be encoded by the Rust Opus path.
    pub fn validate(self) -> Result<(), MediaError> {
        if self.sample_rate_hz != WEBRTC_OPUS_SAMPLE_RATE_HZ {
            return Err(MediaError::InvalidAudioFrame(
                "voice capture must be normalized to 48 kHz before Opus encode".into(),
            ));
        }
        if !matches!(self.channels, 1 | 2) {
            return Err(MediaError::InvalidAudioFrame(
                "voice capture must be mono or stereo".into(),
            ));
        }
        if !matches!(self.frame_duration_ms, 10 | 20 | 40 | 60) {
            return Err(MediaError::InvalidAudioFrame(
                "voice capture frame duration must be 10, 20, 40, or 60 ms".into(),
            ));
        }
        if self.interleaved_samples_per_frame() == 0 {
            return Err(MediaError::InvalidAudioFrame(
                "voice capture frame cannot be empty".into(),
            ));
        }
        Ok(())
    }
}

/// One validated captured PCM frame before Opus encoding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapturedAudioFrame {
    /// Interleaved signed 16-bit PCM samples.
    pub pcm_i16: Vec<i16>,
    /// Capture format for these samples.
    pub format: AudioCaptureFormat,
    /// Monotonic capture timestamp from the local media clock.
    pub captured_at_ms: u64,
}

impl CapturedAudioFrame {
    /// Construct and validate a captured audio frame.
    pub fn new(
        pcm_i16: Vec<i16>,
        format: AudioCaptureFormat,
        captured_at_ms: u64,
    ) -> Result<Self, MediaError> {
        format.validate()?;
        if pcm_i16.len() != format.interleaved_samples_per_frame() {
            return Err(MediaError::InvalidAudioFrame(format!(
                "expected {} interleaved PCM samples, got {}",
                format.interleaved_samples_per_frame(),
                pcm_i16.len()
            )));
        }
        Ok(Self {
            pcm_i16,
            format,
            captured_at_ms,
        })
    }
}

/// Opus packet plus capture metadata before SFrame protection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EncodedOpusFrame {
    /// Monotonic sequence number assigned by the local encoder.
    pub sequence: u64,
    /// Capture format encoded into this Opus packet.
    pub format: AudioCaptureFormat,
    /// Capture timestamp carried across the media path.
    pub captured_at_ms: u64,
    /// Real Opus packet bytes produced by the Rust encoder.
    pub opus_payload: Vec<u8>,
}

impl EncodedOpusFrame {
    /// Convert to a transform-bridge clear frame. Metadata remains transport
    /// metadata; the media payload handed to SFrame is Opus bytes only.
    #[must_use]
    pub fn into_bridge_clear_frame(self) -> BridgeClearFrame {
        BridgeClearFrame {
            bytes: self.opus_payload,
        }
    }
}

/// Rust Opus encoder state for a single capture stream.
#[derive(Debug)]
pub struct OpusAudioEncoder {
    encoder: Encoder,
    format: AudioCaptureFormat,
    next_sequence: u64,
}

impl OpusAudioEncoder {
    /// Create an encoder for validated WebRTC voice capture frames.
    pub fn new(format: AudioCaptureFormat) -> Result<Self, MediaError> {
        format.validate()?;
        let encoder = Encoder::new(
            format.sample_rate_hz as i32,
            format.channels as usize,
            Application::Voip,
        )
        .map_err(|error| MediaError::OpusEncodeFailed(error.to_string()))?;
        Ok(Self {
            encoder,
            format,
            next_sequence: 0,
        })
    }

    /// Encode one captured PCM frame to a real Opus packet.
    pub fn encode(&mut self, frame: CapturedAudioFrame) -> Result<EncodedOpusFrame, MediaError> {
        if frame.format != self.format {
            return Err(MediaError::InvalidAudioFrame(
                "captured frame format changed inside one Opus encoder stream".into(),
            ));
        }
        let opus_payload = self
            .encoder
            .encode_i16_with_frame_bytes(
                &frame.pcm_i16,
                self.format.samples_per_channel(),
                MAX_OPUS_FRAME_DATA_BYTES,
            )
            .map_err(|error| MediaError::OpusEncodeFailed(error.to_string()))?;
        if opus_payload.len() < MIN_OPUS_PACKET_BYTES || opus_payload.len() > MAX_OPUS_PACKET_BYTES
        {
            return Err(MediaError::OpusEncodeFailed(
                "Opus encoder produced an empty, oversized, or invalid packet".into(),
            ));
        }
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(MediaError::CounterOverflow)?;
        Ok(EncodedOpusFrame {
            sequence,
            format: frame.format,
            captured_at_ms: frame.captured_at_ms,
            opus_payload,
        })
    }
}

/// Transport boundary that may receive only already protected media frames.
pub trait ProtectedMediaFrameSink {
    /// Send one protected media frame onto the WebRTC media/data transport.
    fn send_protected_media_frame(&mut self, frame: BridgeProtectedFrame)
        -> Result<(), MediaError>;
}

/// Evidence returned after one PCM frame is encoded, protected, and handed off.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceCaptureSendReport {
    /// Opus encoder sequence assigned before SFrame protection.
    pub sequence: u64,
    /// Capture timestamp for UX/speaking-state correlation.
    pub captured_at_ms: u64,
    /// Number of Opus bytes protected by SFrame.
    pub opus_payload_len: usize,
    /// Number of protected bytes handed to the transport sink.
    pub protected_payload_len: usize,
    /// SFrame key id copied from Rust media sender state.
    pub kid: Vec<u8>,
    /// SFrame sender counter copied from Rust media sender state.
    pub counter: u64,
}

/// End-to-end local send pipeline for one voice capture stream.
pub struct VoiceCaptureSFramePipeline<S> {
    encoder: OpusAudioEncoder,
    bridge: RustTransformBridge,
    sink: S,
}

impl<S: ProtectedMediaFrameSink> VoiceCaptureSFramePipeline<S> {
    /// Construct the send pipeline from Rust-owned encoder, transform, and sink state.
    #[must_use]
    pub fn new(encoder: OpusAudioEncoder, bridge: RustTransformBridge, sink: S) -> Self {
        Self {
            encoder,
            bridge,
            sink,
        }
    }

    /// Encode, protect, and send exactly one captured audio frame.
    pub fn capture_encode_protect_send(
        &mut self,
        frame: CapturedAudioFrame,
    ) -> Result<VoiceCaptureSendReport, MediaError> {
        let encoded = self.encoder.encode(frame)?;
        let sequence = encoded.sequence;
        let captured_at_ms = encoded.captured_at_ms;
        let opus_payload_len = encoded.opus_payload.len();
        let protected = self
            .bridge
            .protect_encoded(encoded.into_bridge_clear_frame())?;
        let report = VoiceCaptureSendReport {
            sequence,
            captured_at_ms,
            opus_payload_len,
            protected_payload_len: protected.bytes.len(),
            kid: protected.kid.clone(),
            counter: protected.counter,
        };
        self.sink.send_protected_media_frame(protected)?;
        Ok(report)
    }

    /// Consume the pipeline and return the transport sink for verification or shutdown.
    #[must_use]
    pub fn into_sink(self) -> S {
        self.sink
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MediaKeyRegistry, ReplayWindow, SFrameReceiver, SFrameSender, SenderBinding};

    #[derive(Default)]
    struct RecordingSink {
        sent: Vec<BridgeProtectedFrame>,
    }

    impl ProtectedMediaFrameSink for RecordingSink {
        fn send_protected_media_frame(
            &mut self,
            frame: BridgeProtectedFrame,
        ) -> Result<(), MediaError> {
            self.sent.push(frame);
            Ok(())
        }
    }

    fn media_bridge() -> Result<RustTransformBridge, MediaError> {
        let binding = SenderBinding {
            kid: b"capture-kid".to_vec(),
            leaf_index: 42,
            device_id: "capture-device".to_owned(),
        };
        let sender = SFrameSender::new(&[4; 32], binding.clone())?;
        let mut registry = MediaKeyRegistry::new();
        registry.register_sender(&[4; 32], binding)?;
        Ok(RustTransformBridge::new(
            sender,
            SFrameReceiver::new(registry, ReplayWindow::default()),
        ))
    }

    fn sine_frame(format: AudioCaptureFormat) -> Vec<i16> {
        (0..format.interleaved_samples_per_frame())
            .map(|sample| {
                let phase = sample as f32 / format.sample_rate_hz as f32;
                (phase * 440.0 * core::f32::consts::TAU)
                    .sin()
                    .mul_add(4_000.0, 0.0) as i16
            })
            .collect()
    }

    #[test]
    fn capture_encodes_real_opus_and_sends_only_sframe_protected_bytes() -> Result<(), MediaError> {
        let format = AudioCaptureFormat::mono_20ms_48khz();
        let mut encoder = OpusAudioEncoder::new(format)?;
        let clear_capture = CapturedAudioFrame::new(sine_frame(format), format, 1_234)?;
        let encoded = encoder.encode(clear_capture.clone())?;
        assert_eq!(encoded.sequence, 0);
        assert_eq!(encoded.format, format);
        assert!(!encoded.opus_payload.is_empty());
        assert_ne!(encoded.opus_payload, b"encoded opus");
        assert!(encoded.opus_payload.len() <= MAX_OPUS_PACKET_BYTES);

        let original_opus = encoded.opus_payload.clone();
        let mut verification_bridge = media_bridge()?;
        let protected = verification_bridge.protect_encoded(encoded.into_bridge_clear_frame())?;
        assert_ne!(protected.bytes, original_opus);
        let reopened = verification_bridge.open_encoded(protected)?;
        assert_eq!(reopened.bytes, original_opus);

        let mut pipeline = VoiceCaptureSFramePipeline::new(
            OpusAudioEncoder::new(format)?,
            media_bridge()?,
            RecordingSink::default(),
        );
        let report = pipeline.capture_encode_protect_send(clear_capture)?;
        assert_eq!(report.sequence, 0);
        assert_eq!(report.captured_at_ms, 1_234);
        assert!(report.protected_payload_len > report.opus_payload_len);
        assert_eq!(report.counter, 0);
        let sink = pipeline.into_sink();
        assert_eq!(sink.sent.len(), 1);
        assert_ne!(sink.sent[0].bytes, original_opus);
        Ok(())
    }

    #[test]
    fn invalid_capture_format_or_frame_size_fails_closed() {
        assert!(AudioCaptureFormat::new(44_100, 1, 20).is_err());
        assert!(AudioCaptureFormat::new(48_000, 3, 20).is_err());
        assert!(AudioCaptureFormat::new(48_000, 1, 30).is_err());
        let format = AudioCaptureFormat::mono_20ms_48khz();
        assert!(CapturedAudioFrame::new(vec![0; 12], format, 0).is_err());
    }
}

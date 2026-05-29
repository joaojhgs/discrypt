//! Voice capture → Opus encode → Rust SFrame protection pipeline.
//!
//! WebRTC captures and plays audio at 48 kHz. This module keeps that media path
//! honest: captured PCM is validated, encoded with a real Rust Opus encoder,
//! passed to the Rust-owned transform bridge, and only protected SFrame bytes are
//! handed to the transport sink.

use crate::{BridgeClearFrame, BridgeProtectedFrame, MediaError, RustTransformBridge};
use libopus_rs::{Application, Decoder, Encoder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const WEBRTC_OPUS_SAMPLE_RATE_HZ: u32 = 48_000;
const MIN_OPUS_PACKET_BYTES: usize = 3;
const MAX_OPUS_PACKET_BYTES: usize = 1_275;
const MAX_OPUS_FRAME_DATA_BYTES: usize = 1_274;
/// 1000 millipercent is unity gain for speaker playback volume.
pub const SPEAKER_VOLUME_UNITY_MILLIPERCENT: u16 = 1_000;
/// Keep local playback gain bounded to avoid clipping abuse or accidental runaway amplification.
pub const SPEAKER_VOLUME_MAX_MILLIPERCENT: u16 = 2_000;

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

/// Rust Opus decoder state for a single receive/playback stream.
#[derive(Debug)]
pub struct OpusAudioDecoder {
    decoder: Decoder,
    format: AudioCaptureFormat,
}

impl OpusAudioDecoder {
    /// Create a decoder for validated WebRTC voice playback frames.
    pub fn new(format: AudioCaptureFormat) -> Result<Self, MediaError> {
        format.validate()?;
        let decoder = Decoder::new(format.sample_rate_hz as i32, format.channels as usize)
            .map_err(|error| MediaError::OpusDecodeFailed(error.to_string()))?;
        Ok(Self { decoder, format })
    }

    /// Decode one Opus packet to interleaved PCM samples.
    pub fn decode(&mut self, opus_payload: &[u8]) -> Result<Vec<i16>, MediaError> {
        let pcm = self
            .decoder
            .decode_i16(opus_payload, false)
            .map_err(|error| MediaError::OpusDecodeFailed(error.to_string()))?;
        if pcm.len() != self.format.interleaved_samples_per_frame() {
            return Err(MediaError::OpusDecodeFailed(format!(
                "decoded {} PCM samples, expected {}",
                pcm.len(),
                self.format.interleaved_samples_per_frame()
            )));
        }
        Ok(pcm)
    }
}

/// Decoded, sender-authenticated audio ready for jitter buffering/playback.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DecodedAudioFrame {
    /// Authenticated SFrame sender binding.
    pub sender: crate::SenderBinding,
    /// Accepted SFrame counter used for jitter ordering.
    pub counter: u64,
    /// Playback format.
    pub format: AudioCaptureFormat,
    /// Interleaved PCM samples after Opus decode.
    pub pcm_i16: Vec<i16>,
}

/// Stable playback-volume key derived only after SFrame authenticated the sender binding.
///
/// The key intentionally follows the user/device inside a group across MLS epoch/KID rotations,
/// so a local volume setting survives membership churn while still being scoped to an
/// authenticated media sender identity.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SpeakerPlaybackKey {
    /// Stable MLS group identifier for this voice room.
    pub group_id: String,
    /// Stable sender device identifier authenticated by the media sender binding.
    pub device_id: String,
}

impl SpeakerPlaybackKey {
    /// Build a playback key from an already authenticated sender binding.
    pub fn from_sender(sender: &crate::SenderBinding) -> Result<Self, MediaError> {
        sender.validate()?;
        Ok(Self {
            group_id: sender.group_id.clone(),
            device_id: sender.device_id.clone(),
        })
    }
}

/// Per-speaker playback volume mixer.
///
/// Volumes are stored as millipercent (`1000 == 100%`). The mixer accepts only
/// authenticated sender bindings from decoded media frames; callers cannot pick an
/// unauthenticated display name and affect another speaker.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlaybackVolumeMixer {
    default_volume_millipercent: u16,
    speaker_volumes_millipercent: BTreeMap<SpeakerPlaybackKey, u16>,
}

impl Default for PlaybackVolumeMixer {
    fn default() -> Self {
        Self::unity()
    }
}

impl PlaybackVolumeMixer {
    /// Create a mixer with unity gain and no per-speaker overrides.
    #[must_use]
    pub fn unity() -> Self {
        Self {
            default_volume_millipercent: SPEAKER_VOLUME_UNITY_MILLIPERCENT,
            speaker_volumes_millipercent: BTreeMap::new(),
        }
    }

    /// Create a mixer with a validated default playback volume.
    pub fn new(default_volume_millipercent: u16) -> Result<Self, MediaError> {
        validate_volume_millipercent(default_volume_millipercent)?;
        Ok(Self {
            default_volume_millipercent,
            speaker_volumes_millipercent: BTreeMap::new(),
        })
    }

    /// Update the default playback volume used when a speaker has no override.
    pub fn set_default_volume_millipercent(
        &mut self,
        volume_millipercent: u16,
    ) -> Result<(), MediaError> {
        validate_volume_millipercent(volume_millipercent)?;
        self.default_volume_millipercent = volume_millipercent;
        Ok(())
    }

    /// Set a volume override for one authenticated sender binding.
    pub fn set_speaker_volume(
        &mut self,
        sender: &crate::SenderBinding,
        volume_millipercent: u16,
    ) -> Result<(), MediaError> {
        let key = SpeakerPlaybackKey::from_sender(sender)?;
        self.set_speaker_volume_by_key(key, volume_millipercent)
    }

    /// Set a volume override for a previously authenticated speaker key.
    pub fn set_speaker_volume_by_key(
        &mut self,
        key: SpeakerPlaybackKey,
        volume_millipercent: u16,
    ) -> Result<(), MediaError> {
        validate_volume_millipercent(volume_millipercent)?;
        if volume_millipercent == self.default_volume_millipercent {
            self.speaker_volumes_millipercent.remove(&key);
        } else {
            self.speaker_volumes_millipercent
                .insert(key, volume_millipercent);
        }
        Ok(())
    }

    /// Read the effective playback volume for an authenticated sender binding.
    pub fn volume_for_sender(&self, sender: &crate::SenderBinding) -> Result<u16, MediaError> {
        let key = SpeakerPlaybackKey::from_sender(sender)?;
        Ok(*self
            .speaker_volumes_millipercent
            .get(&key)
            .unwrap_or(&self.default_volume_millipercent))
    }

    /// Apply the authenticated sender's volume to decoded PCM with saturating i16 bounds.
    pub fn mix_frame(&self, mut frame: DecodedAudioFrame) -> Result<DecodedAudioFrame, MediaError> {
        let volume = self.volume_for_sender(&frame.sender)?;
        for sample in &mut frame.pcm_i16 {
            *sample = apply_volume(*sample, volume);
        }
        Ok(frame)
    }
}

fn validate_volume_millipercent(volume_millipercent: u16) -> Result<(), MediaError> {
    if volume_millipercent <= SPEAKER_VOLUME_MAX_MILLIPERCENT {
        Ok(())
    } else {
        Err(MediaError::InvalidPlaybackVolume(volume_millipercent))
    }
}

fn apply_volume(sample: i16, volume_millipercent: u16) -> i16 {
    let scaled =
        sample as i32 * volume_millipercent as i32 / SPEAKER_VOLUME_UNITY_MILLIPERCENT as i32;
    scaled.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// Source of a voice activity/audio-level event.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VoiceActivitySource {
    /// Local microphone capture before Opus encode.
    LocalCapture,
    /// Remote decoded media before local playback-volume mixing.
    RemotePlayback,
}

/// Audio-level event produced from real PCM samples and VAD state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceActivityLevel {
    /// Where this event was observed.
    pub source: VoiceActivitySource,
    /// Authenticated speaker key for remote playback; local capture uses `None`.
    pub speaker: Option<SpeakerPlaybackKey>,
    /// SFrame counter for remote playback events.
    pub counter: Option<u64>,
    /// Local capture timestamp for capture events.
    pub captured_at_ms: Option<u64>,
    /// Root-mean-square level over the observed PCM frame.
    pub rms_i16: u16,
    /// Peak absolute sample value over the observed PCM frame.
    pub peak_i16: u16,
    /// VAD decision after threshold and hangover.
    pub speaking: bool,
}

/// Deterministic PCM-level VAD used for voice speaking indicators.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceActivityDetector {
    speaking_threshold_rms_i16: u16,
    hangover_frames: u8,
    hangover_remaining: u8,
}

impl Default for VoiceActivityDetector {
    fn default() -> Self {
        Self::voice_defaults()
    }
}

impl VoiceActivityDetector {
    /// Default VAD threshold/hangover for 48 kHz voice frames.
    #[must_use]
    pub const fn voice_defaults() -> Self {
        Self {
            speaking_threshold_rms_i16: 512,
            hangover_frames: 2,
            hangover_remaining: 0,
        }
    }

    /// Construct a VAD with an explicit RMS threshold and hangover.
    pub fn new(speaking_threshold_rms_i16: u16, hangover_frames: u8) -> Result<Self, MediaError> {
        if speaking_threshold_rms_i16 == 0 {
            return Err(MediaError::InvalidAudioFrame(
                "VAD threshold must be greater than zero".into(),
            ));
        }
        Ok(Self {
            speaking_threshold_rms_i16,
            hangover_frames,
            hangover_remaining: 0,
        })
    }

    /// Observe one real local capture frame and produce a speaking event.
    pub fn observe_local_capture(
        &mut self,
        frame: &CapturedAudioFrame,
    ) -> Result<VoiceActivityLevel, MediaError> {
        validate_pcm_len(frame.format, &frame.pcm_i16)?;
        let (rms_i16, peak_i16) = pcm_level_metrics(&frame.pcm_i16);
        let speaking = self.update_speaking(rms_i16);
        Ok(VoiceActivityLevel {
            source: VoiceActivitySource::LocalCapture,
            speaker: None,
            counter: None,
            captured_at_ms: Some(frame.captured_at_ms),
            rms_i16,
            peak_i16,
            speaking,
        })
    }

    /// Observe one authenticated remote decoded frame and produce a speaking event.
    pub fn observe_remote_playback(
        &mut self,
        sender: &crate::SenderBinding,
        counter: u64,
        format: AudioCaptureFormat,
        pcm_i16: &[i16],
    ) -> Result<VoiceActivityLevel, MediaError> {
        sender.validate()?;
        validate_pcm_len(format, pcm_i16)?;
        let (rms_i16, peak_i16) = pcm_level_metrics(pcm_i16);
        let speaking = self.update_speaking(rms_i16);
        Ok(VoiceActivityLevel {
            source: VoiceActivitySource::RemotePlayback,
            speaker: Some(SpeakerPlaybackKey::from_sender(sender)?),
            counter: Some(counter),
            captured_at_ms: None,
            rms_i16,
            peak_i16,
            speaking,
        })
    }

    fn update_speaking(&mut self, rms_i16: u16) -> bool {
        if rms_i16 >= self.speaking_threshold_rms_i16 {
            self.hangover_remaining = self.hangover_frames;
            true
        } else if self.hangover_remaining > 0 {
            self.hangover_remaining -= 1;
            true
        } else {
            false
        }
    }
}

fn validate_pcm_len(format: AudioCaptureFormat, pcm_i16: &[i16]) -> Result<(), MediaError> {
    format.validate()?;
    if pcm_i16.len() == format.interleaved_samples_per_frame() {
        Ok(())
    } else {
        Err(MediaError::InvalidAudioFrame(format!(
            "expected {} interleaved PCM samples for audio-level event, got {}",
            format.interleaved_samples_per_frame(),
            pcm_i16.len()
        )))
    }
}

fn pcm_level_metrics(pcm_i16: &[i16]) -> (u16, u16) {
    if pcm_i16.is_empty() {
        return (0, 0);
    }
    let mut sum_squares = 0_u128;
    let mut peak = 0_u16;
    for sample in pcm_i16 {
        let abs = (*sample as i32).abs().min(i16::MAX as i32) as u16;
        peak = peak.max(abs);
        let abs_u128 = u128::from(abs);
        sum_squares += abs_u128 * abs_u128;
    }
    let mean_square = sum_squares as f64 / pcm_i16.len() as f64;
    let rms = mean_square.sqrt().round().min(i16::MAX as f64) as u16;
    (rms, peak)
}

/// Small deterministic jitter buffer ordered by authenticated SFrame counter per speaker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceJitterBuffer {
    target_depth_frames: usize,
    next_counter_by_speaker: BTreeMap<SpeakerPlaybackKey, u64>,
    buffered: BTreeMap<(SpeakerPlaybackKey, u64), DecodedAudioFrame>,
}

impl VoiceJitterBuffer {
    /// Create a jitter buffer. Depth 0 is permitted for tests/direct playback.
    #[must_use]
    pub fn new(target_depth_frames: usize) -> Self {
        Self {
            target_depth_frames,
            next_counter_by_speaker: BTreeMap::new(),
            buffered: BTreeMap::new(),
        }
    }

    /// Insert a decoded frame and return every contiguous frame ready for playback.
    pub fn push(&mut self, frame: DecodedAudioFrame) -> Result<Vec<DecodedAudioFrame>, MediaError> {
        let speaker_key = SpeakerPlaybackKey::from_sender(&frame.sender)?;
        self.next_counter_by_speaker
            .entry(speaker_key.clone())
            .and_modify(|next| *next = (*next).min(frame.counter))
            .or_insert(frame.counter);
        let counter = frame.counter;
        self.buffered.insert((speaker_key.clone(), counter), frame);
        Ok(self.pop_ready_for_speaker(&speaker_key, false))
    }

    /// Drain all currently contiguous frames, used when closing or after a test burst.
    pub fn drain_contiguous(&mut self) -> Vec<DecodedAudioFrame> {
        let speakers = self
            .next_counter_by_speaker
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        speakers
            .iter()
            .flat_map(|speaker| self.pop_ready_for_speaker(speaker, true))
            .collect()
    }

    fn pop_ready_for_speaker(
        &mut self,
        speaker_key: &SpeakerPlaybackKey,
        drain: bool,
    ) -> Vec<DecodedAudioFrame> {
        let mut ready = Vec::new();
        while drain || self.buffered_len_for_speaker(speaker_key) > self.target_depth_frames {
            let Some(counter) = self.next_counter_by_speaker.get(speaker_key).copied() else {
                break;
            };
            let Some(frame) = self.buffered.remove(&(speaker_key.clone(), counter)) else {
                break;
            };
            match counter.checked_add(1) {
                Some(next) => {
                    self.next_counter_by_speaker
                        .insert(speaker_key.clone(), next);
                }
                None => {
                    self.next_counter_by_speaker.remove(speaker_key);
                }
            }
            ready.push(frame);
        }
        if !self
            .buffered
            .keys()
            .any(|(speaker, _)| speaker == speaker_key)
            && ready.is_empty()
        {
            self.next_counter_by_speaker.remove(speaker_key);
        }
        ready
    }

    fn buffered_len_for_speaker(&self, speaker_key: &SpeakerPlaybackKey) -> usize {
        self.buffered
            .keys()
            .filter(|(speaker, _)| speaker == speaker_key)
            .count()
    }
}

/// Playback boundary that receives authenticated decoded audio only.
pub trait PlaybackAudioSink {
    /// Queue one decoded frame for playback/mixing.
    fn queue_playback_frame(&mut self, frame: DecodedAudioFrame) -> Result<(), MediaError>;
}

/// Protected media receive pipeline for one voice playback stream.
pub struct VoiceReceiveSFramePipeline<S> {
    bridge: RustTransformBridge,
    playback_format: AudioCaptureFormat,
    decoders_by_speaker: BTreeMap<SpeakerPlaybackKey, OpusAudioDecoder>,
    vad_by_speaker: BTreeMap<SpeakerPlaybackKey, VoiceActivityDetector>,
    last_voice_activity_by_speaker: BTreeMap<SpeakerPlaybackKey, VoiceActivityLevel>,
    jitter: VoiceJitterBuffer,
    volume_mixer: PlaybackVolumeMixer,
    sink: S,
}

impl<S: PlaybackAudioSink> VoiceReceiveSFramePipeline<S> {
    /// Construct the receive pipeline from Rust-owned transform, decoder, jitter, and sink state.
    #[must_use]
    pub fn new(
        bridge: RustTransformBridge,
        decoder: OpusAudioDecoder,
        jitter: VoiceJitterBuffer,
        sink: S,
    ) -> Self {
        Self::with_volume_mixer(bridge, decoder, jitter, PlaybackVolumeMixer::unity(), sink)
    }

    /// Construct the receive pipeline with an explicit per-speaker playback volume mixer.
    #[must_use]
    pub fn with_volume_mixer(
        bridge: RustTransformBridge,
        decoder: OpusAudioDecoder,
        jitter: VoiceJitterBuffer,
        volume_mixer: PlaybackVolumeMixer,
        sink: S,
    ) -> Self {
        Self {
            bridge,
            playback_format: decoder.format,
            decoders_by_speaker: BTreeMap::new(),
            vad_by_speaker: BTreeMap::new(),
            last_voice_activity_by_speaker: BTreeMap::new(),
            jitter,
            volume_mixer,
            sink,
        }
    }

    /// Set local playback volume for one authenticated speaker.
    pub fn set_speaker_volume(
        &mut self,
        sender: &crate::SenderBinding,
        volume_millipercent: u16,
    ) -> Result<(), MediaError> {
        self.volume_mixer
            .set_speaker_volume(sender, volume_millipercent)
    }

    /// Read the effective local playback volume for one authenticated speaker.
    pub fn speaker_volume(&self, sender: &crate::SenderBinding) -> Result<u16, MediaError> {
        self.volume_mixer.volume_for_sender(sender)
    }

    /// Return the most recent audio-level/VAD event for an authenticated speaker.
    pub fn last_voice_activity(
        &self,
        sender: &crate::SenderBinding,
    ) -> Result<Option<&VoiceActivityLevel>, MediaError> {
        let key = SpeakerPlaybackKey::from_sender(sender)?;
        Ok(self.last_voice_activity_by_speaker.get(&key))
    }

    /// Return every speaker currently considered active by the latest VAD event.
    #[must_use]
    pub fn speaking_speakers(&self) -> Vec<SpeakerPlaybackKey> {
        self.last_voice_activity_by_speaker
            .iter()
            .filter_map(|(speaker, level)| level.speaking.then_some(speaker.clone()))
            .collect()
    }

    /// Verify sender binding, reject replays, decrypt, decode, jitter, and queue playback.
    pub fn receive_protected_frame(
        &mut self,
        frame: BridgeProtectedFrame,
    ) -> Result<usize, MediaError> {
        let verified = self.bridge.open_protected_frame(frame)?;
        let speaker_key = SpeakerPlaybackKey::from_sender(&verified.sender)?;
        if !self.decoders_by_speaker.contains_key(&speaker_key) {
            self.decoders_by_speaker.insert(
                speaker_key.clone(),
                OpusAudioDecoder::new(self.playback_format)?,
            );
        }
        let decoder = self
            .decoders_by_speaker
            .get_mut(&speaker_key)
            .ok_or_else(|| MediaError::OpusDecodeFailed("missing per-speaker decoder".into()))?;
        let pcm_i16 = decoder.decode(&verified.clear.bytes)?;
        let vad = self.vad_by_speaker.entry(speaker_key.clone()).or_default();
        let voice_activity = vad.observe_remote_playback(
            &verified.sender,
            verified.counter,
            self.playback_format,
            &pcm_i16,
        )?;
        self.last_voice_activity_by_speaker
            .insert(speaker_key, voice_activity);
        let decoded = DecodedAudioFrame {
            sender: verified.sender,
            counter: verified.counter,
            format: self.playback_format,
            pcm_i16,
        };
        let ready = self.jitter.push(decoded)?;
        let queued = ready.len();
        for frame in ready {
            let mixed = self.volume_mixer.mix_frame(frame)?;
            self.sink.queue_playback_frame(mixed)?;
        }
        Ok(queued)
    }

    /// Flush contiguous jitter-buffered frames to playback.
    pub fn flush_playback(&mut self) -> Result<usize, MediaError> {
        let ready = self.jitter.drain_contiguous();
        let queued = ready.len();
        for frame in ready {
            let mixed = self.volume_mixer.mix_frame(frame)?;
            self.sink.queue_playback_frame(mixed)?;
        }
        Ok(queued)
    }

    /// Consume the pipeline and return the playback sink.
    #[must_use]
    pub fn into_sink(self) -> S {
        self.sink
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
    /// Local audio-level/VAD event computed from the captured PCM that was sent.
    pub audio_level: VoiceActivityLevel,
}

/// Result of applying media-path mute control to one captured frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VoiceCaptureSendOutcome {
    /// Frame was encoded, SFrame-protected, and handed to the transport sink.
    Sent(VoiceCaptureSendReport),
    /// Frame was intentionally suppressed before Opus encode/SFrame/transport.
    Muted {
        /// Capture timestamp that was suppressed.
        captured_at_ms: u64,
        /// Number of raw PCM samples discarded locally.
        dropped_pcm_samples: usize,
    },
}

/// End-to-end local send pipeline for one voice capture stream.
pub struct VoiceCaptureSFramePipeline<S> {
    encoder: OpusAudioEncoder,
    bridge: RustTransformBridge,
    sink: S,
    muted: bool,
    vad: VoiceActivityDetector,
}

impl<S: ProtectedMediaFrameSink> VoiceCaptureSFramePipeline<S> {
    /// Construct the send pipeline from Rust-owned encoder, transform, and sink state.
    #[must_use]
    pub fn new(encoder: OpusAudioEncoder, bridge: RustTransformBridge, sink: S) -> Self {
        Self {
            encoder,
            bridge,
            sink,
            muted: false,
            vad: VoiceActivityDetector::voice_defaults(),
        }
    }

    /// Set local media-path mute state. When muted, captured PCM is discarded before encode.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    /// Current local media-path mute state.
    #[must_use]
    pub const fn is_muted(&self) -> bool {
        self.muted
    }

    /// Apply mute control, or encode/protect/send exactly one captured audio frame.
    pub fn capture_encode_protect_or_mute(
        &mut self,
        frame: CapturedAudioFrame,
    ) -> Result<VoiceCaptureSendOutcome, MediaError> {
        if self.muted {
            return Ok(VoiceCaptureSendOutcome::Muted {
                captured_at_ms: frame.captured_at_ms,
                dropped_pcm_samples: frame.pcm_i16.len(),
            });
        }
        let audio_level = self.vad.observe_local_capture(&frame)?;
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
            audio_level,
        };
        self.sink.send_protected_media_frame(protected)?;
        Ok(VoiceCaptureSendOutcome::Sent(report))
    }

    /// Encode, protect, and send exactly one captured audio frame; fail closed if muted.
    pub fn capture_encode_protect_send(
        &mut self,
        frame: CapturedAudioFrame,
    ) -> Result<VoiceCaptureSendReport, MediaError> {
        match self.capture_encode_protect_or_mute(frame)? {
            VoiceCaptureSendOutcome::Sent(report) => Ok(report),
            VoiceCaptureSendOutcome::Muted { .. } => Err(MediaError::MediaMuted),
        }
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

    #[derive(Default)]
    struct PlaybackSink {
        played: Vec<DecodedAudioFrame>,
    }

    impl PlaybackAudioSink for PlaybackSink {
        fn queue_playback_frame(&mut self, frame: DecodedAudioFrame) -> Result<(), MediaError> {
            self.played.push(frame);
            Ok(())
        }
    }

    fn media_bridge() -> Result<RustTransformBridge, MediaError> {
        let binding =
            SenderBinding::derive_for_epoch(&[4; 32], "capture-group", 4, 42, "capture-device")?;
        let sender = SFrameSender::new(&[4; 32], binding.clone())?;
        let mut registry = MediaKeyRegistry::new();
        registry.register_sender(&[4; 32], binding)?;
        Ok(RustTransformBridge::new(
            sender,
            SFrameReceiver::new(registry, ReplayWindow::default()),
        ))
    }

    fn receive_bridge() -> Result<RustTransformBridge, MediaError> {
        let binding =
            SenderBinding::derive_for_epoch(&[4; 32], "capture-group", 4, 42, "capture-device")?;
        let sender =
            SFrameSender::new_for_epoch(&[99; 32], "unused-receive-sender", 99, 1, "unused")?;
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
    fn voice_activity_detector_uses_real_pcm_levels_and_hangover() -> Result<(), MediaError> {
        let format = AudioCaptureFormat::mono_20ms_48khz();
        let mut vad = VoiceActivityDetector::new(512, 1)?;
        let silence =
            CapturedAudioFrame::new(vec![0; format.interleaved_samples_per_frame()], format, 10)?;
        let speech = CapturedAudioFrame::new(sine_frame(format), format, 30)?;
        assert!(!vad.observe_local_capture(&silence)?.speaking);
        let speech_event = vad.observe_local_capture(&speech)?;
        assert!(speech_event.speaking);
        assert_eq!(speech_event.source, VoiceActivitySource::LocalCapture);
        assert!(speech_event.rms_i16 >= 512);
        assert!(vad.observe_local_capture(&silence)?.speaking);
        assert!(!vad.observe_local_capture(&silence)?.speaking);
        Ok(())
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
        assert_eq!(report.audio_level.source, VoiceActivitySource::LocalCapture);
        assert_eq!(report.audio_level.captured_at_ms, Some(1_234));
        assert!(report.audio_level.speaking);
        assert!(report.audio_level.rms_i16 > 0);
        let sink = pipeline.into_sink();
        assert_eq!(sink.sent.len(), 1);
        assert_ne!(sink.sent[0].bytes, original_opus);
        Ok(())
    }

    #[test]
    fn mute_control_suppresses_pcm_before_encode_or_transport() -> Result<(), MediaError> {
        let format = AudioCaptureFormat::mono_20ms_48khz();
        let mut pipeline = VoiceCaptureSFramePipeline::new(
            OpusAudioEncoder::new(format)?,
            media_bridge()?,
            RecordingSink::default(),
        );
        pipeline.set_muted(true);
        let muted = pipeline.capture_encode_protect_or_mute(CapturedAudioFrame::new(
            sine_frame(format),
            format,
            3_000,
        )?)?;
        assert_eq!(
            muted,
            VoiceCaptureSendOutcome::Muted {
                captured_at_ms: 3_000,
                dropped_pcm_samples: format.interleaved_samples_per_frame(),
            }
        );
        assert_eq!(
            pipeline.capture_encode_protect_send(CapturedAudioFrame::new(
                sine_frame(format),
                format,
                3_020,
            )?),
            Err(MediaError::MediaMuted)
        );
        assert!(pipeline.is_muted());
        pipeline.set_muted(false);
        let report = pipeline.capture_encode_protect_send(CapturedAudioFrame::new(
            sine_frame(format),
            format,
            3_040,
        )?)?;
        assert_eq!(report.sequence, 0);
        let sink = pipeline.into_sink();
        assert_eq!(sink.sent.len(), 1);
        Ok(())
    }

    #[test]
    fn playback_volume_mixer_applies_only_authenticated_speaker_gain() -> Result<(), MediaError> {
        let format = AudioCaptureFormat::mono_20ms_48khz();
        let alice = SenderBinding::derive_for_epoch(&[21; 32], "mix-group", 21, 1, "alice")?;
        let bob = SenderBinding::derive_for_epoch(&[22; 32], "mix-group", 22, 2, "bob")?;
        let mut mixer = PlaybackVolumeMixer::unity();
        mixer.set_speaker_volume(&alice, 500)?;
        mixer.set_speaker_volume(&bob, 1_500)?;

        let alice_mixed = mixer.mix_frame(DecodedAudioFrame {
            sender: alice.clone(),
            counter: 0,
            format,
            pcm_i16: vec![1_000, -1_000, 30_000, -30_000],
        })?;
        assert_eq!(alice_mixed.pcm_i16, vec![500, -500, 15_000, -15_000]);

        let bob_mixed = mixer.mix_frame(DecodedAudioFrame {
            sender: bob.clone(),
            counter: 0,
            format,
            pcm_i16: vec![1_000, -1_000, 30_000, -30_000],
        })?;
        assert_eq!(bob_mixed.pcm_i16, vec![1_500, -1_500, 32_767, -32_768]);
        assert_eq!(mixer.volume_for_sender(&alice)?, 500);
        assert_eq!(mixer.volume_for_sender(&bob)?, 1_500);
        assert_eq!(
            mixer.set_speaker_volume(&alice, SPEAKER_VOLUME_MAX_MILLIPERCENT + 1),
            Err(MediaError::InvalidPlaybackVolume(
                SPEAKER_VOLUME_MAX_MILLIPERCENT + 1
            ))
        );
        Ok(())
    }

    #[test]
    fn receive_pipeline_applies_per_speaker_volume_before_playback() -> Result<(), MediaError> {
        let format = AudioCaptureFormat::mono_20ms_48khz();
        let capture_binding =
            SenderBinding::derive_for_epoch(&[4; 32], "capture-group", 4, 42, "capture-device")?;
        let mut sender_bridge = media_bridge()?;
        let mut encoder = OpusAudioEncoder::new(format)?;
        let encoded =
            encoder.encode(CapturedAudioFrame::new(sine_frame(format), format, 2_500)?)?;
        let protected = sender_bridge.protect_encoded(encoded.into_bridge_clear_frame())?;

        let mut pipeline = VoiceReceiveSFramePipeline::new(
            receive_bridge()?,
            OpusAudioDecoder::new(format)?,
            VoiceJitterBuffer::new(0),
            PlaybackSink::default(),
        );
        pipeline.set_speaker_volume(&capture_binding, 0)?;
        assert_eq!(pipeline.speaker_volume(&capture_binding)?, 0);
        assert_eq!(pipeline.receive_protected_frame(protected)?, 1);
        let activity = pipeline
            .last_voice_activity(&capture_binding)?
            .ok_or_else(|| MediaError::PlaybackFailed("missing VAD event".into()))?;
        assert_eq!(activity.source, VoiceActivitySource::RemotePlayback);
        assert_eq!(
            activity.speaker,
            Some(SpeakerPlaybackKey::from_sender(&capture_binding)?)
        );
        assert_eq!(activity.counter, Some(0));
        assert!(activity.speaking);
        assert_eq!(
            pipeline.speaking_speakers(),
            vec![SpeakerPlaybackKey::from_sender(&capture_binding)?]
        );
        let sink = pipeline.into_sink();
        assert_eq!(sink.played.len(), 1);
        assert!(sink.played[0].pcm_i16.iter().all(|sample| *sample == 0));
        assert_eq!(sink.played[0].sender.device_id, "capture-device");
        Ok(())
    }

    #[test]
    fn receive_pipeline_keeps_same_counter_speakers_separate_for_mixing() -> Result<(), MediaError>
    {
        let format = AudioCaptureFormat::mono_20ms_48khz();
        let alice = SenderBinding::derive_for_epoch(&[31; 32], "room", 31, 1, "alice")?;
        let bob = SenderBinding::derive_for_epoch(&[32; 32], "room", 32, 2, "bob")?;
        let mut registry = MediaKeyRegistry::new();
        registry.register_sender(&[31; 32], alice.clone())?;
        registry.register_sender(&[32; 32], bob.clone())?;
        let receive_sender = SFrameSender::new_for_epoch(&[33; 32], "unused", 33, 9, "unused")?;
        let receive_bridge = RustTransformBridge::new(
            receive_sender,
            SFrameReceiver::new(registry, ReplayWindow::default()),
        );

        let mut alice_sender = SFrameSender::new(&[31; 32], alice.clone())?;
        let mut bob_sender = SFrameSender::new(&[32; 32], bob.clone())?;
        let mut alice_encoder = OpusAudioEncoder::new(format)?;
        let mut bob_encoder = OpusAudioEncoder::new(format)?;
        let alice_encoded =
            alice_encoder.encode(CapturedAudioFrame::new(sine_frame(format), format, 4_000)?)?;
        let bob_encoded =
            bob_encoder.encode(CapturedAudioFrame::new(sine_frame(format), format, 4_000)?)?;
        let alice_protected: BridgeProtectedFrame =
            alice_sender.protect(&alice_encoded.opus_payload)?.into();
        let bob_protected: BridgeProtectedFrame =
            bob_sender.protect(&bob_encoded.opus_payload)?.into();
        assert_eq!(alice_protected.counter, 0);
        assert_eq!(bob_protected.counter, 0);

        let mut pipeline = VoiceReceiveSFramePipeline::new(
            receive_bridge,
            OpusAudioDecoder::new(format)?,
            VoiceJitterBuffer::new(1),
            PlaybackSink::default(),
        );
        pipeline.set_speaker_volume(&alice, 0)?;
        pipeline.set_speaker_volume(&bob, 1_000)?;
        assert_eq!(pipeline.receive_protected_frame(alice_protected)?, 0);
        assert_eq!(pipeline.receive_protected_frame(bob_protected)?, 0);
        assert_eq!(pipeline.flush_playback()?, 2);
        let sink = pipeline.into_sink();
        assert_eq!(sink.played.len(), 2);
        assert_eq!(sink.played[0].sender.device_id, "alice");
        assert_eq!(sink.played[1].sender.device_id, "bob");
        assert!(sink.played[0].pcm_i16.iter().all(|sample| *sample == 0));
        assert!(sink.played[1].pcm_i16.iter().any(|sample| *sample != 0));
        Ok(())
    }

    #[test]
    fn receive_pipeline_verifies_decodes_jitters_and_rejects_replay() -> Result<(), MediaError> {
        let format = AudioCaptureFormat::mono_20ms_48khz();
        let mut sender_bridge = media_bridge()?;
        let mut encoder = OpusAudioEncoder::new(format)?;
        let encoded0 =
            encoder.encode(CapturedAudioFrame::new(sine_frame(format), format, 2_000)?)?;
        let encoded1 =
            encoder.encode(CapturedAudioFrame::new(sine_frame(format), format, 2_020)?)?;
        let protected0 = sender_bridge.protect_encoded(encoded0.into_bridge_clear_frame())?;
        let protected1 = sender_bridge.protect_encoded(encoded1.into_bridge_clear_frame())?;

        let mut tampered = protected0.clone();
        if let Some(first) = tampered.bytes.first_mut() {
            *first ^= 0x01;
        }

        let mut pipeline = VoiceReceiveSFramePipeline::new(
            receive_bridge()?,
            OpusAudioDecoder::new(format)?,
            VoiceJitterBuffer::new(1),
            PlaybackSink::default(),
        );
        assert_eq!(
            pipeline.receive_protected_frame(tampered),
            Err(MediaError::AuthenticationFailed)
        );
        assert_eq!(pipeline.receive_protected_frame(protected1.clone())?, 0);
        assert_eq!(pipeline.receive_protected_frame(protected0.clone())?, 1);
        assert_eq!(
            pipeline.receive_protected_frame(protected0),
            Err(MediaError::Replay)
        );
        assert_eq!(pipeline.flush_playback()?, 1);
        let sink = pipeline.into_sink();
        assert_eq!(sink.played.len(), 2);
        assert_eq!(sink.played[0].counter, 0);
        assert_eq!(sink.played[1].counter, 1);
        assert_eq!(sink.played[0].sender.group_id, "capture-group");
        assert_eq!(sink.played[0].sender.epoch, 4);
        assert_eq!(
            sink.played[0].pcm_i16.len(),
            format.interleaved_samples_per_frame()
        );
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

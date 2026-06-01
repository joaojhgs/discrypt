//! Media transport path selection and native Android WebRTC contingency configuration.

use serde::{Deserialize, Serialize};

/// Phase-1 media transport path decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MediaTransportPath {
    /// Desktop/Android webview path using WebRTC Encoded Transform hooks.
    WebviewEncodedTransform,
    /// Android contingency path backed by native `webrtc-rs` plumbing.
    NativeWebRtcRsContingency,
}

/// Selected capture backend for the voice media path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceCaptureBackend {
    /// Runtime WebView owns `getUserMedia`; Rust receives encoded frames through Encoded Transform.
    WebviewGetUserMedia,
    /// Native Android contingency owns capture through the Rust `webrtc` crate path.
    AndroidNativeWebRtcRs,
}

/// Selected playback backend for the voice media path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoicePlaybackBackend {
    /// Runtime WebView owns audio output selection and playback.
    WebviewAudioOutput,
    /// Native Android contingency owns playback through the Rust `webrtc` crate path.
    AndroidNativeWebRtcRs,
}

/// Selected Opus codec owner for encoded voice frames that cross the Rust media boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceCodecBackend {
    /// Rust Opus encode/decode uses the workspace-pinned `libopus-rs` crate.
    RustLibopusRs,
    /// WebRTC runtime supplies already-encoded Opus frames to the keyless transform bridge.
    WebRtcRuntimeOpus,
}

/// Auditable ADR-001 media path decision exported to UI/ops evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WebRtcMediaPathDecision {
    /// Runtime platform label used for the decision.
    pub platform: String,
    /// Transport/media engine selected for voice.
    pub path: MediaTransportPath,
    /// Whether the desktop/mobile WebView `RTCPeerConnection` Encoded Transform path is active.
    pub webview_rtc_peer_connection: bool,
    /// Whether the Rust `webrtc` crate is active as an Android media contingency.
    pub native_webrtc_rs_contingency: bool,
    /// Whether WebRTC Encoded Transform support must be present for this path.
    pub encoded_transform_required: bool,
    /// Capture backend selected by the ADR.
    pub capture_backend: VoiceCaptureBackend,
    /// Playback backend selected by the ADR.
    pub playback_backend: VoicePlaybackBackend,
    /// Opus codec owner for frames crossing the Rust media boundary.
    pub codec_backend: VoiceCodecBackend,
    /// Raw SFrame and MLS exporter keys stay in Rust-owned state.
    pub rust_sframe_key_owner: bool,
    /// JavaScript may only see encoded frame bytes, KIDs, and counters.
    pub js_raw_key_export_allowed: bool,
}

impl WebRtcMediaPathDecision {
    /// Build the ADR-001 media path decision from runtime capabilities.
    #[must_use]
    pub fn for_runtime(platform: impl Into<String>, encoded_transform_supported: bool) -> Self {
        let platform = platform.into();
        let android_native =
            platform.eq_ignore_ascii_case("android") && !encoded_transform_supported;
        if android_native {
            Self {
                platform,
                path: MediaTransportPath::NativeWebRtcRsContingency,
                webview_rtc_peer_connection: false,
                native_webrtc_rs_contingency: true,
                encoded_transform_required: false,
                capture_backend: VoiceCaptureBackend::AndroidNativeWebRtcRs,
                playback_backend: VoicePlaybackBackend::AndroidNativeWebRtcRs,
                codec_backend: VoiceCodecBackend::RustLibopusRs,
                rust_sframe_key_owner: true,
                js_raw_key_export_allowed: false,
            }
        } else {
            Self {
                platform,
                path: MediaTransportPath::WebviewEncodedTransform,
                webview_rtc_peer_connection: true,
                native_webrtc_rs_contingency: false,
                encoded_transform_required: true,
                capture_backend: VoiceCaptureBackend::WebviewGetUserMedia,
                playback_backend: VoicePlaybackBackend::WebviewAudioOutput,
                codec_backend: VoiceCodecBackend::WebRtcRuntimeOpus,
                rust_sframe_key_owner: true,
                js_raw_key_export_allowed: false,
            }
        }
    }

    /// True only when the selected path preserves the no-raw-key JavaScript boundary.
    #[must_use]
    pub const fn preserves_rust_sframe_boundary(&self) -> bool {
        self.rust_sframe_key_owner && !self.js_raw_key_export_allowed
    }
}

/// Browser/native microphone permission state reported before joining voice.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicrophonePermissionState {
    /// Permission has not been requested yet.
    Unknown,
    /// Runtime is expected to prompt the user.
    Prompt,
    /// User/runtime granted microphone capture access.
    Granted,
    /// User/runtime denied microphone capture access.
    Denied,
}

impl MicrophonePermissionState {
    /// Whether capture-dependent voice join may proceed.
    #[must_use]
    pub const fn allows_capture(self) -> bool {
        matches!(self, Self::Granted)
    }
}

/// Audio device kind exposed by browser/native enumeration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceDeviceKind {
    /// Microphone/input capture device.
    AudioInput,
    /// Speaker/output playback device.
    AudioOutput,
}

/// Redacted voice device descriptor safe for command/UI state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceDeviceDescriptor {
    /// Runtime device id or `default`.
    pub device_id: String,
    /// User-visible label or a redacted fallback label.
    pub label: String,
    /// Input/output classification.
    pub kind: VoiceDeviceKind,
}

impl VoiceDeviceDescriptor {
    /// Build a sanitized descriptor.
    #[must_use]
    pub fn new(
        device_id: impl Into<String>,
        label: impl Into<String>,
        kind: VoiceDeviceKind,
    ) -> Self {
        let device_id = normalize_device_field(device_id.into(), "default");
        let label = normalize_device_field(
            label.into(),
            match kind {
                VoiceDeviceKind::AudioInput => "Default microphone",
                VoiceDeviceKind::AudioOutput => "Default speaker",
            },
        );
        Self {
            device_id,
            label,
            kind,
        }
    }
}

/// Voice device/permission selection passed to the media join gate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceDeviceSelection {
    /// Microphone permission state observed by the shell before joining.
    pub microphone_permission: MicrophonePermissionState,
    /// Selected microphone/input device.
    pub input_device: Option<VoiceDeviceDescriptor>,
    /// Selected speaker/output device.
    pub output_device: Option<VoiceDeviceDescriptor>,
}

impl VoiceDeviceSelection {
    /// Create a selection from runtime-enumerated devices.
    #[must_use]
    pub const fn new(
        microphone_permission: MicrophonePermissionState,
        input_device: Option<VoiceDeviceDescriptor>,
        output_device: Option<VoiceDeviceDescriptor>,
    ) -> Self {
        Self {
            microphone_permission,
            input_device,
            output_device,
        }
    }

    /// Denied selection used to surface permission-denied UI state without joining.
    #[must_use]
    pub const fn denied() -> Self {
        Self {
            microphone_permission: MicrophonePermissionState::Denied,
            input_device: None,
            output_device: None,
        }
    }

    /// True only when the app has permission and a microphone descriptor.
    #[must_use]
    pub fn can_join_voice(&self) -> bool {
        self.microphone_permission.allows_capture()
            && self
                .input_device
                .as_ref()
                .is_some_and(|device| device.kind == VoiceDeviceKind::AudioInput)
    }

    /// Honest status copy for command/UI surfaces.
    #[must_use]
    pub fn status_copy(&self) -> String {
        if !self.microphone_permission.allows_capture() {
            return "Microphone permission denied; voice was not joined and no capture is running"
                .to_owned();
        }
        match (&self.input_device, &self.output_device) {
            (Some(input), Some(output)) => format!(
                "Microphone capture authorized using {} and playback routed to {}",
                input.label, output.label
            ),
            (Some(input), None) => format!(
                "Microphone capture authorized using {}; output device is the system default",
                input.label
            ),
            _ => "Microphone permission granted but no input device was selected; voice was not joined"
                .to_owned(),
        }
    }
}

fn normalize_device_field(value: String, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.chars().take(160).collect()
    }
}

/// Runtime capabilities used to select the voice media transport path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AndroidVoiceContingency {
    /// Runtime platform label (`android`, `linux`, `windows`, `macos`, ...).
    pub platform: String,
    /// Whether Encoded Transform hooks are available in the webview runtime.
    pub encoded_transform_supported: bool,
}

impl AndroidVoiceContingency {
    /// Return the complete ADR-001 media path decision for this runtime.
    #[must_use]
    pub fn media_path_decision(&self) -> WebRtcMediaPathDecision {
        WebRtcMediaPathDecision::for_runtime(&self.platform, self.encoded_transform_supported)
    }

    /// Select webview transforms unless Android lacks encoded-frame hooks.
    #[must_use]
    pub fn selected_path(&self) -> MediaTransportPath {
        if self.requires_native_contingency() {
            MediaTransportPath::NativeWebRtcRsContingency
        } else {
            MediaTransportPath::WebviewEncodedTransform
        }
    }

    /// True when Android must bypass webview media transforms and use native WebRTC plumbing.
    #[must_use]
    pub fn requires_native_contingency(&self) -> bool {
        self.platform.eq_ignore_ascii_case("android") && !self.encoded_transform_supported
    }

    /// Build a validated native Android media path plan when the runtime needs it.
    pub fn native_plan(
        &self,
        ice_servers: Vec<String>,
        voice_selection: VoiceDeviceSelection,
    ) -> Result<Option<NativeWebRtcRsContingency>, NativeWebRtcRsContingencyError> {
        if !self.requires_native_contingency() {
            return Ok(None);
        }
        NativeWebRtcRsContingency::android(ice_servers, voice_selection).map(Some)
    }
}

/// Error returned when the native Android media contingency cannot be configured safely.
#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum NativeWebRtcRsContingencyError {
    /// Native fallback is only valid for Android runtimes lacking encoded transforms.
    #[error("native WebRTC contingency is only valid for Android without encoded transforms")]
    UnsupportedRuntime,
    /// The user/runtime has not granted capture or has no microphone device.
    #[error("native WebRTC contingency requires granted microphone permission and input device")]
    CaptureNotAllowed,
    /// No usable STUN/TURN endpoint was supplied.
    #[error("native WebRTC contingency requires at least one STUN/TURN ICE endpoint")]
    MissingIceServer,
    /// ICE endpoint URL is unsupported.
    #[error("invalid native WebRTC ICE endpoint: {0}")]
    InvalidIceServer(String),
}

/// Validated Android native WebRTC media contingency path.
///
/// This is the Rust-owned fallback for Android WebViews that cannot expose encoded
/// transform hooks. It is intentionally explicit about capture/playback ownership and
/// SFrame: media frames still pass through the Rust Opus/SFrame path before network
/// transit; JavaScript never receives raw media keys.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NativeWebRtcRsContingency {
    /// Selected media path.
    pub path: MediaTransportPath,
    /// Sanitized STUN/TURN endpoint URLs fed into native ICE.
    pub ice_servers: Vec<String>,
    /// Runtime capture/output devices approved for this native path.
    pub voice_selection: VoiceDeviceSelection,
    /// Whether Rust SFrame protection is mandatory before any network transit.
    pub rust_sframe_required: bool,
    /// Native capture is required because webview encoded transforms are unavailable.
    pub native_capture_required: bool,
    /// Native playback is required because decoded frames cannot be handed to webview transforms.
    pub native_playback_required: bool,
    /// Honest status copy safe for UI/ops surfaces.
    pub status_copy: String,
}

impl NativeWebRtcRsContingency {
    /// Construct the production-shaped Android fallback plan.
    pub fn android(
        ice_servers: Vec<String>,
        voice_selection: VoiceDeviceSelection,
    ) -> Result<Self, NativeWebRtcRsContingencyError> {
        if !voice_selection.can_join_voice() {
            return Err(NativeWebRtcRsContingencyError::CaptureNotAllowed);
        }
        let ice_servers = sanitize_ice_servers(ice_servers)?;
        Ok(Self {
            path: MediaTransportPath::NativeWebRtcRsContingency,
            ice_servers,
            voice_selection,
            rust_sframe_required: true,
            native_capture_required: true,
            native_playback_required: true,
            status_copy: "Android native WebRTC media path selected because webview encoded transforms are unavailable; Rust Opus/SFrame remains mandatory".to_owned(),
        })
    }

    /// True when the fallback is ready to carry protected media frames.
    #[must_use]
    pub fn ready_for_protected_media(&self) -> bool {
        self.path == MediaTransportPath::NativeWebRtcRsContingency
            && self.rust_sframe_required
            && self.native_capture_required
            && self.native_playback_required
            && !self.ice_servers.is_empty()
            && self.voice_selection.can_join_voice()
    }
}

fn sanitize_ice_servers(
    ice_servers: Vec<String>,
) -> Result<Vec<String>, NativeWebRtcRsContingencyError> {
    let mut sanitized = ice_servers
        .into_iter()
        .map(|server| server.trim().to_owned())
        .filter(|server| !server.is_empty())
        .collect::<Vec<_>>();
    sanitized.sort();
    sanitized.dedup();
    if sanitized.is_empty() {
        return Err(NativeWebRtcRsContingencyError::MissingIceServer);
    }
    for server in &sanitized {
        if !(server.starts_with("stun:")
            || server.starts_with("stuns:")
            || server.starts_with("turn:")
            || server.starts_with("turns:"))
        {
            return Err(NativeWebRtcRsContingencyError::InvalidIceServer(
                server.clone(),
            ));
        }
    }
    Ok(sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_without_encoded_transform_selects_native_contingency(
    ) -> Result<(), NativeWebRtcRsContingencyError> {
        let decision = AndroidVoiceContingency {
            platform: "android".into(),
            encoded_transform_supported: false,
        };
        assert_eq!(
            decision.selected_path(),
            MediaTransportPath::NativeWebRtcRsContingency
        );
        let plan = decision
            .native_plan(
                vec![
                    " turn:relay.example:3478 ".into(),
                    "stun:stun.example:3478".into(),
                    "stun:stun.example:3478".into(),
                    "  ".into(),
                ],
                VoiceDeviceSelection::new(
                    MicrophonePermissionState::Granted,
                    Some(VoiceDeviceDescriptor::new(
                        "android-mic",
                        "Android microphone",
                        VoiceDeviceKind::AudioInput,
                    )),
                    Some(VoiceDeviceDescriptor::new(
                        "android-speaker",
                        "Android speaker",
                        VoiceDeviceKind::AudioOutput,
                    )),
                ),
            )?
            .ok_or(NativeWebRtcRsContingencyError::UnsupportedRuntime)?;
        assert!(plan.ready_for_protected_media());
        assert!(plan.rust_sframe_required);
        assert!(plan.native_capture_required);
        assert!(plan.native_playback_required);
        assert_eq!(
            plan.ice_servers,
            vec![
                "stun:stun.example:3478".to_owned(),
                "turn:relay.example:3478".to_owned()
            ]
        );
        Ok(())
    }

    #[test]
    fn android_native_contingency_rejects_unsafe_missing_inputs() {
        assert_eq!(
            NativeWebRtcRsContingency::android(
                vec!["stun:stun.example:3478".into()],
                VoiceDeviceSelection::denied(),
            ),
            Err(NativeWebRtcRsContingencyError::CaptureNotAllowed)
        );
        let selection = VoiceDeviceSelection::new(
            MicrophonePermissionState::Granted,
            Some(VoiceDeviceDescriptor::new(
                "android-mic",
                "Android microphone",
                VoiceDeviceKind::AudioInput,
            )),
            None,
        );
        assert_eq!(
            NativeWebRtcRsContingency::android(Vec::new(), selection.clone()),
            Err(NativeWebRtcRsContingencyError::MissingIceServer)
        );
        assert_eq!(
            NativeWebRtcRsContingency::android(vec!["https://not-ice".into()], selection),
            Err(NativeWebRtcRsContingencyError::InvalidIceServer(
                "https://not-ice".to_owned()
            ))
        );
    }

    #[test]
    fn desktop_uses_webview_transform_path() {
        let decision = AndroidVoiceContingency {
            platform: "linux".into(),
            encoded_transform_supported: false,
        };
        assert_eq!(
            decision.selected_path(),
            MediaTransportPath::WebviewEncodedTransform
        );
    }

    #[test]
    fn desktop_never_builds_android_native_contingency_even_without_encoded_transform(
    ) -> Result<(), NativeWebRtcRsContingencyError> {
        let decision = AndroidVoiceContingency {
            platform: "linux".into(),
            encoded_transform_supported: false,
        };
        let selection = VoiceDeviceSelection::new(
            MicrophonePermissionState::Granted,
            Some(VoiceDeviceDescriptor::new(
                "desktop-mic",
                "Desktop microphone",
                VoiceDeviceKind::AudioInput,
            )),
            Some(VoiceDeviceDescriptor::new(
                "desktop-speaker",
                "Desktop speaker",
                VoiceDeviceKind::AudioOutput,
            )),
        );

        assert_eq!(
            decision.native_plan(vec!["turn:relay.example:3478".into()], selection)?,
            None
        );
        let media_path = decision.media_path_decision();
        assert_eq!(media_path.path, MediaTransportPath::WebviewEncodedTransform);
        assert!(media_path.webview_rtc_peer_connection);
        assert!(!media_path.native_webrtc_rs_contingency);
        assert!(media_path.encoded_transform_required);
        assert!(media_path.preserves_rust_sframe_boundary());
        Ok(())
    }

    #[test]
    fn microphone_permission_and_device_selection_gate_voice_join() {
        let denied = VoiceDeviceSelection::denied();
        assert!(!denied.can_join_voice());
        assert!(denied.status_copy().contains("not joined"));

        let granted = VoiceDeviceSelection::new(
            MicrophonePermissionState::Granted,
            Some(VoiceDeviceDescriptor::new(
                "mic-1",
                "Studio microphone",
                VoiceDeviceKind::AudioInput,
            )),
            Some(VoiceDeviceDescriptor::new(
                "speaker-1",
                "Desk speakers",
                VoiceDeviceKind::AudioOutput,
            )),
        );
        assert!(granted.can_join_voice());
        assert!(granted.status_copy().contains("Studio microphone"));
        assert!(granted.status_copy().contains("Desk speakers"));
    }

    #[test]
    fn adr_001_desktop_decision_uses_webview_peer_connection_and_rust_sframe_boundary() {
        let decision = WebRtcMediaPathDecision::for_runtime("linux", true);
        assert_eq!(decision.path, MediaTransportPath::WebviewEncodedTransform);
        assert!(decision.webview_rtc_peer_connection);
        assert!(!decision.native_webrtc_rs_contingency);
        assert!(decision.encoded_transform_required);
        assert_eq!(
            decision.capture_backend,
            VoiceCaptureBackend::WebviewGetUserMedia
        );
        assert_eq!(
            decision.playback_backend,
            VoicePlaybackBackend::WebviewAudioOutput
        );
        assert_eq!(decision.codec_backend, VoiceCodecBackend::WebRtcRuntimeOpus);
        assert!(decision.preserves_rust_sframe_boundary());
    }

    #[test]
    fn adr_001_android_without_encoded_transform_uses_native_webrtc_rs_and_libopus() {
        let contingency = AndroidVoiceContingency {
            platform: "android".into(),
            encoded_transform_supported: false,
        };
        let decision = contingency.media_path_decision();
        assert_eq!(decision.path, MediaTransportPath::NativeWebRtcRsContingency);
        assert!(!decision.webview_rtc_peer_connection);
        assert!(decision.native_webrtc_rs_contingency);
        assert!(!decision.encoded_transform_required);
        assert_eq!(
            decision.capture_backend,
            VoiceCaptureBackend::AndroidNativeWebRtcRs
        );
        assert_eq!(
            decision.playback_backend,
            VoicePlaybackBackend::AndroidNativeWebRtcRs
        );
        assert_eq!(decision.codec_backend, VoiceCodecBackend::RustLibopusRs);
        assert!(decision.preserves_rust_sframe_boundary());
    }
}

//! Media transport path selection and native WebRTC contingency skeleton.

use serde::{Deserialize, Serialize};

/// Phase-1 media transport path decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MediaTransportPath {
    /// Desktop/Android webview path using WebRTC Encoded Transform hooks.
    WebviewEncodedTransform,
    /// Android contingency path backed by native `webrtc-rs` plumbing.
    NativeWebRtcRsContingency,
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

/// Android voice contingency selector documented by the Phase-1 gate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AndroidVoiceContingency {
    /// Runtime platform label (`android`, `linux`, `windows`, `macos`, ...).
    pub platform: String,
    /// Whether Encoded Transform hooks are available in the webview runtime.
    pub encoded_transform_supported: bool,
}

impl AndroidVoiceContingency {
    /// Select webview transforms unless Android lacks encoded-frame hooks.
    #[must_use]
    pub fn selected_path(&self) -> MediaTransportPath {
        if self.platform.eq_ignore_ascii_case("android") && !self.encoded_transform_supported {
            MediaTransportPath::NativeWebRtcRsContingency
        } else {
            MediaTransportPath::WebviewEncodedTransform
        }
    }
}

/// Minimal native WebRTC skeleton used by harnesses and the Android fallback track.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NativeWebRtcRsSkeleton {
    /// STUN/TURN endpoint URLs the native path would feed into ICE.
    pub ice_servers: Vec<String>,
    /// Whether this path must keep SFrame protection in Rust.
    pub rust_sframe_required: bool,
}

impl NativeWebRtcRsSkeleton {
    /// Construct a skeleton that preserves the no-raw-JS-key invariant.
    #[must_use]
    pub fn android_contingency(ice_servers: Vec<String>) -> Self {
        Self {
            ice_servers,
            rust_sframe_required: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_without_encoded_transform_selects_native_contingency() {
        let decision = AndroidVoiceContingency {
            platform: "android".into(),
            encoded_transform_supported: false,
        };
        assert_eq!(
            decision.selected_path(),
            MediaTransportPath::NativeWebRtcRsContingency
        );
        assert!(
            NativeWebRtcRsSkeleton::android_contingency(vec!["stun:example".into()])
                .rust_sframe_required
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
}

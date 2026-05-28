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
}

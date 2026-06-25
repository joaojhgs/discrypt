//! Media transport path selection, voice fanout, and native Android WebRTC contingency configuration.

use crate::{BridgeProtectedFrame, MediaError};
use discrypt_transport::{
    build_peer_overlay_forwarding_plan, PeerOverlayAckMode, PeerOverlayAdmittedSet,
    PeerOverlayAuth, PeerOverlayCarrier, PeerOverlayDelivery, PeerOverlayForwardingPlan,
    PeerOverlayForwardingPolicy, PeerOverlayFrame, PeerOverlayLoopId, PeerOverlayOpaquePayload,
    PeerOverlayPayloadKind, PeerOverlayPeerRef, PeerOverlayRelayAuthoritySet, PeerOverlayRoute,
    PeerOverlayRouteLegEvidence, PeerOverlayRouteSelection, PeerOverlaySelectedRoute,
    PeerOverlayTtl, TransportError,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

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

/// Route kind selected for one protected voice fanout delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceFanoutRouteKind {
    /// Direct WebRTC media/data route between admitted peers.
    DirectWebRtc,
    /// Configured TURN-backed WebRTC route carrying end-to-end SFrame ciphertext.
    ConfiguredTurnBackedWebRtc,
    /// Peer-assisted overlay route carrying opaque SFrame ciphertext through admitted relays.
    PeerAssistedOverlay,
}

/// One destination delivery for an already SFrame-protected voice frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceFanoutDelivery {
    /// Current-epoch admitted destination peer.
    pub destination: PeerOverlayPeerRef,
    /// Selected route kind for this destination.
    pub route_kind: VoiceFanoutRouteKind,
    /// Redacted live route label from backend/transport evidence.
    pub route_label: String,
    /// SFrame-protected frame handed to the direct/TURN/overlay carrier.
    pub protected_frame: BridgeProtectedFrame,
    /// Overlay forwarding proof for peer-assisted deliveries.
    pub overlay_forwarding: Option<PeerOverlayForwardingPlan>,
    /// Evidence flag: no provider application relay was selected.
    pub provider_application_relay_used: bool,
    /// Evidence flag: relay forwarding did not expose a decrypt/key path.
    pub decrypt_path_exposed: bool,
    /// Evidence flag: relays only receive SFrame ciphertext plus non-secret commitments/metadata.
    pub relay_observed_sframe_ciphertext_only: bool,
}

/// Inputs for building voice fanout over direct plus peer-assisted overlay routes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceOverlayFanoutInput {
    /// Current-epoch admitted source peer.
    pub source: PeerOverlayPeerRef,
    /// Already SFrame-protected encoded audio frame.
    pub protected_frame: BridgeProtectedFrame,
    /// Route selections for every intended destination.
    pub route_selections: Vec<PeerOverlayRouteSelection>,
    /// OpenMLS/backend auth binding for overlay media frames.
    pub auth: PeerOverlayAuth,
    /// Ack/redelivery contract for overlay media frames.
    pub delivery: PeerOverlayDelivery,
    /// TTL for overlay media frames.
    pub ttl: PeerOverlayTtl,
    /// Loop id for duplicate/loop suppression.
    pub loop_id: PeerOverlayLoopId,
    /// Optional forbidden relay-visible markers, usually known plaintext/key bytes in tests.
    #[serde(default)]
    pub forbidden_relay_visible_markers: Vec<Vec<u8>>,
}

/// Fanout plan for one protected voice frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceOverlayFanoutPlan {
    /// Current-epoch admitted source peer.
    pub source: PeerOverlayPeerRef,
    /// Per-destination deliveries.
    pub deliveries: Vec<VoiceFanoutDelivery>,
    /// Evidence flag: at least one direct or configured TURN delivery is present.
    pub direct_or_turn_delivery_present: bool,
    /// Evidence flag: at least one peer-assisted overlay relay delivery is present.
    pub peer_relay_delivery_present: bool,
    /// Evidence flag: no public signaling provider was selected as a media relay.
    pub provider_application_relay_used: bool,
    /// Honest limitation for release evidence and UI consumers.
    pub limitation: String,
}

/// Build a media fanout plan from already-selected transport routes.
///
/// The planner is intentionally keyless and content-blind after SFrame protection.
/// It never decrypts media for relays, never exposes raw SFrame keys, and never
/// converts provider signaling into an application/media relay fallback.
pub fn build_voice_overlay_fanout(
    admitted: &PeerOverlayAdmittedSet,
    relay_authority: &PeerOverlayRelayAuthoritySet,
    input: VoiceOverlayFanoutInput,
) -> Result<VoiceOverlayFanoutPlan, MediaError> {
    admitted
        .validate_ref(&input.source)
        .map_err(media_transport_error)?;
    input
        .auth
        .validate(admitted)
        .map_err(media_transport_error)?;
    input
        .delivery
        .validate(&input.ttl)
        .map_err(media_transport_error)?;
    if input.delivery.ack_mode != PeerOverlayAckMode::AckRequired {
        return Err(MediaError::MediaTransportFailed(
            "voice overlay fanout requires destination acknowledgements".to_owned(),
        ));
    }
    if input.route_selections.is_empty() {
        return Err(MediaError::MediaTransportFailed(
            "voice overlay fanout requires at least one route selection".to_owned(),
        ));
    }

    let forwarding_policy = PeerOverlayForwardingPolicy {
        carrier: PeerOverlayCarrier::PeerAssistedOverlay,
        forbidden_relay_visible_markers: input.forbidden_relay_visible_markers.clone(),
    };
    let mut seen_destinations = BTreeSet::new();
    let mut deliveries = Vec::with_capacity(input.route_selections.len());

    for selection in input.route_selections.clone() {
        let delivery = delivery_for_selection(
            admitted,
            relay_authority,
            &forwarding_policy,
            &input,
            selection,
        )?;
        if !seen_destinations.insert(delivery.destination.peer_id.clone()) {
            return Err(MediaError::MediaTransportFailed(
                "voice overlay fanout destination routes must be unique".to_owned(),
            ));
        }
        deliveries.push(delivery);
    }

    let direct_or_turn_delivery_present = deliveries.iter().any(|delivery| {
        matches!(
            delivery.route_kind,
            VoiceFanoutRouteKind::DirectWebRtc | VoiceFanoutRouteKind::ConfiguredTurnBackedWebRtc
        )
    });
    let peer_relay_delivery_present = deliveries
        .iter()
        .any(|delivery| delivery.route_kind == VoiceFanoutRouteKind::PeerAssistedOverlay);
    Ok(VoiceOverlayFanoutPlan {
        source: input.source,
        deliveries,
        direct_or_turn_delivery_present,
        peer_relay_delivery_present,
        provider_application_relay_used: false,
        limitation:
            "local 3-member media fanout harness/model evidence only; not split-machine production audio"
                .to_owned(),
    })
}

fn delivery_for_selection(
    admitted: &PeerOverlayAdmittedSet,
    relay_authority: &PeerOverlayRelayAuthoritySet,
    forwarding_policy: &PeerOverlayForwardingPolicy,
    input: &VoiceOverlayFanoutInput,
    selection: PeerOverlayRouteSelection,
) -> Result<VoiceFanoutDelivery, MediaError> {
    match selection.selected {
        PeerOverlaySelectedRoute::DirectWebRtc { evidence } => direct_delivery(
            admitted,
            &input.source,
            &input.protected_frame,
            evidence,
            VoiceFanoutRouteKind::DirectWebRtc,
            PeerOverlayCarrier::DirectWebRtcDataChannel,
        ),
        PeerOverlaySelectedRoute::ConfiguredTurnBackedWebRtc { evidence } => direct_delivery(
            admitted,
            &input.source,
            &input.protected_frame,
            evidence,
            VoiceFanoutRouteKind::ConfiguredTurnBackedWebRtc,
            PeerOverlayCarrier::ConfiguredTurnBackedWebRtc,
        ),
        PeerOverlaySelectedRoute::PeerAssistedOverlay {
            relay, evidence, ..
        } => {
            relay_authority
                .authorize_relay(&relay, &input.auth)
                .map_err(media_transport_error)?;
            validate_relay_route_evidence(admitted, &input.source, &evidence)
                .map_err(media_transport_error)?;
            if evidence.relay != relay {
                return Err(MediaError::MediaTransportFailed(
                    "voice overlay fanout relay selection and route evidence differ".to_owned(),
                ));
            }
            let destination = evidence.relay_to_destination.to.clone();
            let overlay_frame = voice_overlay_frame(input, relay, destination.clone())?;
            let forwarding = build_peer_overlay_forwarding_plan(
                admitted,
                relay_authority,
                forwarding_policy,
                &overlay_frame,
            )
            .map_err(media_transport_error)?;
            let relay_observed_sframe_ciphertext_only = forwarding.hops.iter().all(|hop| {
                !forwarding.decrypt_path_exposed
                    && hop.payload_kind == PeerOverlayPayloadKind::Media
                    && hop
                        .relay_visible_bytes
                        .windows(input.protected_frame.bytes.len())
                        .any(|window| window == input.protected_frame.bytes.as_slice())
            });
            Ok(VoiceFanoutDelivery {
                destination,
                route_kind: VoiceFanoutRouteKind::PeerAssistedOverlay,
                route_label: evidence.relay_to_destination.route_label.clone(),
                protected_frame: input.protected_frame.clone(),
                overlay_forwarding: Some(forwarding),
                provider_application_relay_used: false,
                decrypt_path_exposed: false,
                relay_observed_sframe_ciphertext_only,
            })
        }
    }
}

fn direct_delivery(
    admitted: &PeerOverlayAdmittedSet,
    source: &PeerOverlayPeerRef,
    protected_frame: &BridgeProtectedFrame,
    evidence: PeerOverlayRouteLegEvidence,
    route_kind: VoiceFanoutRouteKind,
    expected_carrier: PeerOverlayCarrier,
) -> Result<VoiceFanoutDelivery, MediaError> {
    validate_route_leg(
        admitted,
        &evidence,
        source,
        &evidence.to,
        expected_carrier,
        "voice overlay direct fanout route evidence",
    )
    .map_err(media_transport_error)?;
    Ok(VoiceFanoutDelivery {
        destination: evidence.to,
        route_kind,
        route_label: evidence.route_label,
        protected_frame: protected_frame.clone(),
        overlay_forwarding: None,
        provider_application_relay_used: false,
        decrypt_path_exposed: false,
        relay_observed_sframe_ciphertext_only: true,
    })
}

fn voice_overlay_frame(
    input: &VoiceOverlayFanoutInput,
    relay: PeerOverlayPeerRef,
    destination: PeerOverlayPeerRef,
) -> Result<PeerOverlayFrame, MediaError> {
    Ok(PeerOverlayFrame::new(
        PeerOverlayCarrier::PeerAssistedOverlay,
        PeerOverlayRoute {
            source: input.source.clone(),
            relay_path: vec![relay],
            destination,
            ttl: input.ttl.clone(),
            loop_id: input.loop_id,
        },
        input.auth.clone(),
        input.delivery.clone(),
        PeerOverlayOpaquePayload {
            kind: PeerOverlayPayloadKind::Media,
            key_id: input.protected_frame.kid.clone(),
            sequence: input.protected_frame.counter,
            aad_commitment: voice_frame_aad_commitment(&input.protected_frame),
            opaque_ciphertext: input.protected_frame.bytes.clone(),
        },
    ))
}

fn validate_relay_route_evidence(
    admitted: &PeerOverlayAdmittedSet,
    source: &PeerOverlayPeerRef,
    evidence: &discrypt_transport::PeerOverlayRelayRouteEvidence,
) -> Result<(), TransportError> {
    admitted.validate_ref(&evidence.relay)?;
    if evidence.relay.peer_id == source.peer_id
        || evidence.relay.peer_id == evidence.relay_to_destination.to.peer_id
    {
        return Err(TransportError::InvalidConnectivityPolicy(
            "voice overlay relay cannot be source or destination".to_owned(),
        ));
    }
    validate_relay_leg(
        admitted,
        &evidence.source_to_relay,
        source,
        &evidence.relay,
        "voice overlay source-to-relay route evidence",
    )?;
    validate_relay_leg(
        admitted,
        &evidence.relay_to_destination,
        &evidence.relay,
        &evidence.relay_to_destination.to,
        "voice overlay relay-to-destination route evidence",
    )
}

fn validate_route_leg(
    admitted: &PeerOverlayAdmittedSet,
    evidence: &PeerOverlayRouteLegEvidence,
    from: &PeerOverlayPeerRef,
    to: &PeerOverlayPeerRef,
    expected_carrier: PeerOverlayCarrier,
    label: &str,
) -> Result<(), TransportError> {
    admitted.validate_ref(&evidence.from)?;
    admitted.validate_ref(&evidence.to)?;
    evidence.carrier.validate()?;
    if evidence.route_label.trim().is_empty() || evidence.route_label.trim() != evidence.route_label
    {
        return Err(TransportError::InvalidConnectivityPolicy(format!(
            "{label} must have a non-empty trimmed route label"
        )));
    }
    if evidence.from != *from || evidence.to != *to {
        return Err(TransportError::InvalidConnectivityPolicy(format!(
            "{label} must bind the expected peer pair"
        )));
    }
    if evidence.carrier != expected_carrier {
        return Err(TransportError::InvalidConnectivityPolicy(format!(
            "{label} carrier does not match route selection"
        )));
    }
    if !evidence.live {
        return Err(TransportError::InvalidConnectivityPolicy(format!(
            "{label} must be backed by a live route"
        )));
    }
    Ok(())
}

fn validate_relay_leg(
    admitted: &PeerOverlayAdmittedSet,
    evidence: &PeerOverlayRouteLegEvidence,
    from: &PeerOverlayPeerRef,
    to: &PeerOverlayPeerRef,
    label: &str,
) -> Result<(), TransportError> {
    if matches!(
        evidence.carrier,
        PeerOverlayCarrier::PeerAssistedOverlay | PeerOverlayCarrier::ProviderApplicationRelay
    ) {
        return Err(TransportError::InvalidConnectivityPolicy(format!(
            "{label} must use a direct or configured TURN WebRTC leg"
        )));
    }
    validate_route_leg(admitted, evidence, from, to, evidence.carrier, label)
}

fn voice_frame_aad_commitment(frame: &BridgeProtectedFrame) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"discrypt-voice-overlay-fanout-v1");
    h.update((frame.kid.len() as u64).to_be_bytes());
    h.update(&frame.kid);
    h.update(frame.counter.to_be_bytes());
    h.update((frame.bytes.len() as u64).to_be_bytes());
    h.update(&frame.bytes);
    h.finalize().into()
}

fn media_transport_error(error: TransportError) -> MediaError {
    MediaError::MediaTransportFailed(error.to_string())
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

    fn peer(index: u8, epoch: u64) -> Result<PeerOverlayPeerRef, TransportError> {
        Ok(PeerOverlayPeerRef::new(
            discrypt_transport::SignalingPeerId::new(format!("voice-peer-{index}"))?,
            format!("member-{index}"),
            format!("device-{index}"),
            epoch,
        ))
    }

    fn admitted(epoch: u64) -> Result<PeerOverlayAdmittedSet, TransportError> {
        PeerOverlayAdmittedSet::new(
            epoch,
            [peer(1, epoch)?, peer(2, epoch)?, peer(3, epoch)?],
            [],
        )
    }

    fn auth(epoch: u64) -> PeerOverlayAuth {
        PeerOverlayAuth {
            group_id_commitment: [9; 32],
            epoch,
            sender_leaf_index: 1,
            confirmation_tag_commitment: [8; 32],
            frame_auth_tag: vec![7; 16],
        }
    }

    fn delivery() -> PeerOverlayDelivery {
        PeerOverlayDelivery {
            ack_id: discrypt_transport::PeerOverlayAckId([6; 16]),
            ack_mode: PeerOverlayAckMode::AckRequired,
            redelivery: discrypt_transport::PeerOverlayRedelivery {
                max_attempts: 2,
                max_relay_fanout: 1,
                deadline_ms: 1_500,
            },
        }
    }

    fn ttl() -> PeerOverlayTtl {
        PeerOverlayTtl {
            remaining_hops: 1,
            created_at_ms: 1_000,
            expires_at_ms: 2_000,
        }
    }

    fn leg(
        from: PeerOverlayPeerRef,
        to: PeerOverlayPeerRef,
        carrier: PeerOverlayCarrier,
        label: &str,
    ) -> PeerOverlayRouteLegEvidence {
        PeerOverlayRouteLegEvidence {
            from,
            to,
            carrier,
            route_label: label.to_owned(),
            live: true,
        }
    }

    fn selected_direct(epoch: u64) -> Result<PeerOverlayRouteSelection, TransportError> {
        Ok(PeerOverlayRouteSelection {
            attempts: vec![discrypt_transport::PeerOverlayRouteSelectionAttempt {
                carrier: PeerOverlayCarrier::DirectWebRtcDataChannel,
                selected: true,
            }],
            selected: PeerOverlaySelectedRoute::DirectWebRtc {
                evidence: leg(
                    peer(1, epoch)?,
                    peer(2, epoch)?,
                    PeerOverlayCarrier::DirectWebRtcDataChannel,
                    "alice-to-bob-direct-voice",
                ),
            },
            limitation: "unit route selection fixture".to_owned(),
        })
    }

    fn selected_relay(
        admitted: &PeerOverlayAdmittedSet,
        authority: &PeerOverlayRelayAuthoritySet,
        epoch: u64,
    ) -> Result<PeerOverlayRouteSelection, TransportError> {
        let relay = peer(2, epoch)?;
        Ok(PeerOverlayRouteSelection {
            attempts: vec![
                discrypt_transport::PeerOverlayRouteSelectionAttempt {
                    carrier: PeerOverlayCarrier::DirectWebRtcDataChannel,
                    selected: false,
                },
                discrypt_transport::PeerOverlayRouteSelectionAttempt {
                    carrier: PeerOverlayCarrier::PeerAssistedOverlay,
                    selected: true,
                },
            ],
            selected: PeerOverlaySelectedRoute::PeerAssistedOverlay {
                relay: relay.clone(),
                authorization: authority.authorize_relay(&relay, &auth(epoch))?,
                score: 42,
                evidence: Box::new(discrypt_transport::PeerOverlayRelayRouteEvidence {
                    relay,
                    source_to_relay: leg(
                        peer(1, epoch)?,
                        peer(2, epoch)?,
                        PeerOverlayCarrier::DirectWebRtcDataChannel,
                        "alice-to-bob-direct-voice",
                    ),
                    relay_to_destination: leg(
                        peer(2, epoch)?,
                        peer(3, epoch)?,
                        PeerOverlayCarrier::ConfiguredTurnBackedWebRtc,
                        "bob-to-carol-turn-voice",
                    ),
                }),
            },
            limitation: format!(
                "unit route selection fixture with {} admitted peers",
                [peer(1, epoch)?, peer(2, epoch)?, peer(3, epoch)?]
                    .iter()
                    .filter(|candidate| admitted.validate_ref(candidate).is_ok())
                    .count()
            ),
        })
    }

    fn protected_voice_frame() -> Result<
        (
            BridgeProtectedFrame,
            crate::SFrameReceiver,
            crate::SFrameReceiver,
            Vec<u8>,
        ),
        MediaError,
    > {
        let epoch_secret = [55; 32];
        let plaintext = b"opus-20ms-audio-frame-per74".to_vec();
        let binding = crate::SenderBinding::derive_for_epoch(
            &epoch_secret,
            "per74-voice-group",
            44,
            1,
            "alice-device",
        )?;
        let mut sender = crate::SFrameSender::new(&epoch_secret, binding.clone())?;

        let mut bob_registry = crate::MediaKeyRegistry::new();
        bob_registry.register_sender(&epoch_secret, binding.clone())?;
        let bob = crate::SFrameReceiver::new(bob_registry, crate::ReplayWindow::default());

        let mut carol_registry = crate::MediaKeyRegistry::new();
        carol_registry.register_sender(&epoch_secret, binding)?;
        let carol = crate::SFrameReceiver::new(carol_registry, crate::ReplayWindow::default());

        let frame: BridgeProtectedFrame = sender.protect(&plaintext)?.into();
        Ok((frame, bob, carol, plaintext))
    }

    #[test]
    fn voice_overlay_fanout_three_member_direct_plus_relay_ciphertext_only(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let epoch = 44;
        let admitted = admitted(epoch)?;
        let authority = PeerOverlayRelayAuthoritySet::from_openmls_current_epoch(
            &admitted,
            [9; 32],
            [8; 32],
            [peer(2, epoch)?],
        )?;
        let (protected_frame, mut bob_receiver, mut carol_receiver, plaintext) =
            protected_voice_frame()?;

        let plan = build_voice_overlay_fanout(
            &admitted,
            &authority,
            VoiceOverlayFanoutInput {
                source: peer(1, epoch)?,
                protected_frame: protected_frame.clone(),
                route_selections: vec![
                    selected_direct(epoch)?,
                    selected_relay(&admitted, &authority, epoch)?,
                ],
                auth: auth(epoch),
                delivery: delivery(),
                ttl: ttl(),
                loop_id: PeerOverlayLoopId([5; 16]),
                forbidden_relay_visible_markers: vec![plaintext.clone(), b"raw-media-key".to_vec()],
            },
        )?;

        assert!(plan.direct_or_turn_delivery_present);
        assert!(plan.peer_relay_delivery_present);
        assert!(!plan.provider_application_relay_used);
        assert_eq!(plan.deliveries.len(), 2);

        let bob_delivery = plan
            .deliveries
            .iter()
            .find(|delivery| delivery.destination == peer(2, epoch).unwrap())
            .ok_or("missing direct bob delivery")?;
        assert_eq!(bob_delivery.route_kind, VoiceFanoutRouteKind::DirectWebRtc);
        assert_eq!(
            bob_receiver
                .open(&bob_delivery.protected_frame.clone().into())?
                .plaintext,
            plaintext
        );

        let carol_delivery = plan
            .deliveries
            .iter()
            .find(|delivery| delivery.destination == peer(3, epoch).unwrap())
            .ok_or("missing relay carol delivery")?;
        assert_eq!(
            carol_delivery.route_kind,
            VoiceFanoutRouteKind::PeerAssistedOverlay
        );
        assert!(carol_delivery.relay_observed_sframe_ciphertext_only);
        let forwarding = carol_delivery
            .overlay_forwarding
            .as_ref()
            .ok_or("missing overlay forwarding proof")?;
        assert_eq!(forwarding.hops.len(), 2);
        assert!(!forwarding.decrypt_path_exposed);
        assert!(!forwarding.provider_application_relay_used);
        for hop in &forwarding.hops {
            assert!(hop
                .relay_visible_bytes
                .windows(protected_frame.bytes.len())
                .any(|window| window == protected_frame.bytes.as_slice()));
            assert!(!hop
                .relay_visible_bytes
                .windows(plaintext.len())
                .any(|window| window == plaintext.as_slice()));
        }
        assert_eq!(
            carol_receiver
                .open(&carol_delivery.protected_frame.clone().into())?
                .plaintext,
            plaintext
        );
        Ok(())
    }

    #[test]
    fn voice_overlay_fanout_rejects_provider_relay_route() -> Result<(), Box<dyn std::error::Error>>
    {
        let epoch = 44;
        let admitted = admitted(epoch)?;
        let authority = PeerOverlayRelayAuthoritySet::from_openmls_current_epoch(
            &admitted,
            [9; 32],
            [8; 32],
            [peer(2, epoch)?],
        )?;
        let (protected_frame, _, _, _) = protected_voice_frame()?;
        let mut route = selected_direct(epoch)?;
        if let PeerOverlaySelectedRoute::DirectWebRtc { evidence } = &mut route.selected {
            evidence.carrier = PeerOverlayCarrier::ProviderApplicationRelay;
        }

        let error = build_voice_overlay_fanout(
            &admitted,
            &authority,
            VoiceOverlayFanoutInput {
                source: peer(1, epoch)?,
                protected_frame,
                route_selections: vec![route],
                auth: auth(epoch),
                delivery: delivery(),
                ttl: ttl(),
                loop_id: PeerOverlayLoopId([5; 16]),
                forbidden_relay_visible_markers: Vec::new(),
            },
        )
        .expect_err("provider relay route must fail closed");
        assert!(error
            .to_string()
            .contains("peer overlay frames must not use providers"));
        Ok(())
    }

    #[test]
    fn voice_overlay_fanout_rejects_relay_visible_plaintext_marker(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let epoch = 44;
        let admitted = admitted(epoch)?;
        let authority = PeerOverlayRelayAuthoritySet::from_openmls_current_epoch(
            &admitted,
            [9; 32],
            [8; 32],
            [peer(2, epoch)?],
        )?;
        let (mut protected_frame, _, _, _) = protected_voice_frame()?;
        protected_frame.bytes = b"leaked plaintext audio".to_vec();

        let error = build_voice_overlay_fanout(
            &admitted,
            &authority,
            VoiceOverlayFanoutInput {
                source: peer(1, epoch)?,
                protected_frame,
                route_selections: vec![selected_relay(&admitted, &authority, epoch)?],
                auth: auth(epoch),
                delivery: delivery(),
                ttl: ttl(),
                loop_id: PeerOverlayLoopId([5; 16]),
                forbidden_relay_visible_markers: vec![b"plaintext audio".to_vec()],
            },
        )
        .expect_err("relay-visible plaintext marker must fail closed");
        assert_eq!(
            error,
            MediaError::MediaTransportFailed(TransportError::PlaintextLeak.to_string())
        );
        Ok(())
    }
}

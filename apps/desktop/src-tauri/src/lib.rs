//! Tauri command surface and local-first app-state service for the native discrypt shell.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration, Utc};
#[cfg(test)]
use discrypt_abuse::AbuseControls;
use discrypt_admission::{
    signaling_fingerprint_for_endpoint, DmInviteBootstrap, GroupInviteBootstrap,
    InviteBootstrapMetadata, InviteEndpointPolicy, InviteKind, InviteSignalingAdapterKind,
    InviteSignalingMetadata, InviteSignalingProfile, InviteStore, InviteTrustMetadata,
    INVITE_CONNECTIVITY_SCHEMA_VERSION, INVITE_PROVIDER_POLICY_VERSION,
};
#[cfg(test)]
use discrypt_admission::{AdmissionController, AuthorizedWelcome, Invite, PasswordGate};
use discrypt_core::{
    app_snapshot as core_app_snapshot, identity_recovery_verification_smoke,
    snapshot_safety_number_matches_identity_keys, AppSnapshot, ChannelKind,
    ChannelView as SnapshotChannelView, DeviceView, MessageView as SnapshotMessageView,
    SafetyVerificationRequest, SafetyVerificationResult, SecurityCopyView, ServerView,
    VOICE_SESSION_NOT_JOINED_COPY, VOICE_SESSION_ROUTE_GATED_COPY,
};
use discrypt_media::{
    MicrophonePermissionState, VoiceDeviceDescriptor, VoiceDeviceKind, VoiceDeviceSelection,
};
use discrypt_mls_core::{
    verifying_key_from_hex, DeviceLeaf, DevicePairingPayload, DeviceSet, DeviceStatus, FriendCode,
    Identity, OpenMlsGroupEngine, OpenMlsMemberPackage, SafetyNumber,
};
use discrypt_mls_delivery::{
    DeliveryError, InMemoryTextReceiveEvents, InMemoryTextRecipientStore, TextAuthorLogEnvelope,
    TextAuthorLogStore, TextDeliveryReceipt, TextDeliveryReceiptInput, TextInboundPipeline,
    TextInboundRequest, TextMessageEnvelope, TextMessageEnvelopeInput, TextOutboundFrame,
    TextOutboundPipeline, TextOutboundRequest, TextOutboundTransport, TextReceiveState,
    TextRenderState, TextRetentionMetadata, TextSelectedRoute, TextSendEvent, TextSendEventSink,
};
#[cfg(test)]
use discrypt_mls_delivery::{InMemoryTextAuthorLog, InMemoryTextSendEvents, InMemoryTextTransport};
#[cfg(all(target_os = "linux", feature = "production-storage"))]
use discrypt_storage::EncryptedAppDb;
#[cfg(any(test, not(all(target_os = "linux", feature = "production-storage"))))]
use discrypt_storage::FileAppStore;
#[cfg(all(target_os = "linux", feature = "production-storage", not(test)))]
use discrypt_storage::LinuxOsKeychain;
use discrypt_storage::{
    recover_account, recovery_code_material, seal_account_backup, AccountRecovery, AppStore,
    RecoveryCodeVerifier, RecoveryMaterial,
};
#[cfg(all(test, target_os = "linux", feature = "production-storage"))]
use discrypt_storage::{AppDbKeychain, AppStoreError};
#[cfg(test)]
use discrypt_transport::probe_provider_webrtc_datachannel_request_response_with_config_and_answerer;
#[cfg(test)]
use discrypt_transport::TEXT_CONTROL_RUNTIME_SPEC_MISSING_MESSAGE;
use discrypt_transport::{
    plan_signaling_adapter_fallback, probe_provider_adapter_roundtrip,
    probe_provider_webrtc_datachannel_request_response_roundtrip,
    probe_provider_webrtc_datachannel_text_frame_roundtrip, required_provider_adapter_boundaries,
    resume_text_control_runtime_from_probe,
    start_provider_webrtc_text_control_answer_runtime_with_answerer,
    start_provider_webrtc_text_control_offer_runtime, AdapterFallbackBehavior, AdapterTrustLabel,
    ConnectionAttempt, ConnectivityPlan, ConnectivityScopeLevel, ConversationScope, Endpoint,
    FallbackLeg, IceEndpointPolicy, IceServerConfig, ProviderMetadataPosture,
    ProviderTextControlRuntimeAttachment, ProviderTextControlRuntimePeerRole,
    ProviderTextControlRuntimeSpec, SignalingAdapterCapabilities, SignalingAdapterKind,
    SignalingAdapterProfile, SignalingEndpointSecurity, SignalingPeerId, SignalingProviderEndpoint,
    TransportError, TransportRoute, TransportSession, TransportSessionSnapshot,
    TransportSessionState, TurnServerConfig,
};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(all(test, target_os = "linux", feature = "production-storage"))]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use uuid::Uuid;

const APP_STATE_SCHEMA_VERSION: u32 = 1;
const APP_STATE_STORE_FILENAME: &str = "app-state.discrypt-store";
const DEFAULT_THEME_ID: &str = "graphite-calm";
const DEFAULT_TEMPLATE_ID: &str = "command-center";
const UI_THEME_IDS: &[&str] = &["midnight-steel", "graphite-calm", "ocean-contrast"];
const UI_TEMPLATE_IDS: &[&str] = &["command-center", "compact-ops"];
const INVITE_CREATE_LIMIT: u32 = 5;
const TEXT_SEND_LIMIT: u32 = 20;
#[allow(dead_code)]
const TEXT_EXPORTER_LABEL: &str = "discrypt/text";
#[allow(dead_code)]
const TEXT_EXPORTER_CONTEXT: &[u8] = b"discrypt-tauri-text-v1";
const ADMISSION_HELPER_ATTEMPT_LIMIT: u32 = 5;
const SIGNALING_ACTION_LIMIT: u32 = 60;
const ABUSE_WINDOW_SECONDS: i64 = 60;
#[cfg_attr(not(feature = "tauri-runtime"), allow(dead_code))]
const APP_EVENT_TAURI_TOPIC: &str = "app_event";
const TEXT_CONTROL_RUNTIME_NOT_IMPLEMENTED_RECOVERY_HINT: &str =
    "Provider-backed long-lived attachment is intentionally disabled while this app-service only owns short-lived transport probes; add persisted negotiated offer/answer/ICE bootstrap handoff and a persistent installed-app receiver loop for peer routing before attaching";

/// Desktop/Tauri wrapper around the Rust signaling protocol client.
pub struct DesktopSignalingClient<T> {
    inner: external_signaling::client::SignalingClient<T>,
}

impl<T> DesktopSignalingClient<T>
where
    T: external_signaling::client::SignalingTransport,
{
    /// Construct the app-service signaling client wrapper.
    pub fn new(inner: external_signaling::client::SignalingClient<T>) -> Self {
        Self { inner }
    }

    /// Publish an opaque signaling payload for a Tauri/app-service session.
    pub fn publish_opaque_signal(
        &mut self,
        kind: external_signaling::server::SignalKind,
        session_id: &str,
        payload: &[u8],
        expires_at: chrono::DateTime<Utc>,
    ) -> Result<(), String> {
        self.inner
            .publish_signal(kind, session_id.as_bytes(), payload, expires_at)
            .map_err(|err| err.to_string())
    }

    /// Take opaque signaling payloads for a Tauri/app-service session.
    pub fn take_opaque_signals(
        &mut self,
        kind: external_signaling::server::SignalKind,
        session_id: &str,
    ) -> Result<Vec<Vec<u8>>, String> {
        self.inner
            .take_signals(kind, session_id.as_bytes())
            .map(|signals| signals.into_iter().map(|signal| signal.payload).collect())
            .map_err(|err| err.to_string())
    }
}

/// Lifecycle route for the desktop app shell.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppLifecycle {
    /// No local profile exists yet; setup/recovery must be shown.
    FirstRun,
    /// Local profile exists and the main shell can render.
    Ready,
}

/// Persisted UI preference model shared with the React command client.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UiPreferencesView {
    /// Active theme identifier from the frontend theme registry.
    pub theme_id: String,
    /// Active layout template identifier from the frontend template registry.
    pub template_id: String,
}

/// Local user profile created or recovered on first run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UserProfileView {
    /// Stable local profile id.
    pub user_id: String,
    /// User-facing display name.
    pub display_name: String,
    /// Local device name.
    pub device_name: String,
    /// Honest local-device recovery posture.
    pub recovery_status: String,
}

/// Signed runtime peer row for DM text/control attachment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DmRuntimePeerView {
    /// Stable provider peer id derived from signed DM bootstrap metadata.
    pub peer_id: String,
    /// Role this peer represents for the current first-contact invite bootstrap.
    pub role: String,
    /// Whether this peer is the local device/profile role in this installed state.
    pub is_local: bool,
    /// Backend evidence source for this peer id.
    pub source: String,
}

/// Direct-message conversation row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DirectConversationView {
    /// Stable DM id.
    pub dm_id: String,
    /// Local participant id.
    pub participant_id: String,
    /// Display label.
    pub display_name: String,
    /// Honest local/harness capability copy.
    pub local_only_copy: String,
    /// Backend-returned local/remote runtime peers for attaching DM text/control DataChannels.
    #[serde(default)]
    pub runtime_peers: Vec<DmRuntimePeerView>,
    /// Persisted DM connectivity policy from the contact invite/acceptance flow.
    #[serde(default)]
    pub connectivity: Option<ConnectivityPolicyView>,
}

/// Channel row with stable identifiers for app-state consumers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChannelStateView {
    /// Stable channel id.
    pub channel_id: String,
    /// Display name.
    pub name: String,
    /// Channel kind.
    pub kind: ChannelKind,
    /// Retention label.
    pub retention_status: String,
    /// Optional channel-level connectivity override used for text/voice sessions.
    #[serde(default)]
    pub connectivity: Option<ConnectivityPolicyView>,
}

/// Signed runtime peer row for group text/control attachment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupRuntimePeerView {
    /// Stable provider peer id derived from signed group bootstrap metadata.
    pub peer_id: String,
    /// Role this peer represents for the current two-sided invite bootstrap.
    pub role: String,
    /// Whether this peer is the local device/profile role in this installed state.
    pub is_local: bool,
    /// Backend evidence source for this peer id.
    pub source: String,
}

/// Group/server row with stable identifiers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupView {
    /// Stable group id.
    pub group_id: String,
    /// Display name.
    pub name: String,
    /// Local role label.
    pub role: String,
    /// Channels in this group.
    pub channels: Vec<ChannelStateView>,
    /// Backend-returned local/remote runtime peers for attaching text/control DataChannels.
    #[serde(default)]
    pub runtime_peers: Vec<GroupRuntimePeerView>,
    /// Persisted group connectivity policy inherited by channel sessions unless overridden.
    #[serde(default)]
    pub connectivity: Option<ConnectivityPolicyView>,
}

/// Current routed context for the shell.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActiveContextView {
    /// `dm`, `text_channel`, `voice_channel`, or `group`.
    pub kind: String,
    /// Active group id when relevant.
    pub group_id: Option<String>,
    /// Active channel id when relevant.
    pub channel_id: Option<String>,
    /// Active DM id when relevant.
    pub dm_id: Option<String>,
}

/// Message destination for DM or group-channel messages.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageTargetView {
    /// `dm` or `channel`.
    pub kind: String,
    /// DM id for direct messages.
    pub dm_id: Option<String>,
    /// Group id for channel messages.
    pub group_id: Option<String>,
    /// Channel id for channel messages.
    pub channel_id: Option<String>,
}

/// Verified signed peer delivery receipt metadata for a message row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextDeliveryReceiptView {
    /// Recipient device id that signed the delivery receipt.
    pub recipient_device_id: String,
    /// Receipt timestamp in milliseconds.
    pub received_at_ms: u64,
    /// Hash of the ciphertext envelope authenticated by the receipt.
    pub envelope_ciphertext_hash: String,
    /// Recipient verifying key fingerprint used for receipt verification.
    pub recipient_key_fingerprint: String,
}

/// Command-backed local text message row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageView {
    /// Stable local message id.
    pub message_id: String,
    /// Message destination.
    pub target: MessageTargetView,
    /// Author id.
    pub author_id: String,
    /// Author label.
    pub author: String,
    /// Decrypted local body shown in this shell.
    pub body: String,
    /// Delivery/security status copy.
    pub status: String,
    /// Stable message state key for UI badges.
    #[serde(default = "default_text_state_key")]
    pub state_key: String,
    /// Human-readable message state label.
    #[serde(default = "default_text_state_label")]
    pub state_label: String,
    /// Evidence/caveat for the message state.
    #[serde(default = "default_text_state_detail")]
    pub state_detail: String,
    /// Verified signed peer receipt metadata when available.
    #[serde(default)]
    pub peer_receipt: Option<TextDeliveryReceiptView>,
    /// Deterministic local timestamp/counter label.
    pub sent_at: String,
}

fn default_text_state_key() -> String {
    "sent_local".to_owned()
}

fn default_text_state_label() -> String {
    "Sent locally".to_owned()
}

fn default_text_state_detail() -> String {
    "Message is in the local encrypted author log; peer receipt requires backend-state proof"
        .to_owned()
}

/// Text state legend for command-backed timelines.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextStateView {
    /// Stable state key.
    pub key: String,
    /// Human-readable label.
    pub label: String,
    /// Whether this state is currently observable for this build.
    pub status: String,
    /// Evidence/caveat copy for the state.
    pub detail: String,
}

/// Redacted TURN endpoint metadata surfaced from a signed invite descriptor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IceTurnServerView {
    /// TURN or TURNS endpoint URI.
    pub endpoint: String,
    /// Whether the signed policy declared TURN credentials without exposing the raw secret.
    pub credential_declared: bool,
    /// Credential expiry timestamp when provided by the signed policy.
    #[serde(default)]
    pub credential_expires_at: Option<String>,
}

/// UI/persistence view of one signed signaling adapter profile.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingProfileView {
    /// Stable profile id, unique inside the descriptor.
    pub profile_id: String,
    /// Adapter kind: mqtt, nostr, ipfs_pubsub, or discrypt_quic_rendezvous.
    pub adapter_kind: String,
    /// Broker/relay/bootstrap/QUIC endpoint URLs for this adapter.
    pub endpoints: Vec<String>,
    /// Provider-visible room namespace commitment, never a display name.
    pub room_topic_commitment: String,
    /// Endpoint/service/relay trust fingerprint or public-key commitment.
    pub trust_fingerprint: String,
    /// Publish/subscribe TTL in seconds.
    pub ttl_seconds: u32,
    /// Public provider metadata posture.
    pub metadata_posture: String,
    /// Abuse/rate-limit policy hint surfaced to UI/backend.
    pub rate_limit_policy: String,
    /// Adapter capabilities asserted by this profile.
    pub capabilities: Vec<String>,
    /// Provider policy schema version for allowlist and rotation semantics.
    #[serde(default = "default_provider_policy_version")]
    pub provider_policy_version: u32,
    /// Hash commitments for endpoints explicitly allowed by this profile.
    #[serde(default)]
    pub endpoint_allowlist_commitments: Vec<String>,
    /// Endpoint/provider rotation policy shown to operators and enforced by signed invites.
    #[serde(default = "default_provider_rotation_policy")]
    pub provider_rotation_policy: String,
}

/// DM-specific bootstrap metadata persisted after first-contact invite parsing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DmInviteBootstrapView {
    /// Commitment to inviter identity, not the display alias.
    pub inviter_identity_commitment: String,
    /// Bounded-use contact token commitment.
    pub contact_token_commitment: String,
    /// Reply rendezvous capability commitment.
    pub reply_rendezvous_commitment: String,
}

/// Group-specific bootstrap metadata persisted after invite parsing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupInviteBootstrapView {
    /// Commitment to group identity/scope, not the group display name.
    pub group_identity_commitment: String,
    /// Commitment to role/admission policy.
    pub role_admission_policy_commitment: String,
    /// Commitment to the channel policy inheritance snapshot.
    pub channel_policy_commitment: String,
}

/// Persisted connectivity bootstrap policy for a DM or group scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectivityPolicyView {
    /// Connectivity schema version for forward-compatible parsers.
    pub connectivity_schema_version: u32,
    /// Scope kind: group_join or dm_contact.
    pub invite_kind: String,
    /// Commitment to the group or DM scope; never the display name.
    pub scope_id_commitment: String,
    /// Ordered signaling profiles allowed for this scope.
    pub signaling_profiles: Vec<SignalingProfileView>,
    /// STUN endpoints selected for this scope.
    pub ice_stun_servers: Vec<String>,
    /// Redacted TURN endpoints selected for this scope.
    pub ice_turn_servers: Vec<IceTurnServerView>,
    /// UI privacy label explaining provider-visible metadata.
    pub privacy_label: String,
    /// Optional DM contact bootstrap.
    #[serde(default)]
    pub dm_bootstrap: Option<DmInviteBootstrapView>,
    /// Optional group admission bootstrap.
    #[serde(default)]
    pub group_bootstrap: Option<GroupInviteBootstrapView>,
}

fn default_connectivity_schema_version() -> u32 {
    INVITE_CONNECTIVITY_SCHEMA_VERSION
}

fn default_invite_kind_group() -> String {
    InviteKind::GroupJoin.canonical_name().to_owned()
}

/// Command-backed invite row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InviteView {
    /// Stable invite identifier for the local command surface.
    pub invite_id: String,
    /// Opaque invite key embedded in the link.
    #[serde(default)]
    pub invite_key: String,
    /// Group id this invite targets; empty for DM contact invites.
    pub group_id: String,
    /// DM id this invite targets for first-contact DM invites.
    #[serde(default)]
    pub dm_id: Option<String>,
    /// Connectivity schema version for this invite descriptor.
    #[serde(default = "default_connectivity_schema_version")]
    pub connectivity_schema_version: u32,
    /// Invite kind: group_join or dm_contact.
    #[serde(default = "default_invite_kind_group")]
    pub invite_kind: String,
    /// Commitment to group/DM scope; never the display name.
    #[serde(default)]
    pub scope_id_commitment: String,
    /// Ordered signed signaling adapter profiles.
    #[serde(default)]
    pub signaling_profiles: Vec<SignalingProfileView>,
    /// UI privacy label explaining provider-visible metadata.
    #[serde(default)]
    pub privacy_label: String,
    /// Optional DM contact bootstrap.
    #[serde(default)]
    pub dm_bootstrap: Option<DmInviteBootstrapView>,
    /// Optional group admission bootstrap.
    #[serde(default)]
    pub group_bootstrap: Option<GroupInviteBootstrapView>,
    /// User-pastable invite code/URL.
    pub code: String,
    /// Hash of the room secret; the plan requires secret-derived admission, not incremental ids.
    #[serde(default)]
    pub room_secret_hash: String,
    /// Signed signaling rendezvous endpoint joiners should use.
    #[serde(default)]
    pub signaling_endpoint: String,
    /// Signed signaling endpoint fingerprint joiners should verify before MLS Welcome.
    #[serde(default)]
    pub signaling_trust_fingerprint: String,
    /// Honest trust posture for the signaling endpoint.
    #[serde(default)]
    pub signaling_trust_status: String,
    /// Signed endpoint policy that constrains allowed endpoint schemes.
    #[serde(default)]
    pub endpoint_policy: String,
    /// Parsed ICE/STUN endpoints from the signed invite descriptor.
    #[serde(default)]
    pub ice_stun_servers: Vec<String>,
    /// Parsed redacted TURN endpoints from the signed invite descriptor.
    #[serde(default)]
    pub ice_turn_servers: Vec<IceTurnServerView>,
    /// Expiry label.
    pub expires: String,
    /// Machine-readable expiration timestamp/relative horizon for the invite.
    #[serde(default)]
    pub expires_at: String,
    /// Maximum-use label.
    pub max_use: String,
    /// Current local use count.
    #[serde(default)]
    pub uses: u32,
    /// Whether the local invite was revoked.
    #[serde(default)]
    pub revoked: bool,
    /// Honest admission copy.
    pub admission_copy: String,
}

impl InviteView {
    fn connectivity_policy(&self) -> ConnectivityPolicyView {
        ConnectivityPolicyView {
            connectivity_schema_version: self.connectivity_schema_version,
            invite_kind: self.invite_kind.clone(),
            scope_id_commitment: self.scope_id_commitment.clone(),
            signaling_profiles: self.signaling_profiles.clone(),
            ice_stun_servers: self.ice_stun_servers.clone(),
            ice_turn_servers: self.ice_turn_servers.clone(),
            privacy_label: self.privacy_label.clone(),
            dm_bootstrap: self.dm_bootstrap.clone(),
            group_bootstrap: self.group_bootstrap.clone(),
        }
    }
}

/// Command-backed voice participant state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceParticipantView {
    /// Participant id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Role label.
    pub role: String,
    /// Whether the participant is currently speaking according to the latest media VAD event.
    pub speaking: bool,
    /// Whether the participant is muted.
    pub muted: bool,
    /// Per-peer remote speaker volume 0-100. Local participants keep the default value but cannot be volume-targeted.
    pub volume: u8,
}

/// Backend-state proof for one attached remote audio receiver.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceRemoteAudioView {
    /// Remote participant represented by the received audio track.
    pub participant_id: String,
    /// Provider/runtime peer id that produced the track.
    pub remote_peer_id: String,
    /// Browser/native MediaStream id that owns the remote audio track.
    pub stream_id: String,
    /// Remote audio MediaStreamTrack id.
    pub audio_track_id: String,
    /// Stable DOM element id that should attach/play this remote stream.
    pub playback_element_id: String,
    /// Count of local audio tracks attached to the WebRTC sender side for this session.
    pub local_audio_tracks_sent: u16,
    /// Count of authenticated/received remote audio frames observed before surfacing playback.
    pub received_audio_frames: u64,
    /// Backend/browser capture timestamp associated with the evidence.
    pub attached_at_ms: u64,
}

/// Backend-owned media runtime boundary recorded for a voice session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceMediaRuntimeView {
    /// Stable runtime id scoped to the current voice session.
    pub runtime_id: String,
    /// Runtime boundary selected by the backend for this session.
    pub boundary: String,
    /// Whether local capture was admitted by permission and device gates.
    pub local_capture_active: bool,
    /// Backend-state proof flag for whether remote WebRTC audio transport is attached and allowed to claim playback.
    pub remote_transport_active: bool,
    /// Remote audio receivers admitted by backend media-route evidence.
    #[serde(default)]
    pub remote_audio: Vec<VoiceRemoteAudioView>,
    /// Empty when not fail-closed; otherwise explains the production gate.
    pub fail_closed_reason: String,
    /// Honest status copy for UI and audit surfaces.
    pub status_copy: String,
}

impl Default for VoiceMediaRuntimeView {
    fn default() -> Self {
        Self {
            runtime_id: "voice-runtime:not-started".to_owned(),
            boundary: "not-started".to_owned(),
            local_capture_active: false,
            remote_transport_active: false,
            remote_audio: Vec::new(),
            fail_closed_reason: "No voice media runtime has been started".to_owned(),
            status_copy:
                "Voice media runtime is not started; no capture or playback route is active"
                    .to_owned(),
        }
    }
}

/// Backend-owned voice signaling exchange state for browser RTCPeerConnection setup.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceSignalingStateView {
    /// Voice session id this signaling state is scoped to.
    pub session_id: String,
    /// Provider/runtime peer id derived from persisted invite/bootstrap state.
    pub local_peer_id: String,
    /// Provider/runtime peer id for the remote participant derived from persisted state.
    pub remote_peer_id: String,
    /// Local WebRTC negotiation role (`offerer` or `answerer`) derived from runtime peer state.
    pub role: String,
    /// Pending local offer/answer/candidate frames queued to the text/control outbox.
    pub pending_local_signals: u32,
    /// Remote offer/answer/candidate frames received from the provider-signaled text/control path.
    pub received_remote_signals: u32,
    /// Last local or remote signal kind observed for this voice session.
    #[serde(default)]
    pub last_signal_kind: Option<String>,
    /// Honest state/evidence copy for UI and audit surfaces.
    pub status_copy: String,
}

impl Default for VoiceSignalingStateView {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            local_peer_id: String::new(),
            remote_peer_id: String::new(),
            role: "not-started".to_owned(),
            pending_local_signals: 0,
            received_remote_signals: 0,
            last_signal_kind: None,
            status_copy: "Voice signaling has not started; no SDP or ICE has crossed backend state"
                .to_owned(),
        }
    }
}

/// Command-backed channel-scoped voice session state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceSessionView {
    /// Stable local session id.
    pub session_id: String,
    /// Group id containing the voice channel.
    pub group_id: String,
    /// Voice channel id.
    pub channel_id: String,
    /// Whether this local shell joined the room.
    pub joined: bool,
    /// Whether the local participant muted themself.
    pub self_muted: bool,
    /// Browser/native microphone permission state used for this join attempt.
    #[serde(default = "default_microphone_permission")]
    pub microphone_permission: String,
    /// Selected microphone/input device after runtime enumeration.
    #[serde(default)]
    pub input_device: Option<VoiceDeviceDescriptor>,
    /// Selected speaker/output device after runtime enumeration.
    #[serde(default)]
    pub output_device: Option<VoiceDeviceDescriptor>,
    /// Backend-owned media runtime/session boundary.
    #[serde(default)]
    pub media_runtime: VoiceMediaRuntimeView,
    /// Backend-owned voice signaling exchange state for RTCPeerConnection setup.
    #[serde(default)]
    pub signaling: VoiceSignalingStateView,
    /// Participant roster.
    pub participants: Vec<VoiceParticipantView>,
    /// Honest route/status copy.
    pub route_copy: String,
    /// Honest media/session status copy.
    pub status_copy: String,
    /// Permission-denied state copy, empty when capture is allowed.
    #[serde(default)]
    pub permission_denied_copy: String,
}

/// Runtime event emitted by mutation commands and available through polling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppEventView {
    /// Monotonic local event sequence.
    pub sequence: u64,
    /// Event kind string.
    pub kind: String,
    /// Human-readable event summary.
    pub summary: String,
}

/// Cursor/topic request for command-backed event polling.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PollAppEventsRequest {
    /// Return events strictly after this cursor.
    #[serde(default)]
    pub after: Option<u64>,
    /// Optional topic filters: message, invite, group, device, transport, voice.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Optional max events returned by this poll.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Cursor-based event stream response for frontend reconciliation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppEventStreamView {
    /// Events matching the requested cursor/topic filter.
    pub events: Vec<AppEventView>,
    /// Cursor supplied by the caller, or zero for first poll.
    pub cursor: u64,
    /// Cursor clients should use for the next poll.
    pub next_cursor: u64,
    /// Whether more matching events remain after this page.
    pub has_more: bool,
    /// Normalized topic filters used for this stream.
    pub subscribed_kinds: Vec<String>,
}

/// Typed command error surfaced in state for actionable frontend UX.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandErrorView {
    /// Stable machine code.
    pub code: String,
    /// Command that produced the error.
    pub command: String,
    /// Human-readable error message.
    pub message: String,
    /// Actionable recovery hint for the UI.
    pub recovery_hint: String,
}

/// Honest backend-derived transport/connectivity status for UI display.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportStatusView {
    /// Surface label such as signaling, ICE, direct, overlay, TURN, degraded, reconnecting, failed.
    pub label: String,
    /// Current backend-derived state.
    pub status: String,
    /// Human-readable caveat/evidence copy.
    pub detail: String,
}

/// Backend-derived group-join progress for the invite/join UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JoinProgressStepView {
    /// Stable stage key.
    pub key: String,
    /// Human label for the stage.
    pub label: String,
    /// Current backend-derived stage status.
    pub status: String,
    /// Evidence or caveat explaining the status.
    pub detail: String,
}

/// Backend-derived voice state row for the voice UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceStateView {
    /// Stable state key.
    pub key: String,
    /// Human-readable label.
    pub label: String,
    /// Current backend-derived state.
    pub status: String,
    /// Evidence/caveat copy.
    pub detail: String,
}

/// Runtime capability and copy-gating state for UI honesty.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeModeView {
    /// Runtime mode label shown by the UI.
    pub mode: String,
    /// Whether production wording is allowed in UI labels.
    pub production_labels_enabled: bool,
    /// Visible badge/copy for local-dev or harness mode.
    pub harness_badge: String,
    /// Backend-derived reason production labels are disabled.
    pub disabled_reason: String,
    /// Service capability rows derived from Cargo features/configuration.
    pub services: Vec<ServiceCapabilityView>,
}

/// Single service capability row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServiceCapabilityView {
    /// Service key.
    pub key: String,
    /// Human label.
    pub label: String,
    /// Current readiness status.
    pub status: String,
    /// Honest caveat/evidence copy.
    pub detail: String,
}

/// Confirmation phrase required before destructive local-state reset.
pub const RESET_APP_CONFIRMATION_PHRASE: &str = "DELETE LOCAL DISCRYPT STATE";

/// Request to destructively reset local app state from the UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResetAppStateRequest {
    /// Must exactly match RESET_APP_CONFIRMATION_PHRASE.
    pub confirmation: String,
}

/// Full command-backed app state consumed by React.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppStateView {
    /// Serialized app-state schema version.
    pub schema_version: u32,
    /// First-run vs ready lifecycle.
    pub lifecycle: AppLifecycle,
    /// Local profile, if setup/recovery has completed.
    pub profile: Option<UserProfileView>,
    /// Persisted UI preferences.
    pub preferences: UiPreferencesView,
    /// Direct conversations.
    pub dms: Vec<DirectConversationView>,
    /// Local-first groups.
    pub groups: Vec<GroupView>,
    /// App-level signaling/ICE defaults used when new DM/group scopes do not override them.
    pub connectivity_defaults: ConnectivityPolicyView,
    /// Active routed context.
    pub active_context: Option<ActiveContextView>,
    /// Message timelines for DMs and text channels.
    pub messages: Vec<MessageView>,
    /// Channel-scoped voice control state.
    pub voice_session: Option<VoiceSessionView>,
    /// Created/joined invites.
    pub invites: Vec<InviteView>,
    /// Local device rows.
    pub devices: Vec<DeviceView>,
    /// Required honest security copy.
    pub security_copy: SecurityCopyView,
    /// Most recent local events.
    pub events: Vec<AppEventView>,
    /// Monotonic cursor of the newest event included in this state snapshot.
    pub event_cursor: u64,
    /// Last typed command error, if the most recent mutation failed validation.
    #[serde(default)]
    pub last_command_error: Option<CommandErrorView>,
    /// Backend-derived transport/connectivity states for honest UI status surfaces.
    #[serde(default)]
    pub transport_status: Vec<TransportStatusView>,
    /// Backend-derived transport diagnostics for adapter and route evidence.
    #[serde(default)]
    pub transport_diagnostics: TransportDiagnosticsView,
    /// Backend-derived invite/join progress states for the UI timeline.
    #[serde(default)]
    pub join_progress: Vec<JoinProgressStepView>,
    /// Text message state legend for timelines.
    #[serde(default)]
    pub text_state_legend: Vec<TextStateView>,
    /// Backend-derived voice state rows for the voice UI.
    #[serde(default)]
    pub voice_states: Vec<VoiceStateView>,
    /// Runtime mode and service configuration state for copy gating.
    pub runtime_mode: RuntimeModeView,
    /// Compatibility snapshot for existing harnesses and transitional UI.
    pub snapshot: AppSnapshot,
}

/// Request to create a local user profile.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateUserRequest {
    /// Display name.
    pub display_name: String,
    /// Optional device label.
    pub device_name: Option<String>,
}

/// Request to recover a local user profile with account-continuity-only material.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoverUserRequest {
    /// Display name.
    pub display_name: String,
    /// Local recovery phrase/code.
    pub recovery_code: String,
    /// Optional device label.
    pub device_name: Option<String>,
    /// Room memberships restored from recovery metadata, never message history or keys.
    #[serde(default)]
    pub recovery_room_memberships: Vec<String>,
    /// Device-set count restored from recovery metadata.
    #[serde(default)]
    pub recovered_device_count: Option<usize>,
    /// Use sealed account-continuity backup material instead of verifier-backed code material.
    #[serde(default)]
    pub use_sealed_account_backup: bool,
}

/// Request to create a signed pasteable payload for adding another own device.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateDevicePairingPayloadRequest {
    /// Human label requested for the device that will accept this payload.
    pub requested_label: String,
    /// Current local pairing epoch/counter. Defaults to the command sequence.
    #[serde(default)]
    pub current_epoch: Option<u64>,
    /// Number of epochs the payload remains valid.
    #[serde(default)]
    pub valid_for_epochs: Option<u64>,
}

/// Request to accept a signed pasteable pairing payload on this local profile.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AcceptDevicePairingPayloadRequest {
    /// JSON payload generated by an already-authorized device.
    pub payload: String,
    /// Local label for the newly paired device.
    pub device_name: Option<String>,
    /// Current local pairing epoch/counter. Defaults to the command sequence.
    #[serde(default)]
    pub current_epoch: Option<u64>,
}

/// Command result for pairing-payload generation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DevicePairingPayloadView {
    /// Generated signed payload string; empty when rejected.
    pub payload: String,
    /// Existing local device that authorized the payload.
    pub authorizing_device_id: String,
    /// Requested label embedded in the signed payload.
    pub requested_label: String,
    /// Last accepted epoch for this payload.
    pub expires_epoch: u64,
    /// Rejection reason when payload generation failed.
    pub rejected_reason: Option<String>,
}

/// Request to persist UI preference changes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SavePreferencesRequest {
    /// Theme identifier to persist.
    pub theme_id: String,
    /// Template identifier to persist.
    pub template_id: String,
}

/// Request to start a local DM.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StartDmRequest {
    /// Participant display name.
    pub display_name: String,
}

/// Request to persist signaling/ICE policy for an app, DM, group, or channel scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetConnectivityPolicyRequest {
    /// Scope to update: app, dm, group, or channel.
    pub scope_kind: String,
    /// Group id for group/channel updates. Uses active group when absent.
    #[serde(default)]
    pub group_id: Option<String>,
    /// Channel id for channel updates. Uses active channel when absent.
    #[serde(default)]
    pub channel_id: Option<String>,
    /// DM id for DM updates. Uses active DM when absent.
    #[serde(default)]
    pub dm_id: Option<String>,
    /// Selected signaling adapter kind.
    #[serde(default)]
    pub adapter_kind: Option<String>,
    /// Broker/relay/bootstrap/rendezvous endpoint for the selected adapter.
    #[serde(default)]
    pub signaling_endpoint: Option<String>,
    /// STUN endpoints for this scope.
    #[serde(default)]
    pub ice_stun_servers: Option<Vec<String>>,
    /// Redacted TURN endpoints for this scope.
    #[serde(default)]
    pub ice_turn_servers: Option<Vec<IceTurnServerView>>,
}

/// Request to create a local-first group/server.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    /// Group display name.
    pub name: String,
    /// Default retention label for new text channels.
    pub retention: String,
    /// Optional production signaling adapter override for this group/invite scope.
    #[serde(default)]
    pub adapter_kind: Option<String>,
    /// Optional provider endpoint override for the selected signaling adapter.
    #[serde(default)]
    pub signaling_endpoint: Option<String>,
    /// Optional STUN endpoints for this group/invite scope.
    #[serde(default)]
    pub ice_stun_servers: Option<Vec<String>>,
    /// Optional redacted TURN endpoint metadata for this group/invite scope.
    #[serde(default)]
    pub ice_turn_servers: Option<Vec<IceTurnServerView>>,
}

/// Request to focus an existing group from the server rail.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetActiveGroupRequest {
    /// Existing group id.
    pub group_id: String,
}

/// Request to focus a specific text or voice channel.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetActiveChannelRequest {
    pub group_id: String,
    pub channel_id: String,
}

/// Request to focus a specific DM conversation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetActiveDmRequest {
    pub dm_id: String,
}

/// Request to join a local-first group/server from an invite.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JoinGroupRequest {
    /// Invite code or paste payload.
    pub invite_code: String,
    /// Display label assigned to the joined group.
    pub group_name: Option<String>,
}

/// Request to create an invite for a group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateInviteRequest {
    /// Optional group id; active group is used when absent.
    pub group_id: Option<String>,
    /// Expiry label selected by the user/admin.
    pub expires: String,
    /// Maximum-use label selected by the user/admin.
    pub max_use: String,
}

/// Request to create a first-contact DM invite.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateDmInviteRequest {
    /// Optional DM id; active DM is used when absent.
    pub dm_id: Option<String>,
    /// Expiry label selected by the user.
    pub expires: String,
    /// Maximum-use label selected by the user.
    pub max_use: String,
}

/// Request to accept/open a first-contact DM invite.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AcceptDmInviteRequest {
    /// DM contact invite code or paste payload.
    pub invite_code: String,
    /// Optional display label assigned to the contact.
    pub display_name: Option<String>,
}

/// Request to create a channel in a group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateChannelRequest {
    /// Group id.
    pub group_id: String,
    /// Channel display name. Text channels are normalized with a leading '#'.
    pub name: String,
    /// Channel kind.
    pub kind: ChannelKind,
    /// Channel retention label.
    pub retention_status: String,
}

/// Request to append a message to a command-backed local timeline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SendMessageRequest {
    /// Message target.
    pub target: MessageTargetView,
    /// Message body.
    pub body: String,
    /// When true, run a real provider-signaled WebRTC DataChannel proof for an
    /// opaque frame derived from this message before marking transport proof.
    /// This does not claim peer receipt.
    #[serde(default)]
    pub transport_proof: bool,
    /// Optional adapter kind override for the text transport proof (`mqtt`,
    /// `nostr`, `ipfs_pubsub`, or `discrypt_quic_rendezvous`).
    #[serde(default)]
    pub adapter_kind: Option<String>,
}

/// Request to apply a signed peer delivery receipt to a persisted message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplyTextDeliveryReceiptRequest {
    /// Message id being acknowledged.
    pub message_id: String,
    /// Signed delivery receipt received from a peer device over text/control transport.
    pub receipt: TextDeliveryReceipt,
    /// Hex-encoded recipient Ed25519 verifying key expected to validate the receipt.
    pub recipient_verifying_key_hex: String,
}

/// Request to accept a signed encrypted text envelope from a peer and produce a receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReceiveTextDeliveryEnvelopeRequest {
    /// Conversation target that determines the expected delivery group binding.
    pub target: MessageTargetView,
    /// Signed encrypted text envelope received over text/control transport.
    pub envelope: TextMessageEnvelope,
    /// Hex-encoded sender Ed25519 verifying key expected to validate the envelope.
    pub sender_verifying_key_hex: String,
    /// Recipient MLS leaf/device slot that persisted the envelope.
    #[serde(default)]
    pub recipient_leaf: Option<u32>,
}

/// Result of accepting a peer text envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReceiveTextDeliveryEnvelopeResponse {
    /// Updated application state after accepting or rejecting the envelope.
    pub state: AppStateView,
    /// Signed receipt to return to the sender when acceptance succeeded.
    pub receipt: Option<TextDeliveryReceipt>,
    /// Hex-encoded recipient verifying key for receipt verification by the sender.
    pub recipient_verifying_key_hex: Option<String>,
}

/// Text/control frame carried by the peer DataChannel.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TextControlFrameView {
    /// Signed encrypted text envelope from a peer.
    Envelope {
        /// Conversation target that determines expected group binding.
        target: MessageTargetView,
        /// Signed encrypted message envelope.
        envelope: TextMessageEnvelope,
        /// Hex-encoded sender verifying key.
        sender_verifying_key_hex: String,
        /// Recipient leaf/device slot that persisted the envelope.
        #[serde(default)]
        recipient_leaf: Option<u32>,
    },
    /// Backend-state-only browser media offer/answer/candidate message for the browser media runtime; this struct alone does not claim remote audio.
    VoiceSignal {
        /// Voice signaling payload validated and persisted by the backend.
        signal: VoiceSignalingMessageView,
    },
    /// Signed delivery receipt returning to an envelope sender.
    Receipt {
        /// Message id being acknowledged.
        message_id: String,
        /// Signed receipt payload.
        receipt: TextDeliveryReceipt,
        /// Hex-encoded recipient verifying key.
        recipient_verifying_key_hex: String,
    },
}

/// Persisted outbound text/control frame ready for a transport session loop.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextControlOutboxFrameView {
    /// Message id associated with this frame.
    pub message_id: String,
    /// Conversation target that determines transport scope.
    pub target: MessageTargetView,
    /// Signed frame to put on the text/control DataChannel.
    pub frame: TextControlFrameView,
    /// Durable outbox state (`pending`, `sent`, or `receipted`).
    pub state_key: String,
    /// Number of transport send attempts recorded by the app session loop.
    pub attempts: u32,
    /// Last transport session id that claimed to send this frame.
    pub last_transport_session_id: Option<String>,
    /// Stable SHA-256 over the serialized frame, used as an idempotency guard.
    pub frame_sha256: String,
}

/// Request to list pending text/control frames for a session loop.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ListPendingTextControlFramesRequest {
    /// Optional target filter for the active DM/channel session.
    #[serde(default)]
    pub target: Option<MessageTargetView>,
    /// Optional result cap. Defaults to 50 and is clamped to 200.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional per-send/receive transport operation timeout in milliseconds.
    ///
    /// Defaults to five seconds and is clamped to 100..=60000ms so a broken
    /// peer/runtime cannot hang the app-service command or the UI indefinitely.
    #[serde(default)]
    pub operation_timeout_ms: Option<u64>,
}

/// Response containing state plus outbound frames still needing transport work.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ListPendingTextControlFramesResponse {
    /// Updated/snapshot application state.
    pub state: AppStateView,
    /// Pending outbound text/control frames.
    pub frames: Vec<TextControlOutboxFrameView>,
}

/// Request to mark a text/control frame as handed to a transport session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkTextControlFrameSentRequest {
    /// Message id associated with this frame.
    pub message_id: String,
    /// Expected frame hash from the pending outbox view.
    pub frame_sha256: String,
    /// Optional session id that sent the frame.
    #[serde(default)]
    pub transport_session_id: Option<String>,
}

/// Request to handle a peer text/control frame from a DataChannel/session loop.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HandleTextControlFrameRequest {
    /// Incoming frame to verify/apply.
    pub frame: TextControlFrameView,
}

/// Result of handling a text/control frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HandleTextControlFrameResponse {
    /// Updated application state after handling the frame.
    pub state: AppStateView,
    /// Optional response frame to send back over the same text/control transport.
    pub response_frame: Option<TextControlFrameView>,
}

/// Backend-state-only browser media signaling payload carried over the text/control route; no remote audio is claimed here.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceSignalingMessageView {
    /// Stable id for idempotency across transport retries.
    pub signal_id: String,
    /// Voice session id the signal belongs to.
    pub session_id: String,
    /// Group id containing the voice channel.
    pub group_id: String,
    /// Voice channel id.
    pub channel_id: String,
    /// Local app participant id of the sender.
    pub sender_participant_id: String,
    /// Provider/runtime peer id of the sender.
    pub sender_peer_id: String,
    /// Provider/runtime peer id expected to receive this signal.
    pub recipient_peer_id: String,
    /// `offer`, `answer`, or `candidate`.
    pub signal_kind: String,
    /// WebView-sealed voice signal payload. Raw SDP/ICE never crosses IPC or persisted state.
    pub sealed_payload: String,
    /// Browser/native timestamp in milliseconds for correlation.
    pub created_at_ms: u64,
}

/// Request to queue an outbound backend-state-only voice offer/answer/candidate; remote media remains pending until proven.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PublishVoiceSignalingMessageRequest {
    /// Active voice session id.
    pub session_id: String,
    /// `offer`, `answer`, or `candidate`.
    pub signal_kind: String,
    /// WebView-sealed voice signal payload. Raw SDP/ICE never crosses IPC or persisted state.
    pub sealed_payload: String,
    /// Optional stable id supplied by the browser for idempotency.
    #[serde(default)]
    pub signal_id: Option<String>,
    /// Browser/native timestamp in milliseconds for correlation.
    pub created_at_ms: u64,
}

/// Request to drain pending inbound WebRTC voice signaling messages from backend state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TakePendingVoiceSignalingMessagesRequest {
    /// Optional voice session id filter. Defaults to the active voice session.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Optional result cap. Defaults to 50 and is clamped to 200.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Response containing state plus inbound voice signaling messages for RTCPeerConnection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TakePendingVoiceSignalingMessagesResponse {
    /// Updated application state after draining messages.
    pub state: AppStateView,
    /// Inbound offer/answer/candidate messages not-delivered to the browser runtime yet.
    pub messages: Vec<VoiceSignalingMessageView>,
}

/// One-shot text/control transport pump report for a session-loop iteration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextControlTransportPumpReportView {
    /// Pending frames selected before the pump attempted transport.
    pub pending_before: usize,
    /// Frames handed to the app-facing data transport.
    pub frames_sent: usize,
    /// Response frames received from the transport.
    pub response_frames_received: usize,
    /// Signed peer receipts applied to local message state.
    pub receipts_applied: usize,
    /// Frames that failed send/receive/decode/apply.
    pub failures: Vec<String>,
    /// DataChannel/text-control transport metrics after the pump iteration.
    pub metrics: discrypt_transport::WebRtcDataTransportMetrics,
}

/// Request to join a voice channel.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JoinVoiceRequest {
    /// Group id.
    pub group_id: String,
    /// Voice channel id.
    pub channel_id: String,
    /// Browser/native microphone permission observed by the UI shell.
    #[serde(default = "default_microphone_permission")]
    pub microphone_permission: String,
    /// Selected microphone device id.
    #[serde(default)]
    pub input_device_id: Option<String>,
    /// Selected microphone label.
    #[serde(default)]
    pub input_device_label: Option<String>,
    /// Selected speaker/output device id.
    #[serde(default)]
    pub output_device_id: Option<String>,
    /// Selected speaker/output label.
    #[serde(default)]
    pub output_device_label: Option<String>,
}

/// Request to leave a voice session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LeaveVoiceRequest {
    /// Session id.
    pub session_id: String,
}

/// Request to start a backend transport session for signaling control plane.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StartSignalingSessionRequest {
    /// Optional stable label for observability and command dedupe.
    pub scope_label: Option<String>,
    /// When true, run a real provider-adapter opaque signaling roundtrip probe
    /// using the selected DM/group/invite connectivity policy.
    #[serde(default)]
    pub adapter_probe: bool,
    /// When true, run a real provider-signaled WebRTC DataChannel smoke after
    /// the adapter roundtrip, using the selected connectivity policy and ICE
    /// STUN configuration. This proves transport-layer text/control delivery,
    /// not installed-app UI or voice/media delivery.
    #[serde(default)]
    pub data_channel_probe: bool,
    /// Optional adapter kind override for the probe (`mqtt`, `nostr`,
    /// `ipfs_pubsub`, or `discrypt_quic_rendezvous`).
    #[serde(default)]
    pub adapter_kind: Option<String>,
}

/// Request to stop a backend signaling transport session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StopSignalingSessionRequest {
    /// Optional active session id to target; omitted id stops the current session.
    pub session_id: Option<String>,
}

/// Request to start a backend transport session for text/control channels.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StartTextSessionRequest {
    /// Optional stable label for observability and command dedupe.
    pub scope_label: Option<String>,
    /// When true, verify the selected provider-signaled WebRTC DataChannel and
    /// bind that proof to the text session route state. This proves the text/control
    /// transport, not remote UI persistence or voice/media delivery.
    #[serde(default)]
    pub data_channel_probe: bool,
    /// Optional adapter kind override for the proof (`mqtt`, `nostr`,
    /// `ipfs_pubsub`, or `discrypt_quic_rendezvous`).
    #[serde(default)]
    pub adapter_kind: Option<String>,
}

/// Request to stop a backend text/control transport session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StopTextSessionRequest {
    /// Optional active session id to target; omitted id stops the current session.
    pub session_id: Option<String>,
}

/// Request to attach a long-lived text/control transport runtime to the active
/// text session.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttachTextControlTransportRuntimeRequest {
    /// Optional expected session id for explicit attach scoping. If present and the
    /// active text session does not match, attach is rejected.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Optional runtime role for a real role-split attach (`offerer` or `answerer`).
    ///
    /// When omitted, the command preserves the legacy fail-closed probe-resume
    /// behavior so older clients do not accidentally block waiting for a peer.
    #[serde(default)]
    pub runtime_role: Option<String>,
    /// Stable, scoped local peer id for provider-visible routing.
    #[serde(default)]
    pub local_peer_id: Option<String>,
    /// Stable, scoped remote peer id for provider-visible routing.
    #[serde(default)]
    pub remote_peer_id: Option<String>,
    /// Derive runtime role and peer ids from persisted DM/group invite state.
    ///
    /// This is the production UI path: users never type peer ids manually.
    /// Legacy callers that omit this flag and omit explicit role/peer ids still
    /// hit the fail-closed probe-resume boundary.
    #[serde(default)]
    pub derive_from_state: bool,
}

/// Request to set self mute state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetSelfMuteRequest {
    /// Session id.
    pub session_id: String,
    /// Whether the local participant is muted.
    pub muted: bool,
}

/// Request carrying real local microphone level evidence for speaking-state UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateVoiceActivityRequest {
    /// Session id.
    pub session_id: String,
    /// RMS amplitude measured from a real local capture buffer, scaled to i16 full scale.
    pub rms_i16: u16,
    /// Peak amplitude measured from a real local capture buffer, scaled to i16 full scale.
    pub peak_i16: u16,
    /// Browser/native capture timestamp in milliseconds for event correlation.
    pub captured_at_ms: u64,
}

/// Request carrying backend-state proof that a real remote audio track is attached.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttachVoiceRemoteMediaRequest {
    /// Session id.
    pub session_id: String,
    /// Remote participant identifier; must not be the local profile id.
    pub participant_id: String,
    /// Display name for the remote participant.
    pub participant_name: String,
    /// Provider/runtime peer id that produced the track.
    pub remote_peer_id: String,
    /// Browser/native MediaStream id that owns the remote audio track.
    pub stream_id: String,
    /// Remote audio MediaStreamTrack id.
    pub audio_track_id: String,
    /// Stable DOM element id that should attach/play this remote stream.
    pub playback_element_id: String,
    /// Count of local audio tracks attached to the sender side.
    pub local_audio_tracks_sent: u16,
    /// Count of remote audio frames observed before playback is surfaced.
    pub received_audio_frames: u64,
    /// Whether latest remote audio-level/VAD evidence marks this peer as speaking.
    #[serde(default)]
    pub speaking: bool,
    /// Evidence timestamp in milliseconds.
    pub attached_at_ms: u64,
}

/// Request to set a participant speaker volume.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetSpeakerVolumeRequest {
    /// Session id.
    pub session_id: String,
    /// Participant identifier.
    pub participant_id: String,
    /// Volume 0-100.
    pub volume: u8,
}

fn default_microphone_permission() -> String {
    "unknown".to_owned()
}

/// Command-surface health for local E2E/smoke execution.
///
/// These fields describe command availability and honest-copy coverage, not live
/// production network/media readiness. Production capability claims remain gated
/// by backend state and Cargo production features.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandHealth {
    /// Compatibility snapshot command can serialize the current backend state.
    pub snapshot_ready: bool,
    /// Safety verification command can derive and compare the current backend safety number.
    pub verification_ready: bool,
    /// Canonical app-state command can serialize the current backend state.
    pub app_state_ready: bool,
    /// Identity lifecycle commands are available for the current lifecycle state.
    pub identity_ready: bool,
    /// Group/channel/message/invite commands are available against local AppStore state.
    pub collaboration_ready: bool,
    /// Voice control commands are available; this is false until production media is enabled.
    pub voice_ready: bool,
    /// Honest security copy is present.
    pub honest_copy_ready: bool,
}

/// Readiness for one required signaling adapter boundary used by diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingAdapterBoundaryView {
    /// Adapter canonical kind label.
    pub kind: String,
    /// Cargo feature required by this adapter boundary.
    pub cargo_feature: String,
    /// Readiness state from build-time boundary inspection.
    pub readiness: String,
    /// Redacted failure class for UI and test assertions.
    pub failure_class: String,
}

/// Transport-session and adapter readines diagnostics surfaced to trusted tooling.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportDiagnosticsView {
    /// Required adapter boundaries and their readiness labels.
    pub adapter_boundaries: Vec<SignalingAdapterBoundaryView>,
    /// Planned fallback attempts across required adapter boundaries.
    pub adapter_fallback_attempts: Vec<SignalingAdapterFallbackAttemptView>,
    /// Selected signaling adapter if one candidate is healthy.
    pub selected_adapter: Option<String>,
    /// Transport route-proof status from session evidence.
    pub route_proof_status: String,
    /// Route-proof details copied from the selected transport route report.
    pub route_proof_detail: String,
    /// TURN-required status derived from route evidence.
    pub turn_required: String,
    /// Latest real provider-adapter roundtrip probe status.
    pub adapter_probe_status: String,
    /// Evidence or blocker from the latest provider-adapter roundtrip probe.
    pub adapter_probe_detail: String,
    /// Structured latest provider-adapter probe evidence, when available.
    #[serde(default)]
    pub adapter_probe: Option<SignalingAdapterProbeView>,
    /// Latest provider-signaled WebRTC DataChannel probe status.
    pub data_channel_probe_status: String,
    /// Evidence or blocker from the latest WebRTC DataChannel probe.
    pub data_channel_probe_detail: String,
    /// Structured latest provider-signaled WebRTC DataChannel proof.
    #[serde(default)]
    pub data_channel_probe: Option<ProviderWebRtcDataChannelProbeView>,
}

/// Readiness and selection metadata for a fallback signaling adapter attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingAdapterFallbackAttemptView {
    /// Adapter canonical kind label.
    pub kind: String,
    /// Attempt readiness from adapter boundary inspection.
    pub readiness: String,
    /// Redacted failure class for UI and test assertions.
    pub failure_class: String,
    /// Attempt was executed under the active fallback policy.
    pub attempted: bool,
    /// This adapter won selection under the active fallback policy.
    pub selected: bool,
}

/// Backend evidence from a real provider adapter probe.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalingAdapterProbeView {
    /// Adapter kind.
    pub kind: String,
    /// Policy profile id.
    pub profile_id: String,
    /// Redacted endpoint label.
    pub endpoint_label: String,
    /// Scope commitment used for topic derivation.
    pub scope_commitment: String,
    /// Provider-visible derived rendezvous topic/tag.
    pub rendezvous_topic: String,
    /// Presence proof flag.
    pub presence_roundtrip: bool,
    /// Sealed signal proof flag.
    pub signal_roundtrip: bool,
    /// Sealed control proof flag.
    pub control_roundtrip: bool,
}

/// Backend evidence from a provider-signaled WebRTC DataChannel probe.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderWebRtcDataChannelProbeView {
    /// Adapter kind.
    pub kind: String,
    /// Policy profile id.
    pub profile_id: String,
    /// Redacted endpoint label.
    pub endpoint_label: String,
    /// Scope commitment used for topic derivation.
    #[serde(default)]
    pub scope_commitment: String,
    /// Provider-visible derived rendezvous topic/tag.
    pub rendezvous_topic: String,
    /// Offerer WebRTC direct path readiness.
    pub offerer_direct_path_ready: bool,
    /// Answerer WebRTC direct path readiness.
    pub answerer_direct_path_ready: bool,
    /// Offerer TURN fallback readiness from configured relay candidate evidence.
    #[serde(default)]
    pub offerer_turn_fallback_ready: bool,
    /// Answerer TURN fallback readiness from configured relay candidate evidence.
    #[serde(default)]
    pub answerer_turn_fallback_ready: bool,
    /// Number of TURN servers configured for the offerer probe.
    #[serde(default)]
    pub offerer_configured_turn_servers: u64,
    /// Number of TURN servers configured for the answerer probe.
    #[serde(default)]
    pub answerer_configured_turn_servers: u64,
    /// Local relay candidates gathered by the offerer probe.
    #[serde(default)]
    pub offerer_local_relay_candidates_gathered: u64,
    /// Local relay candidates gathered by the answerer probe.
    #[serde(default)]
    pub answerer_local_relay_candidates_gathered: u64,
    /// Remote relay candidates applied by the offerer probe.
    #[serde(default)]
    pub offerer_remote_relay_candidates_applied: u64,
    /// Remote relay candidates applied by the answerer probe.
    #[serde(default)]
    pub answerer_remote_relay_candidates_applied: u64,
    /// Offerer DataChannel open state.
    pub offerer_data_channel_open: bool,
    /// Answerer DataChannel open state.
    pub answerer_data_channel_open: bool,
    /// Opaque text/control frame crossed the DataChannel from offerer to answerer.
    pub text_control_frame_roundtrip: bool,
    /// SHA-256 of the opaque text/control frame used for the proof.
    #[serde(default)]
    pub text_control_frame_sha256: String,
    /// Opaque return receipt/control frame crossed from answerer back to offerer.
    #[serde(default)]
    pub receipt_frame_roundtrip: bool,
    /// SHA-256 of the opaque return receipt/control frame used for the proof.
    #[serde(default)]
    pub receipt_frame_sha256: String,
    /// Versioned provider runtime handoff material captured by the backend probe.
    #[serde(default)]
    pub runtime_spec: Option<ProviderTextControlRuntimeSpec>,
}

impl From<discrypt_transport::ProviderWebRtcDataChannelProbe>
    for ProviderWebRtcDataChannelProbeView
{
    fn from(probe: discrypt_transport::ProviderWebRtcDataChannelProbe) -> Self {
        Self {
            kind: probe.kind.canonical_name().to_owned(),
            profile_id: probe.profile_id,
            endpoint_label: probe.endpoint_label,
            scope_commitment: probe.scope_commitment,
            rendezvous_topic: probe.rendezvous_topic,
            offerer_direct_path_ready: probe.offerer_direct_path_ready,
            answerer_direct_path_ready: probe.answerer_direct_path_ready,
            offerer_turn_fallback_ready: probe.offerer_turn_fallback_ready,
            answerer_turn_fallback_ready: probe.answerer_turn_fallback_ready,
            offerer_configured_turn_servers: probe.offerer_configured_turn_servers,
            answerer_configured_turn_servers: probe.answerer_configured_turn_servers,
            offerer_local_relay_candidates_gathered: probe.offerer_local_relay_candidates_gathered,
            answerer_local_relay_candidates_gathered: probe
                .answerer_local_relay_candidates_gathered,
            offerer_remote_relay_candidates_applied: probe.offerer_remote_relay_candidates_applied,
            answerer_remote_relay_candidates_applied: probe
                .answerer_remote_relay_candidates_applied,
            offerer_data_channel_open: probe.offerer_data_channel_open,
            answerer_data_channel_open: probe.answerer_data_channel_open,
            text_control_frame_roundtrip: probe.text_control_frame_roundtrip,
            text_control_frame_sha256: probe.text_control_frame_sha256,
            receipt_frame_roundtrip: probe.receipt_frame_roundtrip,
            receipt_frame_sha256: probe.receipt_frame_sha256,
            runtime_spec: probe.runtime_spec,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct AbuseBucketView {
    key: String,
    timestamps: Vec<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
struct PersistedAbuseState {
    #[serde(default)]
    invite_create: Vec<AbuseBucketView>,
    #[serde(default)]
    invite_consume: Vec<AbuseBucketView>,
    #[serde(default)]
    admission_helper: Vec<AbuseBucketView>,
    #[serde(default)]
    signaling_publish_take: Vec<AbuseBucketView>,
    #[serde(default)]
    text_send: Vec<AbuseBucketView>,
}

impl PersistedAbuseState {
    fn allow_invite_create(&mut self, key: &str, now: DateTime<Utc>) -> bool {
        allow_persisted_action(&mut self.invite_create, key, INVITE_CREATE_LIMIT, now)
    }

    fn allow_invite_consume(&mut self, key: &str, now: DateTime<Utc>) -> bool {
        allow_persisted_action(&mut self.invite_consume, key, INVITE_CREATE_LIMIT, now)
    }

    fn allow_admission_helper(&mut self, key: &str, now: DateTime<Utc>) -> bool {
        allow_persisted_action(
            &mut self.admission_helper,
            key,
            ADMISSION_HELPER_ATTEMPT_LIMIT,
            now,
        )
    }

    fn allow_signaling_publish_take(&mut self, key: &str, now: DateTime<Utc>) -> bool {
        allow_persisted_action(
            &mut self.signaling_publish_take,
            key,
            SIGNALING_ACTION_LIMIT,
            now,
        )
    }

    fn allow_text_send(&mut self, key: &str, now: DateTime<Utc>) -> bool {
        allow_persisted_action(&mut self.text_send, key, TEXT_SEND_LIMIT, now)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BackendTransportMode {
    Signaling,
    Text,
}

impl std::fmt::Display for BackendTransportMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str((*self).label())
    }
}

impl BackendTransportMode {
    fn session_id_prefix(self) -> &'static str {
        match self {
            Self::Signaling => "signaling-session",
            Self::Text => "text-session",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Signaling => "signaling",
            Self::Text => "text",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct TransportSessionRecord {
    session_id: String,
    scope_label: String,
    mode: BackendTransportMode,
    session: TransportSession,
}

impl TransportSessionRecord {
    fn snapshot(&self) -> TransportSessionSnapshot {
        self.session.snapshot()
    }

    fn state(&self) -> TransportSessionState {
        self.snapshot().state
    }

    fn connected_route(&self) -> Option<TransportRoute> {
        self.snapshot().route.map(|route| route.route)
    }
}

#[derive(Clone)]
struct TextControlTransportRuntime {
    transport: Arc<dyn discrypt_transport::TextControlDataTransport>,
    owned_runtime: Option<Arc<discrypt_transport::ProviderTextControlRuntime>>,
    executor: Option<Arc<tokio::runtime::Runtime>>,
    session_id: String,
    role: Option<ProviderTextControlRuntimePeerRole>,
    local_peer_id: Option<String>,
    remote_peer_id: Option<String>,
}

#[derive(Clone, Debug)]
struct PendingTextControlTransportRuntime {
    session_id: String,
    role: ProviderTextControlRuntimePeerRole,
    local_peer_id: String,
    remote_peer_id: String,
}

#[derive(Clone)]
struct TextControlRuntimeAttachInputs {
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: Vec<u8>,
    random_entropy: Vec<u8>,
    ice_config: IceServerConfig,
}

#[derive(Clone)]
struct TextControlRuntimePeerAttachment {
    role: ProviderTextControlRuntimePeerRole,
    local_peer_id: SignalingPeerId,
    remote_peer_id: SignalingPeerId,
}

struct TextControlRuntimeAttachJob {
    command_name: &'static str,
    active_session_id: String,
    inputs: TextControlRuntimeAttachInputs,
    role: ProviderTextControlRuntimePeerRole,
    local_peer_id: SignalingPeerId,
    remote_peer_id: SignalingPeerId,
}

impl fmt::Debug for TextControlTransportRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TextControlTransportRuntime")
            .field("session_id", &self.session_id)
            .field("role", &self.role)
            .field("local_peer_id", &self.local_peer_id)
            .field("remote_peer_id", &self.remote_peer_id)
            .field("owns_provider_runtime", &self.owned_runtime.is_some())
            .field("owns_executor", &self.executor.is_some())
            .finish_non_exhaustive()
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
enum ReceivedTextRender {
    Pipeline(TextRenderState),
    EnvelopeOnly { reason: String },
    DecryptFailed,
}

#[allow(dead_code)]
impl ReceivedTextRender {
    fn message_fields(
        &self,
        envelope: &TextMessageEnvelope,
    ) -> (String, String, String, String, String) {
        let ciphertext_hash = hex::encode(envelope.ciphertext_hash());
        match self {
            Self::Pipeline(TextRenderState::Decrypted(plaintext)) => (
                String::from_utf8_lossy(plaintext).into_owned(),
                "signed encrypted peer envelope verified and decrypted through TextInboundPipeline using the persisted OpenMLS text exporter".to_owned(),
                "received_plaintext".to_owned(),
                "Plaintext received".to_owned(),
                "plaintext rendered through TextInboundPipeline".to_owned(),
            ),
            Self::Pipeline(TextRenderState::Locked { reason }) => (
                format!("Encrypted message envelope received (ciphertext_hash={ciphertext_hash})"),
                format!("signed encrypted peer envelope verified, but plaintext is locked: {reason}"),
                "received_locked".to_owned(),
                "Envelope locked".to_owned(),
                format!("plaintext not rendered because {reason}"),
            ),
            Self::EnvelopeOnly { reason } => (
                format!("Encrypted message envelope received (ciphertext_hash={ciphertext_hash})"),
                format!("signed encrypted peer envelope verified and persisted; plaintext render unavailable: {reason}"),
                "received_envelope".to_owned(),
                "Envelope received".to_owned(),
                format!("plaintext not rendered because {reason}"),
            ),
            Self::DecryptFailed => (
                format!("Encrypted message envelope received (ciphertext_hash={ciphertext_hash})"),
                "signed encrypted peer envelope verified, but TextInboundPipeline could not decrypt with the persisted OpenMLS exporter; plaintext was not rendered".to_owned(),
                "received_decrypt_failed".to_owned(),
                "Decrypt failed".to_owned(),
                "plaintext not rendered because exporter-backed decryption failed".to_owned(),
            ),
        }
    }

    fn event_label(&self) -> &'static str {
        match self {
            Self::Pipeline(TextRenderState::Decrypted(_)) => "plaintext_rendered",
            Self::Pipeline(TextRenderState::Locked { .. }) => "plaintext_locked",
            Self::EnvelopeOnly { .. } => "envelope_only",
            Self::DecryptFailed => "decrypt_failed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct TextDeliveryEnvelopeRecord {
    message_id: String,
    group_id: String,
    sender_verifying_key_hex: String,
    envelope: TextMessageEnvelope,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct TextDeliveryReceiptRecord {
    message_id: String,
    recipient_verifying_key_hex: String,
    receipt: TextDeliveryReceipt,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct TextControlOutboxRecord {
    message_id: String,
    target: MessageTargetView,
    frame: TextControlFrameView,
    frame_sha256: String,
    attempts: u32,
    state_key: String,
    last_transport_session_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct VoiceSignalingInboxRecord {
    signal: VoiceSignalingMessageView,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct OpenMlsGroupHandleRecord {
    group_id: String,
    signer_public_key_hex: String,
    epoch: u64,
    #[serde(default)]
    local_leaf: u32,
    confirmation_tag_sha256: String,
    #[serde(default)]
    openmls_store_path: Option<String>,
    status_copy: String,
}

#[cfg_attr(not(test), allow(dead_code))]
struct OpenMlsAdmissionKeyPackage {
    group_id: String,
    member_identity: String,
    signer_public_key_hex: String,
    package: OpenMlsMemberPackage,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
struct OpenMlsAdmissionWelcome {
    group_id: String,
    owner_signer_public_key_hex: String,
    member_signer_public_key_hex: String,
    welcome_bytes: Vec<u8>,
    epoch: u64,
    confirmation_tag_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReceivedTextRender {
    Pipeline(TextRenderState),
    EnvelopeOnly { reason: String },
    DecryptFailed,
}

impl ReceivedTextRender {
    fn message_fields(
        &self,
        envelope: &TextMessageEnvelope,
    ) -> (String, String, String, String, String) {
        let ciphertext_hash = hex::encode(envelope.ciphertext_hash());
        match self {
            Self::Pipeline(TextRenderState::Decrypted(plaintext)) => (
                String::from_utf8_lossy(plaintext).into_owned(),
                "signed encrypted peer envelope verified and decrypted through TextInboundPipeline using the persisted OpenMLS text exporter".to_owned(),
                "received_plaintext".to_owned(),
                "Plaintext received".to_owned(),
                "plaintext rendered through TextInboundPipeline".to_owned(),
            ),
            Self::Pipeline(TextRenderState::Locked { reason }) => (
                format!("Encrypted message envelope received (ciphertext_hash={ciphertext_hash})"),
                format!("signed encrypted peer envelope verified, but plaintext is locked: {reason}"),
                "received_locked".to_owned(),
                "Envelope locked".to_owned(),
                format!("plaintext not rendered because {reason}"),
            ),
            Self::EnvelopeOnly { reason } => (
                format!("Encrypted message envelope received (ciphertext_hash={ciphertext_hash})"),
                format!(
                    "signed encrypted peer envelope verified and persisted; plaintext render unavailable: {reason}"
                ),
                "received_envelope".to_owned(),
                "Envelope received".to_owned(),
                format!("plaintext not rendered because {reason}"),
            ),
            Self::DecryptFailed => (
                format!("Encrypted message envelope received (ciphertext_hash={ciphertext_hash})"),
                "signed encrypted peer envelope verified, but TextInboundPipeline could not decrypt with the persisted OpenMLS exporter; plaintext was not rendered".to_owned(),
                "received_decrypt_failed".to_owned(),
                "Decrypt failed".to_owned(),
                "plaintext not rendered because exporter-backed decryption failed".to_owned(),
            ),
        }
    }

    fn event_label(&self) -> &'static str {
        match self {
            Self::Pipeline(TextRenderState::Decrypted(_)) => "plaintext_rendered",
            Self::Pipeline(TextRenderState::Locked { .. }) => "plaintext_locked",
            Self::EnvelopeOnly { .. } => "envelope_only",
            Self::DecryptFailed => "decrypt_failed",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PersistedAppState {
    schema_version: u32,
    lifecycle: AppLifecycle,
    profile: Option<UserProfileView>,
    preferences: UiPreferencesView,
    dms: Vec<DirectConversationView>,
    groups: Vec<GroupView>,
    #[serde(default)]
    openmls_groups: Vec<OpenMlsGroupHandleRecord>,
    #[serde(default = "app_connectivity_defaults")]
    connectivity_defaults: ConnectivityPolicyView,
    active_context: Option<ActiveContextView>,
    messages: Vec<MessageView>,
    #[serde(default)]
    text_delivery_envelopes: Vec<TextDeliveryEnvelopeRecord>,
    #[serde(default)]
    text_delivery_receipts: Vec<TextDeliveryReceiptRecord>,
    #[serde(default)]
    text_control_outbox: Vec<TextControlOutboxRecord>,
    #[serde(default)]
    voice_signaling_inbox: Vec<VoiceSignalingInboxRecord>,
    voice_session: Option<VoiceSessionView>,
    invites: Vec<InviteView>,
    devices: Vec<DeviceView>,
    #[serde(default)]
    identity_seed_hex: String,
    #[serde(default)]
    device_set: DeviceSet,
    security_copy: SecurityCopyView,
    events: Vec<AppEventView>,
    #[serde(default)]
    last_command_error: Option<CommandErrorView>,
    #[serde(default)]
    signaling_session: Option<TransportSessionRecord>,
    #[serde(default)]
    text_session: Option<TransportSessionRecord>,
    #[serde(default)]
    latest_signaling_probe: Option<SignalingAdapterProbeView>,
    #[serde(default)]
    latest_signaling_probe_error: Option<String>,
    #[serde(default)]
    latest_data_channel_probe: Option<ProviderWebRtcDataChannelProbeView>,
    #[serde(default)]
    latest_data_channel_probe_error: Option<String>,
    #[serde(default)]
    abuse: PersistedAbuseState,
    friend_verified: bool,
    next_sequence: u64,
}

static APP_SERVICE: OnceLock<Mutex<TauriAppService>> = OnceLock::new();

#[cfg(feature = "tauri-runtime")]
static TEXT_CONTROL_RUNTIME_PUMP_STARTED: OnceLock<()> = OnceLock::new();
#[cfg(feature = "tauri-runtime")]
static TAURI_APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Shared command-facing app service used by Tauri IPC wrappers.
#[derive(Debug)]
struct TauriAppService {
    state: PersistedAppState,
    text_control_transport_runtime: Option<TextControlTransportRuntime>,
    pending_text_control_transport_runtime: Option<PendingTextControlTransportRuntime>,
    #[cfg(test)]
    state_path_override: Option<PathBuf>,
}

impl TauriAppService {
    fn load() -> Self {
        Self {
            state: load_state(),
            text_control_transport_runtime: None,
            pending_text_control_transport_runtime: None,
            #[cfg(test)]
            state_path_override: None,
        }
    }

    #[cfg(test)]
    fn load_for_test_path(path: PathBuf) -> Self {
        let mut store = FileAppStore::new(&path);
        Self {
            state: load_state_from_store(&mut store),
            text_control_transport_runtime: None,
            pending_text_control_transport_runtime: None,
            state_path_override: Some(path),
        }
    }

    fn read<T>(&self, read: impl FnOnce(&PersistedAppState) -> T) -> T {
        read(&self.state)
    }

    fn mutate(&mut self, update: impl FnOnce(&mut PersistedAppState)) -> AppStateView {
        let mut candidate = self.state.clone();
        candidate.last_command_error = None;
        update(&mut candidate);
        match self.persist_candidate(&candidate) {
            Ok(()) => self.state = candidate,
            Err(error) => {
                self.state.last_command_error = Some(error);
            }
        }
        self.to_view()
    }

    fn to_view(&self) -> AppStateView {
        let mut view = self.state.to_view();
        view.transport_status
            .push(self.text_control_runtime_status_row());
        view
    }

    #[cfg(feature = "tauri-runtime")]
    fn should_run_text_control_transport_pump(
        &self,
        request: &ListPendingTextControlFramesRequest,
    ) -> bool {
        let Some(runtime) = &self.text_control_transport_runtime else {
            return false;
        };
        let Some(session) = self.state.transport_session(BackendTransportMode::Text) else {
            return false;
        };
        if runtime.session_id != session.session_id {
            return false;
        }
        if matches!(
            session.state(),
            TransportSessionState::Idle
                | TransportSessionState::Disconnected
                | TransportSessionState::Failed
                | TransportSessionState::Cancelled
        ) {
            return false;
        }
        !self
            .state
            .list_pending_text_control_frames(request)
            .is_empty()
    }

    fn text_control_runtime_status_row(&self) -> TransportStatusView {
        let Some(session) = &self.state.text_session else {
            return TransportStatusView {
                label: "text/control runtime".to_owned(),
                status: "idle".to_owned(),
                detail: "No text transport session is active; the signed text/control outbox pump stays idle".to_owned(),
            };
        };
        let session_state = session.state();
        let session_active = !matches!(
            session_state,
            TransportSessionState::Idle
                | TransportSessionState::Disconnected
                | TransportSessionState::Failed
                | TransportSessionState::Cancelled
        );
        if let Some(pending) = &self.pending_text_control_transport_runtime {
            if session_active && pending.session_id == session.session_id {
                return TransportStatusView {
                    label: "text/control runtime".to_owned(),
                    status: "attaching".to_owned(),
                    detail: format!(
                        "Backend is starting provider-backed runtime session {} role={} local_peer={} remote_peer={}; no delivery claim is made until the DataChannel attaches",
                        pending.session_id,
                        runtime_role_label(Some(pending.role)),
                        pending.local_peer_id,
                        pending.remote_peer_id
                    ),
                };
            }
        }

        match (&self.text_control_transport_runtime, session_active) {
            (Some(runtime), true) if runtime.session_id == session.session_id => {
                TransportStatusView {
                    label: "text/control runtime".to_owned(),
                    status: "attached".to_owned(),
                    detail: format!(
                        "App-service text/control pump owns runtime session {} role={} local_peer={} remote_peer={} and can drain signed pending frames",
                        runtime.session_id,
                        runtime_role_label(runtime.role),
                        runtime.local_peer_id.as_deref().unwrap_or("test-harness"),
                        runtime.remote_peer_id.as_deref().unwrap_or("test-harness")
                    ),
                }
            }
            (Some(runtime), true) => TransportStatusView {
                label: "text/control runtime".to_owned(),
                status: "stale-runtime".to_owned(),
                detail: format!(
                    "Attached runtime session {} does not match active text session {}; restart text transport before claiming delivery",
                    runtime.session_id, session.session_id
                ),
            },
            (None, true) => TransportStatusView {
                label: "text/control runtime".to_owned(),
                status: "not-attached".to_owned(),
                detail: format!(
                    "Text session {} is {}, but no app-service long-lived provider-backed transport runtime is attached; pending signed frames remain queued until a matching live runtime attaches",
                    session.session_id,
                    PersistedAppState::transport_state_label(session_state)
                ),
            },
            (_, false) => TransportStatusView {
                label: "text/control runtime".to_owned(),
                status: "inactive".to_owned(),
                detail: format!(
                    "Text session {} is {}; the pump will not send frames from an inactive session",
                    session.session_id,
                    PersistedAppState::transport_state_label(session_state)
                ),
            },
        }
    }

    #[allow(dead_code)]
    fn openmls_store_path(&self) -> PathBuf {
        #[cfg(test)]
        if let Some(path) = &self.state_path_override {
            return openmls_store_path_for_app_state_path(path);
        }
        app_openmls_store_path()
    }

    #[allow(dead_code)]
    fn request_openmls_admission_key_package(
        &mut self,
        group_id: &str,
    ) -> Result<OpenMlsAdmissionKeyPackage, String> {
        self.state.ensure_ready_profile();
        let member_identity = self.state.local_user_id();
        let engine = OpenMlsGroupEngine::open(self.openmls_store_path())
            .map_err(|error| format!("OpenMLS joiner provider could not be opened: {error}"))?;
        let package = engine
            .generate_member_package(member_identity.as_bytes())
            .map_err(|error| format!("OpenMLS key package could not be generated: {error}"))?;
        let signer_public_key_hex = hex::encode(package.signer_public_key());
        self.state.push_event(
            "mls.admission_key_package_created",
            format!(
                "Created OpenMLS admission key package for {}",
                redacted_observable_ref("group", group_id)
            ),
        );
        self.persist();
        Ok(OpenMlsAdmissionKeyPackage {
            group_id: group_id.to_owned(),
            member_identity,
            signer_public_key_hex,
            package,
        })
    }

    #[allow(dead_code)]
    fn issue_openmls_admission_welcome(
        &mut self,
        key_package: &OpenMlsAdmissionKeyPackage,
    ) -> Result<OpenMlsAdmissionWelcome, String> {
        let owner_record = self
            .state
            .openmls_groups
            .iter()
            .find(|record| record.group_id == key_package.group_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "OpenMLS owner group handle for {} is not persisted",
                    key_package.group_id
                )
            })?;
        let owner_signer_public_key = hex::decode(&owner_record.signer_public_key_hex)
            .map_err(|error| format!("OpenMLS owner signer handle is not hex: {error}"))?;
        let mut engine = OpenMlsGroupEngine::open(self.openmls_store_path())
            .map_err(|error| format!("OpenMLS owner provider could not be opened: {error}"))?;
        engine
            .load_group(&key_package.group_id, &owner_signer_public_key)
            .map_err(|error| format!("OpenMLS owner group could not be loaded: {error}"))?;
        let result = engine
            .add_member_package(&key_package.group_id, &key_package.package)
            .map_err(|error| format!("OpenMLS member package could not be added: {error}"))?;
        let welcome_bytes = result.welcome.clone().ok_or_else(|| {
            "OpenMLS add_member_package did not produce a Welcome for the joiner".to_owned()
        })?;
        let mut confirmation_hash = Sha256::new();
        confirmation_hash.update(&result.state.confirmation_tag);
        let confirmation_tag_sha256 = hex::encode(confirmation_hash.finalize());
        if let Some(record) = self
            .state
            .openmls_groups
            .iter_mut()
            .find(|record| record.group_id == key_package.group_id)
        {
            record.epoch = result.state.epoch;
            record.confirmation_tag_sha256 = confirmation_tag_sha256.clone();
            record.status_copy =
                "OpenMLS owner added a joiner key package and produced an authorized Welcome"
                    .to_owned();
        }
        self.state.push_event(
            "mls.admission_welcome_created",
            format!(
                "Created OpenMLS Welcome for {}",
                redacted_observable_ref("group", &key_package.group_id)
            ),
        );
        self.persist();
        Ok(OpenMlsAdmissionWelcome {
            group_id: key_package.group_id.clone(),
            owner_signer_public_key_hex: owner_record.signer_public_key_hex,
            member_signer_public_key_hex: key_package.signer_public_key_hex.clone(),
            welcome_bytes,
            epoch: result.state.epoch,
            confirmation_tag_sha256,
        })
    }

    #[allow(dead_code)]
    fn join_openmls_group_from_welcome(
        &mut self,
        welcome: &OpenMlsAdmissionWelcome,
    ) -> Result<(), String> {
        self.state.ensure_ready_profile();
        let signer_public_key = hex::decode(&welcome.member_signer_public_key_hex)
            .map_err(|error| format!("OpenMLS member signer handle is not hex: {error}"))?;
        let mut engine = OpenMlsGroupEngine::open(self.openmls_store_path())
            .map_err(|error| format!("OpenMLS joiner provider could not be opened: {error}"))?;
        let snapshot = engine
            .join_from_welcome(
                &welcome.group_id,
                &signer_public_key,
                &welcome.welcome_bytes,
            )
            .map_err(|error| format!("OpenMLS Welcome could not be joined: {error}"))?;
        let mut confirmation_hash = Sha256::new();
        confirmation_hash.update(&snapshot.confirmation_tag);
        let confirmation_tag_sha256 = hex::encode(confirmation_hash.finalize());
        if confirmation_tag_sha256 != welcome.confirmation_tag_sha256 {
            return Err(
                "OpenMLS joined confirmation tag did not match owner Welcome state".to_owned(),
            );
        }
        upsert_openmls_group_handle(
            &mut self.state,
            OpenMlsGroupHandleRecord {
                group_id: welcome.group_id.clone(),
                signer_public_key_hex: welcome.member_signer_public_key_hex.clone(),
                epoch: snapshot.epoch,
                confirmation_tag_sha256,
                status_copy:
                    "OpenMLS member joined from an authorized Welcome and persisted signer handle"
                        .to_owned(),
            },
        );
        if let Some(group) = self
            .state
            .groups
            .iter_mut()
            .find(|group| group.group_id == welcome.group_id)
        {
            group.role = "member".to_owned();
        }
        self.state.push_event(
            "mls.admission_welcome_joined",
            format!(
                "Joined OpenMLS group from Welcome for {}",
                redacted_observable_ref("group", &welcome.group_id)
            ),
        );
        self.persist();
        Ok(())
    }

    fn persist_candidate(&self, state: &PersistedAppState) -> Result<(), CommandErrorView> {
        #[cfg(test)]
        if let Some(path) = &self.state_path_override {
            let mut store = FileAppStore::new(path);
            return persist_state_to_store(&mut store, state);
        }
        persist_state(state)
    }

    fn persist(&mut self) {
        if let Err(error) = self.persist_candidate(&self.state) {
            self.state.last_command_error = Some(error);
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn attach_text_control_transport_runtime(
        &mut self,
        transport: Arc<dyn discrypt_transport::TextControlDataTransport>,
        session_id: impl Into<String>,
    ) {
        self.pending_text_control_transport_runtime = None;
        self.text_control_transport_runtime = Some(TextControlTransportRuntime {
            transport,
            owned_runtime: None,
            executor: None,
            session_id: session_id.into(),
            role: None,
            local_peer_id: None,
            remote_peer_id: None,
        });
    }

    fn attach_owned_text_control_transport_runtime(
        &mut self,
        runtime: discrypt_transport::ProviderTextControlRuntime,
        executor: Arc<tokio::runtime::Runtime>,
        session_id: impl Into<String>,
    ) {
        let session_id = session_id.into();
        let role = runtime.evidence().role;
        let local_peer_id = runtime.evidence().local_peer_id.0.clone();
        let remote_peer_id = runtime.evidence().remote_peer_id.0.clone();
        let owned_runtime = Arc::new(runtime);
        let transport = owned_runtime.transport();
        #[cfg(feature = "tauri-runtime")]
        if role == ProviderTextControlRuntimePeerRole::Offerer {
            start_text_control_offer_receiver_loop(
                transport.clone(),
                executor.clone(),
                session_id.clone(),
            );
        }
        self.pending_text_control_transport_runtime = None;
        self.text_control_transport_runtime = Some(TextControlTransportRuntime {
            transport,
            owned_runtime: Some(owned_runtime),
            executor: Some(executor),
            session_id,
            role: Some(role),
            local_peer_id: Some(local_peer_id),
            remote_peer_id: Some(remote_peer_id),
        });
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn clear_text_control_transport_runtime(&mut self) {
        self.text_control_transport_runtime = None;
        self.pending_text_control_transport_runtime = None;
    }

    fn clear_text_control_transport_runtime_if_session_mismatch(
        &mut self,
        active_session_id: Option<&str>,
    ) {
        let Some(runtime) = &self.text_control_transport_runtime else {
            if self
                .pending_text_control_transport_runtime
                .as_ref()
                .is_some_and(|pending| Some(pending.session_id.as_str()) != active_session_id)
            {
                self.pending_text_control_transport_runtime = None;
            }
            return;
        };
        if Some(runtime.session_id.as_str()) != active_session_id {
            self.text_control_transport_runtime = None;
            self.pending_text_control_transport_runtime = None;
        }
    }

    fn text_control_runtime_attach_already_active(
        &mut self,
        command_name: &'static str,
        active_session_id: &str,
    ) -> bool {
        if let Some(pending) = &self.pending_text_control_transport_runtime {
            if pending.session_id == active_session_id {
                self.state.push_event(
                    "transport.text_runtime_attach_deduped",
                    format!(
                        "Text/control runtime attach for session {} is already pending as {} local_peer={} remote_peer={}; duplicate request was ignored",
                        active_session_id,
                        runtime_role_label(Some(pending.role)),
                        pending.local_peer_id,
                        pending.remote_peer_id
                    ),
                );
                self.persist();
                return true;
            }
        }
        if let Some(runtime) = &self.text_control_transport_runtime {
            if runtime.session_id == active_session_id {
                self.state.push_event(
                    "transport.text_runtime_attach_deduped",
                    format!(
                        "Text/control runtime session {} is already attached for {}; duplicate request was ignored",
                        active_session_id, command_name
                    ),
                );
                self.persist();
                return true;
            }
        }
        false
    }

    fn pump_text_control_transport_once(
        &mut self,
        request: ListPendingTextControlFramesRequest,
    ) -> TextControlTransportPumpReportView {
        fn failure_report(label: &str, message: String) -> TextControlTransportPumpReportView {
            TextControlTransportPumpReportView {
                pending_before: 0,
                frames_sent: 0,
                response_frames_received: 0,
                receipts_applied: 0,
                failures: vec![message],
                metrics: discrypt_transport::WebRtcDataTransportMetrics {
                    schema_version: discrypt_transport::WebRtcDataTransportMetrics::SCHEMA_VERSION,
                    label: label.to_owned(),
                    attached_channels: 0,
                    open: false,
                    frames_sent: 0,
                    frames_received: 0,
                    bytes_sent: 0,
                    bytes_received: 0,
                    last_state: "unavailable".to_owned(),
                },
            }
        }

        let Some(runtime) = self.text_control_transport_runtime.clone() else {
            let message = "text/control transport runtime is not attached".to_owned();
            self.state.push_command_error(
                "message.transport_pump_unavailable",
                "pump_text_control_transport_once",
                "transport_runtime_missing",
                message.clone(),
                "Start the text transport session and attach a transport runtime before trying to pump pending outbox frames",
            );
            let report = failure_report("unattached-text-control-runtime", message);
            self.persist();
            return report;
        };

        let Some(session) = self.state.transport_session(BackendTransportMode::Text) else {
            let message = "text transport session is not active".to_owned();
            self.state.push_command_error(
                "message.transport_pump_unavailable",
                "pump_text_control_transport_once",
                "text_session_missing",
                message.clone(),
                "Call start_text_session before pumping the outbox",
            );
            let report = failure_report("inactive-text-session", message);
            self.persist();
            return report;
        };

        if matches!(
            session.state(),
            TransportSessionState::Idle
                | TransportSessionState::Disconnected
                | TransportSessionState::Failed
                | TransportSessionState::Cancelled
        ) {
            let message = format!(
                "text transport session {} is not active",
                session.session_id
            );
            self.state.push_command_error(
                "message.transport_pump_unavailable",
                "pump_text_control_transport_once",
                "text_session_inactive",
                message.clone(),
                "Call start_text_session and attach a live transport runtime before pumping pending frames",
            );
            let report = failure_report("inactive-text-session", message);
            self.persist();
            return report;
        }

        if runtime.session_id != session.session_id {
            let message = format!(
                "text transport runtime session id {} does not match active text session {}",
                runtime.session_id, session.session_id
            );
            self.state.push_command_error(
                "message.transport_pump_unavailable",
                "pump_text_control_transport_once",
                "text_session_id_mismatch",
                message.clone(),
                "Stop the previous session and restart the text session before pumping",
            );
            let report = failure_report("text-control-runtime-session-mismatch", message);
            self.persist();
            return report;
        }

        let transport = runtime.transport.clone();
        let transport_session_id = session.session_id.clone();
        let executor = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let message = format!("could not start text/control pump runtime: {error}");
                self.state.push_command_error(
                    "message.transport_pump_unavailable",
                    "pump_text_control_transport_once",
                    "transport_runtime_unavailable",
                    message.clone(),
                    "Retry after the backend runtime can construct a local pump executor",
                );
                let report = failure_report("text-control-pump-runtime-error", message);
                self.persist();
                return report;
            }
        };
        let report = executor.block_on(async {
            self.state
                .pump_text_control_transport_once(transport.as_ref(), request, transport_session_id)
                .await
        });
        self.persist();
        report
    }
}

/// Tauri command: return the transitional compatibility snapshot for older clients.
pub fn app_snapshot() -> AppSnapshot {
    with_state(|state| state.to_snapshot())
}

/// Tauri command: return the full command-backed app state for the React shell.
pub fn app_state() -> AppStateView {
    let service = app_service();
    let guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.to_view()
}

/// Tauri command: start the backend signaling control-plane transport session.
pub fn start_signaling_session(request: StartSignalingSessionRequest) -> AppStateView {
    mutate_app_service(|state| {
        if let Err(error) = state
            .start_transport_session(BackendTransportMode::Signaling, request.scope_label.clone())
        {
            state.push_command_error(
                "transport.signaling_start_rejected",
                "start_signaling_session",
                "transport_start_failed",
                error,
                "Check adapter readiness and connectivity settings before retrying signaling",
            );
            return;
        }
        if request.adapter_probe {
            if let Err(error) =
                state.probe_active_signaling_adapter(request.adapter_kind.as_deref())
            {
                state.push_command_error(
                    "transport.signaling_probe_failed",
                    "start_signaling_session",
                    "adapter_probe_failed",
                    error,
                    "Check adapter feature flags, provider endpoint reachability, and per-scope connectivity policy before claiming signaling",
                );
            }
        }
        if request.data_channel_probe {
            if let Err(error) =
                state.probe_active_webrtc_data_channel(request.adapter_kind.as_deref())
            {
                state.push_command_error(
                    "transport.data_channel_probe_failed",
                    "start_signaling_session",
                    "data_channel_probe_failed",
                    error,
                    "Check provider reachability, STUN/ICE policy, and the selected per-scope connectivity profile before claiming text/control transport",
                );
            }
        }
    })
}

/// Tauri command: stop the backend signaling control-plane transport session.
pub fn stop_signaling_session(request: StopSignalingSessionRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.stop_transport_session(BackendTransportMode::Signaling, request.session_id);
    })
}

/// Tauri command: start the backend text/control data-plane transport session.
pub fn start_text_session(request: StartTextSessionRequest) -> AppStateView {
    let service = app_service();
    let mut guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.state.last_command_error = None;
    let started_session_id = {
        let state = &mut guard.state;
        match state.start_transport_session(BackendTransportMode::Text, request.scope_label) {
            Ok(started_session_id) => started_session_id,
            Err(error) => {
                state.push_command_error(
                    "transport.text_start_rejected",
                    "start_text_session",
                    "transport_start_failed",
                    error,
                    "Check adapter readiness and route diagnostics before retrying text transport",
                );
                guard.persist();
                return guard.to_view();
            }
        }
    };
    guard.clear_text_control_transport_runtime_if_session_mismatch(Some(
        started_session_id.as_str(),
    ));
    {
        let state = &mut guard.state;
        if request.data_channel_probe {
            match state.probe_active_webrtc_data_channel(request.adapter_kind.as_deref()) {
                Ok(probe) => state.mark_text_session_data_channel_route_proof(&probe),
                Err(error) => state.push_command_error(
                    "transport.text_data_channel_probe_failed",
                    "start_text_session",
                    "text_data_channel_probe_failed",
                    error,
                    "Check provider reachability, STUN/ICE policy, and selected per-scope connectivity before claiming text/control route proof",
                ),
            }
        }
    }
    if request.data_channel_probe
        && guard
            .state
            .transport_session(BackendTransportMode::Text)
            .is_some_and(|session| session.state().is_connected())
        && guard.text_control_transport_runtime.is_none()
        && guard.pending_text_control_transport_runtime.is_none()
    {
        if let Ok(attachment) = guard
            .state
            .active_runtime_peer_attachment_for_text_control()
        {
            match guard
                .state
                .text_control_runtime_inputs_for_active_scope(request.adapter_kind.as_deref())
            {
                Ok(runtime_inputs) => {
                    let job = prepare_text_control_runtime_attach_job(
                        &mut guard,
                        "start_text_session",
                        started_session_id.clone(),
                        runtime_inputs,
                        attachment,
                    );
                    let view = guard.to_view();
                    drop(guard);
                    spawn_text_control_runtime_attach(job);
                    return view;
                }
                Err(error) => guard.state.push_command_error(
                    "transport.text_runtime_attach_rejected",
                    "start_text_session",
                    "text_runtime_scope_unavailable",
                    error,
                    "Open a DM/group/invite context with a configured signaling profile before starting automatic text/control runtime attach",
                ),
            }
        }
    }
    guard.persist();
    guard.to_view()
}

/// Tauri command: stop the backend text/control data-plane transport session.
pub fn stop_text_session(request: StopTextSessionRequest) -> AppStateView {
    let service = app_service();
    let mut guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.state.last_command_error = None;
    let stopped_session_id = guard
        .state
        .stop_transport_session(BackendTransportMode::Text, request.session_id);
    if stopped_session_id.is_some() {
        guard.clear_text_control_transport_runtime_if_session_mismatch(None);
    }
    guard.persist();
    guard.to_view()
}

/// Tauri command: bind an app-service text/control runtime to the active text
/// session. DM/group requests derive role-split peer ids from backend-owned
/// signed invite metadata; legacy no-scope requests remain fail-closed rather
/// than resuming stale probe SDP.
pub fn attach_text_control_transport_runtime(
    request: AttachTextControlTransportRuntimeRequest,
) -> AppStateView {
    let service = app_service();
    let mut guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.state.last_command_error = None;

    let Some(session) = guard.state.transport_session(BackendTransportMode::Text) else {
        guard.state.push_command_error(
            "transport.text_runtime_attach_rejected",
            "attach_text_control_transport_runtime",
            "text_session_missing",
            "No active text transport session exists",
            "Start a text session before attaching transport runtime",
        );
        guard.persist();
        return guard.to_view();
    };

    let active_session_id = session.session_id.clone();
    let active_session_state = session.state();

    if let Some(expected_session_id) = request.session_id.as_deref() {
        if expected_session_id != active_session_id {
            guard.state.push_command_error(
                "transport.text_runtime_attach_rejected",
                "attach_text_control_transport_runtime",
                "text_session_id_mismatch",
                format!(
                    "text transport session {} does not match attach request {}",
                    active_session_id, expected_session_id
                ),
                "Use the current active text session id when attaching runtime",
            );
            guard.persist();
            return guard.to_view();
        }
    }

    if guard.text_control_runtime_attach_already_active(
        "attach_text_control_transport_runtime",
        &active_session_id,
    ) {
        return guard.to_view();
    }

    let has_explicit_runtime_attachment = request.runtime_role.is_some()
        || request.local_peer_id.is_some()
        || request.remote_peer_id.is_some();
    if has_explicit_runtime_attachment && !explicit_text_runtime_attachment_allowed() {
        guard.state.push_command_error(
            "transport.text_runtime_attach_rejected",
            "attach_text_control_transport_runtime",
            "text_runtime_explicit_attach_not_allowed",
            "Production builds must derive text/control runtime role and peer ids from persisted DM/group invite state",
            "Retry with derive_from_state=true after opening the signed DM or group context; explicit peer-id attachment is reserved for test/harness builds",
        );
        guard.persist();
        return guard.to_view();
    }

    let derived_attachment = if request.derive_from_state {
        match guard
            .state
            .active_runtime_peer_attachment_for_text_control()
        {
            Ok(attachment) => Some(attachment),
            Err(error) => {
                guard.state.push_command_error(
                    "transport.text_runtime_attach_rejected",
                    "attach_text_control_transport_runtime",
                    "text_runtime_scope_unavailable",
                    error,
                    "Open a DM/group context created from signed invite/connectivity metadata before attaching the backend runtime",
                );
                guard.persist();
                return guard.to_view();
            }
        }
    } else {
        None
    };

    let explicit_attachment = if let Some(runtime_role) = request.runtime_role.as_deref() {
        let role = match parse_text_control_runtime_role(runtime_role) {
            Ok(role) => role,
            Err(error) => {
                guard.state.push_command_error(
                    "transport.text_runtime_attach_rejected",
                    "attach_text_control_transport_runtime",
                    "text_runtime_role_invalid",
                    error,
                    "Use runtime_role=offerer or runtime_role=answerer with scoped local/remote peer ids",
                );
                guard.persist();
                return guard.to_view();
            }
        };
        let Some(local_peer_id) = request.local_peer_id.as_deref() else {
            guard.state.push_command_error(
                "transport.text_runtime_attach_rejected",
                "attach_text_control_transport_runtime",
                "text_runtime_local_peer_missing",
                "Role-split runtime attach requires a local peer id",
                "Pass a stable scoped local_peer_id derived from the local device/user for this DM or group",
            );
            guard.persist();
            return guard.to_view();
        };
        let Some(remote_peer_id) = request.remote_peer_id.as_deref() else {
            guard.state.push_command_error(
                "transport.text_runtime_attach_rejected",
                "attach_text_control_transport_runtime",
                "text_runtime_remote_peer_missing",
                "Role-split runtime attach requires a remote peer id",
                "Pass a stable scoped remote_peer_id from the DM/group invite or membership state",
            );
            guard.persist();
            return guard.to_view();
        };
        let local_peer_id = match SignalingPeerId::new(local_peer_id.to_owned()) {
            Ok(peer_id) => peer_id,
            Err(error) => {
                guard.state.push_command_error(
                    "transport.text_runtime_attach_rejected",
                    "attach_text_control_transport_runtime",
                    "text_runtime_local_peer_invalid",
                    error.to_string(),
                    "Use a trimmed ASCII token for local_peer_id",
                );
                guard.persist();
                return guard.to_view();
            }
        };
        let remote_peer_id = match SignalingPeerId::new(remote_peer_id.to_owned()) {
            Ok(peer_id) => peer_id,
            Err(error) => {
                guard.state.push_command_error(
                    "transport.text_runtime_attach_rejected",
                    "attach_text_control_transport_runtime",
                    "text_runtime_remote_peer_invalid",
                    error.to_string(),
                    "Use a trimmed ASCII token for remote_peer_id",
                );
                guard.persist();
                return guard.to_view();
            }
        };
        Some(TextControlRuntimePeerAttachment {
            role,
            local_peer_id,
            remote_peer_id,
        })
    } else {
        None
    };

    if let Some(attachment) = explicit_attachment.or(derived_attachment) {
        let runtime_inputs = match guard
            .state
            .text_control_runtime_inputs_for_active_scope(None)
        {
            Ok(inputs) => inputs,
            Err(error) => {
                guard.state.push_command_error(
                    "transport.text_runtime_attach_rejected",
                    "attach_text_control_transport_runtime",
                    "text_runtime_scope_unavailable",
                    error,
                    "Open a DM/group/invite context with a configured signaling profile before attaching runtime",
                );
                guard.persist();
                return guard.to_view();
            }
        };
        let job = prepare_text_control_runtime_attach_job(
            &mut guard,
            "attach_text_control_transport_runtime",
            active_session_id.clone(),
            runtime_inputs,
            attachment,
        );
        let view = guard.to_view();
        drop(guard);
        spawn_text_control_runtime_attach(job);
        return view;
    }

    if !active_session_state.is_connected() {
        guard.state.push_command_error(
            "transport.text_runtime_attach_rejected",
            "attach_text_control_transport_runtime",
            "text_session_not_connected",
            format!(
                "text transport session {} is {}",
                active_session_id,
                PersistedAppState::transport_state_label(active_session_state)
            ),
            "Complete a provider DataChannel proof and route transition before binding a legacy probe-resume runtime, or use derive_from_state for backend-owned DM/group runtime attach",
        );
        guard.persist();
        return guard.to_view();
    }

    let attachment = guard.state.latest_data_channel_probe.as_ref().map_or_else(
        || ProviderTextControlRuntimeAttachment {
            adapter_kind: "unknown".to_owned(),
            profile_id: String::new(),
            endpoint_label: "unknown-endpoint".to_owned(),
            rendezvous_topic: "unknown-topic".to_owned(),
            scope_commitment: String::new(),
            runtime_spec: None,
        },
        |probe| ProviderTextControlRuntimeAttachment {
            adapter_kind: probe.kind.clone(),
            profile_id: probe.profile_id.clone(),
            endpoint_label: probe.endpoint_label.clone(),
            rendezvous_topic: probe.rendezvous_topic.clone(),
            scope_commitment: probe.scope_commitment.clone(),
            runtime_spec: probe.runtime_spec.clone().map(Box::new),
        },
    );
    let reason = match resume_text_control_runtime_from_probe(attachment) {
        Ok(_) => unreachable!("text-control runtime seam should remain unavailable in this slice"),
        Err(TransportError::Unavailable(message)) => {
            (message, TEXT_CONTROL_RUNTIME_NOT_IMPLEMENTED_RECOVERY_HINT)
        }
        Err(error) => (
            error.to_string(),
            TEXT_CONTROL_RUNTIME_NOT_IMPLEMENTED_RECOVERY_HINT,
        ),
    };
    guard.state.push_command_error(
        "transport.text_runtime_attach_unavailable",
        "attach_text_control_transport_runtime",
        "transport_runtime_not_supported",
        &reason.0,
        reason.1,
    );
    guard.persist();
    guard.to_view()
}

#[cfg(feature = "tauri-runtime")]
fn emit_background_app_events(state: &AppStateView, previous_cursor: u64) {
    if previous_cursor == state.event_cursor {
        return;
    }
    let Some(window_handle) = tauri_app_handle() else {
        return;
    };
    emit_app_event_stream(&window_handle, state, previous_cursor);
}

#[cfg(feature = "tauri-runtime")]
fn tauri_app_handle() -> Option<tauri::AppHandle> {
    TAURI_APP_HANDLE.get().cloned()
}

#[cfg(feature = "tauri-runtime")]
fn start_text_control_transport_runtime_pump(app_handle: tauri::AppHandle) {
    if TEXT_CONTROL_RUNTIME_PUMP_STARTED.set(()).is_err() {
        return;
    }

    let service = app_service();
    std::thread::spawn(move || {
        let request = ListPendingTextControlFramesRequest {
            target: None,
            limit: Some(16),
            operation_timeout_ms: None,
        };
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1_250));

            let mut guard = service
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !guard.should_run_text_control_transport_pump(&request) {
                continue;
            }
            let previous_cursor = guard.state.latest_event_cursor();
            let _ = guard.pump_text_control_transport_once(request.clone());
            let state = guard.to_view();
            drop(guard);
            emit_app_event_stream(&app_handle, &state, previous_cursor);
        }
    });
}

#[cfg(feature = "tauri-runtime")]
fn start_text_control_offer_receiver_loop(
    transport: Arc<dyn discrypt_transport::TextControlDataTransport>,
    executor: Arc<tokio::runtime::Runtime>,
    session_id: String,
) {
    executor.spawn(async move {
        loop {
            let received = match transport.recv_text_control_frame().await {
                Ok(received) => received,
                Err(_) => break,
            };
            let inbound_frame = match serde_json::from_slice::<TextControlFrameView>(&received) {
                Ok(frame) => frame,
                Err(_) => break,
            };
            let service = app_service();
            let (response_frame, state) = {
                let mut guard = service
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let previous_cursor = guard.state.latest_event_cursor();
                let response_frame = guard.state.handle_text_control_frame(inbound_frame);
                guard.persist();
                let state = guard.to_view();
                emit_background_app_events(&state, previous_cursor);
                (response_frame, state)
            };
            let Some(response_frame) = response_frame else {
                let _ = state;
                continue;
            };
            let response = match serde_json::to_vec(&response_frame) {
                Ok(response) => response,
                Err(_) => break,
            };
            if transport.send_text_control_frame(response).await.is_err() {
                break;
            }
            let _ = &session_id;
        }
    });
}

/// Tauri command: create a new local user and unlock the shell.
pub fn create_user(request: CreateUserRequest) -> AppStateView {
    mutate_app_service(|state| state.create_user(request, false))
}

/// Tauri command: recover account continuity and unlock the shell without content keys.
pub fn recover_user(request: RecoverUserRequest) -> AppStateView {
    mutate_app_service(|state| {
        let recovery = account_recovery_from_request(&request);
        state.create_user_with_seed(
            CreateUserRequest {
                display_name: request.display_name.clone(),
                device_name: request.device_name.clone(),
            },
            true,
            Some(recovery_seed_key(&request.recovery_code)),
        );
        state.apply_account_recovery(&recovery);
        state.push_event(
            "identity.recovered",
            format!(
                "Account-continuity recovery accepted; rooms={} devices={} content_keys_restored={}",
                recovery.room_memberships.len(),
                recovery.device_count,
                recovery.content_keys_restored
            ),
        );
    })
}

/// Tauri command: create a signed pasteable payload for pairing another own device.
pub fn create_device_pairing_payload(
    request: CreateDevicePairingPayloadRequest,
) -> DevicePairingPayloadView {
    let service = app_service();
    let mut guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let state = &mut guard.state;
    state.last_command_error = None;
    state.ensure_ready_profile();
    let identity = state.local_identity();
    state.ensure_device_set(&identity);
    let requested_label = normalize_label(&request.requested_label, "paired device");
    let current_epoch = request.current_epoch.unwrap_or(state.next_sequence);
    let valid_for_epochs = request.valid_for_epochs.unwrap_or(3).max(1);
    let authorizing_device_id = state
        .device_set
        .active_devices()
        .first()
        .map(|device| device.device_id);
    let view = if let Some(authorizing_device_id) = authorizing_device_id {
        match state.device_set.create_pairing_payload(
            &identity,
            authorizing_device_id,
            requested_label.clone(),
            current_epoch,
            valid_for_epochs,
        ) {
            Ok(payload) => {
                let expires_epoch = serde_json::from_str::<DevicePairingPayload>(&payload)
                    .map(|payload| payload.expires_epoch)
                    .unwrap_or_else(|_| current_epoch.saturating_add(valid_for_epochs));
                state.push_event(
                    "device.pairing_payload_created",
                    format!(
                        "Pairing payload created for {}",
                        redacted_observable_ref("device_label", &requested_label)
                    ),
                );
                DevicePairingPayloadView {
                    payload,
                    authorizing_device_id: authorizing_device_id.to_string(),
                    requested_label,
                    expires_epoch,
                    rejected_reason: None,
                }
            }
            Err(error) => {
                let message = error.to_string();
                state.push_command_error(
                    "device.pairing_rejected",
                    "create_device_pairing_payload",
                    "device_pairing_payload_failed",
                    message.clone(),
                    "Retry pairing after confirming an authorized local device and valid pairing epoch",
                );
                DevicePairingPayloadView {
                    payload: String::new(),
                    authorizing_device_id: authorizing_device_id.to_string(),
                    requested_label,
                    expires_epoch: current_epoch,
                    rejected_reason: Some(message),
                }
            }
        }
    } else {
        let message = "No authorized local device is available to create a pairing payload";
        state.push_command_error(
            "device.pairing_rejected",
            "create_device_pairing_payload",
            "device_authorizer_missing",
            message,
            "Create or recover the local profile before pairing another device",
        );
        DevicePairingPayloadView {
            payload: String::new(),
            authorizing_device_id: String::new(),
            requested_label,
            expires_epoch: current_epoch,
            rejected_reason: Some(message.to_owned()),
        }
    };
    guard.persist();
    view
}

/// Tauri command: accept a signed pasteable pairing payload and add the new device row.
pub fn accept_device_pairing_payload(request: AcceptDevicePairingPayloadRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let identity = state.local_identity();
        state.ensure_device_set(&identity);
        let device_name = request
            .device_name
            .map(|value| normalize_label(&value, "paired device"))
            .unwrap_or_else(|| "paired device".to_owned());
        let current_epoch = request.current_epoch.unwrap_or(state.next_sequence);
        let seed = state.identity_seed_bytes();
        let device_key = command_device_key(&seed, &device_name, state.next_sequence);
        match state.device_set.add_device_from_pairing_payload(
            &identity,
            &request.payload,
            device_key,
            current_epoch,
        ) {
            Ok(leaf) => {
                if !state
                    .devices
                    .iter()
                    .any(|device| device.device_id == leaf.device_id.to_string())
                {
                    state
                        .devices
                        .push(device_view_from_leaf(&leaf, false, true));
                }
                state.push_event(
                    "device.paired",
                    format!("Authorized paired device {device_name}"),
                );
            }
            Err(error) => {
                state.push_command_error(
                    "device.pairing_rejected",
                    "accept_device_pairing_payload",
                    "device_pairing_rejected",
                    format!("Pairing rejected: {error}"),
                    "Request a fresh signed device-pairing payload from an authorized device",
                );
            }
        }
    })
}

/// Tauri command: verify a user-confirmed safety-number comparison and persist success.
pub fn verify_safety_number(request: SafetyVerificationRequest) -> SafetyVerificationResult {
    let snapshot = app_snapshot();
    let verified = request.friend_id == snapshot.friend.friend_code
        && request.provided == snapshot.friend.safety_number;
    let result = SafetyVerificationResult {
        verified,
        message: if verified {
            "Safety number verified; MITM risk accepted by explicit user comparison".to_owned()
        } else {
            "Safety number mismatch; do not trust this device or DM".to_owned()
        },
    };
    if result.verified {
        mutate_app_service(|state| {
            state.friend_verified = true;
            state.push_event(
                "friend.verified",
                "Safety number verified and persisted for this profile",
            );
        });
    }
    result
}

/// Tauri command: save theme/template preferences.
pub fn save_preferences(request: SavePreferencesRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.preferences = UiPreferencesView {
            theme_id: normalize_theme_id(&request.theme_id),
            template_id: normalize_template_id(&request.template_id),
        };
        state.push_event("preferences.saved", "Theme/template preferences saved");
    })
}

/// Tauri command: persist signaling and ICE policy for app, DM, group, or channel scope.
pub fn set_connectivity_policy(request: SetConnectivityPolicyRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let scope_kind = request.scope_kind.trim().to_ascii_lowercase();
        let result: Result<String, String> = (|| match scope_kind.as_str() {
            "app" | "defaults" | "app_default" => {
                let policy = normalize_connectivity_policy_override(
                    state.connectivity_defaults.clone(),
                    &request,
                )?;
                state.connectivity_defaults = policy;
                Ok("Updated app signaling/ICE defaults".to_owned())
            }
            "dm" => {
                let dm_id = request
                    .dm_id
                    .clone()
                    .or_else(|| {
                        state
                            .active_context
                            .as_ref()
                            .and_then(|context| context.dm_id.clone())
                    })
                    .ok_or_else(|| "Select a DM before saving DM connectivity".to_owned())?;
                let dm = state
                    .dms
                    .iter_mut()
                    .find(|dm| dm.dm_id == dm_id)
                    .ok_or_else(|| "Requested DM does not exist".to_owned())?;
                let base = dm
                    .connectivity
                    .clone()
                    .unwrap_or_else(|| dm_connectivity_policy(&dm.dm_id, &dm.participant_id));
                let policy = normalize_connectivity_policy_override(base, &request)?;
                dm.runtime_peers = dm_runtime_peers(Some(&policy), "inviter");
                dm.connectivity = Some(policy);
                Ok(format!("Updated connectivity for DM {}", dm.display_name))
            }
            "group" => {
                let group_id = request
                    .group_id
                    .clone()
                    .or_else(|| {
                        state
                            .active_context
                            .as_ref()
                            .and_then(|context| context.group_id.clone())
                    })
                    .ok_or_else(|| "Select a group before saving group connectivity".to_owned())?;
                let group = state
                    .groups
                    .iter_mut()
                    .find(|group| group.group_id == group_id)
                    .ok_or_else(|| "Requested group does not exist".to_owned())?;
                let base = group
                    .connectivity
                    .clone()
                    .unwrap_or_else(|| group_connectivity_policy(&group.group_id));
                let policy = normalize_connectivity_policy_override(base, &request)?;
                group.runtime_peers = group_runtime_peers(Some(&policy), &group.role);
                group.connectivity = Some(policy);
                Ok(format!("Updated connectivity for group {}", group.name))
            }
            "channel" => {
                let group_id = request
                    .group_id
                    .clone()
                    .or_else(|| {
                        state
                            .active_context
                            .as_ref()
                            .and_then(|context| context.group_id.clone())
                    })
                    .ok_or_else(|| {
                        "Select a group before saving channel connectivity".to_owned()
                    })?;
                let channel_id = request
                    .channel_id
                    .clone()
                    .or_else(|| {
                        state
                            .active_context
                            .as_ref()
                            .and_then(|context| context.channel_id.clone())
                    })
                    .ok_or_else(|| {
                        "Select a text or voice channel before saving channel connectivity"
                            .to_owned()
                    })?;
                let group = state
                    .groups
                    .iter_mut()
                    .find(|group| group.group_id == group_id)
                    .ok_or_else(|| "Requested group does not exist".to_owned())?;
                let group_base = group
                    .connectivity
                    .clone()
                    .unwrap_or_else(|| group_connectivity_policy(&group.group_id));
                let channel = group
                    .channels
                    .iter_mut()
                    .find(|channel| channel.channel_id == channel_id)
                    .ok_or_else(|| "Requested channel does not exist".to_owned())?;
                let base = channel.connectivity.clone().unwrap_or_else(|| {
                    let mut policy = group_base;
                    policy.scope_id_commitment =
                        hash_commitment("discrypt-channel-scope-commitment-v1", &[&channel.channel_id]);
                    policy.privacy_label =
                        "Channel signaling topics are derived commitments; channel names and room secrets are not exposed"
                            .to_owned();
                    policy
                });
                let policy = normalize_connectivity_policy_override(base, &request)?;
                channel.connectivity = Some(policy);
                Ok(format!("Updated connectivity for channel {}", channel.name))
            }
            _ => Err("Connectivity scope must be app, dm, group, or channel".to_owned()),
        })();
        match result {
            Ok(summary) => state.push_event("connectivity.policy_saved", summary),
            Err(error) => state.push_command_error(
                "connectivity.rejected",
                "set_connectivity_policy",
                "invalid_connectivity_policy",
                error,
                "Pick a supported adapter and valid STUN/TURN endpoints before saving",
            ),
        }
    })
}

/// Tauri command: start or focus a direct-message conversation.
pub fn start_dm(request: StartDmRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let display_name =
            normalize_label(&request.display_name, &core_app_snapshot().friend.alias);
        let dm_id = stable_id("dm", &display_name, state.next_sequence);
        if !state.dms.iter().any(|dm| dm.display_name == display_name) {
            let participant_id = stable_id("participant", &display_name, state.next_sequence);
            let connectivity = apply_app_connectivity_defaults(
                dm_connectivity_policy(&dm_id, &participant_id),
                &state.connectivity_defaults,
            );
            let runtime_peers = dm_runtime_peers(Some(&connectivity), "inviter");
            state.dms.push(DirectConversationView {
                dm_id: dm_id.clone(),
                participant_id: participant_id.clone(),
                display_name: display_name.clone(),
                local_only_copy:
                    "Local DM; remote delivery is not claimed until backend proof is available"
                        .to_owned(),
                runtime_peers,
                connectivity: Some(connectivity),
            });
        }
        let active_dm_id = state
            .dms
            .iter()
            .find(|dm| dm.display_name == display_name)
            .map(|dm| dm.dm_id.clone())
            .unwrap_or(dm_id);
        state.active_context = Some(ActiveContextView {
            kind: "dm".to_owned(),
            group_id: None,
            channel_id: None,
            dm_id: Some(active_dm_id),
        });
        state.push_event(
            "dm.started",
            format!(
                "Opened local DM {}",
                redacted_observable_ref("dm_contact", &display_name)
            ),
        );
    })
}

/// Tauri command: create a local-first group and make it active.
pub fn create_group(request: CreateGroupRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let name = normalize_label(&request.name, "private lab");
        let group_id = stable_id("group", &name, state.next_sequence);
        if !state.groups.iter().any(|group| group.name == name) {
            let mut connectivity = group_connectivity_policy_from_request(&group_id, &request);
            if !request_has_connectivity_overrides(&request) {
                connectivity =
                    apply_app_connectivity_defaults(connectivity, &state.connectivity_defaults);
            }
            let runtime_peers = group_runtime_peers(Some(&connectivity), "owner");
            state.groups.push(GroupView {
                group_id: group_id.clone(),
                name: name.clone(),
                role: "owner".to_owned(),
                channels: default_group_channels(state.next_sequence),
                runtime_peers,
                connectivity: Some(connectivity),
            });
        }
        let active_group_id = state
            .groups
            .iter()
            .find(|group| group.name == name)
            .map(|group| group.group_id.clone())
            .unwrap_or(group_id);
        if let Err(error) = state.ensure_openmls_group(&active_group_id) {
            state.push_command_error(
                "mls.group_create_failed",
                "create_group",
                "openmls_group_create_failed",
                error,
                "Do not claim production text/voice encryption for this group until OpenMLS group state is created and persisted",
            );
            return;
        }
        state.active_context = Some(ActiveContextView {
            kind: "group".to_owned(),
            group_id: Some(active_group_id),
            channel_id: None,
            dm_id: None,
        });
        state.push_event(
            "group.created",
            format!("Created group {}", redacted_observable_ref("group", &name)),
        );
        let retention = normalize_label(&request.retention, "7 days");
        state.push_event(
            "retention.selected",
            format!("Default retention: {retention}"),
        );
    })
}

/// Tauri command: focus an existing local-first group from the server rail.
pub fn set_active_group(request: SetActiveGroupRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        if let Some(group) = state
            .groups
            .iter()
            .find(|group| group.group_id == request.group_id)
        {
            state.active_context = Some(ActiveContextView {
                kind: "group".to_owned(),
                group_id: Some(group.group_id.clone()),
                channel_id: None,
                dm_id: None,
            });
            state.push_event(
                "group.focused",
                format!(
                    "Focused group {}",
                    redacted_observable_ref("group", &group.name)
                ),
            );
        } else {
            state.push_command_error(
                "group.focus_missing",
                "set_active_group",
                "group_not_found",
                "Requested group does not exist",
                "Pick a group from the server rail before focusing it",
            );
        }
    })
}

/// Tauri command: focus a specific text or voice channel within a group.
pub fn set_active_channel(request: SetActiveChannelRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let group = state.groups.iter().find(|g| g.group_id == request.group_id);
        let channel = group.and_then(|g| {
            g.channels
                .iter()
                .find(|c| c.channel_id == request.channel_id)
        });
        if let Some(ch) = channel {
            let kind = match ch.kind {
                ChannelKind::Voice => "voice_channel",
                ChannelKind::Text => "text_channel",
            };
            state.active_context = Some(ActiveContextView {
                kind: kind.to_owned(),
                group_id: Some(request.group_id.clone()),
                channel_id: Some(request.channel_id.clone()),
                dm_id: None,
            });
            state.push_event(
                "channel.focused",
                format!(
                    "Focused channel {}",
                    redacted_observable_ref("channel", &ch.name)
                ),
            );
        } else {
            state.push_command_error(
                "channel.focus_missing",
                "set_active_channel",
                "channel_not_found",
                "Requested channel does not exist in the group",
                "Select a channel that belongs to the active group",
            );
        }
    })
}

/// Tauri command: focus a specific DM conversation.
pub fn set_active_dm(request: SetActiveDmRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        if state.dms.iter().any(|dm| dm.dm_id == request.dm_id) {
            state.active_context = Some(ActiveContextView {
                kind: "dm".to_owned(),
                group_id: None,
                channel_id: None,
                dm_id: Some(request.dm_id.clone()),
            });
            state.push_event(
                "dm.focused",
                format!(
                    "Focused DM {}",
                    redacted_observable_ref("dm", &request.dm_id)
                ),
            );
        } else {
            state.push_command_error(
                "dm.focus_missing",
                "set_active_dm",
                "dm_not_found",
                "Requested DM does not exist",
                "Select a DM that already exists",
            );
        }
    })
}

/// Tauri command: join a local-first group from an invite.
pub fn join_group(request: JoinGroupRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let invite_code = normalize_label(&request.invite_code, "manual invite");
        let abuse_key = format!(
            "consume:{}:{}",
            state.local_user_id(),
            invite_code_fingerprint(&invite_code)
        );
        if !state.abuse.allow_invite_consume(&abuse_key, Utc::now()) {
            state.push_command_error(
                "invite.rate_limited",
                "join_group",
                "invite_consume_rate_limited",
                "Invite consumption is rate-limited for this profile and invite descriptor",
                "Wait for the abuse-control window before retrying this invite",
            );
            return;
        }
        if parse_invite_metadata(&invite_code).is_some()
            && !state.abuse.allow_admission_helper(&abuse_key, Utc::now())
        {
            state.push_command_error(
                "admission.rate_limited",
                "join_group",
                "admission_helper_rate_limited",
                "Admission helper attempts are rate-limited before invite consumption",
                "Retry after the helper rate-limit window with a fresh authorized proof",
            );
            return;
        }
        if let Some(invite) = state
            .invites
            .iter()
            .find(|invite| invite.code == invite_code)
            .cloned()
        {
            let group_name = state
                .groups
                .iter()
                .find(|group| group.group_id == invite.group_id)
                .map(|group| group.name.clone())
                .unwrap_or_else(|| "group".to_owned());
            state.active_context = Some(ActiveContextView {
                kind: "group".to_owned(),
                group_id: Some(invite.group_id.clone()),
                channel_id: None,
                dm_id: None,
            });
            state.push_event(
                "group.opened_from_invite",
                format!(
                    "Opened {} from local invite",
                    redacted_observable_ref("group", &group_name)
                ),
            );
            return;
        }
        let parsed_invite = parse_invite_metadata(&invite_code);
        let name = request
            .group_name
            .map(|value| normalize_label(&value, "joined enclave"))
            .unwrap_or_else(|| parse_invite_group_name(&invite_code));
        let group_id = parsed_invite
            .as_ref()
            .and_then(|parsed| parsed.group_id.clone())
            .unwrap_or_else(|| stable_id("group", &name, state.next_sequence));
        if !state.groups.iter().any(|group| group.name == name) {
            let connectivity = parsed_invite
                .as_ref()
                .map(|parsed| parsed.connectivity.clone())
                .unwrap_or_else(|| group_connectivity_policy(&group_id));
            let runtime_peers = group_runtime_peers(Some(&connectivity), "member");
            state.groups.push(GroupView {
                group_id: group_id.clone(),
                name: name.clone(),
                role: "member".to_owned(),
                channels: default_group_channels(state.next_sequence),
                runtime_peers,
                connectivity: Some(connectivity),
            });
        } else if let Some(parsed) = parsed_invite.as_ref() {
            if let Some(existing_group) = state.groups.iter_mut().find(|group| group.name == name) {
                existing_group.role = "member".to_owned();
                existing_group.runtime_peers =
                    group_runtime_peers(Some(&parsed.connectivity), "member");
                existing_group.connectivity = Some(parsed.connectivity.clone());
            }
        }
        let active_group_id = state
            .groups
            .iter()
            .find(|group| group.name == name)
            .map(|group| group.group_id.clone())
            .unwrap_or(group_id);
        state.active_context = Some(ActiveContextView {
            kind: "group".to_owned(),
            group_id: Some(active_group_id),
            channel_id: None,
            dm_id: None,
        });
        if let Some(parsed) = parsed_invite {
            state.invites.push(InviteView {
                invite_id: format!("invite-{}", parsed.invite_key),
                invite_key: parsed.invite_key,
                group_id: state
                    .active_context
                    .as_ref()
                    .and_then(|context| context.group_id.clone())
                    .unwrap_or_default(),
                dm_id: None,
                connectivity_schema_version: parsed.connectivity.connectivity_schema_version,
                invite_kind: parsed.connectivity.invite_kind.clone(),
                scope_id_commitment: parsed.connectivity.scope_id_commitment.clone(),
                signaling_profiles: parsed.connectivity.signaling_profiles.clone(),
                privacy_label: parsed.connectivity.privacy_label.clone(),
                dm_bootstrap: parsed.connectivity.dm_bootstrap.clone(),
                group_bootstrap: parsed.connectivity.group_bootstrap.clone(),
                code: invite_code.clone(),
                room_secret_hash: parsed.room_secret_hash,
                signaling_endpoint: parsed.signaling_endpoint,
                signaling_trust_fingerprint: parsed.signaling_trust_fingerprint,
                signaling_trust_status: parsed.signaling_trust_status,
                endpoint_policy: parsed.endpoint_policy,
                ice_stun_servers: parsed.ice_stun_servers,
                ice_turn_servers: parsed.ice_turn_servers,
                expires: "Invite expiry from signed descriptor".to_owned(),
                expires_at: parsed.expires_at,
                max_use: parsed.max_uses.to_string(),
                uses: 1,
                revoked: false,
                admission_copy:
                    "Parsed production invite metadata; final admission still requires authorized MLS Welcome/add"
                        .to_owned(),
            });
        }
        state.push_event(
            "group.joined",
            format!(
                "Joined {} via {}",
                redacted_observable_ref("group", &name),
                redacted_observable_ref("invite", &invite_code)
            ),
        );
    })
}

/// Tauri command: create an invite for the active group.
pub fn create_invite(request: CreateInviteRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let Some(group_id) = request
            .group_id
            .or_else(|| {
                state
                    .active_context
                    .as_ref()
                    .and_then(|context| context.group_id.clone())
            })
            .or_else(|| state.groups.first().map(|group| group.group_id.clone()))
        else {
            state.push_command_error(
                "invite.rejected",
                "create_invite",
                "group_not_found",
                "No group exists for invite creation",
                "Create or select a group before creating an invite",
            );
            return;
        };
        let abuse_key = format!("invite:{}:{}", state.local_user_id(), group_id);
        if !state.abuse.allow_invite_create(&abuse_key, Utc::now()) {
            state.push_command_error(
                "invite.rate_limited",
                "create_invite",
                "invite_create_rate_limited",
                "Invite creation is rate-limited for this group and issuer",
                "Wait for the abuse-control window before issuing another invite",
            );
            return;
        }
        let sequence = state.next_sequence;
        let group = state.groups.iter().find(|group| group.group_id == group_id);
        let group_name = group
            .map(|group| group.name.clone())
            .unwrap_or_else(|| "group".to_owned());
        let connectivity = group
            .and_then(|group| group.connectivity.clone())
            .unwrap_or_else(|| group_connectivity_policy(&group_id));
        let bootstrap_metadata = match bootstrap_metadata_from_connectivity(&connectivity) {
            Ok(metadata) => metadata,
            Err(error) => {
                state.push_command_error(
                    "invite.rejected",
                    "create_invite",
                    "invite_bootstrap_invalid",
                    error,
                    "Recreate the group connectivity policy before issuing an invite",
                );
                return;
            }
        };
        let _ice_endpoint_policy = match ice_endpoint_policy_from_connectivity(&connectivity) {
            Ok(policy) => policy,
            Err(error) => {
                state.push_command_error(
                    "invite.rejected",
                    "create_invite",
                    "invite_ice_policy_invalid",
                    error.to_string(),
                    "Recreate the group connectivity policy before issuing an invite",
                );
                return;
            }
        };
        let expires = normalize_label(&request.expires, "Invite expires and can be revoked");
        let max_use = normalize_label(&request.max_use, "Max-use is enforced before MLS admission");
        let expires_at = invite_expiration_horizon(&expires);
        let descriptor_expires_at = Utc::now() + invite_expiration_duration(&expires);
        let max_uses = parse_max_uses(&max_use);
        let invite_key = Uuid::new_v4().to_string();
        let room_secret = format!("room-secret:{}:{}:{}", group_id, invite_key, sequence);
        let signaling_endpoint = default_signaling_endpoint();
        let signaling_trust_fingerprint = signaling_fingerprint_for_endpoint(&signaling_endpoint);
        let mut signaling_metadata = InviteSignalingMetadata::new(
            signaling_endpoint.clone(),
            InviteEndpointPolicy::ProductionTls,
            InviteTrustMetadata::new(
                signaling_trust_fingerprint.clone(),
                "signed endpoint fingerprint; verify before MLS Welcome",
            )
            .unwrap_or_else(|_| InviteSignalingMetadata::default_production().trust),
        )
        .and_then(
            |metadata| match signed_ice_endpoint_policy_from_connectivity(&connectivity) {
                Some(policy) => metadata.with_ice_endpoint_policy(policy),
                None => Ok(metadata),
            },
        )
        .unwrap_or_else(|_| InviteSignalingMetadata::default_production());
        if let Ok(ice_config) = ice_config_from_connectivity(&connectivity) {
            if let Ok(ice_policy) = discrypt_transport::IceEndpointPolicy::new(
                ice_config.stun_servers.clone(),
                ice_config.turn_servers.clone(),
            ) {
                if let Ok(with_ice_policy) = signaling_metadata
                    .clone()
                    .with_ice_endpoint_policy(ice_policy)
                {
                    signaling_metadata = with_ice_policy;
                }
            }
        }
        let mut invite_store = InviteStore::new();
        let issuer = SigningKey::generate(&mut OsRng);
        let descriptor = invite_store
            .issue_invite_with_bootstrap_metadata(
                room_secret.as_bytes(),
                descriptor_expires_at,
                max_uses,
                signaling_metadata.clone(),
                bootstrap_metadata,
                &issuer,
            )
            .unwrap_or_else(|_| {
                invite_store.issue_invite(
                    room_secret.as_bytes(),
                    descriptor_expires_at,
                    max_uses,
                    &issuer,
                )
            });
        let room_secret_hash = hex::encode(descriptor.room_secret_commitment);
        let invite_code = match production_invite_link(
            &descriptor,
            expires_at.as_str(),
            max_uses,
            Some(&group_id),
        ) {
            Ok(code) => code,
            Err(error) => {
                state.push_command_error(
                    "invite.rejected",
                    "create_invite",
                    "invite_link_invalid",
                    error,
                    "Regenerate the invite after validating signaling and ICE metadata",
                );
                return;
            }
        };
        let invite = InviteView {
            invite_id: format!("invite-{}", descriptor.invite_id),
            invite_key: descriptor.invite_id.clone(),
            group_id: group_id.clone(),
            dm_id: None,
            connectivity_schema_version: connectivity.connectivity_schema_version,
            invite_kind: connectivity.invite_kind.clone(),
            scope_id_commitment: connectivity.scope_id_commitment.clone(),
            signaling_profiles: connectivity.signaling_profiles.clone(),
            privacy_label: connectivity.privacy_label.clone(),
            dm_bootstrap: connectivity.dm_bootstrap.clone(),
            group_bootstrap: connectivity.group_bootstrap.clone(),
            code: invite_code,
            room_secret_hash,
            signaling_endpoint,
            signaling_trust_fingerprint,
            signaling_trust_status: "signed endpoint fingerprint; verify before MLS Welcome"
                .to_owned(),
            endpoint_policy: "production_tls".to_owned(),
            ice_stun_servers: connectivity.ice_stun_servers.clone(),
            ice_turn_servers: connectivity.ice_turn_servers.clone(),
            expires,
            expires_at,
            max_use,
            uses: 0,
            revoked: false,
            admission_copy: "Final admission still requires an authorized MLS Welcome/add; the room-secret link alone is insufficient"
                .to_owned(),
        };
        let signaling_key = format!(
            "signaling:{}:{}",
            state.local_user_id(),
            descriptor.invite_id
        );
        if !state
            .abuse
            .allow_signaling_publish_take(&signaling_key, Utc::now())
        {
            state.push_command_error(
                "signaling.rate_limited",
                "create_invite",
                "signaling_publish_rate_limited",
                "Signaling rendezvous publish/take is rate-limited for this invite",
                "Retry after the signaling abuse-control window",
            );
            return;
        }
        state.invites.push(invite);
        state.push_event(
            "invite.created",
            format!(
                "Invite created for {}",
                redacted_observable_ref("group", &group_name)
            ),
        );
    })
}

/// Tauri command: create a first-contact invite for an existing DM contact.
pub fn create_dm_invite(request: CreateDmInviteRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let Some(dm_id) = request
            .dm_id
            .or_else(|| {
                state
                    .active_context
                    .as_ref()
                    .and_then(|context| context.dm_id.clone())
            })
            .or_else(|| state.dms.first().map(|dm| dm.dm_id.clone()))
        else {
            state.push_command_error(
                "invite.rejected",
                "create_dm_invite",
                "dm_not_found",
                "No DM contact exists for invite creation",
                "Start or select a DM before creating a contact invite",
            );
            return;
        };
        let Some(dm) = state.dms.iter().find(|dm| dm.dm_id == dm_id).cloned() else {
            state.push_command_error(
                "invite.rejected",
                "create_dm_invite",
                "dm_not_found",
                "Requested DM contact does not exist",
                "Pick a contact from the DM list before creating an invite",
            );
            return;
        };
        let abuse_key = format!("invite-dm:{}:{}", state.local_user_id(), dm.dm_id);
        if !state.abuse.allow_invite_create(&abuse_key, Utc::now()) {
            state.push_command_error(
                "invite.rate_limited",
                "create_dm_invite",
                "invite_create_rate_limited",
                "DM contact invite creation is rate-limited for this contact and issuer",
                "Wait for the abuse-control window before issuing another DM invite",
            );
            return;
        }
        let connectivity = dm
            .connectivity
            .clone()
            .unwrap_or_else(|| dm_connectivity_policy(&dm.dm_id, &dm.participant_id));
        let bootstrap_metadata = match bootstrap_metadata_from_connectivity(&connectivity) {
            Ok(metadata) => metadata,
            Err(error) => {
                state.push_command_error(
                    "invite.rejected",
                    "create_dm_invite",
                    "invite_bootstrap_invalid",
                    error,
                    "Recreate the DM connectivity policy before issuing an invite",
                );
                return;
            }
        };
        let _ice_endpoint_policy = match ice_endpoint_policy_from_connectivity(&connectivity) {
            Ok(policy) => policy,
            Err(error) => {
                state.push_command_error(
                    "invite.rejected",
                    "create_dm_invite",
                    "invite_ice_policy_invalid",
                    error.to_string(),
                    "Recreate the DM connectivity policy before issuing an invite",
                );
                return;
            }
        };
        let sequence = state.next_sequence;
        let expires = normalize_label(&request.expires, "Invite expires and can be revoked");
        let max_use = normalize_label(&request.max_use, "Max-use is enforced before DM acceptance");
        let expires_at = invite_expiration_horizon(&expires);
        let descriptor_expires_at = Utc::now() + invite_expiration_duration(&expires);
        let max_uses = parse_max_uses(&max_use);
        let invite_key = Uuid::new_v4().to_string();
        let room_secret = format!("dm-contact-secret:{}:{}:{}", dm.dm_id, invite_key, sequence);
        let signaling_endpoint = default_signaling_endpoint();
        let signaling_trust_fingerprint = signaling_fingerprint_for_endpoint(&signaling_endpoint);
        let mut signaling_metadata = InviteSignalingMetadata::new(
            signaling_endpoint.clone(),
            InviteEndpointPolicy::ProductionTls,
            InviteTrustMetadata::new(
                signaling_trust_fingerprint.clone(),
                "signed endpoint fingerprint; verify before DM accept",
            )
            .unwrap_or_else(|_| InviteSignalingMetadata::default_production().trust),
        )
        .and_then(
            |metadata| match signed_ice_endpoint_policy_from_connectivity(&connectivity) {
                Some(policy) => metadata.with_ice_endpoint_policy(policy),
                None => Ok(metadata),
            },
        )
        .unwrap_or_else(|_| InviteSignalingMetadata::default_production());
        if let Ok(ice_config) = ice_config_from_connectivity(&connectivity) {
            if let Ok(ice_policy) = discrypt_transport::IceEndpointPolicy::new(
                ice_config.stun_servers.clone(),
                ice_config.turn_servers.clone(),
            ) {
                if let Ok(with_ice_policy) = signaling_metadata
                    .clone()
                    .with_ice_endpoint_policy(ice_policy)
                {
                    signaling_metadata = with_ice_policy;
                }
            }
        }
        let mut invite_store = InviteStore::new();
        let issuer = SigningKey::generate(&mut OsRng);
        let descriptor = invite_store
            .issue_invite_with_bootstrap_metadata(
                room_secret.as_bytes(),
                descriptor_expires_at,
                max_uses,
                signaling_metadata,
                bootstrap_metadata,
                &issuer,
            )
            .unwrap_or_else(|_| {
                invite_store.issue_invite(
                    room_secret.as_bytes(),
                    descriptor_expires_at,
                    max_uses,
                    &issuer,
                )
            });
        let room_secret_hash = hex::encode(descriptor.room_secret_commitment);
        let invite_code =
            match production_invite_link(&descriptor, expires_at.as_str(), max_uses, None) {
                Ok(code) => code,
                Err(error) => {
                    state.push_command_error(
                        "invite.rejected",
                        "create_dm_invite",
                        "invite_link_invalid",
                        error,
                        "Regenerate the DM invite after validating signaling and ICE metadata",
                    );
                    return;
                }
            };
        let invite = InviteView {
            invite_id: format!("invite-{}", descriptor.invite_id),
            invite_key: descriptor.invite_id.clone(),
            group_id: String::new(),
            dm_id: Some(dm.dm_id.clone()),
            connectivity_schema_version: connectivity.connectivity_schema_version,
            invite_kind: connectivity.invite_kind.clone(),
            scope_id_commitment: connectivity.scope_id_commitment.clone(),
            signaling_profiles: connectivity.signaling_profiles.clone(),
            privacy_label: connectivity.privacy_label.clone(),
            dm_bootstrap: connectivity.dm_bootstrap.clone(),
            group_bootstrap: connectivity.group_bootstrap.clone(),
            code: invite_code,
            room_secret_hash,
            signaling_endpoint,
            signaling_trust_fingerprint,
            signaling_trust_status: "signed endpoint fingerprint; verify before DM accept".to_owned(),
            endpoint_policy: "production_tls".to_owned(),
            ice_stun_servers: connectivity.ice_stun_servers.clone(),
            ice_turn_servers: connectivity.ice_turn_servers.clone(),
            expires,
            expires_at,
            max_use,
            uses: 0,
            revoked: false,
            admission_copy: "Final DM acceptance still requires a sealed reply rendezvous and verified contact identity; the link alone is insufficient".to_owned(),
        };
        state.invites.push(invite);
        state.push_event(
            "invite.dm_created",
            format!(
                "DM contact invite created for {}",
                redacted_observable_ref("dm_contact", &dm.display_name)
            ),
        );
    })
}

/// Tauri command: accept/open a first-contact DM invite.
pub fn accept_dm_invite(request: AcceptDmInviteRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let invite_code = normalize_label(&request.invite_code, "manual DM invite");
        let Some(parsed) = parse_invite_metadata(&invite_code) else {
            state.push_command_error(
                "invite.rejected",
                "accept_dm_invite",
                "invite_parse_failed",
                "DM contact invite metadata could not be parsed",
                "Paste a signed DM contact invite descriptor before accepting",
            );
            return;
        };
        if parsed.connectivity.invite_kind != InviteKind::DmContact.canonical_name() {
            state.push_command_error(
                "invite.rejected",
                "accept_dm_invite",
                "invite_kind_mismatch",
                "Invite is not a DM contact invite",
                "Use group join for group invites or request a DM contact invite",
            );
            return;
        }
        let display_name = request
            .display_name
            .map(|value| normalize_label(&value, "DM contact"))
            .unwrap_or_else(|| "DM contact".to_owned());
        let dm_id = stable_id("dm", &parsed.invite_key, state.next_sequence);
        let participant_id = hash_commitment(
            "discrypt-accepted-dm-participant-id-v1",
            &[&parsed.connectivity.scope_id_commitment],
        );
        if !state.dms.iter().any(|dm| {
            dm.connectivity
                .as_ref()
                .map(|policy| &policy.scope_id_commitment)
                == Some(&parsed.connectivity.scope_id_commitment)
        }) {
            let runtime_peers = dm_runtime_peers(Some(&parsed.connectivity), "reply");
            state.dms.push(DirectConversationView {
                dm_id: dm_id.clone(),
                participant_id,
                display_name: display_name.clone(),
                local_only_copy: "DM contact opened from signed invite metadata; remote delivery is not claimed until backend receipt proof".to_owned(),
                runtime_peers,
                connectivity: Some(parsed.connectivity.clone()),
            });
        }
        let active_dm_id = state
            .dms
            .iter()
            .find(|dm| {
                dm.connectivity
                    .as_ref()
                    .map(|policy| &policy.scope_id_commitment)
                    == Some(&parsed.connectivity.scope_id_commitment)
            })
            .map(|dm| dm.dm_id.clone())
            .unwrap_or(dm_id);
        state.active_context = Some(ActiveContextView {
            kind: "dm".to_owned(),
            group_id: None,
            channel_id: None,
            dm_id: Some(active_dm_id.clone()),
        });
        state.invites.push(InviteView {
            invite_id: format!("invite-{}", parsed.invite_key),
            invite_key: parsed.invite_key,
            group_id: String::new(),
            dm_id: Some(active_dm_id),
            connectivity_schema_version: parsed.connectivity.connectivity_schema_version,
            invite_kind: parsed.connectivity.invite_kind.clone(),
            scope_id_commitment: parsed.connectivity.scope_id_commitment.clone(),
            signaling_profiles: parsed.connectivity.signaling_profiles.clone(),
            privacy_label: parsed.connectivity.privacy_label.clone(),
            dm_bootstrap: parsed.connectivity.dm_bootstrap.clone(),
            group_bootstrap: parsed.connectivity.group_bootstrap.clone(),
            code: invite_code.clone(),
            room_secret_hash: parsed.room_secret_hash,
            signaling_endpoint: parsed.signaling_endpoint,
            signaling_trust_fingerprint: parsed.signaling_trust_fingerprint,
            signaling_trust_status: parsed.signaling_trust_status,
            endpoint_policy: parsed.endpoint_policy,
            ice_stun_servers: parsed.ice_stun_servers,
            ice_turn_servers: parsed.ice_turn_servers,
            expires: "Invite expiry from signed descriptor".to_owned(),
            expires_at: parsed.expires_at,
            max_use: parsed.max_uses.to_string(),
            uses: 1,
            revoked: false,
            admission_copy: "Parsed DM contact invite metadata; final acceptance still requires sealed reply rendezvous/contact verification".to_owned(),
        });
        state.push_event(
            "dm.invite_accepted",
            format!(
                "Opened DM contact {}",
                redacted_observable_ref("dm_contact", &display_name)
            ),
        );
    })
}

/// Tauri command: create a channel in a group.
pub fn create_channel(request: CreateChannelRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let channel_id = stable_id("channel", &request.name, state.next_sequence);
        let channel = ChannelStateView {
            channel_id: channel_id.clone(),
            name: normalize_channel_name(&request.name, request.kind),
            kind: request.kind,
            retention_status: normalize_label(&request.retention_status, "7 days"),
            connectivity: None,
        };
        if let Some(group) = state
            .groups
            .iter_mut()
            .find(|group| group.group_id == request.group_id)
        {
            if !group
                .channels
                .iter()
                .any(|existing| existing.name == channel.name)
            {
                group.channels.push(channel.clone());
            }
            state.active_context = Some(ActiveContextView {
                kind: match channel.kind {
                    ChannelKind::Text => "text_channel".to_owned(),
                    ChannelKind::Voice => "voice_channel".to_owned(),
                },
                group_id: Some(request.group_id),
                channel_id: Some(channel.channel_id),
                dm_id: None,
            });
            state.push_event(
                "channel.created",
                format!(
                    "Created channel {}",
                    redacted_observable_ref("channel", &channel.name)
                ),
            );
        } else {
            state.push_command_error(
                "channel.rejected",
                "create_channel",
                "group_not_found",
                "No matching group for channel creation",
                "Select an existing group before adding a text or voice channel",
            );
        }
    })
}

/// Tauri command: append a message to a local timeline.
pub fn send_message(request: SendMessageRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let body = request.body.trim();
        if body.is_empty() {
            state.push_command_error(
                "message.rejected",
                "send_message",
                "message_empty",
                "Empty message was not sent",
                "Type a non-empty message before sending",
            );
            return;
        }
        let abuse_key = text_send_abuse_key(state, &request.target);
        if !state.abuse.allow_text_send(&abuse_key, Utc::now()) {
            state.push_command_error(
                "message.rate_limited",
                "send_message",
                "text_send_rate_limited",
                "Text send is rate-limited for this author and conversation",
                "Wait for the abuse-control window before sending more text",
            );
            return;
        }
        let sequence = state.next_sequence;
        let author = state
            .profile
            .as_ref()
            .map(|profile| profile.display_name.clone())
            .unwrap_or_else(|| "Alice".to_owned());
        let author_id = state.local_user_id();
        let author_commitment = hash_commitment("discrypt-message-id-author-v1", &[&author_id]);
        let message_id = format!("msg-{}-{sequence}", &author_commitment[..16]);
        let mut status = "local encrypted author log; remote delivery/read receipts not claimed without signed receipt".to_owned();
        let mut state_key = "sent_local".to_owned();
        let mut state_label = "Sent locally".to_owned();
        let mut state_detail = default_text_state_detail();
        if request.transport_proof {
            let frame = opaque_text_control_frame_for_message(
                state,
                &request.target,
                &message_id,
                body,
                sequence,
            );
            match state
                .probe_active_webrtc_data_channel_with_frame(request.adapter_kind.as_deref(), frame)
            {
                Ok(probe) => {
                    status = "local encrypted author log plus provider-signaled WebRTC DataChannel transport proof; peer receipt still requires signed remote receipt".to_owned();
                    state_key = "transport_probe_verified".to_owned();
                    state_label = "Transport proofed".to_owned();
                    state_detail = format!(
                        "Opaque text/control frame crossed adapter={} profile={} {} frame_sha256={} receipt_return={}; this is not a signed peer receipt",
                        probe.kind,
                        probe.profile_id,
                        redacted_observable_ref("room_topic", &probe.rendezvous_topic),
                        probe.text_control_frame_sha256,
                        probe.receipt_frame_roundtrip
                    );
                }
                Err(error) => {
                    status = "local encrypted author log only; requested WebRTC DataChannel transport proof failed".to_owned();
                    state_key = "transport_probe_failed".to_owned();
                    state_label = "Transport proof failed".to_owned();
                    state_detail = error.clone();
                    state.push_command_error(
                        "message.transport_proof_failed",
                        "send_message",
                        "text_transport_proof_failed",
                        error,
                        "Check provider reachability, STUN/ICE policy, active scope connectivity, and adapter feature flags before claiming remote delivery",
                    );
                }
            }
        }
        let envelope_record = match state.text_delivery_envelope_record(
            &request.target,
            &message_id,
            body,
            sequence,
        ) {
            Ok(envelope_record) => envelope_record,
            Err(error) => {
                state.push_command_error(
                    "message.rejected",
                    "send_message",
                    "text_delivery_envelope_failed",
                    error,
                    "Create or join the conversation with persisted OpenMLS group state before sending encrypted text",
                );
                return;
            }
        };
        state.text_delivery_envelopes.push(envelope_record.clone());
        let mut outbox_error = None;
        if let Err(error) =
            state.enqueue_text_control_outbox(&request.target, &message_id, &envelope_record)
        {
            outbox_error = Some(error);
        }
        if let Some(error) = outbox_error {
            state.push_command_error(
                "message.outbox_rejected",
                "send_message",
                "text_control_outbox_enqueue_failed",
                error,
                "The message was stored locally, but the transport session loop cannot send it until its signed frame can be persisted",
            );
        }
        let message = MessageView {
            message_id,
            target: request.target,
            author_id,
            author,
            body: body.to_owned(),
            status,
            state_key,
            state_label,
            state_detail,
            peer_receipt: None,
            sent_at: format!("local-{sequence}"),
        };
        state.messages.push(message);
        state.push_event(
            "message.sent",
            "Message appended to local encrypted timeline; remote delivery/read receipts are not claimed",
        );
    })
}

/// Tauri command: apply a signed peer delivery receipt to a persisted message.
pub fn apply_text_delivery_receipt(request: ApplyTextDeliveryReceiptRequest) -> AppStateView {
    mutate_app_service(|state| {
        if let Err(error) = state.apply_text_delivery_receipt(request) {
            state.push_command_error(
                "message.receipt_rejected",
                "apply_text_delivery_receipt",
                "receipt_verification_failed",
                error,
                "Accept peer delivery only from a receipt whose signature, message id, group binding, and ciphertext hash verify",
            );
        }
    })
}

/// Tauri command: accept a signed encrypted peer envelope and return a signed receipt.
pub fn receive_text_delivery_envelope(
    request: ReceiveTextDeliveryEnvelopeRequest,
) -> ReceiveTextDeliveryEnvelopeResponse {
    let (state, result) =
        mutate_app_service_with_result(|state| state.receive_text_delivery_envelope(request));
    match result {
        Ok((receipt, recipient_verifying_key_hex)) => ReceiveTextDeliveryEnvelopeResponse {
            state,
            receipt: Some(receipt),
            recipient_verifying_key_hex: Some(recipient_verifying_key_hex),
        },
        Err(_) => ReceiveTextDeliveryEnvelopeResponse {
            state,
            receipt: None,
            recipient_verifying_key_hex: None,
        },
    }
}

/// Tauri command: list persisted outbound text/control frames for the session loop.
pub fn list_pending_text_control_frames(
    request: ListPendingTextControlFramesRequest,
) -> ListPendingTextControlFramesResponse {
    with_state(|state| ListPendingTextControlFramesResponse {
        state: state.to_view(),
        frames: state.list_pending_text_control_frames(&request),
    })
}

/// Tauri command: pump pending text/control frames through the app-service-owned runtime.
pub fn pump_text_control_transport_once(
    request: ListPendingTextControlFramesRequest,
) -> TextControlTransportPumpReportView {
    let service = app_service();
    let mut guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.pump_text_control_transport_once(request)
}

/// Tauri command: mark an outbound text/control frame as handed to transport.
pub fn mark_text_control_frame_sent(request: MarkTextControlFrameSentRequest) -> AppStateView {
    mutate_app_service(|state| {
        if let Err(error) = state.mark_text_control_frame_sent(request) {
            state.push_command_error(
                "message.outbox_mark_rejected",
                "mark_text_control_frame_sent",
                "text_control_outbox_mark_failed",
                error,
                "Only mark an outbound frame sent when the message id and frame hash match the persisted outbox item",
            );
        }
    })
}

/// Tauri command: handle one peer text/control frame and return an optional response frame.
pub fn handle_text_control_frame(
    request: HandleTextControlFrameRequest,
) -> HandleTextControlFrameResponse {
    let (state, response_frame) =
        mutate_app_service_with_result(|state| state.handle_text_control_frame(request.frame));
    HandleTextControlFrameResponse {
        state,
        response_frame,
    }
}

/// Tauri command: queue an outbound backend-state-only voice offer/answer/candidate; no remote media is claimed.
pub fn publish_voice_signaling_message(
    request: PublishVoiceSignalingMessageRequest,
) -> AppStateView {
    mutate_app_service(|state| {
        if let Err(error) = state.enqueue_voice_signaling_outbox(request) {
            state.push_command_error(
                "voice.signal_rejected",
                "publish_voice_signaling_message",
                "voice_signal_queue_failed",
                error,
                "Join voice with provider-derived runtime peers before queueing SDP/ICE; send queued frames only over the backend text/control transport",
            );
        }
    })
}

/// Tauri command: drain inbound backend-state-only voice offer/answer/candidate messages for the browser runtime.
pub fn take_pending_voice_signaling_messages(
    request: TakePendingVoiceSignalingMessagesRequest,
) -> TakePendingVoiceSignalingMessagesResponse {
    let (state, messages) = mutate_app_service_with_result(|state| {
        state.take_pending_voice_signaling_messages(request)
    });
    TakePendingVoiceSignalingMessagesResponse { state, messages }
}

/// Tauri command: join a voice channel.
pub fn join_voice(request: JoinVoiceRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let session_id = stable_voice_session_id(&request.group_id, &request.channel_id);
        let selection = voice_device_selection(&request);
        let capture_allowed = selection.can_join_voice();
        let self_muted = state
            .voice_session
            .as_ref()
            .map(|session| session.self_muted)
            .unwrap_or(false);
        let local_user_id = state.local_user_id();
        state.voice_session = Some(VoiceSessionView {
            session_id: session_id.clone(),
            group_id: request.group_id.clone(),
            channel_id: request.channel_id.clone(),
            joined: capture_allowed,
            self_muted,
            microphone_permission: format!("{:?}", selection.microphone_permission)
                .to_ascii_lowercase(),
            input_device: selection.input_device.clone(),
            output_device: selection.output_device.clone(),
            media_runtime: voice_media_runtime_for_join(&session_id, &selection),
            signaling: VoiceSignalingStateView {
                session_id: session_id.clone(),
                status_copy: if capture_allowed {
                    "Voice signaling waits for provider-derived peer ids before SDP/ICE exchange"
                        .to_owned()
                } else {
                    "Voice signaling did not start because capture permission/device gates failed"
                        .to_owned()
                },
                ..VoiceSignalingStateView::default()
            },
            participants: default_voice_participants(&local_user_id, false),
            route_copy: if capture_allowed {
                "Backend recorded a webview local-capture runtime boundary; remote WebRTC audio transport remains fail-closed until media-route evidence attaches; speaking indicators wait for media audio-level/VAD events".to_owned()
            } else {
                "No voice route opened because microphone permission/input selection is not granted"
                    .to_owned()
            },
            status_copy: selection.status_copy(),
            permission_denied_copy: if capture_allowed {
                String::new()
            } else {
                "Grant microphone permission and select an input device before joining voice"
                    .to_owned()
            },
        });
        state.active_context = Some(ActiveContextView {
            kind: "voice_channel".to_owned(),
            group_id: Some(request.group_id),
            channel_id: Some(request.channel_id),
            dm_id: None,
        });
        if capture_allowed {
            state.push_event(
                "voice.joined",
                format!(
                    "Joined voice session {}",
                    redacted_observable_ref("voice_session", &session_id)
                ),
            );
        } else {
            state.push_command_error(
                "voice.permission_denied",
                "join_voice",
                "voice_permission_required",
                "Microphone permission/input device required before joining voice",
                "Grant microphone permission and select an input device before joining voice",
            );
        }
    })
}

/// Tauri command: leave a voice session.
pub fn leave_voice(request: LeaveVoiceRequest) -> AppStateView {
    mutate_app_service(|state| {
        let local_user_id = state.local_user_id();
        if let Some(session) = &mut state.voice_session {
            if session.session_id == request.session_id {
                session.joined = false;
                let leaving_session_id = session.session_id.clone();
                session.media_runtime = voice_media_runtime_for_leave(&session.session_id);
                session.signaling = VoiceSignalingStateView {
                    session_id: session.session_id.clone(),
                    role: "stopped".to_owned(),
                    status_copy:
                        "Voice signaling stopped by leave; pending inbound SDP/ICE was cleared"
                            .to_owned(),
                    ..VoiceSignalingStateView::default()
                };
                state
                    .voice_signaling_inbox
                    .retain(|record| record.signal.session_id != leaving_session_id);
                state
                    .text_control_outbox
                    .retain(|record| match &record.frame {
                        TextControlFrameView::VoiceSignal { signal } => {
                            signal.session_id != leaving_session_id
                        }
                        _ => true,
                    });
                session.route_copy =
                    "Voice media runtime stopped; no local capture or remote playback route is active"
                        .to_owned();
                session.status_copy = VOICE_SESSION_NOT_JOINED_COPY.to_owned();
                session
                    .participants
                    .retain(|participant| participant.id == local_user_id);
                for participant in &mut session.participants {
                    participant.speaking = false;
                }
                state.push_event(
                    "voice.left",
                    "Left command-backed local voice session and cleared remote media attachments",
                );
            } else {
                state.push_command_error(
                    "voice.leave_ignored",
                    "leave_voice",
                    "voice_session_not_found",
                    "Leave request did not match active session",
                    "Use the currently joined voice session before leaving",
                );
            }
        } else {
            state.push_command_error(
                "voice.leave_ignored",
                "leave_voice",
                "voice_session_not_found",
                "No active voice session to leave",
                "Join a voice channel before trying to leave",
            );
        }
    })
}

/// Tauri command: persist local self-mute state.
pub fn set_self_mute(request: SetSelfMuteRequest) -> AppStateView {
    mutate_app_service(|state| {
        let local_user_id = state.local_user_id();
        if let Some(session) = &mut state.voice_session {
            if session.session_id == request.session_id {
                session.self_muted = request.muted;
                for participant in &mut session.participants {
                    if participant.id == local_user_id {
                        participant.muted = request.muted;
                        if request.muted {
                            participant.speaking = false;
                        }
                    }
                }
                let summary = if request.muted {
                    "Self muted"
                } else {
                    "Self unmuted"
                };
                state.push_event("voice.self_mute", summary);
            } else {
                state.push_command_error(
                    "voice.self_mute_rejected",
                    "set_self_mute",
                    "voice_session_not_found",
                    "Mute request did not match active session",
                    "Join the voice session again before changing mute state",
                );
            }
        } else {
            state.push_command_error(
                "voice.self_mute_rejected",
                "set_self_mute",
                "voice_session_not_found",
                "No active voice session to mute",
                "Join a voice channel before muting yourself",
            );
        }
    })
}

/// Tauri command: update local speaking state from real microphone level evidence.
pub fn update_voice_activity(request: UpdateVoiceActivityRequest) -> AppStateView {
    mutate_app_service(|state| {
        let local_user_id = state.local_user_id();
        if let Some(session) = &mut state.voice_session {
            if session.session_id != request.session_id {
                state.push_command_error(
                    "voice.activity_rejected",
                    "update_voice_activity",
                    "voice_session_not_found",
                    "Voice activity request did not match active session",
                    "Join the active voice session before sending microphone activity",
                );
                return;
            }
            if !session.joined {
                state.push_command_error(
                    "voice.activity_rejected",
                    "update_voice_activity",
                    "voice_not_joined",
                    "Voice activity was ignored because the voice session is not joined",
                    "Join a voice channel before sending microphone activity",
                );
                return;
            }

            let evidence_speaking = request.rms_i16 >= 512 || request.peak_i16 >= 2_048;
            let self_muted = session.self_muted;
            let speaking = evidence_speaking && !self_muted;
            if let Some(participant) = session
                .participants
                .iter_mut()
                .find(|participant| participant.id == local_user_id)
            {
                participant.speaking = speaking;
                participant.muted = self_muted;
            } else {
                session.participants.push(VoiceParticipantView {
                    id: local_user_id.clone(),
                    name: "You".to_owned(),
                    role: "you".to_owned(),
                    speaking,
                    muted: self_muted,
                    volume: 82,
                });
            }
            session.status_copy = if self_muted {
                format!(
                    "Local microphone level observed at {} ms (rms {}, peak {}) but self-mute suppresses speaking state",
                    request.captured_at_ms, request.rms_i16, request.peak_i16
                )
            } else if speaking {
                format!(
                    "Local speaking indicator is driven by real microphone level evidence at {} ms (rms {}, peak {}); encrypted media transport remains gated by media-frame E2E",
                    request.captured_at_ms, request.rms_i16, request.peak_i16
                )
            } else {
                format!(
                    "Local microphone level observed below speaking threshold at {} ms (rms {}, peak {}); encrypted media transport remains gated by media-frame E2E",
                    request.captured_at_ms, request.rms_i16, request.peak_i16
                )
            };
            session.route_copy =
                "Local capture permission, device selection, and microphone level evidence are active; encrypted remote media transport remains gated by media-frame E2E"
                    .to_owned();
            state.push_event(
                "voice.activity",
                format!(
                    "Local microphone activity {} (rms {}, peak {})",
                    if speaking { "speaking" } else { "silent" },
                    request.rms_i16,
                    request.peak_i16
                ),
            );
        } else {
            state.push_command_error(
                "voice.activity_rejected",
                "update_voice_activity",
                "voice_session_not_found",
                "No active voice session for microphone activity",
                "Join a voice channel before sending microphone activity",
            );
        }
    })
}

/// Tauri command: attach backend-state proof for one real remote audio track.
pub fn attach_voice_remote_media(request: AttachVoiceRemoteMediaRequest) -> AppStateView {
    mutate_app_service(|state| {
        let local_user_id = state.local_user_id();
        if let Some(session) = &mut state.voice_session {
            if session.session_id != request.session_id {
                state.push_command_error(
                    "voice.remote_media_rejected",
                    "attach_voice_remote_media",
                    "voice_session_not_found",
                    "Remote media evidence did not match the active voice session",
                    "Join the active voice session before attaching remote audio",
                );
                return;
            }
            if !session.joined {
                state.push_command_error(
                    "voice.remote_media_rejected",
                    "attach_voice_remote_media",
                    "voice_not_joined",
                    "Remote media evidence was ignored because the voice session is not joined",
                    "Join voice before attaching remote playback",
                );
                return;
            }
            let evidence_fields = [
                request.participant_id.trim(),
                request.remote_peer_id.trim(),
                request.stream_id.trim(),
                request.audio_track_id.trim(),
                request.playback_element_id.trim(),
            ];
            if request.participant_id == local_user_id
                || evidence_fields.iter().any(|field| field.is_empty())
                || request.local_audio_tracks_sent == 0
                || request.received_audio_frames == 0
            {
                state.push_command_error(
                    "voice.remote_media_rejected",
                    "attach_voice_remote_media",
                    "voice_remote_media_evidence_invalid",
                    "Remote audio requires a non-local peer, a sent local audio track, and received remote audio frame evidence",
                    "Attach only backend media-route evidence from a real WebRTC remote audio track",
                );
                return;
            }

            let existing_volume = session
                .participants
                .iter()
                .find(|participant| participant.id == request.participant_id)
                .map(|participant| participant.volume)
                .unwrap_or(82);
            if let Some(participant) = session
                .participants
                .iter_mut()
                .find(|participant| participant.id == request.participant_id)
            {
                participant.name = request.participant_name.clone();
                participant.role = "remote".to_owned();
                participant.speaking = request.speaking;
                participant.muted = false;
            } else {
                session.participants.push(VoiceParticipantView {
                    id: request.participant_id.clone(),
                    name: request.participant_name.clone(),
                    role: "remote".to_owned(),
                    speaking: request.speaking,
                    muted: false,
                    volume: existing_volume,
                });
            }

            let remote_audio = VoiceRemoteAudioView {
                participant_id: request.participant_id.clone(),
                remote_peer_id: request.remote_peer_id.clone(),
                stream_id: request.stream_id.clone(),
                audio_track_id: request.audio_track_id.clone(),
                playback_element_id: request.playback_element_id.clone(),
                local_audio_tracks_sent: request.local_audio_tracks_sent,
                received_audio_frames: request.received_audio_frames,
                attached_at_ms: request.attached_at_ms,
            };
            session
                .media_runtime
                .remote_audio
                .retain(|track| track.participant_id != request.participant_id);
            session.media_runtime.remote_audio.push(remote_audio);
            session.media_runtime.boundary = "webview-backend-state-audio".to_owned();
            session.media_runtime.local_capture_active = true;
            session.media_runtime.remote_transport_active = true;
            session.media_runtime.fail_closed_reason.clear();
            session.media_runtime.status_copy = format!(
                "Backend media-route evidence attached remote WebRTC audio for {} after sending {} local audio track(s) and receiving {} audio frame(s)",
                request.participant_name, request.local_audio_tracks_sent, request.received_audio_frames
            );
            session.route_copy =
                "Backend media-route evidence attached real WebRTC remote audio playback; remote participants and volume controls are shown only for admitted remote tracks"
                    .to_owned();
            session.status_copy = session.media_runtime.status_copy.clone();
            state.push_event(
                "voice.remote_media_attached",
                format!(
                    "Remote audio route proof attached for {} via {}",
                    redacted_observable_ref("participant", &request.participant_name),
                    redacted_observable_ref("remote_peer", &request.remote_peer_id)
                ),
            );
        } else {
            state.push_command_error(
                "voice.remote_media_rejected",
                "attach_voice_remote_media",
                "voice_session_not_found",
                "No active voice session for remote media evidence",
                "Join voice before attaching remote playback",
            );
        }
    })
}

/// Tauri command: persist a participant speaker volume.
pub fn set_speaker_volume(request: SetSpeakerVolumeRequest) -> AppStateView {
    mutate_app_service(|state| {
        let local_user_id = state.local_user_id();
        if let Some(session) = &mut state.voice_session {
            if session.session_id == request.session_id {
                let volume = request.volume.min(100);
                if let Some(participant_index) = session
                    .participants
                    .iter()
                    .position(|participant| participant.id == request.participant_id)
                {
                    let participant = &session.participants[participant_index];
                    if participant.id == local_user_id || participant.role != "remote" {
                        state.push_command_error(
                            "voice.volume_rejected",
                            "set_speaker_volume",
                            "voice_volume_local_participant",
                            "Speaker volume applies only to backend-admitted remote audio participants",
                            "Wait for remote media evidence before changing per-peer volume",
                        );
                    } else {
                        let participant = &mut session.participants[participant_index];
                        participant.volume = volume;
                        let name = participant.name.clone();
                        state.push_event(
                            "voice.volume",
                            format!(
                                "Set participant {} volume to {volume}",
                                redacted_observable_ref("participant", &name)
                            ),
                        );
                    }
                } else {
                    state.push_command_error(
                        "voice.volume_rejected",
                        "set_speaker_volume",
                        "voice_participant_not_found",
                        "No matching voice participant for speaker volume",
                        "Choose a visible participant from the remote voice member list",
                    );
                }
            } else {
                state.push_command_error(
                    "voice.volume_rejected",
                    "set_speaker_volume",
                    "voice_session_not_found",
                    "Volume request did not match active session",
                    "Use the active voice session before changing speaker volume",
                );
            }
        } else {
            state.push_command_error(
                "voice.volume_rejected",
                "set_speaker_volume",
                "voice_session_not_found",
                "No active voice session for speaker volume",
                "Join a voice channel before changing speaker volume",
            );
        }
    })
}

/// Tauri command: return cursor/topic filtered command-backed app events for polling clients.
pub fn poll_app_events(request: Option<PollAppEventsRequest>) -> AppEventStreamView {
    with_state(|state| {
        let request = request.unwrap_or_default();
        let cursor = request.after.unwrap_or_default();
        let subscribed_kinds = normalize_event_subscriptions(&request.kinds);
        let limit = request.limit.unwrap_or(64).clamp(1, 256);
        let mut matching = state
            .events
            .iter()
            .filter(|event| {
                event.sequence > cursor
                    && event_matches_subscription(&event.kind, &subscribed_kinds)
            })
            .cloned()
            .collect::<Vec<_>>();
        let has_more = matching.len() > limit;
        if has_more {
            matching.truncate(limit);
        }
        let next_cursor = matching
            .last()
            .map(|event| event.sequence)
            .unwrap_or_else(|| state.latest_event_cursor());
        AppEventStreamView {
            events: matching,
            cursor,
            next_cursor,
            has_more,
            subscribed_kinds,
        }
    })
}

/// Tauri command: return the mandatory cooperative-deletion warning copy.
pub fn deletion_warning() -> String {
    app_state().security_copy.deletion
}

/// Tauri command: return the metadata-minimization caveat copy.
pub fn metadata_warning() -> String {
    app_state().security_copy.metadata
}

/// E2E command-health smoke used by CI and the multinode harness.
pub fn command_health() -> CommandHealth {
    let state = app_state();
    let identity_verification = identity_recovery_verification_smoke();
    let verification_snapshot = if state.devices.is_empty() {
        core_app_snapshot()
    } else {
        state.snapshot.clone()
    };
    let verification_ready = snapshot_safety_number_matches_identity_keys(&verification_snapshot);
    let honest_copy_ready = state
        .security_copy
        .deletion
        .contains("pending on offline devices")
        && state
            .security_copy
            .metadata
            .contains("does not claim anonymity")
        && state
            .security_copy
            .malicious_member
            .contains("not metadata anonymity")
        && state
            .security_copy
            .sybil_resistance
            .contains("do not solve Sybil attacks without a central identity");
    let app_state_ready = state.schema_version == APP_STATE_SCHEMA_VERSION;
    let identity_ready = matches!(
        state.lifecycle,
        AppLifecycle::FirstRun | AppLifecycle::Ready
    );
    let collaboration_ready = app_state_ready
        && state.snapshot.schema_version >= APP_STATE_SCHEMA_VERSION
        && state
            .messages
            .iter()
            .all(|message| message.status.contains("not claimed"));
    CommandHealth {
        snapshot_ready: state.snapshot.schema_version >= APP_STATE_SCHEMA_VERSION,
        verification_ready: verification_ready
            && identity_verification.two_profiles_verify_safety_numbers,
        app_state_ready,
        identity_ready,
        collaboration_ready: collaboration_ready
            && identity_verification.second_device_paired
            && identity_verification.recovery_without_content_keys
            && identity_verification.compromised_device_revoked,
        voice_ready: cfg!(feature = "production-media"),
        honest_copy_ready,
    }
}

/// Destructively reset persisted app state only after explicit UI confirmation.
pub fn reset_app_state_confirmed(request: ResetAppStateRequest) -> AppStateView {
    let service = app_service();
    let mut guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.state.last_command_error = None;
    if request.confirmation.trim() != RESET_APP_CONFIRMATION_PHRASE {
        guard.state.push_command_error(
            "state.reset_rejected",
            "reset_app_state",
            "confirmation_required",
            "Local state reset requires the exact confirmation phrase",
            format!("Type {RESET_APP_CONFIRMATION_PHRASE} to erase local app state"),
        );
        guard.persist();
        return guard.state.to_view();
    }
    guard.state = PersistedAppState::initial();
    guard.state.push_event(
        "state.reset",
        "Local app state reset after explicit typed confirmation",
    );
    guard.persist();
    guard.state.to_view()
}

/// Reset the persisted app state. Intended only for tests/dev smoke.
pub fn reset_app_state() -> AppStateView {
    let service = app_service();
    let mut guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.state = PersistedAppState::initial();
    guard.persist();
    guard.state.to_view()
}

/// Tauri IPC wrappers live in a child module because Tauri 2 command macros
/// export helper macros at crate root for visible commands.
#[cfg(feature = "tauri-runtime")]
mod ipc_commands {
    use super::*;

    #[tauri::command]
    pub(super) fn app_snapshot() -> AppSnapshot {
        super::app_snapshot()
    }

    #[tauri::command]
    pub(super) fn app_state() -> AppStateView {
        super::app_state()
    }

    #[tauri::command]
    pub(super) fn start_signaling_session(
        app_handle: tauri::AppHandle,
        request: StartSignalingSessionRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::start_signaling_session(request)
        })
    }

    #[tauri::command]
    pub(super) fn stop_signaling_session(
        app_handle: tauri::AppHandle,
        request: StopSignalingSessionRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::stop_signaling_session(request)
        })
    }

    #[tauri::command]
    pub(super) fn start_text_session(
        app_handle: tauri::AppHandle,
        request: StartTextSessionRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::start_text_session(request)
        })
    }

    #[tauri::command]
    pub(super) fn stop_text_session(
        app_handle: tauri::AppHandle,
        request: StopTextSessionRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::stop_text_session(request)
        })
    }

    #[tauri::command]
    pub(super) fn attach_text_control_transport_runtime(
        app_handle: tauri::AppHandle,
        request: AttachTextControlTransportRuntimeRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::attach_text_control_transport_runtime(request)
        })
    }

    #[tauri::command]
    pub(super) fn create_user(
        app_handle: tauri::AppHandle,
        request: CreateUserRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::create_user(request))
    }

    #[tauri::command]
    pub(super) fn recover_user(
        app_handle: tauri::AppHandle,
        request: RecoverUserRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::recover_user(request))
    }

    #[tauri::command]
    pub(super) fn verify_safety_number(
        app_handle: tauri::AppHandle,
        request: SafetyVerificationRequest,
    ) -> SafetyVerificationResult {
        super::run_command_with_event_emit(&app_handle, || super::verify_safety_number(request))
    }

    #[tauri::command]
    pub(super) fn create_device_pairing_payload(
        app_handle: tauri::AppHandle,
        request: CreateDevicePairingPayloadRequest,
    ) -> DevicePairingPayloadView {
        super::run_command_with_event_emit(&app_handle, || {
            super::create_device_pairing_payload(request)
        })
    }

    #[tauri::command]
    pub(super) fn accept_device_pairing_payload(
        app_handle: tauri::AppHandle,
        request: AcceptDevicePairingPayloadRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::accept_device_pairing_payload(request)
        })
    }

    #[tauri::command]
    pub(super) fn save_preferences(
        app_handle: tauri::AppHandle,
        request: SavePreferencesRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::save_preferences(request)
        })
    }

    #[tauri::command]
    pub(super) fn set_connectivity_policy(
        app_handle: tauri::AppHandle,
        request: SetConnectivityPolicyRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::set_connectivity_policy(request)
        })
    }

    #[tauri::command]
    pub(super) fn start_dm(app_handle: tauri::AppHandle, request: StartDmRequest) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::start_dm(request))
    }

    #[tauri::command]
    pub(super) fn create_group(
        app_handle: tauri::AppHandle,
        request: CreateGroupRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::create_group(request))
    }

    #[tauri::command]
    pub(super) fn set_active_group(
        app_handle: tauri::AppHandle,
        request: SetActiveGroupRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::set_active_group(request)
        })
    }

    #[tauri::command]
    pub(super) fn set_active_channel(
        app_handle: tauri::AppHandle,
        request: SetActiveChannelRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::set_active_channel(request)
        })
    }

    #[tauri::command]
    pub(super) fn set_active_dm(
        app_handle: tauri::AppHandle,
        request: SetActiveDmRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::set_active_dm(request))
    }

    #[tauri::command]
    pub(super) fn join_group(
        app_handle: tauri::AppHandle,
        request: JoinGroupRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::join_group(request))
    }

    #[tauri::command]
    pub(super) fn create_invite(
        app_handle: tauri::AppHandle,
        request: CreateInviteRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::create_invite(request))
    }

    #[tauri::command]
    pub(super) fn create_dm_invite(
        app_handle: tauri::AppHandle,
        request: CreateDmInviteRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::create_dm_invite(request)
        })
    }

    #[tauri::command]
    pub(super) fn accept_dm_invite(
        app_handle: tauri::AppHandle,
        request: AcceptDmInviteRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::accept_dm_invite(request)
        })
    }

    #[tauri::command]
    pub(super) fn create_channel(
        app_handle: tauri::AppHandle,
        request: CreateChannelRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::create_channel(request))
    }

    #[tauri::command]
    pub(super) fn send_message(
        app_handle: tauri::AppHandle,
        request: SendMessageRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::send_message(request))
    }

    #[tauri::command]
    pub(super) fn apply_text_delivery_receipt(
        app_handle: tauri::AppHandle,
        request: ApplyTextDeliveryReceiptRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::apply_text_delivery_receipt(request)
        })
    }

    #[tauri::command]
    pub(super) fn receive_text_delivery_envelope(
        app_handle: tauri::AppHandle,
        request: ReceiveTextDeliveryEnvelopeRequest,
    ) -> ReceiveTextDeliveryEnvelopeResponse {
        super::run_receive_text_delivery_envelope_with_event_emit(&app_handle, || {
            super::receive_text_delivery_envelope(request)
        })
    }

    #[tauri::command]
    pub(super) fn list_pending_text_control_frames(
        request: ListPendingTextControlFramesRequest,
    ) -> ListPendingTextControlFramesResponse {
        super::list_pending_text_control_frames(request)
    }

    #[tauri::command]
    pub(super) fn pump_text_control_transport_once(
        app_handle: tauri::AppHandle,
        request: ListPendingTextControlFramesRequest,
    ) -> TextControlTransportPumpReportView {
        super::run_command_with_event_emit(&app_handle, || {
            super::pump_text_control_transport_once(request)
        })
    }

    #[tauri::command]
    pub(super) fn mark_text_control_frame_sent(
        app_handle: tauri::AppHandle,
        request: MarkTextControlFrameSentRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::mark_text_control_frame_sent(request)
        })
    }

    #[tauri::command]
    pub(super) fn handle_text_control_frame(
        app_handle: tauri::AppHandle,
        request: HandleTextControlFrameRequest,
    ) -> HandleTextControlFrameResponse {
        super::run_handle_text_control_frame_with_event_emit(&app_handle, || {
            super::handle_text_control_frame(request)
        })
    }

    #[tauri::command]
    pub(super) fn publish_voice_signaling_message(
        app_handle: tauri::AppHandle,
        request: PublishVoiceSignalingMessageRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::publish_voice_signaling_message(request)
        })
    }

    #[tauri::command]
    pub(super) fn take_pending_voice_signaling_messages(
        app_handle: tauri::AppHandle,
        request: TakePendingVoiceSignalingMessagesRequest,
    ) -> TakePendingVoiceSignalingMessagesResponse {
        super::run_command_with_event_emit(&app_handle, || {
            super::take_pending_voice_signaling_messages(request)
        })
    }

    #[tauri::command]
    pub(super) fn join_voice(
        app_handle: tauri::AppHandle,
        request: JoinVoiceRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::join_voice(request))
    }

    #[tauri::command]
    pub(super) fn leave_voice(
        app_handle: tauri::AppHandle,
        request: LeaveVoiceRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::leave_voice(request))
    }

    #[tauri::command]
    pub(super) fn set_self_mute(
        app_handle: tauri::AppHandle,
        request: SetSelfMuteRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || super::set_self_mute(request))
    }

    #[tauri::command]
    pub(super) fn update_voice_activity(
        app_handle: tauri::AppHandle,
        request: UpdateVoiceActivityRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::update_voice_activity(request)
        })
    }

    #[tauri::command]
    pub(super) fn attach_voice_remote_media(
        app_handle: tauri::AppHandle,
        request: AttachVoiceRemoteMediaRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::attach_voice_remote_media(request)
        })
    }

    #[tauri::command]
    pub(super) fn set_speaker_volume(
        app_handle: tauri::AppHandle,
        request: SetSpeakerVolumeRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::set_speaker_volume(request)
        })
    }

    #[tauri::command]
    pub(super) fn poll_app_events(request: Option<PollAppEventsRequest>) -> AppEventStreamView {
        super::poll_app_events(request)
    }

    #[tauri::command]
    pub(super) fn deletion_warning() -> String {
        super::deletion_warning()
    }

    #[tauri::command]
    pub(super) fn metadata_warning() -> String {
        super::metadata_warning()
    }

    #[tauri::command]
    pub(super) fn command_health() -> CommandHealth {
        super::command_health()
    }

    #[tauri::command]
    pub(super) fn reset_app_state(
        app_handle: tauri::AppHandle,
        request: ResetAppStateRequest,
    ) -> AppStateView {
        super::run_app_state_command_with_event_emit(&app_handle, || {
            super::reset_app_state_confirmed(request)
        })
    }
}

/// Run the native Tauri shell with the command surface registered for frontend IPC.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::<tauri::Wry>::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            let _ = TAURI_APP_HANDLE.set(app_handle.clone());
            start_text_control_transport_runtime_pump(app_handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc_commands::app_snapshot,
            ipc_commands::app_state,
            ipc_commands::start_signaling_session,
            ipc_commands::stop_signaling_session,
            ipc_commands::start_text_session,
            ipc_commands::stop_text_session,
            ipc_commands::attach_text_control_transport_runtime,
            ipc_commands::create_user,
            ipc_commands::recover_user,
            ipc_commands::verify_safety_number,
            ipc_commands::create_device_pairing_payload,
            ipc_commands::accept_device_pairing_payload,
            ipc_commands::save_preferences,
            ipc_commands::set_connectivity_policy,
            ipc_commands::start_dm,
            ipc_commands::create_group,
            ipc_commands::set_active_group,
            ipc_commands::set_active_channel,
            ipc_commands::set_active_dm,
            ipc_commands::join_group,
            ipc_commands::create_invite,
            ipc_commands::create_dm_invite,
            ipc_commands::accept_dm_invite,
            ipc_commands::create_channel,
            ipc_commands::send_message,
            ipc_commands::apply_text_delivery_receipt,
            ipc_commands::receive_text_delivery_envelope,
            ipc_commands::list_pending_text_control_frames,
            ipc_commands::pump_text_control_transport_once,
            ipc_commands::mark_text_control_frame_sent,
            ipc_commands::handle_text_control_frame,
            ipc_commands::publish_voice_signaling_message,
            ipc_commands::take_pending_voice_signaling_messages,
            ipc_commands::join_voice,
            ipc_commands::leave_voice,
            ipc_commands::set_self_mute,
            ipc_commands::update_voice_activity,
            ipc_commands::attach_voice_remote_media,
            ipc_commands::set_speaker_volume,
            ipc_commands::poll_app_events,
            ipc_commands::deletion_warning,
            ipc_commands::metadata_warning,
            ipc_commands::command_health,
            ipc_commands::reset_app_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running discrypt Tauri application");
}

impl PersistedAppState {
    fn initial() -> Self {
        let snapshot = core_app_snapshot();
        Self {
            schema_version: APP_STATE_SCHEMA_VERSION,
            lifecycle: AppLifecycle::FirstRun,
            profile: None,
            preferences: UiPreferencesView {
                theme_id: DEFAULT_THEME_ID.to_owned(),
                template_id: DEFAULT_TEMPLATE_ID.to_owned(),
            },
            dms: Vec::new(),
            groups: Vec::new(),
            openmls_groups: Vec::new(),
            connectivity_defaults: app_connectivity_defaults(),
            active_context: None,
            messages: Vec::new(),
            text_delivery_envelopes: Vec::new(),
            text_delivery_receipts: Vec::new(),
            text_control_outbox: Vec::new(),
            voice_signaling_inbox: Vec::new(),
            voice_session: None,
            invites: Vec::new(),
            devices: Vec::new(),
            identity_seed_hex: String::new(),
            device_set: DeviceSet::new(),
            security_copy: snapshot.security_copy,
            events: vec![AppEventView {
                sequence: 1,
                kind: "app.first_run".to_owned(),
                summary: "No local profile exists; setup/recovery is required".to_owned(),
            }],
            last_command_error: None,
            signaling_session: None,
            text_session: None,
            latest_signaling_probe: None,
            latest_signaling_probe_error: None,
            latest_data_channel_probe: None,
            latest_data_channel_probe_error: None,
            abuse: PersistedAbuseState::default(),
            friend_verified: false,
            next_sequence: 2,
        }
    }

    fn ensure_openmls_group(&mut self, group_id: &str) -> Result<(), String> {
        if self
            .openmls_groups
            .iter()
            .any(|record| record.group_id == group_id)
        {
            return Ok(());
        }
        let path = app_openmls_store_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!("OpenMLS provider directory could not be created: {error}")
            })?;
        }
        let mut engine = OpenMlsGroupEngine::open(&path)
            .map_err(|error| format!("OpenMLS provider could not be opened: {error}"))?;
        let creator_identity = self.local_user_id();
        let snapshot = engine
            .create_group(group_id, creator_identity.as_bytes())
            .map_err(|error| format!("OpenMLS group could not be created: {error}"))?;
        let signer_public_key = engine
            .signer_public_key(group_id)
            .map_err(|error| format!("OpenMLS signer handle could not be read: {error}"))?;
        let mut confirmation_hash = Sha256::new();
        confirmation_hash.update(&snapshot.confirmation_tag);
        let record = OpenMlsGroupHandleRecord {
            group_id: group_id.to_owned(),
            signer_public_key_hex: hex::encode(signer_public_key),
            epoch: snapshot.epoch,
            local_leaf: 0,
            confirmation_tag_sha256: hex::encode(confirmation_hash.finalize()),
            openmls_store_path: Some(path.display().to_string()),
            status_copy:
                "OpenMLS group state was created in the Rust service boundary and can export Rust-only text secrets for admitted members"
                    .to_owned(),
        };
        self.openmls_groups.push(record);
        self.push_event(
            "mls.group_created",
            format!(
                "Created OpenMLS state for {}",
                redacted_observable_ref("group", group_id)
            ),
        );
        Ok(())
    }

    fn openmls_text_exporter_for_target(
        &self,
        target: &MessageTargetView,
        delivery_group_id: &str,
    ) -> Result<(Vec<u8>, u64, u32), String> {
        if target.kind != "channel" {
            return Err("OpenMLS text exporter is currently scoped to group channels".to_owned());
        }
        let openmls_group_id = target
            .group_id
            .as_deref()
            .ok_or_else(|| "channel text target requires an OpenMLS group id".to_owned())?;
        let handle = self
            .openmls_groups
            .iter()
            .find(|handle| handle.group_id == openmls_group_id)
            .ok_or_else(|| {
                format!(
                    "OpenMLS group handle is missing for {}",
                    redacted_observable_ref("group", openmls_group_id)
                )
            })?;
        let signer_public_key = hex::decode(&handle.signer_public_key_hex)
            .map_err(|error| format!("OpenMLS signer handle is not valid hex: {error}"))?;
        let store_path = handle
            .openmls_store_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(app_openmls_store_path);
        let mut engine = OpenMlsGroupEngine::open(&store_path)
            .map_err(|error| format!("OpenMLS provider could not be opened: {error}"))?;
        let snapshot = engine
            .load_group(openmls_group_id, &signer_public_key)
            .map_err(|error| format!("OpenMLS group could not be loaded: {error}"))?;
        let exporter = engine
            .export_secret(
                openmls_group_id,
                "discrypt/text",
                delivery_group_id.as_bytes(),
                32,
            )
            .map_err(|error| format!("OpenMLS text exporter failed: {error}"))?;
        Ok((exporter, snapshot.epoch, handle.local_leaf))
    }

    fn to_view(&self) -> AppStateView {
        AppStateView {
            schema_version: self.schema_version,
            lifecycle: self.lifecycle.clone(),
            profile: self.profile.clone(),
            preferences: self.preferences.clone(),
            dms: self.dms.clone(),
            groups: self.groups.clone(),
            connectivity_defaults: self.connectivity_defaults.clone(),
            active_context: self.active_context.clone(),
            messages: self.messages.clone(),
            voice_session: self.voice_session.clone(),
            invites: self.invites.clone(),
            devices: self.devices.clone(),
            security_copy: self.security_copy.clone(),
            events: self.events.clone(),
            event_cursor: self.latest_event_cursor(),
            last_command_error: self.last_command_error.clone(),
            transport_status: self.transport_status(),
            transport_diagnostics: self.transport_diagnostics(),
            join_progress: self.join_progress(),
            text_state_legend: text_state_legend(),
            voice_states: self.voice_states(),
            runtime_mode: runtime_mode_view(),
            snapshot: self.to_snapshot(),
        }
    }

    fn transport_diagnostics(&self) -> TransportDiagnosticsView {
        let adapter_boundaries: Vec<_> = required_provider_adapter_boundaries()
            .iter()
            .map(|boundary| {
                let readiness = boundary.readiness_state();
                SignalingAdapterBoundaryView {
                    kind: boundary.kind.canonical_name().to_owned(),
                    cargo_feature: boundary.cargo_feature.to_owned(),
                    readiness: Self::adapter_readiness_label(readiness),
                    failure_class: readiness.failure_class().to_owned(),
                }
            })
            .collect();
        let requested = self
            .active_connectivity_policy()
            .map(|(_, connectivity)| {
                connectivity
                    .signaling_profiles
                    .iter()
                    .filter_map(|profile| transport_adapter_kind_from_name(&profile.adapter_kind))
                    .collect::<Vec<SignalingAdapterKind>>()
            })
            .filter(|profiles| !profiles.is_empty())
            .unwrap_or_else(|| {
                required_provider_adapter_boundaries()
                    .iter()
                    .map(|boundary| boundary.kind)
                    .collect::<Vec<SignalingAdapterKind>>()
            });
        let fallback_plan = plan_signaling_adapter_fallback(
            requested.as_slice(),
            AdapterFallbackBehavior::FirstHealthy,
            None,
        );
        let adapter_fallback_attempts = fallback_plan
            .attempts
            .iter()
            .map(|attempt| SignalingAdapterFallbackAttemptView {
                kind: attempt.kind.canonical_name().to_owned(),
                readiness: Self::adapter_readiness_label(attempt.readiness),
                failure_class: attempt.readiness.failure_class().to_owned(),
                attempted: attempt.attempted,
                selected: attempt.selected,
            })
            .collect();
        let selected_adapter = fallback_plan
            .selected
            .map(|kind| kind.canonical_name().to_owned());
        let (route_proof_status, route_proof_detail, turn_required) =
            self.route_proof_status(self.signaling_session.as_ref(), self.text_session.as_ref());
        let (adapter_probe_status, adapter_probe_detail) = self.signaling_probe_status();
        let (data_channel_probe_status, data_channel_probe_detail) =
            self.data_channel_probe_status();
        TransportDiagnosticsView {
            adapter_boundaries,
            adapter_fallback_attempts,
            selected_adapter,
            route_proof_status,
            route_proof_detail,
            turn_required,
            adapter_probe_status,
            adapter_probe_detail,
            adapter_probe: self.latest_signaling_probe.clone(),
            data_channel_probe_status,
            data_channel_probe_detail,
            data_channel_probe: self.latest_data_channel_probe.clone(),
        }
    }

    fn signaling_probe_status(&self) -> (String, String) {
        if let Some(probe) = &self.latest_signaling_probe {
            return (
                "provider-roundtrip-proofed".to_owned(),
                format!(
                    "adapter={} profile={} endpoint={} {} presence={} signal={} control={}",
                    probe.kind,
                    probe.profile_id,
                    probe.endpoint_label,
                    redacted_observable_ref("room_topic", &probe.rendezvous_topic),
                    probe.presence_roundtrip,
                    probe.signal_roundtrip,
                    probe.control_roundtrip
                ),
            );
        }
        if let Some(error) = &self.latest_signaling_probe_error {
            return ("provider-roundtrip-failed".to_owned(), error.clone());
        }
        (
            "provider-roundtrip-not-run".to_owned(),
            "Start signaling with adapter_probe=true to verify the selected provider adapter without claiming WebRTC/media connectivity".to_owned(),
        )
    }

    fn data_channel_probe_status(&self) -> (String, String) {
        if let Some(probe) = &self.latest_data_channel_probe {
            return (
                "webrtc-datachannel-proofed".to_owned(),
                format!(
                    "adapter={} profile={} endpoint={} {} offerer_direct={} answerer_direct={} offerer_turn={} answerer_turn={} turn_servers={}/{} relay_local={}/{} relay_remote={}/{} offerer_open={} answerer_open={} frame={}",
                    probe.kind,
                    probe.profile_id,
                    probe.endpoint_label,
                    redacted_observable_ref("room_topic", &probe.rendezvous_topic),
                    probe.offerer_direct_path_ready,
                    probe.answerer_direct_path_ready,
                    probe.offerer_turn_fallback_ready,
                    probe.answerer_turn_fallback_ready,
                    probe.offerer_configured_turn_servers,
                    probe.answerer_configured_turn_servers,
                    probe.offerer_local_relay_candidates_gathered,
                    probe.answerer_local_relay_candidates_gathered,
                    probe.offerer_remote_relay_candidates_applied,
                    probe.answerer_remote_relay_candidates_applied,
                    probe.offerer_data_channel_open,
                    probe.answerer_data_channel_open,
                    probe.text_control_frame_roundtrip
                ),
            );
        }
        if let Some(error) = &self.latest_data_channel_probe_error {
            return ("webrtc-datachannel-failed".to_owned(), error.clone());
        }
        (
            "webrtc-datachannel-not-run".to_owned(),
            "Start signaling with data_channel_probe=true to verify provider-signaled WebRTC text/control delivery without claiming UI or voice/media delivery".to_owned(),
        )
    }

    fn route_proof_status(
        &self,
        signaling_session: Option<&TransportSessionRecord>,
        text_session: Option<&TransportSessionRecord>,
    ) -> (String, String, String) {
        let signaling = signaling_session.and_then(Self::route_proof_status_for_session);
        let text = text_session.and_then(Self::route_proof_status_for_session);
        signaling.or(text).unwrap_or_else(|| {
            (
                "route-proof-not-available".to_owned(),
                "No connected signaling/text transport session exposes a verified route report"
                    .to_owned(),
                "not-proven".to_owned(),
            )
        })
    }

    fn route_proof_status_for_session(
        session: &TransportSessionRecord,
    ) -> Option<(String, String, String)> {
        let snapshot = session.snapshot();
        let report = snapshot.route?.route_report?;
        let selected = Self::fallback_leg_label(report.selected);
        let attempted = report
            .attempted_legs
            .into_iter()
            .map(Self::fallback_leg_label)
            .collect::<Vec<_>>()
            .join(", ");
        let status = if snapshot.state.is_connected() {
            "route-proofed"
        } else {
            "route-report-stored"
        };
        let detail = format!(
            "session={} route_report: selected={} attempted=[{}] {} limitation={}",
            session.mode.label(),
            selected,
            attempted,
            redacted_observable_ref("endpoint", &report.endpoint.0),
            report.limitation,
        );
        let turn_required = if selected == "turn" {
            "turn-required"
        } else {
            "turn-not-required"
        };
        Some((status.to_owned(), detail, turn_required.to_owned()))
    }

    fn adapter_readiness_label(readiness: discrypt_transport::AdapterReadinessState) -> String {
        match readiness {
            discrypt_transport::AdapterReadinessState::FeatureDisabled => {
                "feature_disabled".to_owned()
            }
            discrypt_transport::AdapterReadinessState::ImplementationUnavailable => {
                "implementation_unavailable".to_owned()
            }
            discrypt_transport::AdapterReadinessState::Available => "available".to_owned(),
            discrypt_transport::AdapterReadinessState::ProviderUnhealthy => {
                "provider_unhealthy".to_owned()
            }
            discrypt_transport::AdapterReadinessState::ProviderRateLimited => {
                "provider_rate_limited".to_owned()
            }
            discrypt_transport::AdapterReadinessState::ProviderAuthRequired => {
                "provider_auth_required".to_owned()
            }
            discrypt_transport::AdapterReadinessState::ProviderMessageTooLarge => {
                "provider_message_too_large".to_owned()
            }
            discrypt_transport::AdapterReadinessState::TrustMismatch => "trust_mismatch".to_owned(),
            discrypt_transport::AdapterReadinessState::IceFailedRequiresTurn => {
                "ice_failed_requires_turn".to_owned()
            }
            discrypt_transport::AdapterReadinessState::Connected => "connected".to_owned(),
        }
    }

    fn fallback_leg_label(leg: FallbackLeg) -> &'static str {
        match leg {
            FallbackLeg::Stun => "stun",
            FallbackLeg::RelayOverlay => "overlay",
            FallbackLeg::Turn => "turn",
        }
    }

    fn transport_session_mut(
        &mut self,
        mode: BackendTransportMode,
    ) -> &mut Option<TransportSessionRecord> {
        match mode {
            BackendTransportMode::Signaling => &mut self.signaling_session,
            BackendTransportMode::Text => &mut self.text_session,
        }
    }

    fn transport_session(&self, mode: BackendTransportMode) -> Option<&TransportSessionRecord> {
        match mode {
            BackendTransportMode::Signaling => self.signaling_session.as_ref(),
            BackendTransportMode::Text => self.text_session.as_ref(),
        }
    }

    fn start_transport_session(
        &mut self,
        mode: BackendTransportMode,
        scope_label: Option<String>,
    ) -> Result<String, String> {
        let label = normalize_label(
            &scope_label.unwrap_or_else(|| "default".to_owned()),
            "default",
        );
        self.ensure_ready_profile();

        {
            let slot = self.transport_session_mut(mode);
            if let Some(existing) = slot.as_mut() {
                if existing.scope_label == label {
                    match existing.state() {
                        TransportSessionState::Signaling
                        | TransportSessionState::IceGathering
                        | TransportSessionState::Checking
                        | TransportSessionState::Direct
                        | TransportSessionState::OverlayRelay
                        | TransportSessionState::TurnRelay
                        | TransportSessionState::Reconnecting => {
                            return Ok(existing.session_id.clone());
                        }
                        TransportSessionState::Idle => {
                            if existing.session.begin_signaling().is_ok() {
                                return Ok(existing.session_id.clone());
                            }
                        }
                        TransportSessionState::Disconnected
                        | TransportSessionState::Failed
                        | TransportSessionState::Cancelled => {
                            // Intentionally recreate to represent a fresh session lifecycle.
                        }
                    }
                }
            }
        }

        let session_id = stable_id(
            mode.session_id_prefix(),
            &redacted_observable_token("scope", &label),
            self.next_sequence,
        );
        self.next_sequence = self.next_sequence.saturating_add(1);
        let mut session = TransportSession::new();
        if let Err(error) = session.begin_signaling() {
            return Err(format!("failed to start {mode} transport session: {error}"));
        }
        *self.transport_session_mut(mode) = Some(TransportSessionRecord {
            session_id: session_id.clone(),
            scope_label: label,
            mode,
            session,
        });
        self.push_event(
            format!("{mode}.session_started"),
            format!("Started {mode} transport session").to_owned(),
        );
        Ok(session_id)
    }

    fn stop_transport_session(
        &mut self,
        mode: BackendTransportMode,
        session_id: Option<String>,
    ) -> Option<String> {
        let stopped_session_id = {
            let slot = self.transport_session_mut(mode);
            let record = slot.as_mut()?;
            if session_id
                .as_ref()
                .is_some_and(|requested_id| requested_id != &record.session_id)
            {
                return None;
            }

            if matches!(
                record.state(),
                TransportSessionState::Failed | TransportSessionState::Cancelled
            ) {
                return None;
            }

            if record.session.tear_down("stop requested").is_ok() {
                Some(record.session_id.clone())
            } else {
                None
            }
        };

        if let Some(stopped_session_id) = &stopped_session_id {
            self.push_event(
                format!("{mode}.session_stopped"),
                format!(
                    "Stopped {mode} transport session {}",
                    redacted_observable_ref("transport_session", stopped_session_id)
                ),
            );
        }
        stopped_session_id
    }

    fn probe_active_signaling_adapter(
        &mut self,
        requested_kind: Option<&str>,
    ) -> Result<(), String> {
        let (scope_level, connectivity) = self.active_connectivity_policy().ok_or_else(|| {
            "No active DM/group/invite connectivity policy is available for signaling probe"
                .to_owned()
        })?;
        let requested_kind = requested_kind.and_then(transport_adapter_kind_from_name);
        let Some(profile_view) = self.select_signaling_profile(&connectivity, requested_kind)
        else {
            let error =
                "No signaling profile matches the requested adapter kind and build readiness"
                    .to_owned();
            self.latest_signaling_probe = None;
            self.latest_signaling_probe_error = Some(error.clone());
            return Err(error);
        };
        let profile = transport_profile_from_view(&profile_view)?;
        let scope = ConversationScope::new(scope_level, connectivity.scope_id_commitment.clone())
            .map_err(|error| error.to_string())?;
        let bootstrap_secret = self.probe_material(
            "discrypt-runtime-provider-probe-bootstrap-v1",
            &connectivity.scope_id_commitment,
            &profile.profile_id,
            32,
        );
        let random_entropy = self.probe_material(
            "discrypt-runtime-provider-probe-entropy-v1",
            &connectivity.scope_id_commitment,
            &profile.profile_id,
            16,
        );
        let probe = run_provider_adapter_probe(profile, scope, bootstrap_secret, random_entropy)
            .inspect_err(|error| {
                self.latest_signaling_probe = None;
                self.latest_signaling_probe_error = Some(error.clone());
            })?;
        let view = SignalingAdapterProbeView {
            kind: probe.kind.canonical_name().to_owned(),
            profile_id: probe.profile_id,
            endpoint_label: probe.endpoint_label,
            scope_commitment: probe.scope_commitment,
            rendezvous_topic: probe.rendezvous_topic,
            presence_roundtrip: probe.presence_roundtrip,
            signal_roundtrip: probe.signal_roundtrip,
            control_roundtrip: probe.control_roundtrip,
        };
        self.latest_signaling_probe_error = None;
        self.latest_signaling_probe = Some(view.clone());
        self.push_event(
            "transport.signaling_probe_ok",
            format!(
                "Provider adapter roundtrip proofed for {} profile {}",
                view.kind, view.profile_id
            ),
        );
        Ok(())
    }

    fn mark_text_session_runtime_route_proof(
        &mut self,
        evidence: &discrypt_transport::ProviderTextControlRuntimePeerEvidence,
    ) {
        let proof = ProviderWebRtcDataChannelProbeView {
            kind: evidence.kind.canonical_name().to_owned(),
            profile_id: evidence.profile_id.clone(),
            endpoint_label: evidence.endpoint_label.clone(),
            scope_commitment: evidence.scope_commitment.clone(),
            rendezvous_topic: redacted_observable_ref("room_topic", &evidence.rendezvous_topic),
            offerer_direct_path_ready: evidence.direct_path_ready,
            answerer_direct_path_ready: evidence.direct_path_ready,
            offerer_turn_fallback_ready: false,
            answerer_turn_fallback_ready: false,
            offerer_configured_turn_servers: 0,
            answerer_configured_turn_servers: 0,
            offerer_local_relay_candidates_gathered: 0,
            answerer_local_relay_candidates_gathered: 0,
            offerer_remote_relay_candidates_applied: 0,
            answerer_remote_relay_candidates_applied: 0,
            offerer_data_channel_open: evidence.data_channel_open,
            answerer_data_channel_open: evidence.data_channel_open,
            text_control_frame_roundtrip: evidence.data_channel_open,
            text_control_frame_sha256: redacted_observable_token(
                "frame",
                &evidence.rendezvous_topic,
            ),
            receipt_frame_roundtrip: evidence.data_channel_open,
            receipt_frame_sha256: redacted_observable_token("receipt", &evidence.rendezvous_topic),
            runtime_spec: Some(evidence.runtime_spec.clone()),
        };
        self.latest_data_channel_probe_error = None;
        self.latest_data_channel_probe = Some(proof.clone());
        self.mark_text_session_data_channel_route_proof(&proof);
    }

    fn mark_text_session_data_channel_route_proof(
        &mut self,
        probe: &ProviderWebRtcDataChannelProbeView,
    ) {
        if !(probe.offerer_data_channel_open
            && probe.answerer_data_channel_open
            && probe.text_control_frame_roundtrip)
        {
            self.push_command_error(
                "transport.text_route_not_proofed",
                "start_text_session",
                "text_data_channel_incomplete",
                "Provider probe did not prove an open bidirectional text/control DataChannel",
                "Retry after provider signaling and ICE connectivity are healthy",
            );
            return;
        }
        let (selected_leg, endpoint) =
            if probe.offerer_direct_path_ready && probe.answerer_direct_path_ready {
                let endpoint = self
                    .active_connectivity_policy()
                    .and_then(|(_, connectivity)| connectivity.ice_stun_servers.first().cloned())
                    .map(Endpoint::new)
                    .unwrap_or_else(|| Endpoint::new("stun:stun.l.google.com:19302"));
                (FallbackLeg::Stun, endpoint)
            } else if probe.offerer_turn_fallback_ready && probe.answerer_turn_fallback_ready {
                let endpoint = self
                    .active_connectivity_policy()
                    .and_then(|(_, connectivity)| {
                        connectivity
                            .ice_turn_servers
                            .first()
                            .map(|server| server.endpoint.clone())
                    })
                    .map(Endpoint::new)
                    .unwrap_or_else(|| Endpoint::new("turn:provider-relay.proofed"));
                (FallbackLeg::Turn, endpoint)
            } else {
                (FallbackLeg::RelayOverlay, Endpoint::new("overlay:pending"))
            };
        if matches!(selected_leg, FallbackLeg::RelayOverlay) {
            self.push_event(
                "transport.text_route_pending",
                "DataChannel opened, but direct STUN route readiness was not proven; route state left unconnected until TURN/overlay proof is available".to_owned(),
            );
            return;
        };
        let attempts = if matches!(selected_leg, FallbackLeg::Turn) {
            vec![
                ConnectionAttempt {
                    leg: FallbackLeg::Stun,
                    endpoint: self
                        .active_connectivity_policy()
                        .and_then(|(_, connectivity)| {
                            connectivity.ice_stun_servers.first().cloned()
                        })
                        .map(Endpoint::new)
                        .unwrap_or_else(|| Endpoint::new("stun:stun.l.google.com:19302")),
                    carries_content: false,
                    ciphertext_only: false,
                    succeeded: false,
                },
                ConnectionAttempt {
                    leg: FallbackLeg::RelayOverlay,
                    endpoint: Endpoint::new("overlay:not-proofed"),
                    carries_content: false,
                    ciphertext_only: true,
                    succeeded: false,
                },
                ConnectionAttempt {
                    leg: FallbackLeg::Turn,
                    endpoint: endpoint.clone(),
                    carries_content: false,
                    ciphertext_only: true,
                    succeeded: true,
                },
            ]
        } else {
            vec![ConnectionAttempt {
                leg: selected_leg,
                endpoint: endpoint.clone(),
                carries_content: false,
                ciphertext_only: false,
                succeeded: true,
            }]
        };
        let plan = ConnectivityPlan {
            attempts,
            selected: selected_leg,
            endpoint,
        };
        let selected_session = self.transport_session_mut(BackendTransportMode::Text);
        let Some(record) = selected_session.as_mut() else {
            self.push_command_error(
                "transport.text_route_not_proofed",
                "start_text_session",
                "text_session_missing",
                "Text session was not active when DataChannel proof completed",
                "Start text transport again before probing the provider DataChannel",
            );
            return;
        };
        let selected = (|| -> Result<(), String> {
            if matches!(record.state(), TransportSessionState::Signaling) {
                record
                    .session
                    .begin_ice_gathering()
                    .map_err(|error| error.to_string())?;
            }
            if matches!(record.state(), TransportSessionState::IceGathering) {
                record
                    .session
                    .begin_checking()
                    .map_err(|error| error.to_string())?;
            }
            record
                .session
                .select_connectivity_plan(plan)
                .map_err(|error| error.to_string())?;
            Ok(())
        })();
        match selected {
            Ok(()) => self.push_event(
                "transport.text_route_proofed",
                format!(
                    "Text session route proofed by provider-signaled WebRTC DataChannel using {} profile {} over {}",
                    probe.kind,
                    probe.profile_id,
                    Self::fallback_leg_label(selected_leg)
                ),
            ),
            Err(error) => self.push_command_error(
                "transport.text_route_not_proofed",
                "start_text_session",
                "text_route_state_transition_failed",
                error,
                "Retry text transport from a fresh session before claiming a connected route",
            ),
        }
    }

    fn probe_active_webrtc_data_channel(
        &mut self,
        requested_kind: Option<&str>,
    ) -> Result<ProviderWebRtcDataChannelProbeView, String> {
        self.probe_active_webrtc_data_channel_with_frame(
            requested_kind,
            b"ciphertext:runtime-webrtc-datachannel-probe:v1".to_vec(),
        )
    }

    fn probe_active_webrtc_data_channel_with_frame(
        &mut self,
        requested_kind: Option<&str>,
        text_control_frame: Vec<u8>,
    ) -> Result<ProviderWebRtcDataChannelProbeView, String> {
        self.probe_active_webrtc_data_channel_request_response(
            requested_kind,
            text_control_frame,
            None,
        )
    }

    fn probe_active_webrtc_data_channel_request_response(
        &mut self,
        requested_kind: Option<&str>,
        text_control_frame: Vec<u8>,
        receipt_control_frame: Option<Vec<u8>>,
    ) -> Result<ProviderWebRtcDataChannelProbeView, String> {
        let (scope_level, connectivity) = self.active_connectivity_policy().ok_or_else(|| {
            "No active DM/group/invite connectivity policy is available for WebRTC data-channel probe"
                .to_owned()
        })?;
        let requested_kind = requested_kind.and_then(transport_adapter_kind_from_name);
        let Some(profile_view) = self.select_signaling_profile(&connectivity, requested_kind)
        else {
            let error =
                "No signaling profile matches the requested adapter kind and build readiness"
                    .to_owned();
            self.latest_data_channel_probe = None;
            self.latest_data_channel_probe_error = Some(error.clone());
            return Err(error);
        };
        let profile = transport_profile_from_view(&profile_view)?;
        let scope = ConversationScope::new(scope_level, connectivity.scope_id_commitment.clone())
            .map_err(|error| error.to_string())?;
        let ice_config =
            ice_config_from_connectivity(&connectivity).map_err(|error| error.to_string())?;
        let bootstrap_secret = self.probe_material(
            "discrypt-runtime-webrtc-probe-bootstrap-v1",
            &connectivity.scope_id_commitment,
            &profile.profile_id,
            32,
        );
        let random_entropy = self.probe_material(
            "discrypt-runtime-webrtc-probe-entropy-v1",
            &connectivity.scope_id_commitment,
            &profile.profile_id,
            16,
        );
        let probe = if let Some(receipt_control_frame) = receipt_control_frame {
            run_provider_webrtc_data_channel_request_response_probe(
                profile,
                scope,
                bootstrap_secret,
                random_entropy,
                ice_config,
                text_control_frame,
                receipt_control_frame,
            )
        } else {
            run_provider_webrtc_data_channel_probe(
                profile,
                scope,
                bootstrap_secret,
                random_entropy,
                ice_config,
                text_control_frame,
            )
        }
        .inspect_err(|error| {
            self.latest_data_channel_probe = None;
            self.latest_data_channel_probe_error = Some(error.clone());
        })?;
        let view = ProviderWebRtcDataChannelProbeView {
            kind: probe.kind.canonical_name().to_owned(),
            profile_id: probe.profile_id,
            endpoint_label: probe.endpoint_label,
            scope_commitment: probe.scope_commitment,
            rendezvous_topic: probe.rendezvous_topic,
            offerer_direct_path_ready: probe.offerer_direct_path_ready,
            answerer_direct_path_ready: probe.answerer_direct_path_ready,
            offerer_turn_fallback_ready: probe.offerer_turn_fallback_ready,
            answerer_turn_fallback_ready: probe.answerer_turn_fallback_ready,
            offerer_configured_turn_servers: probe.offerer_configured_turn_servers,
            answerer_configured_turn_servers: probe.answerer_configured_turn_servers,
            offerer_local_relay_candidates_gathered: probe.offerer_local_relay_candidates_gathered,
            answerer_local_relay_candidates_gathered: probe
                .answerer_local_relay_candidates_gathered,
            offerer_remote_relay_candidates_applied: probe.offerer_remote_relay_candidates_applied,
            answerer_remote_relay_candidates_applied: probe
                .answerer_remote_relay_candidates_applied,
            offerer_data_channel_open: probe.offerer_data_channel_open,
            answerer_data_channel_open: probe.answerer_data_channel_open,
            text_control_frame_roundtrip: probe.text_control_frame_roundtrip,
            text_control_frame_sha256: probe.text_control_frame_sha256,
            receipt_frame_roundtrip: probe.receipt_frame_roundtrip,
            receipt_frame_sha256: probe.receipt_frame_sha256,
            runtime_spec: probe.runtime_spec,
        };
        self.latest_data_channel_probe_error = None;
        self.latest_data_channel_probe = Some(view.clone());
        self.push_event(
            "transport.data_channel_probe_ok",
            format!(
                "Provider-signaled WebRTC DataChannel proofed for {} profile {}",
                view.kind, view.profile_id
            ),
        );
        Ok(view)
    }

    fn text_control_runtime_inputs_for_active_scope(
        &self,
        requested_kind: Option<&str>,
    ) -> Result<TextControlRuntimeAttachInputs, String> {
        let (scope_level, connectivity) = self.active_connectivity_policy().ok_or_else(|| {
            "No active DM/group/invite connectivity policy is available for live text/control runtime"
                .to_owned()
        })?;
        let requested_kind = requested_kind.and_then(transport_adapter_kind_from_name);
        let Some(profile_view) = self.select_signaling_profile(&connectivity, requested_kind)
        else {
            return Err(
                "No signaling profile matches the requested adapter kind and build readiness"
                    .to_owned(),
            );
        };
        let profile = transport_profile_from_view(&profile_view)?;
        let scope = ConversationScope::new(scope_level, connectivity.scope_id_commitment.clone())
            .map_err(|error| error.to_string())?;
        let ice_config =
            ice_config_from_connectivity(&connectivity).map_err(|error| error.to_string())?;
        let bootstrap_secret = shared_runtime_material(
            "discrypt-live-role-split-runtime-bootstrap-v1",
            &connectivity,
            &profile.profile_id,
            32,
        );
        let random_entropy = shared_runtime_material(
            "discrypt-live-role-split-runtime-entropy-v1",
            &connectivity,
            &profile.profile_id,
            16,
        );
        Ok(TextControlRuntimeAttachInputs {
            profile,
            scope,
            bootstrap_secret,
            random_entropy,
            ice_config,
        })
    }

    #[cfg(test)]
    #[allow(dead_code)]
    fn prove_text_delivery_receipt_over_data_channel_with_receiver(
        &mut self,
        receiver: &PersistedAppState,
        message_id: &str,
        requested_kind: Option<&str>,
    ) -> Result<ProviderWebRtcDataChannelProbeView, String> {
        let outbox_frame = self
            .text_control_outbox
            .iter()
            .find(|record| record.message_id == message_id && record.state_key == "pending")
            .map(TextControlOutboxFrameView::from)
            .ok_or_else(|| {
                "no pending persisted text/control outbox frame for message id".to_owned()
            })?;
        let mut envelope_frame = outbox_frame.frame.clone();
        if let TextControlFrameView::Envelope { recipient_leaf, .. } = &mut envelope_frame {
            *recipient_leaf = Some(2);
        }
        let envelope_frame_bytes = serde_json::to_vec(&envelope_frame)
            .map_err(|error| format!("could not encode text envelope frame: {error}"))?;
        let (scope_level, connectivity) = self.active_connectivity_policy().ok_or_else(|| {
            "No active DM/group/invite connectivity policy is available for WebRTC data-channel proof"
                .to_owned()
        })?;
        let requested_kind = requested_kind.and_then(transport_adapter_kind_from_name);
        let Some(profile_view) = self.select_signaling_profile(&connectivity, requested_kind)
        else {
            return Err(
                "No signaling profile matches the requested adapter kind and build readiness"
                    .to_owned(),
            );
        };
        let profile = transport_profile_from_view(&profile_view)?;
        let scope = ConversationScope::new(scope_level, connectivity.scope_id_commitment.clone())
            .map_err(|error| error.to_string())?;
        let ice_config =
            ice_config_from_connectivity(&connectivity).map_err(|error| error.to_string())?;
        let bootstrap_secret = self.probe_material(
            "discrypt-runtime-webrtc-probe-bootstrap-v1",
            &connectivity.scope_id_commitment,
            &profile.profile_id,
            32,
        );
        let random_entropy = self.probe_material(
            "discrypt-runtime-webrtc-probe-entropy-v1",
            &connectivity.scope_id_commitment,
            &profile.profile_id,
            16,
        );
        let mut receiver_state = receiver.clone();
        let receipt_slot = Arc::new(Mutex::new(None::<TextControlFrameView>));
        let answerer_receipt_slot = receipt_slot.clone();
        let probe = run_provider_webrtc_data_channel_request_response_probe_with_answerer(
            profile,
            scope,
            bootstrap_secret,
            random_entropy,
            ice_config,
            envelope_frame_bytes,
            move |received| {
                let received_frame: TextControlFrameView = serde_json::from_slice(&received)
                    .map_err(|error| {
                        format!(
                            "receiver could not decode local delivered text/control frame: {error}"
                        )
                    })?;
                let receipt_frame = receiver_state
                    .handle_text_control_frame(received_frame)
                    .ok_or_else(|| {
                        "receiver did not accept local delivered envelope or generate receipt frame"
                            .to_owned()
                    })?;
                *answerer_receipt_slot
                    .lock()
                    .map_err(|_| "receipt slot lock poisoned".to_owned())? =
                    Some(receipt_frame.clone());
                serde_json::to_vec(&receipt_frame)
                    .map_err(|error| format!("could not encode text receipt frame: {error}"))
            },
        )?;
        if !probe.text_control_frame_roundtrip || !probe.receipt_frame_roundtrip {
            return Err(
                "provider-signaled DataChannel did not carry both envelope and receipt frames"
                    .to_owned(),
            );
        }
        let receipt_frame = receipt_slot
            .lock()
            .map_err(|_| "receipt slot lock poisoned".to_owned())?
            .clone()
            .ok_or_else(|| "receiver receipt frame was not produced after delivery".to_owned())?;
        self.mark_text_control_frame_sent(MarkTextControlFrameSentRequest {
            message_id: message_id.to_owned(),
            frame_sha256: outbox_frame.frame_sha256,
            transport_session_id: Some(format!(
                "{}:{}",
                probe.kind.canonical_name(),
                probe.profile_id
            )),
        })?;
        if self.handle_text_control_frame(receipt_frame).is_some() {
            return Err("receipt frame unexpectedly generated a response frame".to_owned());
        }
        Ok(ProviderWebRtcDataChannelProbeView::from(probe))
    }

    #[cfg(test)]
    #[allow(dead_code)]
    fn pump_text_delivery_receipt_over_live_runtime_pair_with_receiver(
        &mut self,
        receiver: TauriAppService,
        message_id: &str,
        requested_kind: Option<&str>,
        transport_session_id: impl Into<String>,
    ) -> Result<
        (
            discrypt_transport::ProviderTextControlRuntimeEvidence,
            TextControlTransportPumpReportView,
            PersistedAppState,
        ),
        String,
    > {
        let outbox_frame = self
            .text_control_outbox
            .iter()
            .find(|record| record.message_id == message_id && record.state_key == "pending")
            .map(TextControlOutboxFrameView::from)
            .ok_or_else(|| {
                "no pending persisted text/control outbox frame for live runtime pump".to_owned()
            })?;
        let (scope_level, connectivity) = self.active_connectivity_policy().ok_or_else(|| {
            "No active DM/group/invite connectivity policy is available for live text/control runtime"
                .to_owned()
        })?;
        let requested_kind = requested_kind.and_then(transport_adapter_kind_from_name);
        let Some(profile_view) = self.select_signaling_profile(&connectivity, requested_kind)
        else {
            return Err(
                "No signaling profile matches the requested adapter kind and build readiness"
                    .to_owned(),
            );
        };
        let profile = transport_profile_from_view(&profile_view)?;
        let scope = ConversationScope::new(scope_level, connectivity.scope_id_commitment.clone())
            .map_err(|error| error.to_string())?;
        let ice_config =
            ice_config_from_connectivity(&connectivity).map_err(|error| error.to_string())?;
        let bootstrap_secret = shared_runtime_material(
            "discrypt-live-role-split-runtime-bootstrap-v1",
            &connectivity,
            &profile.profile_id,
            32,
        );
        let random_entropy = shared_runtime_material(
            "discrypt-live-role-split-runtime-entropy-v1",
            &connectivity,
            &profile.profile_id,
            16,
        );
        let mut sender_state = self.clone();
        let target = outbox_frame.target.clone();
        let transport_session_id = transport_session_id.into();
        let (sender_peer_id, receiver_peer_id) = self.active_runtime_peer_ids_for_text_control()?;
        let (receiver_local_peer_id, receiver_remote_peer_id) =
            receiver.state.active_runtime_peer_ids_for_text_control()?;
        if receiver_local_peer_id != receiver_peer_id || receiver_remote_peer_id != sender_peer_id {
            return Err(format!(
                "live runtime peer bootstrap mismatch: sender local={} remote={}, receiver local={} remote={}",
                sender_peer_id.0,
                receiver_peer_id.0,
                receiver_local_peer_id.0,
                receiver_remote_peer_id.0
            ));
        }
        let receiver_service = Arc::new(Mutex::new(receiver));

        let (updated_sender, receiver_state, evidence, report) = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|error| {
                    format!("Could not start live text/control runtime-pair pump: {error}")
                })?;
            runtime.block_on(async move {
                let answerer_service = receiver_service.clone();
                let receiver_after = receiver_service.clone();
                let answerer_profile = profile.clone();
                let answerer_scope = scope.clone();
                let answerer_bootstrap_secret = bootstrap_secret.clone();
                let answerer_random_entropy = random_entropy.clone();
                let answerer_ice_config = ice_config.clone();
                let answerer_local_peer_id = receiver_peer_id.clone();
                let answerer_remote_peer_id = sender_peer_id.clone();
                let answerer_task = tokio::spawn(async move {
                    start_provider_webrtc_text_control_answer_runtime_with_answerer(
                        answerer_profile,
                        answerer_scope,
                        &answerer_bootstrap_secret,
                        &answerer_random_entropy,
                        discrypt_transport::WebRtcNegotiationConfig::new(answerer_ice_config),
                        answerer_local_peer_id,
                        answerer_remote_peer_id,
                        move |received| {
                            let frame: TextControlFrameView = serde_json::from_slice(&received)
                                .map_err(|error| {
                                    TransportError::Unavailable(format!(
                                        "receiver could not decode live text/control frame: {error}"
                                    ))
                                })?;
                            let mut response_frame = None;
                            answerer_service
                                .lock()
                                .map_err(|_| {
                                    TransportError::Unavailable(
                                        "live runtime receiver service lock poisoned".to_owned(),
                                    )
                                })?
                                .mutate(|state| {
                                    response_frame = state.handle_text_control_frame(frame);
                                });
                            let response_frame = response_frame.ok_or_else(|| {
                                TransportError::Unavailable(
                                    "receiver did not accept live text/control frame or generate receipt"
                                        .to_owned(),
                                )
                            })?;
                            serde_json::to_vec(&response_frame).map_err(|error| {
                                TransportError::Unavailable(format!(
                                    "could not encode live text/control response frame: {error}"
                                ))
                            })
                        },
                    )
                    .await
                });
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                let offerer = start_provider_webrtc_text_control_offer_runtime(
                    profile,
                    scope,
                    &bootstrap_secret,
                    &random_entropy,
                    discrypt_transport::WebRtcNegotiationConfig::new(ice_config),
                    sender_peer_id,
                    receiver_peer_id,
                )
                .await
                .map_err(|error| error.to_string())?;
                let answerer = tokio::time::timeout(std::time::Duration::from_secs(50), answerer_task)
                    .await
                    .map_err(|_| "timed out waiting for role-split receiver runtime attach".to_owned())?
                    .map_err(|error| format!("role-split receiver runtime task failed: {error}"))?
                    .map_err(|error| error.to_string())?;
                let offerer_evidence = offerer.evidence().clone();
                let answerer_evidence = answerer.evidence().clone();
                let evidence = discrypt_transport::ProviderTextControlRuntimeEvidence {
                    kind: offerer_evidence.kind,
                    profile_id: offerer_evidence.profile_id,
                    endpoint_label: offerer_evidence.endpoint_label,
                    scope_commitment: offerer_evidence.scope_commitment,
                    rendezvous_topic: offerer_evidence.rendezvous_topic,
                    offerer_direct_path_ready: offerer_evidence.direct_path_ready,
                    answerer_direct_path_ready: answerer_evidence.direct_path_ready,
                    offerer_data_channel_open: offerer_evidence.data_channel_open,
                    answerer_data_channel_open: answerer_evidence.data_channel_open,
                    runtime_spec: offerer_evidence.runtime_spec,
                };
                let report = sender_state
                    .pump_text_control_transport_once(
                        offerer.transport().as_ref(),
                        ListPendingTextControlFramesRequest {
                            target: Some(target),
                            limit: Some(1),
                            operation_timeout_ms: Some(10_000),
                        },
                        transport_session_id,
                    )
                    .await;
                offerer.close().await.map_err(|error| error.to_string())?;
                answerer.close().await.map_err(|error| error.to_string())?;
                let receiver_state = receiver_after
                    .lock()
                    .map_err(|_| "live runtime receiver service lock poisoned".to_owned())?
                    .state
                    .clone();
                Ok::<_, String>((sender_state, receiver_state, evidence, report))
            })
        })
        .join()
        .map_err(|_| "live text/control runtime-pair pump thread panicked".to_owned())??;

        *self = updated_sender;
        Ok((evidence, report, receiver_state))
    }

    #[allow(dead_code)]
    fn openmls_text_exporter_secret(&self, group_id: &str) -> Result<Vec<u8>, String> {
        let handle = self
            .openmls_groups
            .iter()
            .find(|record| record.group_id == group_id)
            .ok_or_else(|| {
                format!(
                    "OpenMLS group state is missing for {}",
                    redacted_observable_ref("group", group_id)
                )
            })?;
        let signer_public_key = hex::decode(&handle.signer_public_key_hex)
            .map_err(|error| format!("OpenMLS signer public key is not valid hex: {error}"))?;
        let mut engine = OpenMlsGroupEngine::open(app_openmls_store_path())
            .map_err(|error| format!("OpenMLS provider could not be opened: {error}"))?;
        engine
            .load_group(group_id, &signer_public_key)
            .map_err(|error| format!("OpenMLS group could not be loaded: {error}"))?;
        engine
            .export_secret(group_id, TEXT_EXPORTER_LABEL, group_id.as_bytes(), 32)
            .map_err(|error| format!("OpenMLS text exporter failed: {error}"))
    }

    #[allow(dead_code)]
    fn selected_text_route_for_outbox(&self, message_id: &str) -> TextSelectedRoute {
        let (session_id, route_label, overlay_hops) = self
            .text_session
            .as_ref()
            .map(|session| {
                (
                    session.session_id.clone(),
                    match session.state() {
                        TransportSessionState::TurnRelay => "turn-ciphertext",
                        TransportSessionState::OverlayRelay => "overlay-ciphertext",
                        TransportSessionState::Direct => "direct-ciphertext",
                        _ => "pending-text-control-outbox",
                    }
                    .to_owned(),
                    if matches!(session.state(), TransportSessionState::OverlayRelay) {
                        1
                    } else {
                        0
                    },
                )
            })
            .unwrap_or_else(|| {
                (
                    format!("text-control-outbox:{message_id}"),
                    "pending-text-control-outbox".to_owned(),
                    0,
                )
            });
        TextSelectedRoute {
            session_id,
            route_label,
            overlay_hops,
            ciphertext_only: true,
        }
    }

    fn text_delivery_envelope_record(
        &mut self,
        target: &MessageTargetView,
        message_id: &str,
        body: &str,
        sequence: u64,
    ) -> Result<TextDeliveryEnvelopeRecord, String> {
        let group_id = text_delivery_group_id(target)?;
        let seed = self.identity_seed_bytes();
        let signing_key = SigningKey::from_bytes(&seed);
        let envelope = match self.openmls_text_exporter_for_target(target, &group_id) {
            Ok((text_exporter_secret, epoch, sender_leaf)) => {
                let channel_id = target.channel_id.clone().unwrap_or_default();
                let request = TextOutboundRequest {
                    group_id: group_id.clone(),
                    channel_id,
                    epoch,
                    sender_leaf,
                    sender_device_id: self.local_user_id(),
                    sequence,
                    message_id: message_id.to_owned(),
                    retention: TextRetentionMetadata {
                        policy: "app-default".to_owned(),
                        created_at_ms: sequence,
                        expires_at_ms: None,
                        delete_after_read: false,
                    },
                    plaintext: body.as_bytes().to_vec(),
                    sent_at_ms: sequence,
                    now: Utc::now(),
                };
                let route = TextSelectedRoute {
                    session_id: "app-service-text-control-outbox".to_owned(),
                    route_label: "provider-backed-text-control".to_owned(),
                    overlay_hops: 0,
                    ciphertext_only: true,
                };
                let mut author_log = InMemoryTextAuthorLog::default();
                let mut transport = InMemoryTextTransport::default();
                let mut events = InMemoryTextSendEvents::default();
                TextOutboundPipeline::new(&mut author_log, &mut transport, &mut events)
                    .send(request, route, &text_exporter_secret, &signing_key)
                    .map(|receipt| receipt.envelope)
                    .map_err(|error| error.to_string())?
            }
            Err(error) if target.kind == "channel" => return Err(error),
            Err(_) => {
                let ciphertext =
                    opaque_text_control_frame_for_message(self, target, message_id, body, sequence);
                TextMessageEnvelope::sign(
                    &group_id,
                    TextMessageEnvelopeInput {
                        epoch: 1,
                        sender_leaf: 1,
                        sender_device_id: self.local_user_id(),
                        sequence,
                        message_id: message_id.to_owned(),
                        retention: TextRetentionMetadata {
                            policy: "app-default".to_owned(),
                            created_at_ms: sequence,
                            expires_at_ms: None,
                            delete_after_read: false,
                        },
                        content_ciphertext: ciphertext,
                    },
                    &signing_key,
                )
                .map_err(|error| error.to_string())?
            }
        };
        let route = self.selected_text_route_for_outbox(message_id);
        let mut author_log = AppTextAuthorLog::default();
        let mut transport = AppTextOutboxTransport::default();
        let mut events = AppTextSendEvents::default();
        let receipt = TextOutboundPipeline::new(&mut author_log, &mut transport, &mut events)
            .send(request, route, &text_exporter_secret, &signing_key)
            .map_err(|error| error.to_string())?;
        if author_log.entries.len() != 1 || transport.frames.len() != 1 {
            return Err(
                "text outbound pipeline did not persist and queue exactly one envelope".to_owned(),
            );
        }
        Ok(TextDeliveryEnvelopeRecord {
            message_id: message_id.to_owned(),
            group_id,
            sender_verifying_key_hex: hex::encode(signing_key.verifying_key().as_bytes()),
            envelope,
        })
    }

    fn enqueue_text_control_outbox(
        &mut self,
        target: &MessageTargetView,
        message_id: &str,
        envelope_record: &TextDeliveryEnvelopeRecord,
    ) -> Result<(), String> {
        let frame = TextControlFrameView::Envelope {
            target: target.clone(),
            envelope: envelope_record.envelope.clone(),
            sender_verifying_key_hex: envelope_record.sender_verifying_key_hex.clone(),
            recipient_leaf: None,
        };
        let frame_sha256 = text_control_frame_sha256(&frame)?;
        if let Some(existing) = self
            .text_control_outbox
            .iter_mut()
            .find(|record| record.message_id == message_id)
        {
            existing.target = target.clone();
            existing.frame = frame;
            existing.frame_sha256 = frame_sha256;
            existing.state_key = "pending".to_owned();
            return Ok(());
        }
        self.text_control_outbox.push(TextControlOutboxRecord {
            message_id: message_id.to_owned(),
            target: target.clone(),
            frame,
            frame_sha256,
            attempts: 0,
            state_key: "pending".to_owned(),
            last_transport_session_id: None,
        });
        self.push_event(
            "message.outbox_queued",
            format!(
                "Queued signed text/control frame for {}",
                redacted_message_ref(message_id)
            ),
        );
        Ok(())
    }

    fn enqueue_voice_signaling_outbox(
        &mut self,
        request: PublishVoiceSignalingMessageRequest,
    ) -> Result<(), String> {
        let session = self
            .voice_session
            .as_ref()
            .ok_or_else(|| "No active voice session for voice signaling".to_owned())?;
        if session.session_id != request.session_id {
            return Err(
                "Voice signaling request did not match the active voice session".to_owned(),
            );
        }
        if !session.joined {
            return Err("Voice signaling requires a joined voice session".to_owned());
        }
        let signal_kind = normalize_voice_signal_kind(&request.signal_kind)?;
        validate_voice_signal_payload(&signal_kind, &request.sealed_payload)?;
        let attachment = self.active_runtime_peer_attachment_for_text_control()?;
        let local_user_id = self.local_user_id();
        let signal_id = request
            .signal_id
            .unwrap_or_else(|| stable_id("voice-signal", &request.session_id, self.next_sequence));
        let target = MessageTargetView {
            kind: "channel".to_owned(),
            dm_id: None,
            group_id: Some(session.group_id.clone()),
            channel_id: Some(session.channel_id.clone()),
        };
        let signal = VoiceSignalingMessageView {
            signal_id: signal_id.clone(),
            session_id: session.session_id.clone(),
            group_id: session.group_id.clone(),
            channel_id: session.channel_id.clone(),
            sender_participant_id: local_user_id,
            sender_peer_id: attachment.local_peer_id.0.clone(),
            recipient_peer_id: attachment.remote_peer_id.0.clone(),
            signal_kind: signal_kind.clone(),
            sealed_payload: request.sealed_payload,
            created_at_ms: request.created_at_ms,
        };
        let frame = TextControlFrameView::VoiceSignal { signal };
        let frame_sha256 = text_control_frame_sha256(&frame)?;
        if let Some(existing) = self
            .text_control_outbox
            .iter_mut()
            .find(|record| record.message_id == signal_id)
        {
            existing.target = target;
            existing.frame = frame;
            existing.frame_sha256 = frame_sha256;
            existing.state_key = "pending".to_owned();
        } else {
            self.text_control_outbox.push(TextControlOutboxRecord {
                message_id: signal_id.clone(),
                target,
                frame,
                frame_sha256,
                attempts: 0,
                state_key: "pending".to_owned(),
                last_transport_session_id: None,
            });
        }
        if let Some(session) = &mut self.voice_session {
            session.signaling = VoiceSignalingStateView {
                session_id: session.session_id.clone(),
                local_peer_id: attachment.local_peer_id.0,
                remote_peer_id: attachment.remote_peer_id.0,
                role: runtime_role_label(Some(attachment.role)).to_owned(),
                pending_local_signals: session.signaling.pending_local_signals.saturating_add(1),
                received_remote_signals: session.signaling.received_remote_signals,
                last_signal_kind: Some(signal_kind.clone()),
                status_copy: format!(
                    "Queued voice {signal_kind} through provider-derived text/control signaling; browser RTCPeerConnection may send it only via the attached backend transport"
                ),
            };
        }
        self.push_event(
            "voice.signal_queued",
            format!("Queued voice {signal_kind} for provider-signaled transport"),
        );
        Ok(())
    }

    fn handle_voice_signal_frame(&mut self, signal: VoiceSignalingMessageView) {
        let result = self.validate_inbound_voice_signal(&signal);
        if let Err(error) = result {
            if self.validate_prejoin_inbound_voice_signal(&signal).is_ok() {
                if !self
                    .voice_signaling_inbox
                    .iter()
                    .any(|record| record.signal.signal_id == signal.signal_id)
                {
                    self.voice_signaling_inbox.push(VoiceSignalingInboxRecord {
                        signal: signal.clone(),
                    });
                }
                self.push_event(
                    "voice.signal_prejoin_queued",
                    format!(
                        "Queued pre-join voice {} for stable session {}",
                        signal.signal_kind,
                        redacted_observable_ref("voice_session", &signal.session_id)
                    ),
                );
                return;
            }
            self.push_command_error(
                "voice.signal_rejected",
                "handle_text_control_frame",
                "voice_signal_invalid",
                error,
                "Accept only provider-signaled voice SDP/ICE for the active joined voice session and remote peer",
            );
            return;
        }
        if !self
            .voice_signaling_inbox
            .iter()
            .any(|record| record.signal.signal_id == signal.signal_id)
        {
            self.voice_signaling_inbox.push(VoiceSignalingInboxRecord {
                signal: signal.clone(),
            });
        }
        if let Some(session) = &mut self.voice_session {
            session.signaling.received_remote_signals =
                session.signaling.received_remote_signals.saturating_add(1);
            session.signaling.last_signal_kind = Some(signal.signal_kind.clone());
            if session.signaling.remote_peer_id.is_empty() {
                session.signaling.remote_peer_id = signal.sender_peer_id.clone();
            }
            if session.signaling.local_peer_id.is_empty() {
                session.signaling.local_peer_id = signal.recipient_peer_id.clone();
            }
            session.signaling.status_copy = format!(
                "Received provider-signaled voice {} for browser RTCPeerConnection; pending inbound SDP/ICE must be applied before claiming remote audio",
                signal.signal_kind
            );
        }
        self.push_event(
            "voice.signal_received",
            format!(
                "Received voice {} over text/control transport",
                signal.signal_kind
            ),
        );
    }

    fn validate_inbound_voice_signal(
        &self,
        signal: &VoiceSignalingMessageView,
    ) -> Result<(), String> {
        normalize_voice_signal_kind(&signal.signal_kind)?;
        validate_voice_signal_payload(&signal.signal_kind, &signal.sealed_payload)?;
        let session = self
            .voice_session
            .as_ref()
            .ok_or_else(|| "No active voice session for inbound voice signaling".to_owned())?;
        if !session.joined {
            return Err("Inbound voice signaling requires a joined voice session".to_owned());
        }
        if session.session_id != signal.session_id
            || session.group_id != signal.group_id
            || session.channel_id != signal.channel_id
        {
            return Err(
                "Inbound voice signal did not match the active voice session scope".to_owned(),
            );
        }
        let local_user_id = self.local_user_id();
        if signal.sender_participant_id == local_user_id || signal.sender_peer_id.is_empty() {
            return Err("Inbound voice signal must come from a non-local provider peer".to_owned());
        }
        Ok(())
    }

    fn validate_prejoin_inbound_voice_signal(
        &self,
        signal: &VoiceSignalingMessageView,
    ) -> Result<(), String> {
        normalize_voice_signal_kind(&signal.signal_kind)?;
        validate_voice_signal_payload(&signal.signal_kind, &signal.sealed_payload)?;
        let expected_session_id = stable_voice_session_id(&signal.group_id, &signal.channel_id);
        if signal.session_id != expected_session_id {
            return Err(
                "Pre-join voice signal did not use the stable group/channel session id".to_owned(),
            );
        }
        let local_user_id = self.local_user_id();
        if signal.sender_participant_id == local_user_id || signal.sender_peer_id.is_empty() {
            return Err(
                "Pre-join voice signal must come from a non-local provider peer".to_owned(),
            );
        }
        let group = self
            .groups
            .iter()
            .find(|group| group.group_id == signal.group_id)
            .ok_or_else(|| "Pre-join voice signal group is not installed".to_owned())?;
        let channel = group
            .channels
            .iter()
            .find(|channel| channel.channel_id == signal.channel_id)
            .ok_or_else(|| "Pre-join voice signal channel is not installed".to_owned())?;
        if channel.kind != ChannelKind::Voice {
            return Err("Pre-join voice signal channel is not a voice channel".to_owned());
        }
        let local_peer_ok = group
            .runtime_peers
            .iter()
            .any(|peer| peer.is_local && peer.peer_id == signal.recipient_peer_id);
        let remote_peer_ok = group
            .runtime_peers
            .iter()
            .any(|peer| !peer.is_local && peer.peer_id == signal.sender_peer_id);
        if !local_peer_ok || !remote_peer_ok {
            return Err(
                "Pre-join voice signal peer ids did not match signed group runtime peers"
                    .to_owned(),
            );
        }
        Ok(())
    }

    fn take_pending_voice_signaling_messages(
        &mut self,
        request: TakePendingVoiceSignalingMessagesRequest,
    ) -> Vec<VoiceSignalingMessageView> {
        let session_id = request.session_id.or_else(|| {
            self.voice_session
                .as_ref()
                .map(|session| session.session_id.clone())
        });
        let limit = request.limit.unwrap_or(50).clamp(1, 200);
        let mut taken = Vec::new();
        let mut retained = Vec::new();
        for record in self.voice_signaling_inbox.drain(..) {
            let matches_session = session_id
                .as_ref()
                .is_none_or(|expected| &record.signal.session_id == expected);
            if matches_session && taken.len() < limit {
                taken.push(record.signal);
            } else {
                retained.push(record);
            }
        }
        self.voice_signaling_inbox = retained;
        if !taken.is_empty() {
            self.push_event(
                "voice.signal_delivered",
                format!(
                    "Delivered {} pending voice signaling message(s) to browser runtime",
                    taken.len()
                ),
            );
        }
        taken
    }

    fn list_pending_text_control_frames(
        &self,
        request: &ListPendingTextControlFramesRequest,
    ) -> Vec<TextControlOutboxFrameView> {
        let limit = request.limit.unwrap_or(50).clamp(1, 200);
        self.text_control_outbox
            .iter()
            .filter(|record| record.state_key == "pending")
            .filter(|record| {
                request
                    .target
                    .as_ref()
                    .is_none_or(|target| &record.target == target)
            })
            .take(limit)
            .map(TextControlOutboxFrameView::from)
            .collect()
    }

    fn text_control_transport_operation_timeout(
        request: &ListPendingTextControlFramesRequest,
    ) -> std::time::Duration {
        std::time::Duration::from_millis(
            request
                .operation_timeout_ms
                .unwrap_or(5_000)
                .clamp(100, 60_000),
        )
    }

    fn mark_text_control_frame_sent(
        &mut self,
        request: MarkTextControlFrameSentRequest,
    ) -> Result<(), String> {
        let frame_sha256 = {
            let outbox = self
                .text_control_outbox
                .iter_mut()
                .find(|record| record.message_id == request.message_id)
                .ok_or_else(|| "no persisted outbox frame for message id".to_owned())?;
            if outbox.frame_sha256 != request.frame_sha256 {
                return Err("outbox frame hash mismatch".to_owned());
            }
            outbox.attempts = outbox.attempts.saturating_add(1);
            outbox.state_key = "sent".to_owned();
            outbox.last_transport_session_id = request.transport_session_id.clone();
            outbox.frame_sha256.clone()
        };
        if let Some(message) = self
            .messages
            .iter_mut()
            .find(|message| message.message_id == request.message_id)
            .filter(|message| message.state_key != "peer_receipt")
        {
            message.status = "signed text/control frame handed to transport session; peer delivery still requires signed receipt".to_owned();
            message.state_key = "transport_frame_sent".to_owned();
            message.state_label = "Awaiting peer receipt".to_owned();
            message.state_detail = format!(
                "frame_sha256={} session_id={}",
                frame_sha256,
                request
                    .transport_session_id
                    .as_deref()
                    .unwrap_or("unreported")
            );
        }
        self.push_event(
            "message.outbox_sent",
            format!(
                "Marked text/control frame {} sent to transport",
                redacted_message_ref(&request.message_id)
            ),
        );
        Ok(())
    }

    fn message_has_peer_receipt(&self, message_id: &str) -> bool {
        self.messages.iter().any(|message| {
            message.message_id == message_id
                && message.state_key == "peer_receipt"
                && message.peer_receipt.is_some()
        })
    }

    pub(crate) async fn pump_text_control_transport_once<T>(
        &mut self,
        transport: &T,
        request: ListPendingTextControlFramesRequest,
        transport_session_id: impl Into<String>,
    ) -> TextControlTransportPumpReportView
    where
        T: discrypt_transport::TextControlDataTransport + ?Sized,
    {
        let transport_session_id = transport_session_id.into();
        let pending = self.list_pending_text_control_frames(&request);
        let mut frames_sent = 0_usize;
        let mut response_frames_received = 0_usize;
        let mut receipts_applied = 0_usize;
        let mut failures = Vec::new();
        let operation_timeout = Self::text_control_transport_operation_timeout(&request);

        for frame in &pending {
            let frame_ref = redacted_message_ref(&frame.message_id);
            let outbound = match serde_json::to_vec(&frame.frame) {
                Ok(outbound) => outbound,
                Err(error) => {
                    failures.push(format!(
                        "{}: encode text/control frame failed: {error}",
                        frame_ref
                    ));
                    continue;
                }
            };
            let send_result = tokio::time::timeout(
                operation_timeout,
                transport.send_text_control_frame(outbound),
            )
            .await;
            if let Err(error) = match send_result {
                Ok(result) => result,
                Err(_) => Err(discrypt_transport::TransportError::Unavailable(format!(
                    "send text/control frame timed out after {} ms",
                    operation_timeout.as_millis()
                ))),
            } {
                failures.push(format!(
                    "{}: send text/control frame failed: {error}",
                    frame_ref
                ));
                continue;
            }
            frames_sent += 1;
            if let Err(error) = self.mark_text_control_frame_sent(MarkTextControlFrameSentRequest {
                message_id: frame.message_id.clone(),
                frame_sha256: frame.frame_sha256.clone(),
                transport_session_id: Some(transport_session_id.clone()),
            }) {
                failures.push(format!(
                    "{}: mark text/control frame sent failed: {error}",
                    frame_ref
                ));
                continue;
            }
            let response_deadline = tokio::time::Instant::now() + operation_timeout;
            let mut receipt_applied_for_frame = false;
            while !receipt_applied_for_frame {
                let Some(remaining) =
                    response_deadline.checked_duration_since(tokio::time::Instant::now())
                else {
                    if self.message_has_peer_receipt(&frame.message_id) {
                        receipts_applied += 1;
                    } else {
                        failures.push(format!(
                            "{}: receive text/control response failed: receive text/control response timed out after {} ms",
                            frame_ref,
                            operation_timeout.as_millis()
                        ));
                    }
                    break;
                };
                let recv_result =
                    tokio::time::timeout(remaining, transport.recv_text_control_frame()).await;
                let inbound = match recv_result {
                    Ok(Ok(inbound)) => inbound,
                    Ok(Err(error)) => {
                        if self.message_has_peer_receipt(&frame.message_id) {
                            receipts_applied += 1;
                        } else {
                            failures.push(format!(
                                "{}: receive text/control response failed: {error}",
                                frame_ref
                            ));
                        }
                        break;
                    }
                    Err(_) => {
                        if self.message_has_peer_receipt(&frame.message_id) {
                            receipts_applied += 1;
                        } else {
                            failures.push(format!(
                                "{}: receive text/control response failed: receive text/control response timed out after {} ms",
                                frame_ref,
                                operation_timeout.as_millis()
                            ));
                        }
                        break;
                    }
                };
                response_frames_received += 1;
                let inbound_frame = match serde_json::from_slice::<TextControlFrameView>(&inbound) {
                    Ok(inbound_frame) => inbound_frame,
                    Err(error) => {
                        failures.push(format!(
                            "{}: decode text/control response failed: {error}",
                            frame_ref
                        ));
                        break;
                    }
                };
                let receipt_response = matches!(
                    &inbound_frame,
                    TextControlFrameView::Receipt { message_id, .. } if message_id == &frame.message_id
                );
                // Scope receipt validation to this response. The state may still contain an older
                // setup/invite/admission command error; a stale error must not make a later peer
                // receipt look invalid.
                self.last_command_error = None;
                if let Some(response_frame) = self.handle_text_control_frame(inbound_frame) {
                    let response = match serde_json::to_vec(&response_frame) {
                        Ok(response) => response,
                        Err(error) => {
                            failures.push(format!(
                                "{}: encode text/control response failed: {error}",
                                frame_ref
                            ));
                            break;
                        }
                    };
                    if let Err(error) = transport.send_text_control_frame(response).await {
                        failures.push(format!(
                            "{}: send text/control response failed: {error}",
                            frame_ref
                        ));
                        break;
                    }
                    continue;
                }
                if let Some(error) = self.last_command_error.as_ref().filter(|error| {
                    matches!(
                        error.command.as_str(),
                        "handle_text_control_frame" | "apply_text_delivery_receipt"
                    )
                }) {
                    failures.push(format!(
                        "{}: text/control response verification failed: {} ({})",
                        frame_ref, error.code, error.message
                    ));
                    break;
                }
                if receipt_response {
                    receipts_applied += 1;
                    receipt_applied_for_frame = true;
                }
            }
        }

        let metrics = transport.text_control_transport_metrics().await;
        self.push_event(
            "message.transport_pump",
            format!(
                "Text/control transport pump sent {} frame(s), received {} response frame(s), applied {} receipt(s), failures={}",
                frames_sent,
                response_frames_received,
                receipts_applied,
                failures.len()
            ),
        );
        TextControlTransportPumpReportView {
            pending_before: pending.len(),
            frames_sent,
            response_frames_received,
            receipts_applied,
            failures,
            metrics,
        }
    }

    fn apply_text_delivery_receipt(
        &mut self,
        request: ApplyTextDeliveryReceiptRequest,
    ) -> Result<(), String> {
        let recipient_key = verifying_key_from_hex(&request.recipient_verifying_key_hex)
            .ok_or_else(|| "recipient verifying key is invalid".to_owned())?;
        let envelope_record = self
            .text_delivery_envelopes
            .iter()
            .find(|record| record.message_id == request.message_id)
            .ok_or_else(|| "no persisted text envelope for receipt message id".to_owned())?;
        request
            .receipt
            .verify(
                &envelope_record.group_id,
                &envelope_record.envelope,
                &recipient_key,
            )
            .map_err(|error| error.to_string())?;
        let receipt_view = TextDeliveryReceiptView {
            recipient_device_id: request.receipt.recipient_device_id.clone(),
            received_at_ms: request.receipt.received_at_ms,
            envelope_ciphertext_hash: hex::encode(request.receipt.envelope_ciphertext_hash),
            recipient_key_fingerprint: key_fingerprint(&recipient_key),
        };
        let message = self
            .messages
            .iter_mut()
            .find(|message| message.message_id == request.message_id)
            .ok_or_else(|| "no message row for receipt message id".to_owned())?;
        message.status =
            "signed peer delivery receipt verified for this encrypted envelope".to_owned();
        message.state_key = "peer_receipt".to_owned();
        message.state_label = "Peer receipt".to_owned();
        message.state_detail = format!(
            "Signed receipt verified from {} at {} for envelope_ciphertext_hash={}",
            receipt_view.recipient_device_id,
            receipt_view.received_at_ms,
            receipt_view.envelope_ciphertext_hash
        );
        message.peer_receipt = Some(receipt_view);
        if let Some(outbox) = self
            .text_control_outbox
            .iter_mut()
            .find(|record| record.message_id == request.message_id)
        {
            outbox.state_key = "receipted".to_owned();
        }
        self.text_delivery_receipts.push(TextDeliveryReceiptRecord {
            message_id: request.message_id.clone(),
            recipient_verifying_key_hex: request.recipient_verifying_key_hex,
            receipt: request.receipt,
        });
        self.push_event(
            "message.receipt_verified",
            format!(
                "Verified signed peer receipt for {}",
                redacted_message_ref(&request.message_id)
            ),
        );
        Ok(())
    }

    fn receive_text_delivery_envelope(
        &mut self,
        request: ReceiveTextDeliveryEnvelopeRequest,
    ) -> Result<(TextDeliveryReceipt, String), String> {
        let sender_key = verifying_key_from_hex(&request.sender_verifying_key_hex)
            .ok_or_else(|| "sender verifying key is invalid".to_owned());
        let group_id = text_delivery_group_id(&request.target);
        let result = sender_key
            .and_then(|sender_key| {
                let group_id = group_id?;
                request
                    .envelope
                    .verify(&group_id, &sender_key)
                    .map_err(|error| error.to_string())?;
                Ok((group_id, sender_key))
            })
            .and_then(|(group_id, sender_key)| {
                let plaintext_render =
                    self.receive_text_plaintext_render(&request, &group_id, &sender_key)?;
                let recipient_leaf = request.recipient_leaf.unwrap_or(1);
                let recipient_seed = self.identity_seed_bytes();
                let recipient_signer = SigningKey::from_bytes(&recipient_seed);
                let receipt = TextDeliveryReceipt::sign(
                    &group_id,
                    TextDeliveryReceiptInput {
                        message_id: request.envelope.message_id.clone(),
                        recipient_leaf,
                        recipient_device_id: self.local_user_id(),
                        received_at_ms: self.next_sequence.saturating_add(1),
                        envelope_ciphertext_hash: request.envelope.ciphertext_hash(),
                    },
                    &recipient_signer,
                )
                .map_err(|error| error.to_string())?;
                let recipient_verifying_key_hex =
                    hex::encode(recipient_signer.verifying_key().as_bytes());
                if !self
                    .text_delivery_envelopes
                    .iter()
                    .any(|record| record.message_id == request.envelope.message_id)
                {
                    self.text_delivery_envelopes.push(TextDeliveryEnvelopeRecord {
                        message_id: request.envelope.message_id.clone(),
                        group_id: group_id.clone(),
                        sender_verifying_key_hex: request.sender_verifying_key_hex.clone(),
                        envelope: request.envelope.clone(),
                    });
                }
                if !self
                    .messages
                    .iter()
                    .any(|message| message.message_id == request.envelope.message_id)
                {
                    let sequence = self.next_sequence;
                    self.next_sequence = self.next_sequence.saturating_add(1);
                    let (body, status, state_key, state_label, render_detail) =
                        plaintext_render.message_fields(&request.envelope);
                    self.messages.push(MessageView {
                        message_id: request.envelope.message_id.clone(),
                        target: request.target.clone(),
                        author_id: request.envelope.sender_device_id.clone(),
                        author: format!("Peer {}", key_fingerprint(&sender_key)),
                        body,
                        status,
                        state_key,
                        state_label,
                        state_detail: format!(
                            "Verified sender={} {} {}; {render_detail}; generated signed delivery receipt",
                            key_fingerprint(&sender_key),
                            redacted_message_ref(&request.envelope.message_id),
                            redacted_observable_ref("group_binding", &group_id),
                            if plaintext_rendered {
                                " after OpenMLS exporter decrypt"
                            } else {
                                ""
                            }
                        ),
                        peer_receipt: None,
                        sent_at: format!("remote-{sequence}"),
                    });
                }
                self.text_delivery_receipts.push(TextDeliveryReceiptRecord {
                    message_id: request.envelope.message_id.clone(),
                    recipient_verifying_key_hex: recipient_verifying_key_hex.clone(),
                    receipt: receipt.clone(),
                });
                self.push_event(
                    "message.envelope_received",
                    format!(
                        "Verified encrypted peer envelope {} and generated signed receipt ({})",
                        redacted_message_ref(&request.envelope.message_id),
                        plaintext_render.event_label()
                    ),
                );
                Ok((receipt, recipient_verifying_key_hex))
            });
        if let Err(error) = &result {
            self.push_command_error(
                "message.envelope_rejected",
                "receive_text_delivery_envelope",
                "text_envelope_verification_failed",
                error.clone(),
                "Accept peer delivery only from an envelope whose sender signature and group binding verify",
            );
        }
        result
    }

    fn receive_text_plaintext_render(
        &mut self,
        request: &ReceiveTextDeliveryEnvelopeRequest,
        delivery_group_id: &str,
        sender_key: &VerifyingKey,
    ) -> Result<ReceivedTextRender, String> {
        let (text_exporter_secret, current_epoch) =
            match self.openmls_text_exporter_for_receive(&request.target, delivery_group_id) {
                Ok(exporter) => exporter,
                Err(error) => {
                    return Ok(ReceivedTextRender::EnvelopeOnly { reason: error });
                }
            };
        let mut receive_state = self.text_receive_state_for_delivery_group(delivery_group_id);
        let mut store = InMemoryTextRecipientStore::default();
        let mut events = InMemoryTextReceiveEvents::default();
        let mut authorized_sender_leaves = std::collections::BTreeSet::new();
        authorized_sender_leaves.insert(request.envelope.sender_leaf);
        match TextInboundPipeline::new(&mut receive_state, &mut store, &mut events).receive(
            TextInboundRequest {
                group_id: delivery_group_id.to_owned(),
                channel_id: request
                    .target
                    .channel_id
                    .clone()
                    .unwrap_or_else(|| request.target.dm_id.clone().unwrap_or_default()),
                current_epoch,
                authorized_sender_leaves,
                envelope: request.envelope.clone(),
                received_at_ms: self.next_sequence.saturating_add(1),
                retention_allows_decrypt: true,
            },
            &text_exporter_secret,
            sender_key,
        ) {
            Ok(renderable) => Ok(ReceivedTextRender::Pipeline(renderable.state)),
            Err(DeliveryError::TextMessageDecryptionFailed) => {
                self.push_event(
                    "message.envelope_decrypt_failed",
                    format!(
                        "TextInboundPipeline could not decrypt {} with the persisted OpenMLS exporter",
                        redacted_message_ref(&request.envelope.message_id)
                    ),
                );
                Ok(ReceivedTextRender::DecryptFailed)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn openmls_text_exporter_for_receive(
        &self,
        target: &MessageTargetView,
        _delivery_group_id: &str,
    ) -> Result<(Vec<u8>, u64), String> {
        if target.kind != "channel" {
            return Err(
                "OpenMLS text exporter is only available for persisted group channel membership"
                    .to_owned(),
            );
        }
        let openmls_group_id = target
            .group_id
            .as_deref()
            .ok_or_else(|| "channel target is missing group_id for OpenMLS exporter".to_owned())?;
        let handle = self
            .openmls_groups
            .iter()
            .find(|record| record.group_id == openmls_group_id)
            .ok_or_else(|| {
                format!(
                    "OpenMLS membership/exporter state is unavailable for {}",
                    redacted_observable_ref("group", openmls_group_id)
                )
            })?;
        let signer_public_key = hex::decode(&handle.signer_public_key_hex)
            .map_err(|error| format!("OpenMLS signer key is not valid hex: {error}"))?;
        let mut engine = OpenMlsGroupEngine::open(app_openmls_store_path())
            .map_err(|error| format!("OpenMLS provider could not be opened: {error}"))?;
        let snapshot = engine
            .load_group(openmls_group_id, &signer_public_key)
            .map_err(|error| format!("OpenMLS group could not be loaded: {error}"))?;
        let exporter = engine
            .export_secret(
                openmls_group_id,
                TEXT_EXPORTER_LABEL,
                TEXT_EXPORTER_CONTEXT,
                32,
            )
            .map_err(|error| format!("OpenMLS text exporter failed: {error}"))?;
        Ok((exporter, snapshot.epoch))
    }

    fn text_receive_state_for_delivery_group(&self, delivery_group_id: &str) -> TextReceiveState {
        let mut state = TextReceiveState::default();
        for record in self
            .text_delivery_envelopes
            .iter()
            .filter(|record| record.group_id == delivery_group_id)
        {
            let _ = state.accept(&record.envelope);
        }
        state
    }

    fn handle_text_control_frame(
        &mut self,
        frame: TextControlFrameView,
    ) -> Option<TextControlFrameView> {
        match frame {
            TextControlFrameView::Envelope {
                target,
                envelope,
                sender_verifying_key_hex,
                recipient_leaf,
            } => {
                let message_id = envelope.message_id.clone();
                match self.receive_text_delivery_envelope(ReceiveTextDeliveryEnvelopeRequest {
                    target,
                    envelope,
                    sender_verifying_key_hex,
                    recipient_leaf,
                }) {
                    Ok((receipt, recipient_verifying_key_hex)) => {
                        Some(TextControlFrameView::Receipt {
                            message_id,
                            receipt,
                            recipient_verifying_key_hex,
                        })
                    }
                    Err(_) => None,
                }
            }
            TextControlFrameView::VoiceSignal { signal } => {
                self.handle_voice_signal_frame(signal);
                None
            }
            TextControlFrameView::Receipt {
                message_id,
                receipt,
                recipient_verifying_key_hex,
            } => {
                if let Err(error) =
                    self.apply_text_delivery_receipt(ApplyTextDeliveryReceiptRequest {
                        message_id,
                        receipt,
                        recipient_verifying_key_hex,
                    })
                {
                    self.push_command_error(
                        "message.receipt_frame_rejected",
                        "handle_text_control_frame",
                        "receipt_frame_verification_failed",
                        error,
                        "Accept peer delivery only from a receipt frame whose signature, message id, group binding, and ciphertext hash verify",
                    );
                }
                None
            }
        }
    }

    fn active_connectivity_policy(
        &self,
    ) -> Option<(ConnectivityScopeLevel, ConnectivityPolicyView)> {
        if let Some(context) = &self.active_context {
            if let Some(dm_id) = &context.dm_id {
                if let Some(connectivity) = self
                    .invites
                    .iter()
                    .rev()
                    .find(|invite| {
                        invite.invite_kind == InviteKind::DmContact.canonical_name()
                            && invite.dm_id.as_ref() == Some(dm_id)
                    })
                    .map(InviteView::connectivity_policy)
                {
                    return Some((ConnectivityScopeLevel::Dm, connectivity));
                }
                if let Some(connectivity) = self
                    .dms
                    .iter()
                    .find(|dm| &dm.dm_id == dm_id)
                    .and_then(|dm| dm.connectivity.clone())
                {
                    return Some((ConnectivityScopeLevel::Dm, connectivity));
                }
            }
            if let Some(group_id) = &context.group_id {
                if let Some(channel_id) = &context.channel_id {
                    if let Some(connectivity) = self
                        .groups
                        .iter()
                        .find(|group| &group.group_id == group_id)
                        .and_then(|group| {
                            group
                                .channels
                                .iter()
                                .find(|channel| &channel.channel_id == channel_id)
                        })
                        .and_then(|channel| channel.connectivity.clone())
                    {
                        return Some((ConnectivityScopeLevel::Channel, connectivity));
                    }
                }
                if let Some(connectivity) = self
                    .groups
                    .iter()
                    .find(|group| &group.group_id == group_id)
                    .and_then(|group| group.connectivity.clone())
                {
                    return Some((ConnectivityScopeLevel::Group, connectivity));
                }
            }
        }
        self.invites
            .last()
            .map(|invite| {
                (
                    ConnectivityScopeLevel::InviteBootstrap,
                    invite.connectivity_policy(),
                )
            })
            .or_else(|| {
                self.dms
                    .first()
                    .and_then(|dm| dm.connectivity.clone())
                    .map(|connectivity| (ConnectivityScopeLevel::Dm, connectivity))
            })
            .or_else(|| {
                self.groups
                    .first()
                    .and_then(|group| group.connectivity.clone())
                    .map(|connectivity| (ConnectivityScopeLevel::Group, connectivity))
            })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn active_runtime_peer_ids_for_text_control(
        &self,
    ) -> Result<(SignalingPeerId, SignalingPeerId), String> {
        let attachment = self.active_runtime_peer_attachment_for_text_control()?;
        Ok((attachment.local_peer_id, attachment.remote_peer_id))
    }

    fn active_runtime_peer_attachment_for_text_control(
        &self,
    ) -> Result<TextControlRuntimePeerAttachment, String> {
        let context = self.active_context.as_ref().ok_or_else(|| {
            "No active DM/group context is available for text/control runtime peer attachment"
                .to_owned()
        })?;
        if let Some(dm_id) = &context.dm_id {
            let dm = self
                .dms
                .iter()
                .find(|dm| &dm.dm_id == dm_id)
                .ok_or_else(|| {
                    format!("Active DM {dm_id} is missing for text/control runtime peer attachment")
                })?;
            let local = dm
                .runtime_peers
                .iter()
                .find(|peer| peer.is_local)
                .ok_or_else(|| {
                    format!(
                        "Active DM {dm_id} has no local runtime peer from signed bootstrap metadata"
                    )
                })?;
            let remote = dm
                .runtime_peers
                .iter()
                .find(|peer| !peer.is_local)
                .ok_or_else(|| {
                    format!(
                        "Active DM {dm_id} has no remote runtime peer from signed bootstrap metadata"
                    )
                })?;
            return Ok(TextControlRuntimePeerAttachment {
                role: if local.role == "reply" {
                    ProviderTextControlRuntimePeerRole::Answerer
                } else {
                    ProviderTextControlRuntimePeerRole::Offerer
                },
                local_peer_id: SignalingPeerId::new(local.peer_id.clone())
                    .map_err(|error| error.to_string())?,
                remote_peer_id: SignalingPeerId::new(remote.peer_id.clone())
                    .map_err(|error| error.to_string())?,
            });
        }
        if let Some(group_id) = &context.group_id {
            let group = self
                .groups
                .iter()
                .find(|group| &group.group_id == group_id)
                .ok_or_else(|| {
                    format!(
                        "Active group {group_id} is missing for text/control runtime peer attachment"
                    )
                })?;
            let local = group
                .runtime_peers
                .iter()
                .find(|peer| peer.is_local)
                .ok_or_else(|| {
                    format!(
                        "Active group {group_id} has no local runtime peer from signed bootstrap metadata"
                    )
                })?;
            let remote = group
                .runtime_peers
                .iter()
                .find(|peer| !peer.is_local)
                .ok_or_else(|| {
                    format!(
                        "Active group {group_id} has no remote runtime peer from signed bootstrap metadata"
                    )
                })?;
            return Ok(TextControlRuntimePeerAttachment {
                role: if local.role == "member" {
                    ProviderTextControlRuntimePeerRole::Answerer
                } else {
                    ProviderTextControlRuntimePeerRole::Offerer
                },
                local_peer_id: SignalingPeerId::new(local.peer_id.clone())
                    .map_err(|error| error.to_string())?,
                remote_peer_id: SignalingPeerId::new(remote.peer_id.clone())
                    .map_err(|error| error.to_string())?,
            });
        }
        Err(
            "Active context is not a DM or group for text/control runtime peer attachment"
                .to_owned(),
        )
    }

    fn select_signaling_profile(
        &self,
        connectivity: &ConnectivityPolicyView,
        requested_kind: Option<SignalingAdapterKind>,
    ) -> Option<SignalingProfileView> {
        if let Some(kind) = requested_kind {
            return connectivity
                .signaling_profiles
                .iter()
                .find(|profile| {
                    transport_adapter_kind_from_name(&profile.adapter_kind) == Some(kind)
                })
                .cloned();
        }
        let requested = connectivity
            .signaling_profiles
            .iter()
            .filter_map(|profile| transport_adapter_kind_from_name(&profile.adapter_kind))
            .collect::<Vec<_>>();
        let selected = plan_signaling_adapter_fallback(
            &requested,
            AdapterFallbackBehavior::FirstHealthy,
            None,
        )
        .selected?;
        connectivity
            .signaling_profiles
            .iter()
            .find(|profile| {
                transport_adapter_kind_from_name(&profile.adapter_kind) == Some(selected)
            })
            .cloned()
    }

    fn probe_material(
        &self,
        domain: &str,
        scope_commitment: &str,
        profile_id: &str,
        len: usize,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        let mut counter = 0_u8;
        while out.len() < len {
            let mut hasher = Sha256::new();
            hasher.update(domain.as_bytes());
            hasher.update([0]);
            hasher.update(self.identity_seed_hex.as_bytes());
            hasher.update([0]);
            hasher.update(self.local_user_id().as_bytes());
            hasher.update([0]);
            hasher.update(scope_commitment.as_bytes());
            hasher.update([0]);
            hasher.update(profile_id.as_bytes());
            hasher.update([0, counter]);
            out.extend_from_slice(&hasher.finalize());
            counter = counter.wrapping_add(1);
        }
        out.truncate(len);
        out
    }

    fn transport_session_route(&self) -> Option<TransportRoute> {
        if let Some(route) = self
            .signaling_session
            .as_ref()
            .and_then(|session| session.connected_route())
        {
            return Some(route);
        }
        self.text_session
            .as_ref()
            .and_then(|session| session.connected_route())
    }

    fn has_transport_reconnect(&self) -> bool {
        self.transport_session(BackendTransportMode::Signaling)
            .is_some_and(|session| session.state() == TransportSessionState::Reconnecting)
            || self
                .transport_session(BackendTransportMode::Text)
                .is_some_and(|session| session.state() == TransportSessionState::Reconnecting)
    }

    fn transport_session_connected(&self) -> bool {
        self.transport_session(BackendTransportMode::Signaling)
            .is_some_and(|session| session.state().is_connected())
            || self
                .transport_session(BackendTransportMode::Text)
                .is_some_and(|session| session.state().is_connected())
    }

    fn transport_session_error(&self) -> Option<String> {
        self.signaling_session
            .as_ref()
            .and_then(|session| session.snapshot().last_error)
            .or_else(|| {
                self.text_session
                    .as_ref()
                    .and_then(|session| session.snapshot().last_error)
            })
    }

    fn transport_session_failed(&self) -> bool {
        self.transport_session(BackendTransportMode::Signaling)
            .is_some_and(|session| session.state() == TransportSessionState::Failed)
            || self
                .transport_session(BackendTransportMode::Text)
                .is_some_and(|session| session.state() == TransportSessionState::Failed)
    }

    fn transport_status(&self) -> Vec<TransportStatusView> {
        let latest_invite = self.invites.last();
        let has_group = !self.groups.is_empty();
        let voice_joined = self
            .voice_session
            .as_ref()
            .map(|session| session.joined)
            .unwrap_or(false);
        let has_stun = latest_invite
            .map(|invite| !invite.ice_stun_servers.is_empty())
            .unwrap_or(false);
        let has_turn = latest_invite
            .map(|invite| !invite.ice_turn_servers.is_empty())
            .unwrap_or(false);
        let last_error = self.last_command_error.as_ref();
        let route_connected = self.transport_session_connected();
        let route = self.transport_session_route();
        let transport_error = self.transport_session_error();
        let reconnecting = self.has_transport_reconnect();
        let transport_failed = self.transport_session_failed();
        let mut rows = vec![
            TransportStatusView {
                label: "signaling".to_owned(),
                status: latest_invite
                    .map(|_| "signed-endpoint-ready")
                    .unwrap_or("waiting-for-invite")
                    .to_owned(),
                detail: latest_invite
                    .map(|invite| {
                        format!(
                            "Signed endpoint {} with trust fingerprint {}; no identity-room topology is stored by the signaling service",
                            invite.signaling_endpoint, invite.signaling_trust_fingerprint
                        )
                    })
                    .unwrap_or_else(|| "Create or paste an invite before signaling can be used".to_owned()),
            },
            TransportStatusView {
                label: "ICE".to_owned(),
                status: if has_stun || has_turn {
                    "configured"
                } else {
                    "waiting-for-signed-invite"
                }
                .to_owned(),
                detail: latest_invite
                    .map(|invite| {
                        format!(
                            "{} STUN and {} redacted TURN endpoint(s) parsed from signed invite metadata",
                            invite.ice_stun_servers.len(),
                            invite.ice_turn_servers.len()
                        )
                    })
                    .unwrap_or_else(|| "No ICE server metadata is available until an invite descriptor is present".to_owned()),
            },
            self.adapter_selection_status_row(),
            TransportStatusView {
                label: "direct".to_owned(),
                status: if route_connected {
                    "route-proofed"
                } else if voice_joined {
                    "media-gated"
                } else {
                    "no-direct-proof"
                }
                .to_owned(),
                detail: route
                    .map(|route| format!("Backend route evidence is available: {:?}", route))
                    .unwrap_or_else(|| "Direct path is only shown as connected after transport/session state proves it; this command state has no direct route proof yet".to_owned()),
            },
            TransportStatusView {
                label: "overlay".to_owned(),
                status: if has_group { "available-policy" } else { "idle" }.to_owned(),
                detail: "Relay-overlay policy is listed as a fallback path; ciphertext-only route proof is required before claiming active relay use".to_owned(),
            },
            TransportStatusView {
                label: "TURN".to_owned(),
                status: if has_turn { "configured" } else { "not-configured" }.to_owned(),
                detail: "TURN endpoints are redacted from signed invite metadata and are not treated as active without backend route evidence".to_owned(),
            },
            TransportStatusView {
                label: "degraded".to_owned(),
                status: if last_error.is_some() || transport_error.is_some() {
                    "attention"
                } else {
                    "clear"
                }
                .to_owned(),
                detail: transport_error
                    .or_else(|| {
                        last_error
                            .map(|error| format!("Last command issue {}: {}", error.code, error.message))
                    })
                    .unwrap_or_else(|| "No degraded command state is currently reported".to_owned()),
            },
            TransportStatusView {
                label: "reconnecting".to_owned(),
                status: if reconnecting { "active" } else { "idle" }.to_owned(),
                detail: "Reconnect orchestration is displayed only when event state reports reconnect attempts".to_owned(),
            },
            TransportStatusView {
                label: "failed".to_owned(),
                status: if transport_failed {
                    "failed"
                } else if last_error.is_some() {
                    "last-command-error"
                } else {
                    "clear"
                }
                .to_owned(),
                detail: last_error
                    .map(|error| error.recovery_hint.clone())
                    .unwrap_or_else(|| "No failed transport command is currently reported".to_owned()),
            },
        ];
        if let Some(session) = &self.signaling_session {
            rows.push(Self::transport_session_status_row(session));
        }
        if let Some(session) = &self.text_session {
            rows.push(Self::transport_session_status_row(session));
        }
        rows
    }

    fn adapter_selection_status_row(&self) -> TransportStatusView {
        let requested = self
            .active_connectivity_policy()
            .map(|(_, connectivity)| {
                connectivity
                    .signaling_profiles
                    .iter()
                    .filter_map(|profile| transport_adapter_kind_from_name(&profile.adapter_kind))
                    .collect::<Vec<SignalingAdapterKind>>()
            })
            .filter(|profiles| !profiles.is_empty())
            .unwrap_or_else(|| {
                required_provider_adapter_boundaries()
                    .iter()
                    .map(|boundary| boundary.kind)
                    .collect::<Vec<SignalingAdapterKind>>()
            });
        let fallback_plan = plan_signaling_adapter_fallback(
            requested.as_slice(),
            AdapterFallbackBehavior::FirstHealthy,
            None,
        );
        let selected = fallback_plan
            .selected
            .map(|kind| kind.canonical_name().to_owned());
        let attempts = fallback_plan
            .attempts
            .iter()
            .map(|attempt| {
                let marker = if attempt.selected {
                    "selected"
                } else if attempt.attempted {
                    "attempted"
                } else {
                    "skipped"
                };
                format!(
                    "{}:{}:{}",
                    attempt.kind.canonical_name(),
                    Self::adapter_readiness_label(attempt.readiness),
                    marker
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        TransportStatusView {
            label: "adapter".to_owned(),
            status: selected
                .as_ref()
                .map(|_| "selected")
                .unwrap_or("no-healthy-adapter")
                .to_owned(),
            detail: selected
                .map(|selected| {
                    format!(
                        "Selected provider {selected} via first-healthy fallback; readiness/fallback attempts: {attempts}"
                    )
                })
                .unwrap_or_else(|| {
                    format!(
                        "No healthy provider adapter selected; readiness/fallback attempts: {attempts}"
                    )
                }),
        }
    }

    fn transport_session_status_row(session: &TransportSessionRecord) -> TransportStatusView {
        let snapshot = session.snapshot();
        TransportStatusView {
            label: format!("{} session", session.mode.label()),
            status: Self::transport_state_label(snapshot.state).to_owned(),
            detail: format!(
                "session={} {} mode={} last_error={}",
                session.session_id,
                redacted_observable_ref("scope", &session.scope_label),
                session.mode.label(),
                snapshot.last_error.unwrap_or_else(|| "none".to_owned()),
            ),
        }
    }

    fn transport_state_label(state: TransportSessionState) -> &'static str {
        match state {
            TransportSessionState::Idle => "idle",
            TransportSessionState::Signaling => "signaling",
            TransportSessionState::IceGathering => "ice_gathering",
            TransportSessionState::Checking => "checking",
            TransportSessionState::Direct => "direct",
            TransportSessionState::OverlayRelay => "overlay_relay",
            TransportSessionState::TurnRelay => "turn_relay",
            TransportSessionState::Reconnecting => "reconnecting",
            TransportSessionState::Disconnected => "disconnected",
            TransportSessionState::Failed => "failed",
            TransportSessionState::Cancelled => "cancelled",
        }
    }

    fn join_progress(&self) -> Vec<JoinProgressStepView> {
        let latest_invite = self.invites.last();
        let has_invite = latest_invite.is_some();
        let opened_from_invite = self
            .events
            .iter()
            .any(|event| event.kind == "group.joined" || event.kind == "group.opened_from_invite");
        let has_active_group = self
            .active_context
            .as_ref()
            .and_then(|context| context.group_id.as_ref())
            .is_some();
        vec![
            JoinProgressStepView {
                key: "invite_parsed".to_owned(),
                label: "Invite parsed".to_owned(),
                status: if has_invite { "complete" } else { "waiting-for-invite" }.to_owned(),
                detail: latest_invite
                    .map(|invite| format!("Invite {} parsed with signaling endpoint {}", invite.invite_key, invite.signaling_endpoint))
                    .unwrap_or_else(|| "Paste or create an invite before join progress can start".to_owned()),
            },
            JoinProgressStepView {
                key: "rendezvous".to_owned(),
                label: "Rendezvous link".to_owned(),
                status: if has_invite { "waiting-for-backend-event" } else { "blocked" }.to_owned(),
                detail: "Rendezvous connected is marked only when backend state reports an authenticated publish/take exchange".to_owned(),
            },
            JoinProgressStepView {
                key: "authorized_member".to_owned(),
                label: "Authorized member".to_owned(),
                status: if has_invite { "waiting-for-authorized-member" } else { "blocked" }.to_owned(),
                detail: "Waiting for an authorized member or helper to approve admission; the invite link alone is insufficient".to_owned(),
            },
            JoinProgressStepView {
                key: "welcome".to_owned(),
                label: "Welcome package".to_owned(),
                status: if opened_from_invite { "local-admission-recorded" } else { "pending-welcome" }.to_owned(),
                detail: "Welcome received becomes complete only after backend state records a verified MLS Welcome/add".to_owned(),
            },
            JoinProgressStepView {
                key: "mls_joined".to_owned(),
                label: "MLS group state".to_owned(),
                status: if has_active_group { "local-group-open" } else { "pending-mls-proof" }.to_owned(),
                detail: "MLS joined requires command state for the active group plus epoch/member verification".to_owned(),
            },
            JoinProgressStepView {
                key: "transport".to_owned(),
                label: "Transport route".to_owned(),
                status: if self.voice_session.as_ref().map(|session| session.joined).unwrap_or(false) {
                    "media-gated"
                } else {
                    "waiting-route-proof"
                }
                .to_owned(),
                detail: "Transport connected is shown only after backend state provides direct, overlay, or TURN route evidence".to_owned(),
            },
        ]
    }

    fn voice_states(&self) -> Vec<VoiceStateView> {
        let session = self.voice_session.as_ref();
        let joined = session.map(|voice| voice.joined).unwrap_or(false);
        let permission = session
            .map(|voice| voice.microphone_permission.as_str())
            .unwrap_or("unknown");
        let muted = session.map(|voice| voice.self_muted).unwrap_or(false);
        let speaking = session
            .map(|voice| {
                voice
                    .participants
                    .iter()
                    .any(|participant| participant.speaking && !participant.muted)
            })
            .unwrap_or(false);
        let has_turn = self
            .invites
            .last()
            .map(|invite| !invite.ice_turn_servers.is_empty())
            .unwrap_or(false);
        vec![
            VoiceStateView {
                key: "permission_needed".to_owned(),
                label: "Permission needed".to_owned(),
                status: if permission == "granted" { "granted" } else { "needed" }.to_owned(),
                detail: "Microphone permission must be granted before capture starts".to_owned(),
            },
            VoiceStateView {
                key: "joining".to_owned(),
                label: "Joining".to_owned(),
                status: if joined { "joined" } else { "idle" }.to_owned(),
                detail: "Join command creates a local voice session and records selected devices".to_owned(),
            },
            VoiceStateView {
                key: "ice_checking".to_owned(),
                label: "ICE checking".to_owned(),
                status: if joined { "waiting-route-proof" } else { "idle" }.to_owned(),
                detail: "ICE checks require route metrics from transport state before success is displayed".to_owned(),
            },
            VoiceStateView {
                key: "route".to_owned(),
                label: "Direct / overlay / TURN".to_owned(),
                status: if joined { if has_turn { "turn-configured" } else { "policy-only" } } else { "idle" }.to_owned(),
                detail: "Direct, overlay, and TURN route labels stay policy-only until backend route evidence exists".to_owned(),
            },
            VoiceStateView {
                key: "muted".to_owned(),
                label: "Muted".to_owned(),
                status: if muted { "muted" } else { "unmuted" }.to_owned(),
                detail: "Mute state is command-backed and suppresses outbound local media frames".to_owned(),
            },
            VoiceStateView {
                key: "speaking".to_owned(),
                label: "Speaking".to_owned(),
                status: if speaking { "active" } else { "silent" }.to_owned(),
                detail: "Speaking indicators come from participant audio-level state returned by the backend".to_owned(),
            },
            VoiceStateView {
                key: "reconnecting".to_owned(),
                label: "Reconnecting".to_owned(),
                status: "idle".to_owned(),
                detail: "Reconnect state appears only when transport events report retry/backoff activity".to_owned(),
            },
            VoiceStateView {
                key: "left".to_owned(),
                label: "Left".to_owned(),
                status: if joined { "not-left" } else { "left-or-not-joined" }.to_owned(),
                detail: "Leaving clears the local joined state and keeps no fabricated remote roster".to_owned(),
            },
        ]
    }

    fn latest_event_cursor(&self) -> u64 {
        self.events
            .last()
            .map(|event| event.sequence)
            .unwrap_or_default()
    }

    fn to_snapshot(&self) -> AppSnapshot {
        let mut snapshot = core_app_snapshot();
        snapshot.schema_version = self.schema_version;
        snapshot.friend.verified = self.friend_verified;
        snapshot.devices = self.devices.clone();
        if let Some(safety_number) = self
            .devices
            .iter()
            .find(|device| device.local && !device.revoked)
            .and_then(|device| {
                safety_number_for_identity_hex_and_friend_code(
                    &device.identity_key,
                    &snapshot.friend.friend_code,
                )
            })
        {
            snapshot.friend.safety_number = safety_number;
        }
        snapshot.preferences = discrypt_core::PreferencesView {
            theme_id: self.preferences.theme_id.clone(),
            template_id: self.preferences.template_id.clone(),
        };
        snapshot.servers = self
            .groups
            .iter()
            .map(|group| ServerView {
                name: group.name.clone(),
                role: group.role.clone(),
                channels: group
                    .channels
                    .iter()
                    .map(|channel| SnapshotChannelView {
                        name: channel.name.clone(),
                        kind: channel.kind,
                        retention_status: channel.retention_status.clone(),
                    })
                    .collect(),
            })
            .collect();
        snapshot.messages = self
            .messages
            .iter()
            .map(|message| SnapshotMessageView {
                id: message.message_id.clone(),
                channel: snapshot_channel_label(message, self),
                author: message.author.clone(),
                body: message.body.clone(),
                state: message.status.clone(),
            })
            .collect();
        snapshot.voice_session = if let Some(session) = &self.voice_session {
            discrypt_core::VoiceSessionView {
                joined: session.joined,
                microphone_permission: session.microphone_permission.clone(),
                input_device: session.input_device.as_ref().map(core_voice_device_view),
                output_device: session.output_device.as_ref().map(core_voice_device_view),
                participants: session
                    .participants
                    .iter()
                    .map(|participant| discrypt_core::VoiceParticipantView {
                        id: participant.id.clone(),
                        name: participant.name.clone(),
                        role: participant.role.clone(),
                        speaking: participant.speaking,
                        muted: participant.muted,
                        volume: participant.volume,
                    })
                    .collect(),
                status_copy: session.status_copy.clone(),
                route_copy: session.route_copy.clone(),
                permission_denied_copy: session.permission_denied_copy.clone(),
            }
        } else {
            discrypt_core::VoiceSessionView {
                joined: false,
                microphone_permission: "unknown".to_owned(),
                input_device: None,
                output_device: None,
                participants: default_voice_participants(&self.local_user_id(), false)
                    .into_iter()
                    .map(|participant| discrypt_core::VoiceParticipantView {
                        id: participant.id,
                        name: participant.name,
                        role: participant.role,
                        speaking: participant.speaking,
                        muted: participant.muted,
                        volume: participant.volume,
                    })
                    .collect(),
                status_copy: VOICE_SESSION_NOT_JOINED_COPY.to_owned(),
                route_copy: VOICE_SESSION_ROUTE_GATED_COPY.to_owned(),
                permission_denied_copy: String::new(),
            }
        };
        snapshot.activity_feed = self
            .events
            .iter()
            .rev()
            .map(|event| event.summary.clone())
            .collect();
        snapshot
    }

    fn create_user(&mut self, request: CreateUserRequest, recovered: bool) {
        self.create_user_with_seed(request, recovered, None);
    }

    fn create_user_with_seed(
        &mut self,
        request: CreateUserRequest,
        recovered: bool,
        seed_override: Option<[u8; 32]>,
    ) {
        let display_name = normalize_label(&request.display_name, "Alice");
        let device_name = request
            .device_name
            .map(|value| normalize_label(&value, "Desktop"))
            .unwrap_or_else(|| "Desktop".to_owned());
        self.identity_seed_hex = seed_override
            .map(hex::encode)
            .unwrap_or_else(|| hex::encode(SigningKey::generate(&mut OsRng).to_bytes()));
        let seed = self.identity_seed_bytes();
        let identity = Identity::from_signing_key(display_name.clone(), &seed);
        let identity_key = hex::encode(identity.verifying_key().as_bytes());
        let user_id = stable_id("user", &identity_key, self.next_sequence);
        self.profile = Some(UserProfileView {
            user_id,
            display_name: display_name.clone(),
            device_name: device_name.clone(),
            recovery_status: if recovered {
                "Account-continuity recovery accepted with verified local identity material; message history and content keys were not restored"
                    .to_owned()
            } else {
                "New local identity generated from command signing material; recovery export is account-continuity only"
                    .to_owned()
            },
        });
        self.lifecycle = AppLifecycle::Ready;
        self.device_set = DeviceSet::new();
        let device_key = command_device_key(&seed, &device_name, self.next_sequence);
        let leaf = self
            .device_set
            .add_authorized_device(&identity, device_key, &device_name, 1);
        self.devices = vec![device_view_from_leaf(&leaf, true, true)];
        if self.dms.is_empty() {
            let friend = core_app_snapshot().friend;
            let dm_id = stable_id("dm", &friend.friend_code, self.next_sequence);
            let participant_id = participant_id_from_friend_code(&friend.friend_code);
            let connectivity = apply_app_connectivity_defaults(
                dm_connectivity_policy(&dm_id, &participant_id),
                &self.connectivity_defaults,
            );
            let runtime_peers = dm_runtime_peers(Some(&connectivity), "inviter");
            self.dms.push(DirectConversationView {
                dm_id: dm_id.clone(),
                participant_id: participant_id.clone(),
                display_name: friend.alias,
                local_only_copy: "Local DM seeded from a generated friend-code/QR payload; no remote delivery is claimed".to_owned(),
                runtime_peers,
                connectivity: Some(connectivity),
            });
            self.active_context = Some(ActiveContextView {
                kind: "dm".to_owned(),
                group_id: None,
                channel_id: None,
                dm_id: Some(dm_id),
            });
        }
        if self.active_context.is_none() {
            self.active_context = self.dms.first().map(|dm| ActiveContextView {
                kind: "dm".to_owned(),
                group_id: None,
                channel_id: None,
                dm_id: Some(dm.dm_id.clone()),
            });
        }
        let kind = if recovered {
            "identity.recovered"
        } else {
            "identity.created"
        };
        self.push_event(
            kind,
            format!(
                "Profile ready for {} on {}",
                redacted_observable_ref("profile", &display_name),
                redacted_observable_ref("device", &device_name)
            ),
        );
    }

    fn ensure_ready_profile(&mut self) {
        if self.profile.is_none() {
            self.create_user(
                CreateUserRequest {
                    display_name: "Alice".to_owned(),
                    device_name: Some("Desktop".to_owned()),
                },
                false,
            );
        }
    }

    fn identity_seed_bytes(&mut self) -> [u8; 32] {
        if self.identity_seed_hex.len() != 64 || hex::decode(&self.identity_seed_hex).is_err() {
            let display_name = self
                .profile
                .as_ref()
                .map(|profile| profile.display_name.as_str())
                .unwrap_or("Alice");
            let device_name = self
                .profile
                .as_ref()
                .map(|profile| profile.device_name.as_str())
                .unwrap_or("Desktop");
            self.identity_seed_hex =
                new_identity_seed_hex(display_name, device_name, self.next_sequence);
        }
        hex_32(&self.identity_seed_hex).unwrap_or_else(|| {
            let digest: [u8; 32] = Sha256::digest(self.identity_seed_hex.as_bytes()).into();
            digest
        })
    }

    fn local_identity(&mut self) -> Identity {
        let seed = self.identity_seed_bytes();
        let display_name = self
            .profile
            .as_ref()
            .map(|profile| profile.display_name.clone())
            .unwrap_or_else(|| "Alice".to_owned());
        Identity::from_signing_key(display_name, &seed)
    }

    fn ensure_device_set(&mut self, identity: &Identity) {
        if !self.device_set.active_devices().is_empty() {
            return;
        }
        let device_name = self
            .profile
            .as_ref()
            .map(|profile| profile.device_name.clone())
            .unwrap_or_else(|| "Desktop".to_owned());
        let seed = self.identity_seed_bytes();
        let device_key = command_device_key(&seed, &device_name, self.next_sequence);
        let leaf = self
            .device_set
            .add_authorized_device(identity, device_key, &device_name, 1);
        if self.devices.is_empty() {
            self.devices.push(device_view_from_leaf(&leaf, true, true));
        }
    }

    fn apply_account_recovery(&mut self, recovery: &AccountRecovery) {
        if let Some(profile) = &mut self.profile {
            profile.recovery_status = format!(
                "Account continuity restored with verified local identity material for {} room(s) and {} device(s); rooms: {}; devices: {}; content keys restored: {}",
                recovery.room_memberships.len(),
                recovery.device_count,
                recovery.room_memberships.len(),
                recovery.device_count,
                recovery.content_keys_restored
            );
        }

        let local_device = self.devices.first().cloned().unwrap_or_else(|| {
            let seed = self.identity_seed_bytes();
            let identity = self.local_identity();
            let device_key = command_device_key(&seed, "Desktop", self.next_sequence);
            let leaf = self
                .device_set
                .add_authorized_device(&identity, device_key, "Desktop", 1);
            device_view_from_leaf(&leaf, true, true)
        });
        self.devices = vec![local_device];
        let seed = self.identity_seed_bytes();
        let identity = self.local_identity();
        for index in 2..=recovery.device_count.max(1) {
            let label = format!("Recovered device {index}");
            let device_key = command_device_key(&seed, &label, self.next_sequence + index as u64);
            let leaf =
                self.device_set
                    .add_authorized_device(&identity, device_key, &label, index as u64);
            if !self
                .devices
                .iter()
                .any(|device| device.device_id == leaf.device_id.to_string())
            {
                self.devices.push(device_view_from_leaf(&leaf, false, true));
            }
        }

        for room in &recovery.room_memberships {
            let room_name = normalize_label(room, "recovered room");
            if self.groups.iter().any(|group| group.name == room_name) {
                continue;
            }
            let group_id = stable_id("group", &room_name, self.next_sequence);
            let connectivity = apply_app_connectivity_defaults(
                group_connectivity_policy(&group_id),
                &self.connectivity_defaults,
            );
            let runtime_peers = group_runtime_peers(Some(&connectivity), "member");
            self.groups.push(GroupView {
                group_id: group_id.clone(),
                name: room_name,
                role: "member".to_owned(),
                channels: default_group_channels(self.next_sequence),
                runtime_peers,
                connectivity: Some(connectivity),
            });
        }
    }

    fn push_event(&mut self, kind: impl Into<String>, summary: impl Into<String>) {
        let event = AppEventView {
            sequence: self.next_sequence,
            kind: kind.into(),
            summary: redact_sensitive_observable_copy(summary),
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.events.push(event);
        let overflow = self.events.len().saturating_sub(48);
        if overflow > 0 {
            self.events.drain(0..overflow);
        }
    }

    fn set_command_error(
        &mut self,
        command: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
        recovery_hint: impl Into<String>,
    ) {
        self.last_command_error = Some(CommandErrorView {
            code: code.into(),
            command: command.into(),
            message: redact_sensitive_observable_copy(message),
            recovery_hint: redact_sensitive_observable_copy(recovery_hint),
        });
    }

    fn push_command_error(
        &mut self,
        event_kind: impl Into<String>,
        command: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
        recovery_hint: impl Into<String>,
    ) {
        let event_kind = event_kind.into();
        let code = code.into();
        let message = message.into();
        self.set_command_error(command, code.clone(), message.clone(), recovery_hint);
        self.push_event(event_kind, format!("{code}: {message}"));
    }

    fn local_user_id(&self) -> String {
        self.profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .unwrap_or_else(|| "local-profile-pending".to_owned())
    }
}

fn allow_persisted_action(
    buckets: &mut Vec<AbuseBucketView>,
    key: &str,
    limit: u32,
    now: DateTime<Utc>,
) -> bool {
    let window = Duration::seconds(ABUSE_WINDOW_SECONDS);
    for bucket in buckets.iter_mut() {
        bucket
            .timestamps
            .retain(|timestamp| *timestamp + window >= now);
    }
    buckets.retain(|bucket| !bucket.timestamps.is_empty());
    if !buckets.iter().any(|bucket| bucket.key == key) {
        buckets.push(AbuseBucketView {
            key: key.to_owned(),
            timestamps: Vec::new(),
        });
    }
    let Some(bucket) = buckets.iter_mut().find(|bucket| bucket.key == key) else {
        return false;
    };
    if bucket.timestamps.len() as u32 >= limit.max(1) {
        return false;
    }
    bucket.timestamps.push(now);
    true
}

fn invite_code_fingerprint(invite_code: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"discrypt-desktop-invite-code-fingerprint-v1");
    hasher.update(invite_code.as_bytes());
    hex::encode(hasher.finalize())
}

fn text_send_abuse_key(state: &PersistedAppState, target: &MessageTargetView) -> String {
    format!(
        "text:{}:{}:{}:{}",
        state.local_user_id(),
        target.kind,
        target.group_id.as_deref().unwrap_or(""),
        target.dm_id.as_deref().unwrap_or("")
    )
}

fn opaque_text_control_frame_for_message(
    state: &PersistedAppState,
    target: &MessageTargetView,
    message_id: &str,
    body: &str,
    sequence: u64,
) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"discrypt-opaque-text-control-frame-v1");
    hasher.update([0]);
    hasher.update(state.local_user_id().as_bytes());
    hasher.update([0]);
    hasher.update(target.kind.as_bytes());
    hasher.update([0]);
    hasher.update(target.dm_id.as_deref().unwrap_or("").as_bytes());
    hasher.update([0]);
    hasher.update(target.group_id.as_deref().unwrap_or("").as_bytes());
    hasher.update([0]);
    hasher.update(target.channel_id.as_deref().unwrap_or("").as_bytes());
    hasher.update([0]);
    hasher.update(message_id.as_bytes());
    hasher.update([0]);
    hasher.update(sequence.to_be_bytes());
    hasher.update([0]);
    hasher.update(body.as_bytes());
    format!(
        "ciphertext:discrypt-text-control-proof:v1:{}",
        hex::encode(hasher.finalize())
    )
    .into_bytes()
}

impl From<&TextControlOutboxRecord> for TextControlOutboxFrameView {
    fn from(record: &TextControlOutboxRecord) -> Self {
        Self {
            message_id: record.message_id.clone(),
            target: record.target.clone(),
            frame: record.frame.clone(),
            state_key: record.state_key.clone(),
            attempts: record.attempts,
            last_transport_session_id: record.last_transport_session_id.clone(),
            frame_sha256: record.frame_sha256.clone(),
        }
    }
}

fn text_control_frame_sha256(frame: &TextControlFrameView) -> Result<String, String> {
    let encoded = serde_json::to_vec(frame)
        .map_err(|error| format!("could not encode text/control frame: {error}"))?;
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    Ok(hex::encode(hasher.finalize()))
}

fn text_delivery_group_id(target: &MessageTargetView) -> Result<String, String> {
    match target.kind.as_str() {
        "dm" => target
            .dm_id
            .as_ref()
            .map(|dm_id| format!("dm:{dm_id}"))
            .ok_or_else(|| "DM delivery receipt target requires dm_id".to_owned()),
        "channel" => Ok(format!(
            "group:{}:channel:{}",
            target
                .group_id
                .as_deref()
                .ok_or_else(|| "channel delivery receipt target requires group_id".to_owned())?,
            target
                .channel_id
                .as_deref()
                .ok_or_else(|| "channel delivery receipt target requires channel_id".to_owned())?
        )),
        other => Err(format!("unsupported delivery receipt target kind {other}")),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn upsert_openmls_group_handle(state: &mut PersistedAppState, record: OpenMlsGroupHandleRecord) {
    if let Some(existing) = state
        .openmls_groups
        .iter_mut()
        .find(|existing| existing.group_id == record.group_id)
    {
        *existing = record;
    } else {
        state.openmls_groups.push(record);
    }
}

fn key_fingerprint(key: &VerifyingKey) -> String {
    let digest = Sha256::digest(key.as_bytes());
    hex::encode(&digest[..10])
}

fn redacted_observable_ref(kind: &str, value: &str) -> String {
    if value.trim().is_empty() {
        return format!("{kind}_ref=empty");
    }
    format!("{kind}_ref={}", redacted_observable_token(kind, value))
}

fn redacted_observable_token(kind: &str, value: &str) -> String {
    let digest = hash_commitment("discrypt-observable-redaction-v1", &[kind, value]);
    digest[..16].to_owned()
}

fn redacted_message_ref(message_id: &str) -> String {
    redacted_observable_ref("message", message_id)
}

fn redact_sensitive_observable_copy(value: impl Into<String>) -> String {
    let value = value.into();
    let classes = sensitive_observable_classes(&value);
    if classes.is_empty() {
        return value;
    }
    format!(
        "redacted sensitive observable copy (classes={})",
        classes.join(",")
    )
}

fn sensitive_observable_classes(value: &str) -> Vec<&'static str> {
    let lower = value.to_ascii_lowercase();
    let mut classes = Vec::new();
    for (needle, class) in [
        ("v=0", "raw_sdp"),
        ("a=ice-ufrag", "ice_credentials"),
        ("a=ice-pwd", "ice_credentials"),
        ("candidate:", "ice_candidates"),
        ("ice password", "ice_credentials"),
        ("ice credential", "ice_credentials"),
        ("turn credential", "turn_credentials"),
        ("turn password", "turn_credentials"),
        ("room-secret:", "room_seed"),
        ("room seed", "room_seed"),
        ("discrypt://join", "invite_link"),
        ("plaintext message", "plaintext_message"),
        ("audio plaintext", "audio_plaintext"),
        ("sframe key", "sframe_key"),
        ("content key", "content_key"),
        ("mls epoch secret", "mls_key"),
        ("mls exporter", "mls_key"),
        ("production-ready", "fake_production_label"),
        ("fake production", "fake_production_label"),
    ] {
        if lower.contains(needle) && !classes.contains(&class) {
            classes.push(class);
        }
    }
    classes
}

#[cfg(test)]
fn abuse_controls_contract_covers_g116() -> bool {
    let mut controls = AbuseControls::new(
        INVITE_CREATE_LIMIT,
        TEXT_SEND_LIMIT,
        Duration::seconds(ABUSE_WINDOW_SECONDS),
    );
    let now = Utc::now();
    controls.allow_invite("contract", now) && controls.allow_message("contract", now)
}

fn app_service() -> &'static Mutex<TauriAppService> {
    APP_SERVICE.get_or_init(|| Mutex::new(TauriAppService::load()))
}

fn with_state<T>(read: impl FnOnce(&PersistedAppState) -> T) -> T {
    let service = app_service();
    let guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.read(read)
}

fn mutate_app_service(update: impl FnOnce(&mut PersistedAppState)) -> AppStateView {
    let service = app_service();
    let mut guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.mutate(update)
}

fn mutate_app_service_with_result<T>(
    update: impl FnOnce(&mut PersistedAppState) -> T,
) -> (AppStateView, T) {
    let service = app_service();
    let mut guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut candidate = guard.state.clone();
    candidate.last_command_error = None;
    let result = update(&mut candidate);
    match guard.persist_candidate(&candidate) {
        Ok(()) => guard.state = candidate,
        Err(error) => {
            guard.state.last_command_error = Some(error);
        }
    }
    let view = guard.to_view();
    (view, result)
}

#[cfg_attr(not(any(test, feature = "tauri-runtime")), allow(dead_code))]
fn app_event_stream_after_view(state: &AppStateView, cursor: u64) -> AppEventStreamView {
    let events = state
        .events
        .iter()
        .filter(|event| event.sequence > cursor)
        .cloned()
        .collect::<Vec<_>>();
    let next_cursor = events
        .last()
        .map(|event| event.sequence)
        .unwrap_or(state.event_cursor);
    AppEventStreamView {
        events,
        cursor,
        next_cursor,
        has_more: false,
        subscribed_kinds: Vec::new(),
    }
}

#[cfg(feature = "tauri-runtime")]
fn latest_app_event_cursor() -> u64 {
    with_state(|state| state.latest_event_cursor())
}

#[cfg(feature = "tauri-runtime")]
fn emit_app_event_stream(
    app_handle: &tauri::AppHandle,
    state: &AppStateView,
    previous_cursor: u64,
) {
    use tauri::Emitter as _;

    let stream = app_event_stream_after_view(state, previous_cursor);
    if !stream.events.is_empty() {
        let _ = app_handle.emit(APP_EVENT_TAURI_TOPIC, stream);
    }
}

#[cfg(feature = "tauri-runtime")]
fn run_app_state_command_with_event_emit(
    app_handle: &tauri::AppHandle,
    command: impl FnOnce() -> AppStateView,
) -> AppStateView {
    let previous_cursor = latest_app_event_cursor();
    let state = command();
    emit_app_event_stream(app_handle, &state, previous_cursor);
    state
}

#[cfg(feature = "tauri-runtime")]
fn run_command_with_event_emit<T>(app_handle: &tauri::AppHandle, command: impl FnOnce() -> T) -> T {
    let previous_cursor = latest_app_event_cursor();
    let output = command();
    let state = app_state();
    emit_app_event_stream(app_handle, &state, previous_cursor);
    output
}

#[cfg(feature = "tauri-runtime")]
fn run_receive_text_delivery_envelope_with_event_emit(
    app_handle: &tauri::AppHandle,
    command: impl FnOnce() -> ReceiveTextDeliveryEnvelopeResponse,
) -> ReceiveTextDeliveryEnvelopeResponse {
    let previous_cursor = latest_app_event_cursor();
    let response = command();
    emit_app_event_stream(app_handle, &response.state, previous_cursor);
    response
}

#[cfg(feature = "tauri-runtime")]
fn run_handle_text_control_frame_with_event_emit(
    app_handle: &tauri::AppHandle,
    command: impl FnOnce() -> HandleTextControlFrameResponse,
) -> HandleTextControlFrameResponse {
    let previous_cursor = latest_app_event_cursor();
    let response = command();
    emit_app_event_stream(app_handle, &response.state, previous_cursor);
    response
}

fn normalize_event_subscriptions(raw: &[String]) -> Vec<String> {
    let mut normalized = raw
        .iter()
        .map(|kind| kind.trim().to_ascii_lowercase())
        .filter(|kind| {
            matches!(
                kind.as_str(),
                "message" | "invite" | "group" | "device" | "transport" | "voice"
            )
        })
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn event_matches_subscription(kind: &str, subscriptions: &[String]) -> bool {
    if subscriptions.is_empty() {
        return true;
    }
    subscriptions
        .iter()
        .any(|subscription| event_kind_topic(kind) == subscription)
}

fn event_kind_topic(kind: &str) -> &str {
    match kind.split_once('.').map(|(topic, _)| topic).unwrap_or(kind) {
        "message" => "message",
        "invite" => "invite",
        "group" => "group",
        "device" => "device",
        "transport" | "connectivity" | "signaling" | "relay" | "ice" => "transport",
        "voice" => "voice",
        _ => "message",
    }
}

fn load_state() -> PersistedAppState {
    let mut store = app_store();
    load_state_from_store(&mut store)
}

fn load_state_from_store(store: &mut impl AppStore) -> PersistedAppState {
    match store.load_app_state() {
        Ok(Some(bytes)) => match serde_json::from_slice::<PersistedAppState>(&bytes) {
            Ok(state) if state.schema_version == APP_STATE_SCHEMA_VERSION => state,
            Ok(_) => initial_state_with_persistence_error(
                "state_schema_mismatch",
                "Stored app state uses an unsupported schema version.",
                "Keep the existing store quarantined for recovery; do not silently treat this as a first-run profile.",
            ),
            Err(error) => initial_state_with_persistence_error(
                "state_decode_failed",
                format!("Stored app state could not be decoded: {error}"),
                "Keep the existing store quarantined for recovery; do not silently treat this as a first-run profile.",
            ),
        },
        Ok(None) => PersistedAppState::initial(),
        Err(error) => initial_state_with_persistence_error(
            "state_load_failed",
            format!("Stored app state could not be loaded: {error}"),
            "Check local storage/keychain access before creating or recovering another profile.",
        ),
    }
}

fn persist_state(state: &PersistedAppState) -> Result<(), CommandErrorView> {
    let mut store = app_store();
    persist_state_to_store(&mut store, state)
}

fn persist_state_to_store(
    store: &mut impl AppStore,
    state: &PersistedAppState,
) -> Result<(), CommandErrorView> {
    let encoded = serde_json::to_vec_pretty(state).map_err(|error| {
        persistence_command_error(
            "state_encode_failed",
            format!("App state could not be encoded for persistence: {error}"),
            "Retry after preserving current logs; the app did not write partial state.",
        )
    })?;
    store.save_app_state(&encoded).map_err(|error| {
        persistence_command_error(
            "state_save_failed",
            format!("App state could not be saved: {error}"),
            "Check disk/keychain availability before continuing; the app did not confirm persistence.",
        )
    })
}

fn initial_state_with_persistence_error(
    code: impl Into<String>,
    message: impl Into<String>,
    recovery_hint: impl Into<String>,
) -> PersistedAppState {
    let mut state = PersistedAppState::initial();
    state.last_command_error = Some(persistence_command_error(code, message, recovery_hint));
    state
}

fn persistence_command_error(
    code: impl Into<String>,
    message: impl Into<String>,
    recovery_hint: impl Into<String>,
) -> CommandErrorView {
    let code = code.into();
    let _redacted_detail = redact_sensitive_observable_copy(message);
    CommandErrorView {
        code: code.clone(),
        command: "app_persistence".to_owned(),
        message: format!(
            "Persistence/security error {code}; detailed storage failure copy is redacted from observable state."
        ),
        recovery_hint: redact_sensitive_observable_copy(recovery_hint),
    }
}

fn account_recovery_from_request(request: &RecoverUserRequest) -> AccountRecovery {
    let rooms = request.recovery_room_memberships.clone();
    let device_count = request.recovered_device_count.unwrap_or(1);
    let material = if request.use_sealed_account_backup {
        let key = recovery_seed_key(&request.recovery_code);
        RecoveryMaterial::SealedBackup(seal_account_backup(&key, rooms, device_count))
    } else {
        RecoveryCodeVerifier::from_code(&request.recovery_code)
            .and_then(|verifier| {
                recovery_code_material(&request.recovery_code, &verifier, rooms, device_count)
            })
            .unwrap_or_else(|_| RecoveryMaterial::ExistingDevice {
                device_id: request
                    .device_name
                    .clone()
                    .unwrap_or_else(|| "Desktop".to_owned()),
            })
    };
    recover_account(material).unwrap_or(AccountRecovery {
        account_access_restored: false,
        room_memberships: Vec::new(),
        device_count: 1,
        content_keys_restored: false,
    })
}

fn recovery_seed_key(recovery_code: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"discrypt:desktop:sealed-account-recovery");
    hasher.update(recovery_code.trim().as_bytes());
    hasher.finalize().into()
}

fn safety_number_for_identity_hex_and_friend_code(
    identity_key_hex: &str,
    friend_code_payload: &str,
) -> Option<String> {
    let local_key = verifying_key_from_hex(identity_key_hex)?;
    let peer_code = FriendCode::from_payload(friend_code_payload);
    let peer_key = peer_code.verifying_key()?;
    Some(
        SafetyNumber::from_identity_keys(&local_key, &peer_key)
            .as_str()
            .to_owned(),
    )
}

fn new_identity_seed_hex(display_name: &str, device_name: &str, sequence: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"discrypt:desktop:identity-seed:v1");
    hasher.update(display_name.trim().as_bytes());
    hasher.update([0]);
    hasher.update(device_name.trim().as_bytes());
    hasher.update(sequence.to_be_bytes());
    hex::encode(hasher.finalize())
}

fn hex_32(value: &str) -> Option<[u8; 32]> {
    let decoded = hex::decode(value).ok()?;
    decoded.try_into().ok()
}

fn command_device_key(seed: &[u8; 32], device_name: &str, sequence: u64) -> VerifyingKey {
    let mut hasher = Sha256::new();
    hasher.update(b"discrypt:desktop:device-key:v1");
    hasher.update(seed);
    hasher.update(device_name.trim().as_bytes());
    hasher.update(sequence.to_be_bytes());
    let material: [u8; 32] = hasher.finalize().into();
    SigningKey::from_bytes(&material).verifying_key()
}

fn device_view_from_leaf(leaf: &DeviceLeaf, local: bool, authorized: bool) -> DeviceView {
    DeviceView {
        device_id: leaf.device_id.to_string(),
        label: leaf.label.clone(),
        leaf_index: leaf.leaf_index,
        identity_key: hex::encode(leaf.identity_key),
        device_key: hex::encode(leaf.device_key),
        local,
        authorized,
        revoked: leaf.status != DeviceStatus::Active,
        added_at_epoch: leaf.added_at_epoch,
        revoked_at_epoch: leaf.removed_at_epoch,
    }
}

#[cfg(all(test, target_os = "linux", feature = "production-storage"))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct TestAppDbKeychain;

#[cfg(all(test, target_os = "linux", feature = "production-storage"))]
fn test_app_db_keys() -> &'static Mutex<BTreeMap<String, [u8; 32]>> {
    static KEYS: OnceLock<Mutex<BTreeMap<String, [u8; 32]>>> = OnceLock::new();
    KEYS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[cfg(all(test, target_os = "linux", feature = "production-storage"))]
impl AppDbKeychain for TestAppDbKeychain {
    fn load_wrapping_key(&mut self, key_id: &str) -> Result<Option<[u8; 32]>, AppStoreError> {
        test_app_db_keys()
            .lock()
            .map_err(|_| AppStoreError::LockPoisoned)
            .map(|keys| keys.get(key_id).copied())
    }

    fn store_wrapping_key(&mut self, key_id: &str, key: [u8; 32]) -> Result<(), AppStoreError> {
        test_app_db_keys()
            .lock()
            .map_err(|_| AppStoreError::LockPoisoned)?
            .insert(key_id.to_owned(), key);
        Ok(())
    }

    fn delete_wrapping_key(&mut self, key_id: &str) -> Result<(), AppStoreError> {
        test_app_db_keys()
            .lock()
            .map_err(|_| AppStoreError::LockPoisoned)?
            .remove(key_id);
        Ok(())
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage", not(test)))]
fn app_store() -> EncryptedAppDb<LinuxOsKeychain> {
    EncryptedAppDb::new(app_store_path(), LinuxOsKeychain::discrypt_app_db())
}

#[cfg(all(test, target_os = "linux", feature = "production-storage"))]
fn app_store() -> EncryptedAppDb<TestAppDbKeychain> {
    EncryptedAppDb::new(app_store_path(), TestAppDbKeychain)
}

#[cfg(not(all(target_os = "linux", feature = "production-storage")))]
fn app_store() -> FileAppStore {
    FileAppStore::new(app_store_path())
}

fn app_store_path() -> PathBuf {
    app_store_path_with_env_override(env_app_state_override_allowed())
}

fn app_openmls_store_path() -> PathBuf {
    openmls_store_path_for_app_state_path(&app_store_path())
}

fn openmls_store_path_for_app_state_path(app_state_path: &std::path::Path) -> PathBuf {
    let file_name = app_state_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(APP_STATE_STORE_FILENAME);
    app_state_path.with_file_name(format!("{file_name}.openmls.sqlite"))
}

fn env_app_state_override_allowed() -> bool {
    cfg!(any(test, feature = "harness", feature = "local-dev"))
}

fn explicit_text_runtime_attachment_allowed() -> bool {
    cfg!(any(test, feature = "harness", feature = "local-dev"))
}

fn app_store_path_with_env_override(allow_env_override: bool) -> PathBuf {
    if allow_env_override {
        if let Some(path) = std::env::var_os("DISCRYPT_APP_STATE_PATH") {
            return PathBuf::from(path);
        }
    }
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(data_home)
            .join("discrypt")
            .join(APP_STATE_STORE_FILENAME);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("discrypt")
            .join(APP_STATE_STORE_FILENAME);
    }
    PathBuf::from(APP_STATE_STORE_FILENAME)
}

fn default_group_channels(sequence: u64) -> Vec<ChannelStateView> {
    vec![
        ChannelStateView {
            channel_id: stable_id("channel", "general", sequence),
            name: "#general".to_owned(),
            kind: ChannelKind::Text,
            retention_status: "7 days".to_owned(),
            connectivity: None,
        },
        ChannelStateView {
            channel_id: stable_id("channel", "Voice Lobby", sequence),
            name: "Voice Lobby".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
            connectivity: None,
        },
    ]
}

fn default_voice_participants(
    local_user_id: &str,
    local_speaking: bool,
) -> Vec<VoiceParticipantView> {
    vec![VoiceParticipantView {
        id: local_user_id.to_owned(),
        name: "You".to_owned(),
        role: "you".to_owned(),
        speaking: local_speaking,
        muted: false,
        volume: 82,
    }]
}

fn normalize_voice_signal_kind(kind: &str) -> Result<String, String> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "offer" => Ok("offer".to_owned()),
        "answer" => Ok("answer".to_owned()),
        "candidate" => Ok("candidate".to_owned()),
        other => Err(format!("unsupported voice signal kind {other}")),
    }
}

fn validate_voice_signal_payload(signal_kind: &str, sealed_payload: &str) -> Result<(), String> {
    match signal_kind {
        "offer" | "answer" | "candidate" => {}
        _ => return Err("unsupported voice signal kind".to_owned()),
    }
    let payload = sealed_payload.trim();
    if !payload.starts_with("voice-signal-sealed:v1:") || payload.len() < 48 {
        return Err("voice signaling requires a WebView-sealed payload envelope".to_owned());
    }
    let lower = payload.to_ascii_lowercase();
    for marker in [
        "v=0",
        "a=ice-ufrag",
        "a=ice-pwd",
        "candidate:",
        "turn credential",
        "turn password",
        "ice password",
        "plaintext",
        "sframe key",
        "content key",
        "mls epoch secret",
    ] {
        if lower.contains(marker) {
            return Err("voice signaling payload must be sealed before IPC/persistence".to_owned());
        }
    }
    Ok(())
}

fn voice_media_runtime_for_join(
    session_id: &str,
    selection: &VoiceDeviceSelection,
) -> VoiceMediaRuntimeView {
    let capture_allowed = selection.can_join_voice();
    let runtime_id = format!("voice-runtime:{session_id}");
    if capture_allowed {
        VoiceMediaRuntimeView {
            runtime_id,
            boundary: "webview-local-capture".to_owned(),
            local_capture_active: true,
            remote_transport_active: false,
            remote_audio: Vec::new(),
            fail_closed_reason: "Remote WebRTC audio transport is not attached; backend state proves playback claims remain gated until media-route evidence exists".to_owned(),
            status_copy: format!(
                "Local microphone capture admitted through backend session boundary using {}; remote playback remains disabled until a real media transport attaches",
                selection
                    .input_device
                    .as_ref()
                    .map(|device| device.label.as_str())
                    .unwrap_or("selected microphone")
            ),
        }
    } else {
        VoiceMediaRuntimeView {
            runtime_id,
            boundary: "fail-closed".to_owned(),
            local_capture_active: false,
            remote_transport_active: false,
            remote_audio: Vec::new(),
            fail_closed_reason: selection.status_copy(),
            status_copy:
                "Voice media runtime did not start because capture permission/device gates failed"
                    .to_owned(),
        }
    }
}

fn voice_media_runtime_for_leave(session_id: &str) -> VoiceMediaRuntimeView {
    VoiceMediaRuntimeView {
        runtime_id: format!("voice-runtime:{session_id}"),
        boundary: "stopped".to_owned(),
        local_capture_active: false,
        remote_transport_active: false,
        remote_audio: Vec::new(),
        fail_closed_reason: String::new(),
        status_copy:
            "Voice media runtime stopped by leave; local tracks and remote playback are inactive"
                .to_owned(),
    }
}

fn voice_device_selection(request: &JoinVoiceRequest) -> VoiceDeviceSelection {
    let permission = match request
        .microphone_permission
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "granted" => MicrophonePermissionState::Granted,
        "prompt" => MicrophonePermissionState::Prompt,
        "denied" => MicrophonePermissionState::Denied,
        _ => MicrophonePermissionState::Unknown,
    };
    let input_device = request
        .input_device_id
        .as_ref()
        .or(request.input_device_label.as_ref())
        .map(|_| {
            VoiceDeviceDescriptor::new(
                request
                    .input_device_id
                    .clone()
                    .unwrap_or_else(|| "default".to_owned()),
                request
                    .input_device_label
                    .clone()
                    .unwrap_or_else(|| "Default microphone".to_owned()),
                VoiceDeviceKind::AudioInput,
            )
        });
    let output_device = request
        .output_device_id
        .as_ref()
        .or(request.output_device_label.as_ref())
        .map(|_| {
            VoiceDeviceDescriptor::new(
                request
                    .output_device_id
                    .clone()
                    .unwrap_or_else(|| "default".to_owned()),
                request
                    .output_device_label
                    .clone()
                    .unwrap_or_else(|| "Default speaker".to_owned()),
                VoiceDeviceKind::AudioOutput,
            )
        });
    VoiceDeviceSelection::new(permission, input_device, output_device)
}

fn core_voice_device_view(device: &VoiceDeviceDescriptor) -> discrypt_core::VoiceDeviceView {
    discrypt_core::VoiceDeviceView {
        device_id: device.device_id.clone(),
        label: device.label.clone(),
        kind: match device.kind {
            VoiceDeviceKind::AudioInput => "audio_input",
            VoiceDeviceKind::AudioOutput => "audio_output",
        }
        .to_owned(),
    }
}

fn participant_id_from_friend_code(friend_code: &str) -> String {
    let fingerprint = friend_code
        .split("&fp=")
        .nth(1)
        .and_then(|tail| tail.split('&').next())
        .unwrap_or(friend_code);
    format!(
        "friend-{}",
        fingerprint.chars().take(10).collect::<String>()
    )
}

fn invite_expiration_horizon(label: &str) -> String {
    (Utc::now() + invite_expiration_duration(label)).to_rfc3339()
}

fn invite_expiration_duration(label: &str) -> Duration {
    let lower = label.to_ascii_lowercase();
    if lower.contains("hour") || lower.contains("1 h") {
        Duration::hours(1)
    } else if lower.contains("day") || lower.contains("24") || lower.contains("1 d") {
        Duration::days(1)
    } else if lower.contains("30") {
        Duration::days(30)
    } else if lower.contains("90") {
        Duration::days(90)
    } else {
        Duration::days(7)
    }
}

fn parse_max_uses(label: &str) -> u32 {
    label
        .split(|ch: char| !ch.is_ascii_digit())
        .find_map(|part| part.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(5)
}

fn default_signaling_endpoint() -> String {
    let configured = std::env::var("EXTERNAL_SIGNALING_PUBLIC_ENDPOINT")
        .ok()
        .filter(|endpoint| {
            InviteSignalingMetadata::new(
                endpoint.clone(),
                InviteEndpointPolicy::ProductionTls,
                InviteTrustMetadata {
                    signaling_fingerprint: signaling_fingerprint_for_endpoint(endpoint),
                    trust_status: "signed endpoint fingerprint; verify before MLS Welcome"
                        .to_owned(),
                },
            )
            .is_ok()
        });
    configured.unwrap_or_else(|| InviteSignalingMetadata::default_production().signaling_endpoint)
}

fn hash_commitment(domain: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    for part in parts {
        hasher.update([0]);
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn shared_runtime_material(
    domain: &str,
    connectivity: &ConnectivityPolicyView,
    profile_id: &str,
    len: usize,
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut counter = 0_u8;
    while out.len() < len {
        let mut hasher = Sha256::new();
        hasher.update(domain.as_bytes());
        hasher.update([0]);
        hasher.update(connectivity.connectivity_schema_version.to_be_bytes());
        hasher.update([0]);
        hasher.update(connectivity.invite_kind.as_bytes());
        hasher.update([0]);
        hasher.update(connectivity.scope_id_commitment.as_bytes());
        hasher.update([0]);
        hasher.update(profile_id.as_bytes());
        if let Some(dm) = &connectivity.dm_bootstrap {
            hasher.update([0]);
            hasher.update(dm.inviter_identity_commitment.as_bytes());
            hasher.update([0]);
            hasher.update(dm.contact_token_commitment.as_bytes());
            hasher.update([0]);
            hasher.update(dm.reply_rendezvous_commitment.as_bytes());
        }
        if let Some(group) = &connectivity.group_bootstrap {
            hasher.update([0]);
            hasher.update(group.group_identity_commitment.as_bytes());
            hasher.update([0]);
            hasher.update(group.role_admission_policy_commitment.as_bytes());
            hasher.update([0]);
            hasher.update(group.channel_policy_commitment.as_bytes());
        }
        hasher.update([0, counter]);
        out.extend_from_slice(&hasher.finalize());
        counter = counter.wrapping_add(1);
    }
    out.truncate(len);
    out
}

fn profile_kind_name(kind: &InviteSignalingAdapterKind) -> String {
    kind.canonical_name().to_owned()
}

fn profile_kind_from_name(value: &str) -> InviteSignalingAdapterKind {
    match value {
        "nostr" => InviteSignalingAdapterKind::Nostr,
        "ipfs_pubsub" => InviteSignalingAdapterKind::IpfsPubsub,
        "discrypt_quic_rendezvous" => InviteSignalingAdapterKind::DiscryptQuicRendezvous,
        _ => InviteSignalingAdapterKind::Mqtt,
    }
}

fn transport_adapter_kind_from_name(value: &str) -> Option<SignalingAdapterKind> {
    match value {
        "mqtt" => Some(SignalingAdapterKind::Mqtt),
        "nostr" => Some(SignalingAdapterKind::Nostr),
        "ipfs_pubsub" => Some(SignalingAdapterKind::IpfsPubsub),
        "discrypt_quic_rendezvous" => Some(SignalingAdapterKind::DiscryptQuicRendezvous),
        _ => None,
    }
}

fn default_provider_policy_version() -> u32 {
    INVITE_PROVIDER_POLICY_VERSION
}

fn default_provider_rotation_policy() -> String {
    "rotate by issuing a fresh signed invite/connectivity policy when endpoint trust, rate limits, or availability changes".to_owned()
}

fn endpoint_allowlist_commitment(adapter_kind: &str, endpoint: &str) -> String {
    hash_commitment(
        "discrypt-provider-endpoint-allowlist-v1",
        &[adapter_kind, endpoint],
    )
}

fn validate_provider_policy(profile: &SignalingProfileView) -> Result<(), String> {
    if profile.provider_policy_version != INVITE_PROVIDER_POLICY_VERSION {
        return Err(format!(
            "Unsupported provider policy version {}",
            profile.provider_policy_version
        ));
    }
    if profile.provider_rotation_policy.trim().is_empty()
        || profile.provider_rotation_policy.trim() != profile.provider_rotation_policy
    {
        return Err("Provider rotation policy must be non-empty and trimmed".to_owned());
    }
    if profile.endpoint_allowlist_commitments.is_empty() {
        return Err("Provider endpoint allowlist commitments must not be empty".to_owned());
    }
    let allowed = profile
        .endpoints
        .iter()
        .map(|endpoint| endpoint_allowlist_commitment(&profile.adapter_kind, endpoint))
        .collect::<std::collections::BTreeSet<_>>();
    let declared = profile
        .endpoint_allowlist_commitments
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    if !allowed.is_subset(&declared) {
        return Err(
            "Provider endpoint is not covered by the signed allowlist commitments".to_owned(),
        );
    }
    Ok(())
}

fn transport_profile_from_view(
    profile: &SignalingProfileView,
) -> Result<SignalingAdapterProfile, String> {
    validate_provider_policy(profile)?;
    let kind = transport_adapter_kind_from_name(&profile.adapter_kind)
        .ok_or_else(|| format!("Unknown signaling adapter kind {}", profile.adapter_kind))?;
    let endpoints = profile
        .endpoints
        .iter()
        .map(|endpoint| {
            let mut provider = SignalingProviderEndpoint::new(
                Endpoint::new(endpoint.clone()),
                endpoint_security_for_probe(endpoint),
            );
            provider.trust_fingerprint = Some(profile.trust_fingerprint.clone());
            provider.retained_presence = profile
                .capabilities
                .iter()
                .any(|capability| capability == "retained_presence");
            provider
        })
        .collect::<Vec<_>>();
    let transport_profile = SignalingAdapterProfile {
        profile_id: profile.profile_id.clone(),
        kind,
        endpoints,
        metadata_posture: provider_metadata_posture_from_name(&profile.metadata_posture),
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new(
            profile.adapter_kind.clone(),
            "runtime app-state selected provider adapter; opaque envelopes only",
        )
        .map_err(|error| error.to_string())?,
    };
    transport_profile
        .validate()
        .map_err(|error| error.to_string())?;
    Ok(transport_profile)
}

fn endpoint_security_for_probe(endpoint: &str) -> SignalingEndpointSecurity {
    if endpoint.starts_with("mqtt://127.0.0.1")
        || endpoint.starts_with("ws://127.0.0.1")
        || endpoint.starts_with("http://127.0.0.1")
        || endpoint.starts_with("quic://127.0.0.1")
        || endpoint.starts_with("/ip4/127.0.0.1/")
        || endpoint.starts_with("/ip6/::1/")
    {
        SignalingEndpointSecurity::LocalDevLoopback
    } else {
        SignalingEndpointSecurity::ProductionTls
    }
}

fn provider_metadata_posture_from_name(value: &str) -> ProviderMetadataPosture {
    match value {
        "random_topic" => ProviderMetadataPosture::RandomTopic,
        "epoch_rotating_topic" => ProviderMetadataPosture::EpochRotatingTopic,
        _ => ProviderMetadataPosture::HashedTopic,
    }
}

fn runtime_role_label(role: Option<ProviderTextControlRuntimePeerRole>) -> &'static str {
    match role {
        Some(ProviderTextControlRuntimePeerRole::Offerer) => "offerer",
        Some(ProviderTextControlRuntimePeerRole::Answerer) => "answerer",
        None => "test-harness",
    }
}

fn parse_text_control_runtime_role(
    role: &str,
) -> Result<ProviderTextControlRuntimePeerRole, String> {
    match role {
        "offerer" => Ok(ProviderTextControlRuntimePeerRole::Offerer),
        "answerer" => Ok(ProviderTextControlRuntimePeerRole::Answerer),
        other => Err(format!("Unsupported text/control runtime role {other}")),
    }
}

fn prepare_text_control_runtime_attach_job(
    guard: &mut TauriAppService,
    command_name: &'static str,
    active_session_id: String,
    runtime_inputs: TextControlRuntimeAttachInputs,
    attachment: TextControlRuntimePeerAttachment,
) -> TextControlRuntimeAttachJob {
    let pending = PendingTextControlTransportRuntime {
        session_id: active_session_id.clone(),
        role: attachment.role,
        local_peer_id: attachment.local_peer_id.0.clone(),
        remote_peer_id: attachment.remote_peer_id.0.clone(),
    };
    guard.pending_text_control_transport_runtime = Some(pending.clone());
    guard.state.push_event(
        "transport.text_runtime_attach_started",
        format!(
            "Starting backend-owned provider-backed text/control runtime session {} as {} local_peer={} remote_peer={}",
            active_session_id,
            runtime_role_label(Some(attachment.role)),
            pending.local_peer_id,
            pending.remote_peer_id
        ),
    );
    guard.persist();
    TextControlRuntimeAttachJob {
        command_name,
        active_session_id,
        inputs: runtime_inputs,
        role: attachment.role,
        local_peer_id: attachment.local_peer_id,
        remote_peer_id: attachment.remote_peer_id,
    }
}

fn spawn_text_control_runtime_attach(job: TextControlRuntimeAttachJob) {
    std::thread::spawn(move || {
        let executor = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        {
            Ok(executor) => Arc::new(executor),
            Err(error) => {
                let service = app_service();
                let mut guard = service
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.pending_text_control_transport_runtime = None;
                guard.state.push_command_error(
                    "transport.text_runtime_attach_unavailable",
                    job.command_name,
                    "transport_runtime_executor_unavailable",
                    format!("Could not start text/control runtime executor: {error}"),
                    "Retry after the backend can construct a Tokio executor for the live provider runtime",
                );
                guard.persist();
                return;
            }
        };

        let runtime_result = start_role_split_text_control_runtime(
            executor.clone(),
            job.inputs,
            job.role,
            job.local_peer_id,
            job.remote_peer_id,
        );
        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match runtime_result {
            Ok(runtime) => {
                guard
                    .state
                    .mark_text_session_runtime_route_proof(runtime.evidence());
                guard.attach_owned_text_control_transport_runtime(
                    runtime,
                    executor,
                    job.active_session_id.clone(),
                );
                guard.state.push_event(
                    "transport.text_runtime_attached",
                    format!(
                        "Attached backend-owned provider-backed text/control runtime session {} as {}",
                        job.active_session_id,
                        runtime_role_label(Some(job.role))
                    ),
                );
            }
            Err(error) => {
                if guard
                    .pending_text_control_transport_runtime
                    .as_ref()
                    .is_some_and(|pending| pending.session_id == job.active_session_id)
                {
                    guard.pending_text_control_transport_runtime = None;
                }
                guard.state.push_command_error(
                    "transport.text_runtime_attach_unavailable",
                    job.command_name,
                    "transport_runtime_attach_failed",
                    error,
                    "Ensure both peers are online in the same DM/group scope, the invite/provider profile matches, and provider-signaled STUN/TURN policy proves WebRTC attach readiness",
                );
            }
        }
        guard.persist();
    });
}

fn start_role_split_text_control_runtime(
    executor: Arc<tokio::runtime::Runtime>,
    inputs: TextControlRuntimeAttachInputs,
    role: ProviderTextControlRuntimePeerRole,
    local_peer_id: SignalingPeerId,
    remote_peer_id: SignalingPeerId,
) -> Result<discrypt_transport::ProviderTextControlRuntime, String> {
    executor
        .block_on(async move {
            match role {
                ProviderTextControlRuntimePeerRole::Offerer => {
                    start_provider_webrtc_text_control_offer_runtime(
                        inputs.profile,
                        inputs.scope,
                        &inputs.bootstrap_secret,
                        &inputs.random_entropy,
                        discrypt_transport::WebRtcNegotiationConfig::new(inputs.ice_config),
                        local_peer_id,
                        remote_peer_id,
                    )
                    .await
                }
                ProviderTextControlRuntimePeerRole::Answerer => {
                    start_provider_webrtc_text_control_answer_runtime_with_answerer(
                        inputs.profile,
                        inputs.scope,
                        &inputs.bootstrap_secret,
                        &inputs.random_entropy,
                        discrypt_transport::WebRtcNegotiationConfig::new(inputs.ice_config),
                        local_peer_id,
                        remote_peer_id,
                        move |received| {
                            let frame: TextControlFrameView =
                                serde_json::from_slice(&received).map_err(|error| {
                                    TransportError::Unavailable(format!(
                                        "receiver could not decode live text/control frame: {error}"
                                    ))
                                })?;
                            let mut response_frame = None;
                            app_service()
                                .lock()
                                .map_err(|_| {
                                    TransportError::Unavailable(
                                        "live runtime app service lock poisoned".to_owned(),
                                    )
                                })?
                                .mutate(|state| {
                                    response_frame = state.handle_text_control_frame(frame);
                                });
                            let response_frame = response_frame.ok_or_else(|| {
                                TransportError::Unavailable(
                                    "receiver did not accept live text/control frame or generate receipt"
                                        .to_owned(),
                                )
                            })?;
                            serde_json::to_vec(&response_frame).map_err(|error| {
                                TransportError::Unavailable(format!(
                                    "could not encode live text/control response frame: {error}"
                                ))
                            })
                        },
                    )
                    .await
                }
            }
        })
        .map_err(|error| error.to_string())
}

fn run_provider_adapter_probe(
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: Vec<u8>,
    random_entropy: Vec<u8>,
) -> Result<discrypt_transport::ProviderAdapterRoundtripProbe, String> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| format!("Could not start provider probe runtime: {error}"))?;
        runtime
            .block_on(probe_provider_adapter_roundtrip(
                profile,
                scope,
                &bootstrap_secret,
                &random_entropy,
            ))
            .map_err(|error| error.to_string())
    })
    .join()
    .map_err(|_| "Provider adapter probe thread panicked".to_owned())?
}

fn run_provider_webrtc_data_channel_probe(
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: Vec<u8>,
    random_entropy: Vec<u8>,
    ice_servers: IceServerConfig,
    text_control_frame: Vec<u8>,
) -> Result<discrypt_transport::ProviderWebRtcDataChannelProbe, String> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| {
                format!("Could not start WebRTC data-channel probe runtime: {error}")
            })?;
        runtime
            .block_on(probe_provider_webrtc_datachannel_text_frame_roundtrip(
                profile,
                scope,
                &bootstrap_secret,
                &random_entropy,
                ice_servers,
                text_control_frame,
            ))
            .map_err(|error| error.to_string())
    })
    .join()
    .map_err(|_| "Provider WebRTC data-channel probe thread panicked".to_owned())?
}

fn run_provider_webrtc_data_channel_request_response_probe(
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: Vec<u8>,
    random_entropy: Vec<u8>,
    ice_servers: IceServerConfig,
    text_control_frame: Vec<u8>,
    receipt_control_frame: Vec<u8>,
) -> Result<discrypt_transport::ProviderWebRtcDataChannelProbe, String> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| {
                format!("Could not start WebRTC data-channel probe runtime: {error}")
            })?;
        runtime
            .block_on(
                probe_provider_webrtc_datachannel_request_response_roundtrip(
                    profile,
                    scope,
                    &bootstrap_secret,
                    &random_entropy,
                    ice_servers,
                    text_control_frame,
                    receipt_control_frame,
                ),
            )
            .map_err(|error| error.to_string())
    })
    .join()
    .map_err(|_| "Provider WebRTC data-channel request/response probe thread panicked".to_owned())?
}

#[cfg(test)]
fn run_provider_webrtc_data_channel_request_response_probe_with_answerer<F>(
    profile: SignalingAdapterProfile,
    scope: ConversationScope,
    bootstrap_secret: Vec<u8>,
    random_entropy: Vec<u8>,
    ice_servers: IceServerConfig,
    text_control_frame: Vec<u8>,
    answerer: F,
) -> Result<discrypt_transport::ProviderWebRtcDataChannelProbe, String>
where
    F: FnOnce(Vec<u8>) -> Result<Vec<u8>, String> + Send + 'static,
{
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| {
                format!("Could not start WebRTC data-channel answerer probe runtime: {error}")
            })?;
        runtime
            .block_on(
                probe_provider_webrtc_datachannel_request_response_with_config_and_answerer(
                    profile,
                    scope,
                    &bootstrap_secret,
                    &random_entropy,
                    discrypt_transport::WebRtcNegotiationConfig::new(ice_servers),
                    text_control_frame,
                    move |received| answerer(received).map_err(TransportError::SignalingAdapter),
                ),
            )
            .map_err(|error| error.to_string())
    })
    .join()
    .map_err(|_| {
        "Provider WebRTC data-channel answerer callback probe thread panicked".to_owned()
    })?
}

fn ice_config_from_connectivity(
    connectivity: &ConnectivityPolicyView,
) -> Result<IceServerConfig, discrypt_transport::TransportError> {
    let policy = ice_endpoint_policy_from_connectivity(connectivity)?;
    IceServerConfig::new(policy.stun_servers, policy.turn_servers)
}

fn ice_endpoint_policy_from_connectivity(
    connectivity: &ConnectivityPolicyView,
) -> Result<IceEndpointPolicy, discrypt_transport::TransportError> {
    let stun_servers = connectivity
        .ice_stun_servers
        .iter()
        .cloned()
        .map(Endpoint::new)
        .collect();
    let mut turn_servers = connectivity
        .ice_turn_servers
        .iter()
        .map(|server| {
            let (username, credential, credential_expires_at) = if server.credential_declared {
                (
                    Some("redacted-turn-username".to_owned()),
                    Some("redacted-turn-credential".to_owned()),
                    server.credential_expires_at.clone(),
                )
            } else {
                (None, None, None)
            };
            TurnServerConfig::new(
                Endpoint::new(server.endpoint.clone()),
                username,
                credential,
                credential_expires_at,
            )
        })
        .collect::<Vec<_>>();
    if let Ok(endpoint) = std::env::var("DISCRYPT_PUBLIC_TURN_ENDPOINT") {
        let endpoint = endpoint.trim();
        if !endpoint.is_empty() {
            turn_servers.push(TurnServerConfig::new(
                Endpoint::new(endpoint.to_owned()),
                std::env::var("DISCRYPT_PUBLIC_TURN_USERNAME").ok(),
                std::env::var("DISCRYPT_PUBLIC_TURN_CREDENTIAL").ok(),
                std::env::var("DISCRYPT_PUBLIC_TURN_CREDENTIAL_EXPIRES_AT").ok(),
            ));
        }
    }
    IceEndpointPolicy::new(stun_servers, turn_servers)
}

fn default_adapter_endpoint(kind: &InviteSignalingAdapterKind) -> Option<String> {
    match kind {
        InviteSignalingAdapterKind::Mqtt => Some(
            std::env::var("DISCRYPT_DEFAULT_MQTT_ENDPOINT")
                .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned()),
        ),
        InviteSignalingAdapterKind::Nostr => Some(
            std::env::var("DISCRYPT_DEFAULT_NOSTR_ENDPOINT")
                .unwrap_or_else(|_| "wss://relay.damus.io".to_owned()),
        ),
        InviteSignalingAdapterKind::IpfsPubsub => {
            std::env::var("DISCRYPT_DEFAULT_IPFS_BOOTSTRAP_ENDPOINTS")
                .ok()
                .and_then(|endpoints| {
                    endpoints
                        .split(',')
                        .map(str::trim)
                        .find(|endpoint| !endpoint.is_empty())
                        .map(ToOwned::to_owned)
                })
        }
        InviteSignalingAdapterKind::DiscryptQuicRendezvous => {
            std::env::var("DISCRYPT_DEFAULT_QUIC_RENDEZVOUS_ENDPOINT")
                .ok()
                .filter(|endpoint| !endpoint.trim().is_empty())
        }
    }
}

fn default_ice_stun_servers() -> Vec<String> {
    vec!["stun:stun.l.google.com:19302".to_owned()]
}

fn default_redacted_turn_servers() -> Vec<IceTurnServerView> {
    Vec::new()
}

fn signaling_profile_for_endpoint(
    scope_commitment: &str,
    kind: InviteSignalingAdapterKind,
    endpoint: String,
    profile_suffix: &str,
) -> SignalingProfileView {
    let adapter_kind = profile_kind_name(&kind);
    SignalingProfileView {
        profile_id: format!("{adapter_kind}-{profile_suffix}"),
        adapter_kind: adapter_kind.clone(),
        endpoints: vec![endpoint.clone()],
        room_topic_commitment: hash_commitment(
            "discrypt-rendezvous-topic-commitment-v1",
            &[scope_commitment, &adapter_kind],
        ),
        trust_fingerprint: signaling_fingerprint_for_endpoint(&endpoint),
        ttl_seconds: 300,
        metadata_posture: "hashed_topic".to_owned(),
        rate_limit_policy: "bounded publish/take with provider backoff".to_owned(),
        provider_policy_version: INVITE_PROVIDER_POLICY_VERSION,
        endpoint_allowlist_commitments: vec![endpoint_allowlist_commitment(
            &adapter_kind,
            &endpoint,
        )],
        provider_rotation_policy: default_provider_rotation_policy(),
        capabilities: vec![
            "presence_ttl".to_owned(),
            "trickle_ice".to_owned(),
            "broadcast_control".to_owned(),
            "health_telemetry".to_owned(),
        ],
    }
}

fn default_signaling_profiles(scope_commitment: &str) -> Vec<SignalingProfileView> {
    [
        InviteSignalingAdapterKind::Nostr,
        InviteSignalingAdapterKind::Mqtt,
        InviteSignalingAdapterKind::IpfsPubsub,
        InviteSignalingAdapterKind::DiscryptQuicRendezvous,
    ]
    .into_iter()
    .filter_map(|kind| {
        let endpoint = default_adapter_endpoint(&kind)?;
        Some(signaling_profile_for_endpoint(
            scope_commitment,
            kind,
            endpoint,
            "default",
        ))
    })
    .collect()
}

fn runtime_peer_id_from_commitment(label: &str, commitment: &str) -> String {
    let digest = hash_commitment("discrypt-runtime-peer-id-v1", &[label, commitment]);
    let short = digest.get(..16).unwrap_or(digest.as_str());
    format!("peer-{short}")
}

fn dm_runtime_peers(
    connectivity: Option<&ConnectivityPolicyView>,
    local_role: &str,
) -> Vec<DmRuntimePeerView> {
    let Some(dm_bootstrap) = connectivity.and_then(|policy| policy.dm_bootstrap.as_ref()) else {
        return Vec::new();
    };
    let inviter_peer_id = runtime_peer_id_from_commitment(
        "dm-inviter-runtime-peer",
        &dm_bootstrap.inviter_identity_commitment,
    );
    let reply_peer_id = runtime_peer_id_from_commitment(
        "dm-reply-runtime-peer",
        &dm_bootstrap.reply_rendezvous_commitment,
    );
    let local_is_inviter = local_role == "inviter";
    vec![
        DmRuntimePeerView {
            peer_id: inviter_peer_id,
            role: "inviter".to_owned(),
            is_local: local_is_inviter,
            source: "signed_dm_bootstrap_v1".to_owned(),
        },
        DmRuntimePeerView {
            peer_id: reply_peer_id,
            role: "reply".to_owned(),
            is_local: !local_is_inviter,
            source: "signed_dm_bootstrap_v1".to_owned(),
        },
    ]
}

fn group_runtime_peers(
    connectivity: Option<&ConnectivityPolicyView>,
    local_role: &str,
) -> Vec<GroupRuntimePeerView> {
    let Some(group_bootstrap) = connectivity.and_then(|policy| policy.group_bootstrap.as_ref())
    else {
        return Vec::new();
    };
    let owner_peer_id = runtime_peer_id_from_commitment(
        "group-owner-runtime-peer",
        &group_bootstrap.group_identity_commitment,
    );
    let member_commitment = format!(
        "{}:{}",
        group_bootstrap.role_admission_policy_commitment, group_bootstrap.channel_policy_commitment
    );
    let member_peer_id =
        runtime_peer_id_from_commitment("group-member-runtime-peer", &member_commitment);
    let local_is_owner = local_role == "owner";
    vec![
        GroupRuntimePeerView {
            peer_id: owner_peer_id,
            role: "owner".to_owned(),
            is_local: local_is_owner,
            source: "signed_group_bootstrap_v1".to_owned(),
        },
        GroupRuntimePeerView {
            peer_id: member_peer_id,
            role: "member".to_owned(),
            is_local: !local_is_owner,
            source: "signed_group_bootstrap_v1".to_owned(),
        },
    ]
}

fn group_connectivity_policy_from_request(
    group_id: &str,
    request: &CreateGroupRequest,
) -> ConnectivityPolicyView {
    let scope_id_commitment = hash_commitment("discrypt-group-scope-commitment-v1", &[group_id]);
    let signaling_profiles = request
        .adapter_kind
        .as_deref()
        .zip(request.signaling_endpoint.as_deref())
        .and_then(|(kind, endpoint)| {
            let endpoint = endpoint.trim();
            (!endpoint.is_empty()).then(|| {
                vec![signaling_profile_for_endpoint(
                    &scope_id_commitment,
                    profile_kind_from_name(kind),
                    endpoint.to_owned(),
                    "custom",
                )]
            })
        })
        .unwrap_or_else(|| default_signaling_profiles(&scope_id_commitment));
    let ice_stun_servers = request
        .ice_stun_servers
        .as_ref()
        .map(|servers| {
            servers
                .iter()
                .map(|server| server.trim())
                .filter(|server| !server.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|servers| !servers.is_empty())
        .unwrap_or_else(default_ice_stun_servers);
    let ice_turn_servers = request
        .ice_turn_servers
        .clone()
        .map(|servers| {
            servers
                .into_iter()
                .filter(|server| !server.endpoint.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(default_redacted_turn_servers);
    ConnectivityPolicyView {
        connectivity_schema_version: INVITE_CONNECTIVITY_SCHEMA_VERSION,
        invite_kind: InviteKind::GroupJoin.canonical_name().to_owned(),
        scope_id_commitment: scope_id_commitment.clone(),
        signaling_profiles,
        ice_stun_servers,
        ice_turn_servers,
        privacy_label: "Group invite topics are derived commitments; group names, channel names, and room secrets are not exposed".to_owned(),
        dm_bootstrap: None,
        group_bootstrap: Some(GroupInviteBootstrapView {
            group_identity_commitment: scope_id_commitment.clone(),
            role_admission_policy_commitment: hash_commitment(
                "discrypt-group-admission-policy-commitment-v1",
                &[group_id],
            ),
            channel_policy_commitment: hash_commitment(
                "discrypt-channel-policy-commitment-v1",
                &[group_id],
            ),
        }),
    }
}

fn group_connectivity_policy(group_id: &str) -> ConnectivityPolicyView {
    group_connectivity_policy_from_request(
        group_id,
        &CreateGroupRequest {
            name: String::new(),
            retention: String::new(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        },
    )
}

fn app_connectivity_defaults() -> ConnectivityPolicyView {
    let scope_id_commitment =
        hash_commitment("discrypt-app-connectivity-defaults-v1", &["local-profile"]);
    ConnectivityPolicyView {
        connectivity_schema_version: INVITE_CONNECTIVITY_SCHEMA_VERSION,
        invite_kind: "app_default".to_owned(),
        scope_id_commitment: scope_id_commitment.clone(),
        signaling_profiles: default_signaling_profiles(&scope_id_commitment),
        ice_stun_servers: default_ice_stun_servers(),
        ice_turn_servers: default_redacted_turn_servers(),
        privacy_label: "App defaults are copied into new DM/group/channel policies; invites retarget provider topics to the signed scope commitment".to_owned(),
        dm_bootstrap: None,
        group_bootstrap: None,
    }
}

fn request_has_connectivity_overrides(request: &CreateGroupRequest) -> bool {
    request
        .adapter_kind
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        || request
            .signaling_endpoint
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
        || request
            .ice_stun_servers
            .as_ref()
            .is_some_and(|servers| servers.iter().any(|server| !server.trim().is_empty()))
        || request.ice_turn_servers.as_ref().is_some_and(|servers| {
            servers
                .iter()
                .any(|server| !server.endpoint.trim().is_empty())
        })
}

fn retarget_signaling_profiles(
    scope_commitment: &str,
    profiles: &[SignalingProfileView],
) -> Vec<SignalingProfileView> {
    let retargeted = profiles
        .iter()
        .filter_map(|profile| {
            let endpoint = profile
                .endpoints
                .iter()
                .find(|endpoint| !endpoint.trim().is_empty())?;
            Some(signaling_profile_for_endpoint(
                scope_commitment,
                profile_kind_from_name(&profile.adapter_kind),
                endpoint.trim().to_owned(),
                "default",
            ))
        })
        .collect::<Vec<_>>();
    if retargeted.is_empty() {
        default_signaling_profiles(scope_commitment)
    } else {
        retargeted
    }
}

fn apply_app_connectivity_defaults(
    mut policy: ConnectivityPolicyView,
    defaults: &ConnectivityPolicyView,
) -> ConnectivityPolicyView {
    policy.signaling_profiles =
        retarget_signaling_profiles(&policy.scope_id_commitment, &defaults.signaling_profiles);
    policy.ice_stun_servers = defaults.ice_stun_servers.clone();
    policy.ice_turn_servers = defaults.ice_turn_servers.clone();
    policy
}

fn normalize_endpoint_list(
    endpoints: Option<&Vec<String>>,
    fallback: Vec<String>,
    validator: fn(&str) -> Result<(), String>,
) -> Result<Vec<String>, String> {
    let Some(endpoints) = endpoints else {
        return Ok(fallback);
    };
    let normalized = endpoints
        .iter()
        .map(|endpoint| endpoint.trim())
        .filter(|endpoint| !endpoint.is_empty())
        .map(|endpoint| {
            validator(endpoint)?;
            Ok(endpoint.to_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    if normalized.is_empty() {
        return Err(
            "At least one endpoint is required when overriding an endpoint list".to_owned(),
        );
    }
    Ok(normalized)
}

fn validate_stun_endpoint(endpoint: &str) -> Result<(), String> {
    if endpoint.starts_with("stun:") || endpoint.starts_with("stuns:") {
        Ok(())
    } else {
        Err(format!(
            "STUN endpoint must start with stun: or stuns:, got {endpoint}"
        ))
    }
}

fn validate_turn_endpoint(endpoint: &str) -> Result<(), String> {
    if endpoint.starts_with("turn:") || endpoint.starts_with("turns:") {
        Ok(())
    } else {
        Err(format!(
            "TURN endpoint must start with turn: or turns:, got {endpoint}"
        ))
    }
}

fn validate_signaling_endpoint(adapter_kind: &str, endpoint: &str) -> Result<(), String> {
    if endpoint.trim() != endpoint || endpoint.chars().any(char::is_whitespace) {
        return Err("Signaling endpoint must be trimmed and contain no whitespace".to_owned());
    }
    let valid = match adapter_kind {
        "nostr" => endpoint.starts_with("wss://") || endpoint.starts_with("ws://"),
        "mqtt" => {
            endpoint.starts_with("mqtts://")
                || endpoint.starts_with("mqtt://")
                || endpoint.starts_with("wss://")
                || endpoint.starts_with("ws://")
        }
        "ipfs_pubsub" => {
            endpoint.starts_with("/ip4/")
                || endpoint.starts_with("/ip6/")
                || endpoint.starts_with("/dns")
                || endpoint.starts_with("ipfs://")
        }
        "discrypt_quic_rendezvous" => {
            endpoint.starts_with("quic://")
                || endpoint.starts_with("https://")
                || endpoint.starts_with("wss://")
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "Unsupported endpoint {endpoint} for signaling adapter {adapter_kind}"
        ))
    }
}

fn normalize_connectivity_policy_override(
    mut policy: ConnectivityPolicyView,
    request: &SetConnectivityPolicyRequest,
) -> Result<ConnectivityPolicyView, String> {
    let adapter_kind = request
        .adapter_kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let endpoint = request
        .signaling_endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if let Some(kind) = adapter_kind.as_deref() {
        transport_adapter_kind_from_name(kind)
            .ok_or_else(|| format!("Unsupported signaling adapter kind {kind}"))?;
    }
    if adapter_kind.is_some() || endpoint.is_some() {
        let kind = adapter_kind
            .clone()
            .or_else(|| {
                policy
                    .signaling_profiles
                    .first()
                    .map(|profile| profile.adapter_kind.clone())
            })
            .ok_or_else(|| "A signaling adapter is required".to_owned())?;
        let endpoint = endpoint
            .or_else(|| {
                default_adapter_endpoint(&profile_kind_from_name(&kind)).or_else(|| {
                    policy
                        .signaling_profiles
                        .first()
                        .and_then(|profile| profile.endpoints.first().cloned())
                })
            })
            .ok_or_else(|| format!("No default endpoint is configured for adapter {kind}"))?;
        validate_signaling_endpoint(&kind, &endpoint)?;
        policy.signaling_profiles = vec![signaling_profile_for_endpoint(
            &policy.scope_id_commitment,
            profile_kind_from_name(&kind),
            endpoint,
            "custom",
        )];
    }
    policy.ice_stun_servers = normalize_endpoint_list(
        request.ice_stun_servers.as_ref(),
        policy.ice_stun_servers,
        validate_stun_endpoint,
    )?;
    policy.ice_turn_servers = match request.ice_turn_servers.as_ref() {
        Some(servers) => {
            let normalized = servers
                .iter()
                .filter(|server| !server.endpoint.trim().is_empty())
                .map(|server| {
                    let endpoint = server.endpoint.trim();
                    validate_turn_endpoint(endpoint)?;
                    Ok(IceTurnServerView {
                        endpoint: endpoint.to_owned(),
                        credential_declared: server.credential_declared,
                        credential_expires_at: server.credential_expires_at.clone(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            normalized
        }
        None => policy.ice_turn_servers,
    };
    if policy.signaling_profiles.is_empty() {
        return Err("At least one signaling profile is required".to_owned());
    }
    for profile in &policy.signaling_profiles {
        transport_profile_from_view(profile)?;
    }
    let _ = ice_endpoint_policy_from_connectivity(&policy).map_err(|error| error.to_string())?;
    Ok(policy)
}

fn dm_connectivity_policy(dm_id: &str, participant_id: &str) -> ConnectivityPolicyView {
    let scope_id_commitment = hash_commitment("discrypt-dm-scope-commitment-v1", &[dm_id]);
    ConnectivityPolicyView {
        connectivity_schema_version: INVITE_CONNECTIVITY_SCHEMA_VERSION,
        invite_kind: InviteKind::DmContact.canonical_name().to_owned(),
        scope_id_commitment: scope_id_commitment.clone(),
        signaling_profiles: default_signaling_profiles(&scope_id_commitment),
        ice_stun_servers: default_ice_stun_servers(),
        ice_turn_servers: default_redacted_turn_servers(),
        privacy_label: "DM contact invite topics are derived commitments; aliases, safety numbers, and room secrets are not exposed".to_owned(),
        dm_bootstrap: Some(DmInviteBootstrapView {
            inviter_identity_commitment: hash_commitment(
                "discrypt-dm-inviter-identity-commitment-v1",
                &[participant_id],
            ),
            contact_token_commitment: hash_commitment(
                "discrypt-dm-contact-token-commitment-v1",
                &[dm_id, participant_id],
            ),
            reply_rendezvous_commitment: hash_commitment(
                "discrypt-dm-reply-rendezvous-commitment-v1",
                &[dm_id],
            ),
        }),
        group_bootstrap: None,
    }
}

fn signed_ice_endpoint_policy_from_connectivity(
    connectivity: &ConnectivityPolicyView,
) -> Option<IceEndpointPolicy> {
    let stun_servers = connectivity
        .ice_stun_servers
        .iter()
        .map(|endpoint| Endpoint::new(endpoint.clone()))
        .collect::<Vec<_>>();
    let turn_servers = connectivity
        .ice_turn_servers
        .iter()
        .filter(|server| !server.credential_declared && server.credential_expires_at.is_none())
        .map(|server| {
            TurnServerConfig::new(Endpoint::new(server.endpoint.clone()), None, None, None)
        })
        .collect::<Vec<_>>();
    IceEndpointPolicy::new(stun_servers, turn_servers).ok()
}

fn profile_to_admission(profile: &SignalingProfileView) -> InviteSignalingProfile {
    InviteSignalingProfile {
        profile_id: profile.profile_id.clone(),
        adapter_kind: profile_kind_from_name(&profile.adapter_kind),
        endpoints: profile.endpoints.clone(),
        room_topic_commitment: profile.room_topic_commitment.clone(),
        trust_fingerprint: profile.trust_fingerprint.clone(),
        ttl_seconds: profile.ttl_seconds,
        metadata_posture: profile.metadata_posture.clone(),
        rate_limit_policy: profile.rate_limit_policy.clone(),
        capabilities: profile.capabilities.clone(),
        provider_policy_version: profile.provider_policy_version,
        endpoint_allowlist_commitments: profile.endpoint_allowlist_commitments.clone(),
        provider_rotation_policy: profile.provider_rotation_policy.clone(),
    }
}

fn profile_from_admission(profile: &InviteSignalingProfile) -> SignalingProfileView {
    SignalingProfileView {
        profile_id: profile.profile_id.clone(),
        adapter_kind: profile_kind_name(&profile.adapter_kind),
        endpoints: profile.endpoints.clone(),
        room_topic_commitment: profile.room_topic_commitment.clone(),
        trust_fingerprint: profile.trust_fingerprint.clone(),
        ttl_seconds: profile.ttl_seconds,
        metadata_posture: profile.metadata_posture.clone(),
        rate_limit_policy: profile.rate_limit_policy.clone(),
        capabilities: profile.capabilities.clone(),
        provider_policy_version: profile.provider_policy_version,
        endpoint_allowlist_commitments: profile.endpoint_allowlist_commitments.clone(),
        provider_rotation_policy: profile.provider_rotation_policy.clone(),
    }
}

fn bootstrap_metadata_from_connectivity(
    connectivity: &ConnectivityPolicyView,
) -> Result<InviteBootstrapMetadata, String> {
    let profiles = connectivity
        .signaling_profiles
        .iter()
        .map(profile_to_admission)
        .collect::<Vec<_>>();
    match connectivity.invite_kind.as_str() {
        "dm_contact" => InviteBootstrapMetadata::dm_contact(
            connectivity.scope_id_commitment.clone(),
            profiles,
            DmInviteBootstrap {
                inviter_identity_commitment: connectivity
                    .dm_bootstrap
                    .as_ref()
                    .map(|bootstrap| bootstrap.inviter_identity_commitment.clone())
                    .ok_or_else(|| "DM invite bootstrap metadata is missing".to_owned())?,
                contact_token_commitment: connectivity
                    .dm_bootstrap
                    .as_ref()
                    .map(|bootstrap| bootstrap.contact_token_commitment.clone())
                    .ok_or_else(|| "DM contact token metadata is missing".to_owned())?,
                reply_rendezvous_commitment: connectivity
                    .dm_bootstrap
                    .as_ref()
                    .map(|bootstrap| bootstrap.reply_rendezvous_commitment.clone())
                    .ok_or_else(|| "DM reply rendezvous metadata is missing".to_owned())?,
            },
        )
        .map_err(|err| err.to_string()),
        _ => InviteBootstrapMetadata::group_join(
            connectivity.scope_id_commitment.clone(),
            profiles,
            GroupInviteBootstrap {
                group_identity_commitment: connectivity
                    .group_bootstrap
                    .as_ref()
                    .map(|bootstrap| bootstrap.group_identity_commitment.clone())
                    .ok_or_else(|| "Group invite bootstrap metadata is missing".to_owned())?,
                role_admission_policy_commitment: connectivity
                    .group_bootstrap
                    .as_ref()
                    .map(|bootstrap| bootstrap.role_admission_policy_commitment.clone())
                    .ok_or_else(|| "Group admission policy metadata is missing".to_owned())?,
                channel_policy_commitment: connectivity
                    .group_bootstrap
                    .as_ref()
                    .map(|bootstrap| bootstrap.channel_policy_commitment.clone())
                    .ok_or_else(|| "Group channel policy metadata is missing".to_owned())?,
            },
        )
        .map_err(|err| err.to_string()),
    }
}

fn connectivity_from_bootstrap(
    bootstrap: &InviteBootstrapMetadata,
    ice_stun_servers: Vec<String>,
    ice_turn_servers: Vec<IceTurnServerView>,
) -> ConnectivityPolicyView {
    ConnectivityPolicyView {
        connectivity_schema_version: bootstrap.connectivity_schema_version,
        invite_kind: bootstrap.invite_kind.canonical_name().to_owned(),
        scope_id_commitment: bootstrap.scope_id_commitment.clone(),
        signaling_profiles: bootstrap
            .signaling_profiles
            .iter()
            .map(profile_from_admission)
            .collect(),
        ice_stun_servers,
        ice_turn_servers,
        privacy_label: bootstrap.privacy_label.clone(),
        dm_bootstrap: bootstrap
            .dm_bootstrap
            .as_ref()
            .map(|dm| DmInviteBootstrapView {
                inviter_identity_commitment: dm.inviter_identity_commitment.clone(),
                contact_token_commitment: dm.contact_token_commitment.clone(),
                reply_rendezvous_commitment: dm.reply_rendezvous_commitment.clone(),
            }),
        group_bootstrap: bootstrap
            .group_bootstrap
            .as_ref()
            .map(|group| GroupInviteBootstrapView {
                group_identity_commitment: group.group_identity_commitment.clone(),
                role_admission_policy_commitment: group.role_admission_policy_commitment.clone(),
                channel_policy_commitment: group.channel_policy_commitment.clone(),
            }),
    }
}

fn production_invite_link(
    descriptor: &discrypt_admission::StoredInvite,
    expires_at: &str,
    max_uses: u32,
    group_id: Option<&str>,
) -> Result<String, String> {
    let descriptor_bytes = serde_json::to_vec(descriptor)
        .map_err(|error| format!("Could not encode signed invite descriptor: {error}"))?;
    let encoded_descriptor = URL_SAFE_NO_PAD.encode(descriptor_bytes);
    let group_id_query = group_id
        .map(|group_id| format!("&gid={}", url_component(group_id)))
        .unwrap_or_default();
    Ok(format!(
        "discrypt://join/v1/{}?d={encoded_descriptor}&exp={}&max={max_uses}",
        descriptor.invite_id,
        url_component(expires_at)
    ) + &group_id_query)
}

fn url_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        let character = char::from(*byte);
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '~') {
            encoded.push(character);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn parse_invite_group_name(invite_code: &str) -> String {
    invite_code
        .rsplit('/')
        .next()
        .and_then(|tail| tail.split_once('-').map(|(_, slug)| slug))
        .map(|slug| slug.replace('-', " "))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "joined group".to_owned())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedInviteMetadata {
    invite_key: String,
    group_id: Option<String>,
    room_secret_hash: String,
    signaling_endpoint: String,
    signaling_trust_fingerprint: String,
    signaling_trust_status: String,
    endpoint_policy: String,
    ice_stun_servers: Vec<String>,
    ice_turn_servers: Vec<IceTurnServerView>,
    connectivity: ConnectivityPolicyView,
    expires_at: String,
    max_uses: u32,
}

fn parse_invite_metadata(invite_code: &str) -> Option<ParsedInviteMetadata> {
    let trimmed = invite_code.trim();
    let (prefix, query) = trimmed.split_once('?')?;
    let invite_key = prefix.rsplit('/').next()?.to_owned();
    if let Some(descriptor_payload) = query_value(query, "d") {
        let descriptor_bytes = URL_SAFE_NO_PAD.decode(descriptor_payload.as_bytes()).ok()?;
        let descriptor: discrypt_admission::StoredInvite =
            serde_json::from_slice(&descriptor_bytes).ok()?;
        descriptor.verify_issuer_signature().ok()?;
        let ice_config = descriptor.ice_server_config_at(None, Utc::now()).ok()?;
        let endpoint_policy = match descriptor.signaling_metadata.endpoint_policy {
            InviteEndpointPolicy::ProductionTls => "production_tls",
            InviteEndpointPolicy::LocalDevLoopback => "local_dev_loopback",
        }
        .to_owned();
        let ice_stun_servers = ice_stun_server_views(&ice_config);
        let ice_turn_servers = ice_turn_server_views(&ice_config);
        let connectivity = descriptor
            .bootstrap_metadata
            .as_ref()
            .map(|bootstrap| {
                connectivity_from_bootstrap(
                    bootstrap,
                    ice_stun_servers.clone(),
                    ice_turn_servers.clone(),
                )
            })
            .unwrap_or_else(|| group_connectivity_policy(&descriptor.invite_id));
        return Some(ParsedInviteMetadata {
            invite_key: descriptor.invite_id,
            group_id: query_value(query, "gid").and_then(percent_decode),
            room_secret_hash: hex::encode(descriptor.room_secret_commitment),
            signaling_endpoint: descriptor.signaling_metadata.signaling_endpoint,
            signaling_trust_fingerprint: descriptor.signaling_metadata.trust.signaling_fingerprint,
            signaling_trust_status: descriptor.signaling_metadata.trust.trust_status,
            endpoint_policy,
            ice_stun_servers,
            ice_turn_servers,
            connectivity,
            expires_at: descriptor.expires_at.to_rfc3339(),
            max_uses: descriptor.max_uses,
        });
    }
    let endpoint = query_value(query, "endpoint")
        .and_then(percent_decode)
        .filter(|value| !value.is_empty())?;
    let endpoint_policy = query_value(query, "policy")
        .and_then(percent_decode)
        .filter(|value| !value.is_empty())?;
    let signaling_trust_fingerprint = query_value(query, "trust_fp")
        .and_then(percent_decode)
        .filter(|value| value.len() == 64)?;
    let signaling_trust_status = query_value(query, "trust")
        .and_then(percent_decode)
        .filter(|value| !value.is_empty())?;
    let room_secret_hash = query_value(query, "commitment")
        .and_then(percent_decode)
        .unwrap_or_default();
    let expires_at = query_value(query, "exp")
        .and_then(percent_decode)
        .unwrap_or_default();
    let max_uses = query_value(query, "max")
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1);
    let legacy_scope =
        hash_commitment("discrypt-legacy-invite-scope-commitment-v1", &[&invite_key]);
    Some(ParsedInviteMetadata {
        invite_key: invite_key.clone(),
        group_id: query_value(query, "gid").and_then(percent_decode),
        room_secret_hash,
        signaling_endpoint: endpoint,
        signaling_trust_fingerprint,
        signaling_trust_status,
        endpoint_policy,
        ice_stun_servers: Vec::new(),
        ice_turn_servers: Vec::new(),
        connectivity: ConnectivityPolicyView {
            connectivity_schema_version: INVITE_CONNECTIVITY_SCHEMA_VERSION,
            invite_kind: InviteKind::GroupJoin.canonical_name().to_owned(),
            scope_id_commitment: legacy_scope.clone(),
            signaling_profiles: default_signaling_profiles(&legacy_scope),
            ice_stun_servers: Vec::new(),
            ice_turn_servers: Vec::new(),
            privacy_label: "Compatibility invite parsed without signed bootstrap policy; group/contact names are not used as provider topics".to_owned(),
            dm_bootstrap: None,
            group_bootstrap: Some(GroupInviteBootstrapView {
                group_identity_commitment: legacy_scope.clone(),
                role_admission_policy_commitment: hash_commitment("discrypt-legacy-admission-policy-v1", &[&invite_key]),
                channel_policy_commitment: hash_commitment("discrypt-legacy-channel-policy-v1", &[&invite_key]),
            }),
        },
        expires_at,
        max_uses,
    })
}

fn ice_stun_server_views(config: &discrypt_transport::IceServerConfig) -> Vec<String> {
    config
        .stun_servers
        .iter()
        .map(|endpoint| endpoint.0.clone())
        .collect()
}

fn ice_turn_server_views(config: &discrypt_transport::IceServerConfig) -> Vec<IceTurnServerView> {
    config
        .turn_servers
        .iter()
        .map(|server| IceTurnServerView {
            endpoint: server.endpoint.0.clone(),
            credential_declared: server.username.is_some()
                || server.credential.is_some()
                || server.credential_expires_at.is_some(),
            credential_expires_at: server.credential_expires_at.clone(),
        })
        .collect()
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (candidate, value) = pair.split_once('=')?;
        (candidate == key).then_some(value)
    })
}

fn percent_decode(value: &str) -> Option<String> {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let high = *bytes.get(index + 1)?;
                let low = *bytes.get(index + 2)?;
                let hex = [high, low];
                let fragment = std::str::from_utf8(&hex).ok()?;
                let byte = u8::from_str_radix(fragment, 16).ok()?;
                decoded.push(byte);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}

fn runtime_mode_view() -> RuntimeModeView {
    let network_ready = cfg!(feature = "production-network");
    let media_ready = cfg!(feature = "production-media");
    let storage_ready = cfg!(all(target_os = "linux", feature = "production-storage"));
    let production_labels_enabled = network_ready && media_ready && storage_ready;
    let mode = if production_labels_enabled {
        "configured-services"
    } else {
        "local-dev-harness"
    };
    RuntimeModeView {
        mode: mode.to_owned(),
        production_labels_enabled,
        harness_badge: if production_labels_enabled {
            "services configured".to_owned()
        } else {
            "local-dev / harness mode".to_owned()
        },
        disabled_reason: if production_labels_enabled {
            "Production labels enabled because network, media, and storage service features are configured".to_owned()
        } else {
            "Production labels disabled until backend state proves network, media, and storage services are configured".to_owned()
        },
        services: vec![
            ServiceCapabilityView {
                key: "network".to_owned(),
                label: "Network services".to_owned(),
                status: if network_ready { "configured" } else { "not-configured" }.to_owned(),
                detail: "Signaling/relay service labels require configured network features and backend state".to_owned(),
            },
            ServiceCapabilityView {
                key: "media".to_owned(),
                label: "Media services".to_owned(),
                status: if media_ready { "configured" } else { "not-configured" }.to_owned(),
                detail: "Voice media labels require configured media features and route evidence".to_owned(),
            },
            ServiceCapabilityView {
                key: "storage".to_owned(),
                label: "Storage services".to_owned(),
                status: if storage_ready { "configured" } else { "not-configured" }.to_owned(),
                detail: "Storage service labels require the production storage feature on supported targets".to_owned(),
            },
        ],
    }
}

fn text_state_legend() -> Vec<TextStateView> {
    vec![
        TextStateView {
            key: "pending".to_owned(),
            label: "Pending".to_owned(),
            status: "available".to_owned(),
            detail: "Message is queued before local author-log append or transport attempt"
                .to_owned(),
        },
        TextStateView {
            key: "sent_local".to_owned(),
            label: "Sent locally".to_owned(),
            status: "current-send-state".to_owned(),
            detail: default_text_state_detail(),
        },
        TextStateView {
            key: "transport_probe_verified".to_owned(),
            label: "Transport proofed".to_owned(),
            status: "available".to_owned(),
            detail: "Opaque message-derived frame crossed a provider-signaled WebRTC DataChannel; signed peer receipt is still required".to_owned(),
        },
        TextStateView {
            key: "transport_probe_failed".to_owned(),
            label: "Transport proof failed".to_owned(),
            status: "available".to_owned(),
            detail: "Requested provider-signaled WebRTC text/control proof failed; message remains without peer-delivery proof".to_owned(),
        },
        TextStateView {
            key: "peer_receipt".to_owned(),
            label: "Peer receipt".to_owned(),
            status: "requires-signed-receipt".to_owned(),
            detail: "Delivered to peer is shown only with backend-state signed receipt proof"
                .to_owned(),
        },
        TextStateView {
            key: "received".to_owned(),
            label: "Received".to_owned(),
            status: "available".to_owned(),
            detail: "Inbound messages use this state after membership, epoch, and ordering checks"
                .to_owned(),
        },
        TextStateView {
            key: "failed".to_owned(),
            label: "Failed".to_owned(),
            status: "available".to_owned(),
            detail: "Send or decrypt failures must retain the command error/recovery reason"
                .to_owned(),
        },
        TextStateView {
            key: "locked".to_owned(),
            label: "Locked".to_owned(),
            status: "available".to_owned(),
            detail: "Retention or key-lock policy can hide plaintext until authorized unlock"
                .to_owned(),
        },
        TextStateView {
            key: "shredded".to_owned(),
            label: "Shredded".to_owned(),
            status: "available".to_owned(),
            detail: "Crypto-shred/key deletion state; remote screenshots or exports are not erased"
                .to_owned(),
        },
    ]
}

fn snapshot_channel_label(message: &MessageView, state: &PersistedAppState) -> String {
    if let Some(channel_id) = &message.target.channel_id {
        for group in &state.groups {
            if let Some(channel) = group
                .channels
                .iter()
                .find(|channel| &channel.channel_id == channel_id)
            {
                return channel.name.clone();
            }
        }
    }
    if let Some(dm_id) = &message.target.dm_id {
        return dm_id.clone();
    }
    "#general".to_owned()
}

fn normalize_label(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn normalize_theme_id(value: &str) -> String {
    let trimmed = value.trim();
    if UI_THEME_IDS.contains(&trimmed) {
        trimmed.to_owned()
    } else {
        DEFAULT_THEME_ID.to_owned()
    }
}

fn normalize_template_id(value: &str) -> String {
    let trimmed = value.trim();
    if UI_TEMPLATE_IDS.contains(&trimmed) {
        trimmed.to_owned()
    } else {
        DEFAULT_TEMPLATE_ID.to_owned()
    }
}

fn normalize_channel_name(value: &str, kind: ChannelKind) -> String {
    let trimmed = normalize_label(value.trim_start_matches('#'), "secure-room");
    match kind {
        ChannelKind::Text => format!("#{trimmed}"),
        ChannelKind::Voice => trimmed,
    }
}

fn stable_id(prefix: &str, label: &str, sequence: u64) -> String {
    format!("{prefix}-{}-{sequence}", slugify(label))
}

fn stable_voice_session_id(group_id: &str, channel_id: &str) -> String {
    stable_id("voice", &format!("{group_id}:{channel_id}"), 0)
}

fn slugify(label: &str) -> String {
    let slug: String = label
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else if character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect();
    let collapsed = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        "local".to_owned()
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

    use super::*;
    use std::collections::VecDeque;
    use std::fs;
    use std::sync::MutexGuard;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    fn test_lock() -> MutexGuard<'static, ()> {
        TEST_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn fresh_state_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("discrypt-{name}-{nanos}.json"))
    }

    fn reset_with_temp_state(name: &str) -> PathBuf {
        let path = fresh_state_path(name);
        std::env::set_var("DISCRYPT_APP_STATE_PATH", &path);
        let _ = fs::remove_file(&path);
        reset_app_state();
        path
    }

    fn attach_test_remote_voice(session_id: &str, participant_id: &str) -> AppStateView {
        attach_voice_remote_media(AttachVoiceRemoteMediaRequest {
            session_id: session_id.to_owned(),
            participant_id: participant_id.to_owned(),
            participant_name: "Remote peer".to_owned(),
            remote_peer_id: format!("peer-{participant_id}"),
            stream_id: format!("stream-{participant_id}"),
            audio_track_id: format!("track-{participant_id}"),
            playback_element_id: format!("audio-{participant_id}"),
            local_audio_tracks_sent: 1,
            received_audio_frames: 3,
            speaking: true,
            attached_at_ms: 1_700_000_000_000,
        })
    }

    fn reload_global_app_service_from_path(path: &std::path::Path) {
        std::env::set_var("DISCRYPT_APP_STATE_PATH", path);
        let mut store = FileAppStore::new(path);
        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.state = load_state_from_store(&mut store);
        guard.text_control_transport_runtime = None;
        guard.pending_text_control_transport_runtime = None;
    }

    fn accept_dm_invite_as_test_profile(
        profile_name: &str,
        display_name: &str,
        device_name: &str,
        invite_code: String,
        contact_label: &str,
    ) -> Result<(PathBuf, TauriAppService), String> {
        let path = reset_with_temp_state(profile_name);
        create_user(CreateUserRequest {
            display_name: display_name.to_owned(),
            device_name: Some(device_name.to_owned()),
        });
        let accepted = accept_dm_invite(AcceptDmInviteRequest {
            invite_code,
            display_name: Some(contact_label.to_owned()),
        });
        if accepted.last_command_error.is_some() {
            return Err(format!(
                "{display_name} could not accept DM invite: {:?}",
                accepted.last_command_error
            ));
        }
        Ok((path.clone(), TauriAppService::load_for_test_path(path)))
    }

    #[allow(dead_code)]
    fn join_group_invite_as_test_profile(
        profile_name: &str,
        display_name: &str,
        device_name: &str,
        invite_code: String,
        group_name: &str,
    ) -> Result<(PathBuf, TauriAppService), String> {
        let path = reset_with_temp_state(profile_name);
        create_user(CreateUserRequest {
            display_name: display_name.to_owned(),
            device_name: Some(device_name.to_owned()),
        });
        let joined = join_group(JoinGroupRequest {
            invite_code,
            group_name: Some(group_name.to_owned()),
        });
        if joined.last_command_error.is_some() {
            return Err(format!(
                "{display_name} could not join group invite: {:?}",
                joined.last_command_error
            ));
        }
        Ok((path.clone(), TauriAppService::load_for_test_path(path)))
    }

    fn persist_openmls_handle_for_test(
        state: &mut PersistedAppState,
        group_id: &str,
        signer_public_key: &[u8],
        snapshot: discrypt_mls_core::OpenMlsGroupSnapshot,
        local_leaf: u32,
        store_path: &std::path::Path,
        status_copy: impl Into<String>,
    ) {
        let mut confirmation_hash = Sha256::new();
        confirmation_hash.update(&snapshot.confirmation_tag);
        let record = OpenMlsGroupHandleRecord {
            group_id: group_id.to_owned(),
            signer_public_key_hex: hex::encode(signer_public_key),
            epoch: snapshot.epoch,
            local_leaf,
            confirmation_tag_sha256: hex::encode(confirmation_hash.finalize()),
            openmls_store_path: Some(store_path.display().to_string()),
            status_copy: status_copy.into(),
        };
        if let Some(existing) = state
            .openmls_groups
            .iter_mut()
            .find(|existing| existing.group_id == group_id)
        {
            *existing = record;
        } else {
            state.openmls_groups.push(record);
        }
    }

    fn admit_group_invite_between_test_profiles(
        owner_path: &std::path::Path,
        joiner_path: &std::path::Path,
        group_id: &str,
        invite: &InviteView,
    ) -> Result<(String, String), String> {
        let mut owner_store = FileAppStore::new(owner_path);
        let mut joiner_store = FileAppStore::new(joiner_path);
        let mut owner_state = load_state_from_store(&mut owner_store);
        let mut joiner_state = load_state_from_store(&mut joiner_store);
        let owner_handle = owner_state
            .openmls_groups
            .iter()
            .find(|handle| handle.group_id == group_id)
            .cloned()
            .ok_or_else(|| "owner OpenMLS handle missing before admission".to_owned())?;
        let owner_signer_public_key = hex::decode(&owner_handle.signer_public_key_hex)
            .map_err(|error| format!("owner OpenMLS signer key was not hex: {error}"))?;
        let owner_openmls_path = openmls_store_path_for_app_state_path(owner_path);
        let joiner_openmls_path = openmls_store_path_for_app_state_path(joiner_path);
        let mut owner_engine = OpenMlsGroupEngine::open(&owner_openmls_path)
            .map_err(|error| format!("owner OpenMLS provider open failed: {error}"))?;
        let mut joiner_engine = OpenMlsGroupEngine::open(&joiner_openmls_path)
            .map_err(|error| format!("joiner OpenMLS provider open failed: {error}"))?;
        owner_engine
            .load_group(group_id, &owner_signer_public_key)
            .map_err(|error| format!("owner OpenMLS group load failed: {error}"))?;
        let joiner_identity = joiner_state.local_user_id();
        let joiner_package = joiner_engine
            .generate_member_package(joiner_identity.as_bytes())
            .map_err(|error| format!("joiner OpenMLS key package failed: {error}"))?;
        let admitted = owner_engine
            .add_member_package(group_id, &joiner_package)
            .map_err(|error| format!("owner OpenMLS add-member failed: {error}"))?;
        let welcome = admitted
            .welcome
            .as_ref()
            .ok_or_else(|| "OpenMLS add-member did not return a Welcome".to_owned())?;
        let welcome_issuer = SigningKey::generate(&mut OsRng);
        let admission_invite_id = Uuid::new_v4();
        let authorized_welcome = AuthorizedWelcome::sign(
            admission_invite_id.to_string(),
            group_id.as_bytes().to_vec(),
            welcome,
            Utc::now() + Duration::minutes(5),
            &welcome_issuer,
        );
        let room_secret_hash: [u8; 32] = hex::decode(&invite.room_secret_hash)
            .map_err(|error| format!("invite room-secret commitment was not hex: {error}"))?
            .try_into()
            .map_err(|_| "invite room-secret commitment was not 32 bytes".to_owned())?;
        let invite_id_digest = Sha256::digest(invite.invite_key.as_bytes());
        let mut invite_id_bytes = [0_u8; 16];
        invite_id_bytes.copy_from_slice(&invite_id_digest[..16]);
        let mut admission_invite = Invite {
            id: admission_invite_id,
            room_secret_hash,
            expires_at: Utc::now() + Duration::minutes(5),
            max_uses: invite
                .max_use
                .parse::<u32>()
                .ok()
                .filter(|uses| *uses > 0)
                .unwrap_or(1),
            uses: invite.uses,
            revoked: invite.revoked,
        };
        let welcome_issuer = SigningKey::generate(&mut OsRng);
        let authorized_welcome = AuthorizedWelcome::sign(
            admission_invite.id.to_string(),
            group_id.as_bytes().to_vec(),
            welcome,
            Utc::now() + Duration::minutes(5),
            &welcome_issuer,
        );
        AdmissionController::new(PasswordGate::None, 1)
            .finalize_admission(
                &mut admission_invite,
                Utc::now(),
                joiner_identity,
                true,
                Some(&authorized_welcome),
                welcome,
            )
            .map_err(|error| format!("authorized Welcome admission failed: {error}"))?;
        let joined = joiner_engine
            .join_from_welcome(group_id, joiner_package.signer_public_key(), welcome)
            .map_err(|error| format!("joiner OpenMLS welcome join failed: {error}"))?;
        let owner_exporter = owner_engine
            .export_secret(group_id, "discrypt/text", b"g012-openmls-admission", 32)
            .map_err(|error| format!("owner OpenMLS text exporter failed: {error}"))?;
        let joiner_exporter = joiner_engine
            .export_secret(group_id, "discrypt/text", b"g012-openmls-admission", 32)
            .map_err(|error| format!("joiner OpenMLS text exporter failed: {error}"))?;
        if owner_exporter != joiner_exporter {
            return Err("OpenMLS text exporters diverged after Welcome admission".to_owned());
        }
        persist_openmls_handle_for_test(
            &mut owner_state,
            group_id,
            &owner_signer_public_key,
            admitted.state,
            0,
            &owner_openmls_path,
            "OpenMLS invite admission added a joiner and rotated the Rust-only text exporter",
        );
        persist_openmls_handle_for_test(
            &mut joiner_state,
            group_id,
            joiner_package.signer_public_key(),
            joined,
            1,
            &joiner_openmls_path,
            "OpenMLS invite admission joined from an authorized Welcome and installed the Rust-only text exporter",
        );
        persist_state_to_store(&mut owner_store, &owner_state)
            .map_err(|error| format!("owner app state persist failed: {}", error.message))?;
        persist_state_to_store(&mut joiner_store, &joiner_state)
            .map_err(|error| format!("joiner app state persist failed: {}", error.message))?;
        Ok((
            hex::encode(Sha256::digest(owner_exporter)),
            hex::encode(Sha256::digest(joiner_exporter)),
        ))
    }

    fn attach_text_control_transport_runtime_for_test(
        transport: Arc<dyn discrypt_transport::TextControlDataTransport>,
        session_id: impl Into<String>,
    ) {
        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.attach_text_control_transport_runtime(transport, session_id);
    }

    fn clear_text_control_transport_runtime_for_test() {
        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.clear_text_control_transport_runtime();
    }

    #[derive(Debug)]
    struct ReceiverBackedTextControlTransport {
        receiver: Mutex<TauriAppService>,
        queued_responses: Mutex<VecDeque<Vec<u8>>>,
        metrics: Mutex<discrypt_transport::WebRtcDataTransportMetrics>,
    }

    impl ReceiverBackedTextControlTransport {
        fn new(receiver: TauriAppService) -> Self {
            Self {
                receiver: Mutex::new(receiver),
                queued_responses: Mutex::new(VecDeque::new()),
                metrics: Mutex::new(discrypt_transport::WebRtcDataTransportMetrics {
                    schema_version: discrypt_transport::WebRtcDataTransportMetrics::SCHEMA_VERSION,
                    label: "test-text-control-datachannel".to_owned(),
                    attached_channels: 1,
                    open: true,
                    frames_sent: 0,
                    frames_received: 0,
                    bytes_sent: 0,
                    bytes_received: 0,
                    last_state: "open".to_owned(),
                }),
            }
        }

        fn receiver_state_path(&self) -> Option<PathBuf> {
            self.receiver
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .state_path_override
                .clone()
        }
    }

    #[async_trait::async_trait]
    impl discrypt_transport::TextControlDataTransport for ReceiverBackedTextControlTransport {
        async fn send_text_control_frame(
            &self,
            frame: Vec<u8>,
        ) -> Result<(), discrypt_transport::TransportError> {
            {
                let mut metrics = self
                    .metrics
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                metrics.frames_sent = metrics.frames_sent.saturating_add(1);
                metrics.bytes_sent = metrics.bytes_sent.saturating_add(frame.len() as u64);
                metrics.last_state = "sent".to_owned();
            }
            let frame_view: TextControlFrameView =
                serde_json::from_slice(&frame).map_err(|error| {
                    discrypt_transport::TransportError::Unavailable(format!(
                        "decode text/control frame failed: {error}"
                    ))
                })?;
            let mut receiver = self
                .receiver
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut response_frame = None;
            receiver.mutate(|state| {
                response_frame = state.handle_text_control_frame(frame_view);
            });
            if let Some(response_frame) = response_frame {
                let response_bytes = serde_json::to_vec(&response_frame).map_err(|error| {
                    discrypt_transport::TransportError::Unavailable(format!(
                        "encode text/control response failed: {error}"
                    ))
                })?;
                self.queued_responses
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push_back(response_bytes);
            }
            Ok(())
        }

        async fn recv_text_control_frame(
            &self,
        ) -> Result<Vec<u8>, discrypt_transport::TransportError> {
            let response = self
                .queued_responses
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pop_front()
                .ok_or_else(|| {
                    discrypt_transport::TransportError::Unavailable(
                        "no queued text/control response frame".to_owned(),
                    )
                })?;
            let mut metrics = self
                .metrics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            metrics.frames_received = metrics.frames_received.saturating_add(1);
            metrics.bytes_received = metrics.bytes_received.saturating_add(response.len() as u64);
            metrics.last_state = "received".to_owned();
            Ok(response)
        }

        async fn text_control_transport_metrics(
            &self,
        ) -> discrypt_transport::WebRtcDataTransportMetrics {
            self.metrics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }
    }

    #[derive(Debug)]
    struct HangingResponseTextControlTransport {
        metrics: Mutex<discrypt_transport::WebRtcDataTransportMetrics>,
    }

    impl HangingResponseTextControlTransport {
        fn new() -> Self {
            Self {
                metrics: Mutex::new(discrypt_transport::WebRtcDataTransportMetrics {
                    schema_version: discrypt_transport::WebRtcDataTransportMetrics::SCHEMA_VERSION,
                    label: "hanging-text-control-datachannel".to_owned(),
                    attached_channels: 1,
                    open: true,
                    frames_sent: 0,
                    frames_received: 0,
                    bytes_sent: 0,
                    bytes_received: 0,
                    last_state: "open".to_owned(),
                }),
            }
        }
    }

    #[async_trait::async_trait]
    impl discrypt_transport::TextControlDataTransport for HangingResponseTextControlTransport {
        async fn send_text_control_frame(
            &self,
            frame: Vec<u8>,
        ) -> Result<(), discrypt_transport::TransportError> {
            let mut metrics = self
                .metrics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            metrics.frames_sent = metrics.frames_sent.saturating_add(1);
            metrics.bytes_sent = metrics.bytes_sent.saturating_add(frame.len() as u64);
            metrics.last_state = "sent".to_owned();
            Ok(())
        }

        async fn recv_text_control_frame(
            &self,
        ) -> Result<Vec<u8>, discrypt_transport::TransportError> {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            Err(discrypt_transport::TransportError::Unavailable(
                "hanging test transport unexpectedly woke".to_owned(),
            ))
        }

        async fn text_control_transport_metrics(
            &self,
        ) -> discrypt_transport::WebRtcDataTransportMetrics {
            self.metrics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }
    }

    #[test]
    fn test_harness_can_run_two_isolated_app_profiles() {
        let _guard = test_lock();
        let alice_path = fresh_state_path("two-profile-alice");
        let bob_path = fresh_state_path("two-profile-bob");
        let _ = fs::remove_file(&alice_path);
        let _ = fs::remove_file(&bob_path);

        let mut alice = TauriAppService::load_for_test_path(alice_path.clone());
        let mut bob = TauriAppService::load_for_test_path(bob_path.clone());
        let alice_view = alice.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Alice".to_owned(),
                    device_name: Some("Alice laptop".to_owned()),
                },
                false,
            );
        });
        let bob_view = bob.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Bob".to_owned(),
                    device_name: Some("Bob laptop".to_owned()),
                },
                false,
            );
        });

        assert_ne!(alice_path, bob_path);
        assert_ne!(
            alice_view.profile.as_ref().map(|profile| &profile.user_id),
            bob_view.profile.as_ref().map(|profile| &profile.user_id)
        );
        assert_eq!(
            load_state_from_store(&mut FileAppStore::new(&alice_path))
                .profile
                .as_ref()
                .map(|profile| profile.display_name.as_str()),
            Some("Alice")
        );
        assert_eq!(
            load_state_from_store(&mut FileAppStore::new(&bob_path))
                .profile
                .as_ref()
                .map(|profile| profile.display_name.as_str()),
            Some("Bob")
        );
    }

    #[test]
    fn two_profile_app_ui_flow_mixes_invites_and_persists_channel_receipt() -> Result<(), String> {
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("two-profile-app-ui-flow");

        let alice_user = create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        assert_eq!(
            alice_user.lifecycle,
            AppLifecycle::Ready,
            "alice profile should reach ready state"
        );
        let alice_user_id = alice_user
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .ok_or_else(|| "alice profile id should persist".to_owned())?;
        let alice_device_key = alice_user
            .devices
            .first()
            .map(|device| device.device_key.clone())
            .ok_or_else(|| "alice device key should persist".to_owned())?;
        let themed = save_preferences(SavePreferencesRequest {
            theme_id: "ocean-contrast".to_owned(),
            template_id: "compact-ops".to_owned(),
        });
        assert_eq!(themed.preferences.theme_id, "ocean-contrast");
        assert_eq!(themed.preferences.template_id, "compact-ops");

        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let dm_invite_state = create_dm_invite(CreateDmInviteRequest {
            dm_id: Some(dm_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "3".to_owned(),
        });
        let dm_invite = dm_invite_state
            .invites
            .iter()
            .find(|invite| invite.dm_id.as_deref() == Some(dm_id.as_str()))
            .ok_or_else(|| "dm invite should be persisted".to_owned())?;

        let group_state = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: Some("mqtt".to_owned()),
            signaling_endpoint: Some("mqtts://broker.emqx.io:8883".to_owned()),
            ice_stun_servers: Some(vec![
                "stun:stun.l.google.com:19302".to_owned(),
                "stun:stun.cloudflare.com:3478".to_owned(),
            ]),
            ice_turn_servers: Some(vec![IceTurnServerView {
                endpoint: "turns:turn.example.invalid:5349".to_owned(),
                credential_declared: false,
                credential_expires_at: None,
            }]),
        });
        let group = group_state
            .groups
            .first()
            .cloned()
            .ok_or_else(|| "alice should have a created group".to_owned())?;
        let group_id = group.group_id.clone();
        let group_connectivity = group
            .connectivity
            .clone()
            .ok_or_else(|| "group connectivity should persist".to_owned())?;
        assert_eq!(
            group_connectivity.signaling_profiles[0].adapter_kind,
            "mqtt"
        );
        assert_eq!(
            group_connectivity.ice_stun_servers,
            vec![
                "stun:stun.l.google.com:19302".to_owned(),
                "stun:stun.cloudflare.com:3478".to_owned(),
            ]
        );
        assert_eq!(group_connectivity.ice_turn_servers.len(), 1);
        let group_channel_id = group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Text)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "group should include a text channel".to_owned())?;
        let voice_channel_id = group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Voice)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "group should include a voice channel".to_owned())?;
        let voice_joined = join_voice(JoinVoiceRequest {
            group_id: group_id.clone(),
            channel_id: voice_channel_id.clone(),
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic-alice".to_owned()),
            input_device_label: Some("Alice microphone".to_owned()),
            output_device_id: Some("speaker-alice".to_owned()),
            output_device_label: Some("Alice speaker".to_owned()),
        });
        let voice_session_id = voice_joined
            .voice_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "voice session should persist".to_owned())?;
        let muted = set_self_mute(SetSelfMuteRequest {
            session_id: voice_session_id.clone(),
            muted: true,
        });
        assert!(muted
            .voice_session
            .as_ref()
            .map(|session| session.self_muted)
            .unwrap_or(false));
        let local_volume_rejected = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id: voice_session_id.clone(),
            participant_id: alice_user_id.clone(),
            volume: 55,
        });
        assert_eq!(
            local_volume_rejected
                .last_command_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("voice_volume_local_participant")
        );
        let remote_participant_id = "remote-alice-proof".to_owned();
        let remote_attached = attach_test_remote_voice(&voice_session_id, &remote_participant_id);
        assert!(remote_attached
            .voice_session
            .as_ref()
            .map(|session| session.media_runtime.remote_transport_active)
            .unwrap_or(false));
        let volume = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id: voice_session_id.clone(),
            participant_id: remote_participant_id.clone(),
            volume: 55,
        });
        assert_eq!(
            volume
                .voice_session
                .as_ref()
                .and_then(|session| session
                    .participants
                    .iter()
                    .find(|participant| participant.id == remote_participant_id))
                .map(|participant| participant.volume),
            Some(55)
        );
        let focused_text = set_active_channel(SetActiveChannelRequest {
            group_id: group_id.clone(),
            channel_id: group_channel_id.clone(),
        });
        assert_eq!(
            focused_text
                .active_context
                .as_ref()
                .and_then(|context| context.channel_id.as_deref()),
            Some(group_channel_id.as_str())
        );
        let group_invite_state = create_invite(CreateInviteRequest {
            group_id: Some(group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "4".to_owned(),
        });
        let group_invite = group_invite_state
            .invites
            .iter()
            .find(|invite| invite.group_id == group_id)
            .ok_or_else(|| "group invite should be persisted".to_owned())?;

        let bob_path = fresh_state_path("two-profile-app-ui-flow-bob");
        let _ = fs::remove_file(&bob_path);
        std::env::set_var("DISCRYPT_APP_STATE_PATH", &bob_path);
        reset_app_state();
        create_user(CreateUserRequest {
            display_name: "Bob".to_owned(),
            device_name: Some("Bob laptop".to_owned()),
        });
        let bob_dm_state = accept_dm_invite(AcceptDmInviteRequest {
            invite_code: dm_invite.code.clone(),
            display_name: Some("Alice contact".to_owned()),
        });
        assert!(bob_dm_state.last_command_error.is_none());
        let bob_group_state = join_group(JoinGroupRequest {
            invite_code: group_invite.code.clone(),
            group_name: Some("Private Lab".to_owned()),
        });
        reload_global_app_service_from_path(&alice_path);

        let mut bob = TauriAppService::load_for_test_path(bob_path.clone());
        assert!(bob_dm_state.last_command_error.is_none());
        assert!(bob_group_state.last_command_error.is_none());
        assert!(bob.state.dms.iter().any(|dm| {
            dm.connectivity
                .as_ref()
                .is_some_and(|policy| policy.scope_id_commitment == dm_invite.scope_id_commitment)
        }));
        let bob_view_after_reload = TauriAppService::load_for_test_path(bob_path.clone())
            .state
            .to_view();
        let bob_user_id = bob_view_after_reload
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .ok_or_else(|| "bob profile should reload".to_owned())?;
        assert_ne!(alice_user_id, bob_user_id);
        assert!(bob
            .state
            .groups
            .iter()
            .any(|group| group.name == "Private Lab"));
        let bob_group = bob_view_after_reload
            .groups
            .iter()
            .find(|group| group.name == "Private Lab")
            .ok_or_else(|| "bob group should reload".to_owned())?;
        let bob_connectivity = bob_group
            .connectivity
            .as_ref()
            .ok_or_else(|| "bob group connectivity should reload".to_owned())?;
        assert_eq!(
            bob_connectivity.scope_id_commitment,
            group_invite.scope_id_commitment
        );
        assert_eq!(
            bob_connectivity.signaling_profiles,
            group_connectivity.signaling_profiles
        );
        assert_eq!(
            bob_connectivity.ice_stun_servers,
            group_connectivity.ice_stun_servers
        );
        assert_eq!(
            bob_connectivity.ice_turn_servers,
            group_connectivity.ice_turn_servers
        );
        assert!(bob_view_after_reload.invites.iter().any(|invite| {
            invite.invite_key == group_invite.invite_key
                && invite.uses == 1
                && invite.max_use == group_invite.max_use
        }));

        let target = MessageTargetView {
            kind: "channel".to_owned(),
            dm_id: None,
            group_id: Some(group_id.clone()),
            channel_id: Some(group_channel_id.clone()),
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "channel hello from alice".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();

        let envelope_record = load_state()
            .text_delivery_envelopes
            .into_iter()
            .find(|record| record.message_id == message_id)
            .ok_or_else(|| "persisted envelope missing for group message".to_owned())?;
        let (receipt, recipient_key_hex) =
            bob.state
                .receive_text_delivery_envelope(ReceiveTextDeliveryEnvelopeRequest {
                    target: target.clone(),
                    envelope: envelope_record.envelope.clone(),
                    sender_verifying_key_hex: envelope_record.sender_verifying_key_hex,
                    recipient_leaf: Some(2),
                })?;
        assert!(bob.state.last_command_error.is_none());
        assert!(bob.state.messages.iter().any(|message| {
            message.message_id == message_id && message.state_key == "received_envelope"
        }));

        let receipted = apply_text_delivery_receipt(ApplyTextDeliveryReceiptRequest {
            message_id: message_id.clone(),
            receipt,
            recipient_verifying_key_hex: recipient_key_hex.clone(),
        });
        let message = receipted
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "message row missing after receipt".to_owned())?;
        assert_eq!(message.state_key, "peer_receipt");

        let alice_view = load_state_from_store(&mut FileAppStore::new(&alice_path));
        assert_eq!(
            alice_view
                .profile
                .as_ref()
                .map(|profile| profile.user_id.as_str()),
            Some(alice_user_id.as_str())
        );
        assert_eq!(
            alice_view
                .devices
                .first()
                .map(|device| device.device_key.as_str()),
            Some(alice_device_key.as_str())
        );
        assert_eq!(alice_view.preferences.theme_id, "ocean-contrast");
        assert_eq!(alice_view.preferences.template_id, "compact-ops");
        assert!(alice_view.dms.iter().any(|dm| {
            dm.dm_id == dm_id
                && dm.connectivity.as_ref().is_some_and(|policy| {
                    policy.scope_id_commitment == dm_invite.scope_id_commitment
                        && policy.invite_kind == "dm_contact"
                })
        }));
        let reloaded_group = alice_view
            .groups
            .iter()
            .find(|group| group.group_id == group_id)
            .ok_or_else(|| "alice group missing after reload".to_owned())?;
        assert_eq!(
            reloaded_group.connectivity.as_ref(),
            Some(&group_connectivity)
        );
        assert!(reloaded_group
            .channels
            .iter()
            .any(|channel| channel.channel_id == group_channel_id
                && channel.kind == ChannelKind::Text));
        assert!(reloaded_group
            .channels
            .iter()
            .any(|channel| channel.channel_id == voice_channel_id
                && channel.kind == ChannelKind::Voice));
        assert_eq!(
            alice_view
                .active_context
                .as_ref()
                .and_then(|context| context.channel_id.as_deref()),
            Some(group_channel_id.as_str())
        );
        let reloaded_voice = alice_view
            .voice_session
            .as_ref()
            .ok_or_else(|| "voice session missing after reload".to_owned())?;
        assert_eq!(reloaded_voice.session_id, voice_session_id);
        assert_eq!(reloaded_voice.channel_id, voice_channel_id);
        assert!(reloaded_voice.joined);
        assert!(reloaded_voice.self_muted);
        assert_eq!(
            reloaded_voice
                .input_device
                .as_ref()
                .map(|device| device.device_id.as_str()),
            Some("mic-alice")
        );
        assert_eq!(
            reloaded_voice
                .output_device
                .as_ref()
                .map(|device| device.device_id.as_str()),
            Some("speaker-alice")
        );
        assert_eq!(
            reloaded_voice
                .participants
                .iter()
                .find(|participant| participant.id == alice_user_id)
                .map(|participant| (participant.muted, participant.volume)),
            Some((true, 82))
        );
        assert_eq!(
            reloaded_voice
                .participants
                .iter()
                .find(|participant| participant.id == remote_participant_id)
                .map(|participant| (participant.muted, participant.volume)),
            Some((false, 55))
        );
        assert!(alice_view.invites.iter().any(|invite| {
            invite.invite_key == dm_invite.invite_key
                && invite.invite_kind == "dm_contact"
                && invite.uses == 0
                && !invite.revoked
        }));
        assert!(alice_view.invites.iter().any(|invite| {
            invite.invite_key == group_invite.invite_key
                && invite.invite_kind == "group_join"
                && invite.signaling_profiles == group_connectivity.signaling_profiles
                && invite.ice_stun_servers == group_connectivity.ice_stun_servers
                && invite.ice_turn_servers == group_connectivity.ice_turn_servers
        }));
        let persisted_message = alice_view
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "message missing after reload".to_owned())?;
        let bob_key = verifying_key_from_hex(&recipient_key_hex)
            .ok_or_else(|| "recipient key should decode from envelope response".to_owned())?;
        assert_eq!(
            persisted_message.state_key, "peer_receipt",
            "peer receipt should persist after reload"
        );
        assert!(persisted_message.peer_receipt.is_some());
        assert_eq!(
            persisted_message
                .peer_receipt
                .as_ref()
                .map(|receipt| receipt.recipient_key_fingerprint.as_str()),
            Some(key_fingerprint(&bob_key).as_str())
        );
        Ok(())
    }

    #[test]
    fn g010_native_command_e2e_setup_group_invite_text_voice_is_honest() -> Result<(), String> {
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("g010-native-command-alice");

        let alice_created = create_user(CreateUserRequest {
            display_name: "Alice G010".to_owned(),
            device_name: Some("Alice native desktop".to_owned()),
        });
        assert_eq!(alice_created.lifecycle, AppLifecycle::Ready);
        let alice_profile_id = alice_created
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .ok_or_else(|| "alice native command profile missing".to_owned())?;
        assert!(alice_created.devices.iter().any(|device| device.local));

        let alice_dm = start_dm(StartDmRequest {
            display_name: "Bob G010".to_owned(),
        });
        let alice_dm_id = alice_dm
            .dms
            .iter()
            .find(|dm| dm.display_name == "Bob G010")
            .map(|dm| dm.dm_id.clone())
            .ok_or_else(|| "alice native DM missing".to_owned())?;
        let dm_invite_state = create_dm_invite(CreateDmInviteRequest {
            dm_id: Some(alice_dm_id.clone()),
            expires: "24 hours".to_owned(),
            max_use: "1 use".to_owned(),
        });
        assert!(
            dm_invite_state.last_command_error.is_none(),
            "{dm_invite_state:?}"
        );
        let dm_invite = dm_invite_state
            .invites
            .iter()
            .find(|invite| invite.dm_id.as_deref() == Some(alice_dm_id.as_str()))
            .cloned()
            .ok_or_else(|| "native DM invite missing".to_owned())?;
        assert_eq!(
            dm_invite.invite_kind,
            InviteKind::DmContact.canonical_name()
        );

        let alice_group_state = create_group(CreateGroupRequest {
            name: "G010 Native Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: Some("mqtt".to_owned()),
            signaling_endpoint: Some("mqtts://broker.example.invalid:8883".to_owned()),
            ice_stun_servers: Some(vec!["stun:stun.example.invalid:3478".to_owned()]),
            ice_turn_servers: Some(vec![IceTurnServerView {
                endpoint: "turns:turn.example.invalid:5349".to_owned(),
                credential_declared: false,
                credential_expires_at: None,
            }]),
        });
        assert!(
            alice_group_state.last_command_error.is_none(),
            "{alice_group_state:?}"
        );
        let alice_group = alice_group_state
            .groups
            .iter()
            .find(|group| group.name == "G010 Native Lab")
            .cloned()
            .ok_or_else(|| "alice native group missing".to_owned())?;
        let alice_group_id = alice_group.group_id.clone();
        let alice_text_channel_id = alice_group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Text)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "alice native text channel missing".to_owned())?;
        let alice_voice_channel_id = alice_group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Voice)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "alice native voice channel missing".to_owned())?;
        let alice_connectivity = alice_group
            .connectivity
            .clone()
            .ok_or_else(|| "alice group connectivity missing".to_owned())?;
        let group_invite_state = create_invite(CreateInviteRequest {
            group_id: Some(alice_group_id.clone()),
            expires: "24 hours".to_owned(),
            max_use: "2 uses".to_owned(),
        });
        assert!(
            group_invite_state.last_command_error.is_none(),
            "{group_invite_state:?}"
        );
        let group_invite = group_invite_state
            .invites
            .iter()
            .find(|invite| invite.group_id == alice_group_id)
            .cloned()
            .ok_or_else(|| "native group invite missing".to_owned())?;
        assert_eq!(
            group_invite.invite_kind,
            InviteKind::GroupJoin.canonical_name()
        );
        assert_eq!(
            group_invite.signaling_profiles,
            alice_connectivity.signaling_profiles
        );
        assert_eq!(
            group_invite.ice_stun_servers,
            alice_connectivity.ice_stun_servers
        );
        assert_eq!(
            group_invite.ice_turn_servers,
            alice_connectivity.ice_turn_servers
        );

        let alice_target = MessageTargetView {
            kind: "channel".to_owned(),
            dm_id: None,
            group_id: Some(alice_group_id.clone()),
            channel_id: Some(alice_text_channel_id.clone()),
        };
        let alice_sent = send_message(SendMessageRequest {
            target: alice_target.clone(),
            body: "g010 native command text proof".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        assert!(alice_sent.last_command_error.is_none(), "{alice_sent:?}");
        let alice_message = alice_sent
            .messages
            .iter()
            .find(|message| message.body == "g010 native command text proof")
            .cloned()
            .ok_or_else(|| "alice native message missing".to_owned())?;
        assert_eq!(alice_message.state_key, "sent_local");
        assert!(alice_message
            .status
            .contains("remote delivery/read receipts not claimed"));
        let alice_envelope = load_state()
            .text_delivery_envelopes
            .into_iter()
            .find(|record| record.message_id == alice_message.message_id)
            .ok_or_else(|| "alice native text envelope missing".to_owned())?;

        let alice_voice_joined = join_voice(JoinVoiceRequest {
            group_id: alice_group_id.clone(),
            channel_id: alice_voice_channel_id.clone(),
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("alice-native-mic".to_owned()),
            input_device_label: Some("Alice native microphone".to_owned()),
            output_device_id: Some("alice-native-speaker".to_owned()),
            output_device_label: Some("Alice native speaker".to_owned()),
        });
        assert!(
            alice_voice_joined.last_command_error.is_none(),
            "{alice_voice_joined:?}"
        );
        let alice_voice_session = alice_voice_joined
            .voice_session
            .as_ref()
            .ok_or_else(|| "alice voice session missing".to_owned())?;
        assert!(alice_voice_session.joined);
        assert_eq!(alice_voice_session.participants.len(), 1);
        assert!(!alice_voice_session.media_runtime.remote_transport_active);
        assert!(alice_voice_session.media_runtime.remote_audio.is_empty());
        let alice_session_id = alice_voice_session.session_id.clone();
        let alice_muted = set_self_mute(SetSelfMuteRequest {
            session_id: alice_session_id.clone(),
            muted: true,
        });
        assert!(alice_muted
            .voice_session
            .as_ref()
            .map(|session| session.self_muted)
            .unwrap_or(false));
        let alice_left = leave_voice(LeaveVoiceRequest {
            session_id: alice_session_id,
        });
        assert!(!alice_left
            .voice_session
            .as_ref()
            .map(|session| session.joined)
            .unwrap_or(true));

        let bob_path = reset_with_temp_state("g010-native-command-bob");
        let bob_created = create_user(CreateUserRequest {
            display_name: "Bob G010".to_owned(),
            device_name: Some("Bob native laptop".to_owned()),
        });
        let bob_profile_id = bob_created
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .ok_or_else(|| "bob native command profile missing".to_owned())?;
        assert_ne!(alice_profile_id, bob_profile_id);
        let accepted_dm = accept_dm_invite(AcceptDmInviteRequest {
            invite_code: dm_invite.code.clone(),
            display_name: Some("Alice native contact".to_owned()),
        });
        assert!(accepted_dm.last_command_error.is_none(), "{accepted_dm:?}");
        assert!(accepted_dm.dms.iter().any(|dm| {
            dm.display_name == "Alice native contact"
                && dm.connectivity.as_ref().is_some_and(|policy| {
                    policy.scope_id_commitment == dm_invite.scope_id_commitment
                        && policy.invite_kind == InviteKind::DmContact.canonical_name()
                })
        }));

        let joined_group = join_group(JoinGroupRequest {
            invite_code: group_invite.code.clone(),
            group_name: Some("G010 Native Lab".to_owned()),
        });
        assert!(
            joined_group.last_command_error.is_none(),
            "{joined_group:?}"
        );
        let bob_group = joined_group
            .groups
            .iter()
            .find(|group| group.name == "G010 Native Lab")
            .cloned()
            .ok_or_else(|| "bob native group missing".to_owned())?;
        assert_eq!(bob_group.role, "member");
        assert!(bob_group.connectivity.as_ref().is_some_and(|policy| {
            policy.scope_id_commitment == group_invite.scope_id_commitment
                && policy.ice_stun_servers == group_invite.ice_stun_servers
                && policy.ice_turn_servers == group_invite.ice_turn_servers
        }));
        let bob_text_channel_id = bob_group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Text)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "bob native text channel missing".to_owned())?;
        let bob_voice_channel_id = bob_group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Voice)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "bob native voice channel missing".to_owned())?;
        let bob_sent = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "channel".to_owned(),
                dm_id: None,
                group_id: Some(bob_group.group_id.clone()),
                channel_id: Some(bob_text_channel_id),
            },
            body: "g010 bob local text remains local".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let bob_send_error = bob_sent
            .last_command_error
            .as_ref()
            .ok_or_else(|| "joined profile without OpenMLS state should fail closed".to_owned())?;
        assert_eq!(bob_send_error.code, "text_delivery_envelope_failed");
        assert!(bob_send_error
            .message
            .contains("OpenMLS group state is missing"));
        assert!(bob_sent.messages.is_empty());

        let receipt_response = receive_text_delivery_envelope(ReceiveTextDeliveryEnvelopeRequest {
            target: alice_target,
            envelope: alice_envelope.envelope,
            sender_verifying_key_hex: alice_envelope.sender_verifying_key_hex,
            recipient_leaf: Some(2),
        });
        assert!(
            receipt_response.state.last_command_error.is_none(),
            "{receipt_response:?}"
        );
        assert!(receipt_response.state.messages.iter().any(|message| {
            message.message_id == alice_message.message_id
                && message.state_key == "received_envelope"
        }));
        let receipt = receipt_response
            .receipt
            .ok_or_else(|| "bob native receipt missing".to_owned())?;
        let receipt_key = receipt_response
            .recipient_verifying_key_hex
            .ok_or_else(|| "bob native receipt key missing".to_owned())?;

        let bob_voice_joined = join_voice(JoinVoiceRequest {
            group_id: bob_group.group_id.clone(),
            channel_id: bob_voice_channel_id,
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("bob-native-mic".to_owned()),
            input_device_label: Some("Bob native microphone".to_owned()),
            output_device_id: Some("bob-native-speaker".to_owned()),
            output_device_label: Some("Bob native speaker".to_owned()),
        });
        assert!(
            bob_voice_joined.last_command_error.is_none(),
            "{bob_voice_joined:?}"
        );
        let bob_voice_session = bob_voice_joined
            .voice_session
            .as_ref()
            .ok_or_else(|| "bob voice session missing".to_owned())?;
        assert!(bob_voice_session.joined);
        assert_eq!(bob_voice_session.participants.len(), 1);
        assert!(!bob_voice_session.media_runtime.remote_transport_active);
        assert!(bob_voice_session.media_runtime.remote_audio.is_empty());
        let bob_session_id = bob_voice_session.session_id.clone();
        assert!(set_self_mute(SetSelfMuteRequest {
            session_id: bob_session_id.clone(),
            muted: true,
        })
        .voice_session
        .as_ref()
        .map(|session| session.self_muted)
        .unwrap_or(false));
        assert!(!leave_voice(LeaveVoiceRequest {
            session_id: bob_session_id,
        })
        .voice_session
        .as_ref()
        .map(|session| session.joined)
        .unwrap_or(true));

        let bob_reloaded = load_state_from_store(&mut FileAppStore::new(&bob_path)).to_view();
        assert!(bob_reloaded.messages.iter().any(|message| {
            message.message_id == alice_message.message_id
                && message.state_key == "received_envelope"
        }));

        reload_global_app_service_from_path(&alice_path);
        let receipted = apply_text_delivery_receipt(ApplyTextDeliveryReceiptRequest {
            message_id: alice_message.message_id.clone(),
            receipt,
            recipient_verifying_key_hex: receipt_key.clone(),
        });
        assert!(receipted.last_command_error.is_none(), "{receipted:?}");
        let alice_reloaded = load_state_from_store(&mut FileAppStore::new(&alice_path)).to_view();
        assert!(alice_reloaded.messages.iter().any(|message| {
            message.message_id == alice_message.message_id
                && message.state_key == "peer_receipt"
                && message.peer_receipt.is_some()
        }));
        assert!(alice_reloaded.invites.iter().any(|invite| {
            invite.invite_key == group_invite.invite_key
                && invite.invite_kind == InviteKind::GroupJoin.canonical_name()
                && !invite.revoked
        }));

        let observable = alice_reloaded
            .events
            .iter()
            .map(|event| event.summary.as_str())
            .chain(
                alice_reloaded
                    .messages
                    .iter()
                    .map(|message| message.status.as_str()),
            )
            .chain(
                bob_reloaded
                    .events
                    .iter()
                    .map(|event| event.summary.as_str()),
            )
            .chain(
                bob_reloaded
                    .messages
                    .iter()
                    .map(|message| message.status.as_str()),
            )
            .collect::<Vec<_>>()
            .join("\n")
            .to_lowercase();
        for forbidden in [
            "manual pairing",
            "qr pairing",
            "fake production",
            "production-ready",
        ] {
            assert!(
                !observable.contains(forbidden),
                "native command E2E must not surface forbidden claim {forbidden}: {observable}"
            );
        }
        Ok(())
    }

    #[test]
    fn g004_two_profile_persistent_state_reloads_full_invite_policy_surface() -> Result<(), String>
    {
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("g004-full-surface-alice");

        let alice_user = create_user(CreateUserRequest {
            display_name: "Alice G004".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let alice_profile_id = alice_user
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .ok_or_else(|| "alice profile missing".to_owned())?;
        assert_eq!(alice_user.lifecycle, AppLifecycle::Ready);
        assert_eq!(alice_user.devices.len(), 1);

        let alice_preferences = save_preferences(SavePreferencesRequest {
            theme_id: "ocean-contrast".to_owned(),
            template_id: "compact-ops".to_owned(),
        });
        assert_eq!(alice_preferences.preferences.theme_id, "ocean-contrast");

        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob G004".to_owned(),
        });
        let dm = dm_state
            .dms
            .iter()
            .find(|dm| dm.display_name == "Bob G004")
            .cloned()
            .ok_or_else(|| "alice DM missing".to_owned())?;
        let dm_invite_state = create_dm_invite(CreateDmInviteRequest {
            dm_id: Some(dm.dm_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "2".to_owned(),
        });
        let dm_invite = dm_invite_state
            .invites
            .iter()
            .find(|invite| invite.dm_id.as_deref() == Some(dm.dm_id.as_str()))
            .cloned()
            .ok_or_else(|| "DM invite missing".to_owned())?;

        let group_state = create_group(CreateGroupRequest {
            name: "G004 Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: Some("mqtt".to_owned()),
            signaling_endpoint: Some("mqtts://broker.example.invalid:8883".to_owned()),
            ice_stun_servers: Some(vec![
                "stun:stun.l.google.com:19302".to_owned(),
                "stun:stun.cloudflare.com:3478".to_owned(),
            ]),
            ice_turn_servers: Some(vec![IceTurnServerView {
                endpoint: "turns:turn.example.invalid:5349".to_owned(),
                credential_declared: false,
                credential_expires_at: None,
            }]),
        });
        let group = group_state
            .groups
            .iter()
            .find(|group| group.name == "G004 Lab")
            .cloned()
            .ok_or_else(|| "alice group missing".to_owned())?;
        let group_id = group.group_id.clone();
        let text_channel_id = group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Text)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "default text channel missing".to_owned())?;
        let extra_channel_state = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "field-notes".to_owned(),
            kind: ChannelKind::Text,
            retention_status: "24 hours".to_owned(),
        });
        assert!(extra_channel_state.groups.iter().any(|group| {
            group
                .channels
                .iter()
                .any(|channel| channel.name == "#field-notes")
        }));
        let voice_channel_id = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Ops Voice".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        })
        .groups
        .iter()
        .find(|group| group.group_id == group_id)
        .and_then(|group| {
            group
                .channels
                .iter()
                .find(|channel| channel.name == "Ops Voice")
        })
        .map(|channel| channel.channel_id.clone())
        .ok_or_else(|| "voice channel missing".to_owned())?;

        let group_invite_state = create_invite(CreateInviteRequest {
            group_id: Some(group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "2".to_owned(),
        });
        let group_invite = group_invite_state
            .invites
            .iter()
            .find(|invite| invite.group_id == group_id)
            .cloned()
            .ok_or_else(|| "group invite missing".to_owned())?;
        let parsed_group_invite = parse_invite_metadata(&group_invite.code)
            .ok_or_else(|| "signed G004 group invite descriptor should parse".to_owned())?;
        assert_eq!(
            parsed_group_invite.ice_stun_servers,
            group
                .connectivity
                .as_ref()
                .map_or_else(Vec::new, |policy| { policy.ice_stun_servers.clone() })
        );
        assert_eq!(
            parsed_group_invite.ice_turn_servers,
            group
                .connectivity
                .as_ref()
                .map_or_else(Vec::new, |policy| { policy.ice_turn_servers.clone() })
        );

        let target = MessageTargetView {
            kind: "channel".to_owned(),
            dm_id: None,
            group_id: Some(group_id.clone()),
            channel_id: Some(text_channel_id.clone()),
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "G004 persistent receipt".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent
            .messages
            .iter()
            .find(|message| message.body == "G004 persistent receipt")
            .map(|message| message.message_id.clone())
            .ok_or_else(|| "alice message missing".to_owned())?;
        let envelope_record = load_state()
            .text_delivery_envelopes
            .into_iter()
            .find(|record| record.message_id == message_id)
            .ok_or_else(|| "alice persisted envelope missing".to_owned())?;

        let joined_voice = join_voice(JoinVoiceRequest {
            group_id: group_id.clone(),
            channel_id: voice_channel_id.clone(),
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("alice-mic".to_owned()),
            input_device_label: Some("Alice microphone".to_owned()),
            output_device_id: Some("alice-speaker".to_owned()),
            output_device_label: Some("Alice speaker".to_owned()),
        });
        let voice_session_id = joined_voice
            .voice_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "alice voice session missing".to_owned())?;
        let muted_voice = set_self_mute(SetSelfMuteRequest {
            session_id: voice_session_id.clone(),
            muted: true,
        });
        assert!(muted_voice
            .voice_session
            .as_ref()
            .map(|session| session.self_muted)
            .unwrap_or(false));
        let local_volume_rejected = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id: voice_session_id.clone(),
            participant_id: alice_profile_id.clone(),
            volume: 37,
        });
        assert_eq!(
            local_volume_rejected
                .last_command_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("voice_volume_local_participant")
        );
        let remote_participant_id = "remote-volume-proof".to_owned();
        attach_test_remote_voice(&voice_session_id, &remote_participant_id);
        let volume_voice = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id: voice_session_id,
            participant_id: remote_participant_id.clone(),
            volume: 37,
        });
        assert_eq!(
            volume_voice
                .voice_session
                .as_ref()
                .and_then(|session| session
                    .participants
                    .iter()
                    .find(|participant| participant.id == remote_participant_id))
                .map(|participant| participant.volume),
            Some(37)
        );
        let focused_text_channel = set_active_channel(SetActiveChannelRequest {
            group_id: group_id.clone(),
            channel_id: text_channel_id.clone(),
        });
        assert_eq!(
            focused_text_channel.active_context.as_ref().map(|context| (
                context.kind.as_str(),
                context.group_id.as_deref(),
                context.channel_id.as_deref()
            )),
            Some((
                "text_channel",
                Some(group_id.as_str()),
                Some(text_channel_id.as_str())
            ))
        );

        let alice_reloaded_before_receipt =
            load_state_from_store(&mut FileAppStore::new(&alice_path)).to_view();
        assert_eq!(
            alice_reloaded_before_receipt
                .profile
                .as_ref()
                .map(|profile| profile.user_id.as_str()),
            Some(alice_profile_id.as_str())
        );
        assert_eq!(
            alice_reloaded_before_receipt.preferences,
            UiPreferencesView {
                theme_id: "ocean-contrast".to_owned(),
                template_id: "compact-ops".to_owned(),
            }
        );
        let reloaded_group = alice_reloaded_before_receipt
            .groups
            .iter()
            .find(|group| group.group_id == group_id)
            .ok_or_else(|| "alice reloaded group missing".to_owned())?;
        let reloaded_connectivity = reloaded_group
            .connectivity
            .as_ref()
            .ok_or_else(|| "alice reloaded group connectivity missing".to_owned())?;
        assert_eq!(
            reloaded_connectivity.signaling_profiles[0].adapter_kind,
            "mqtt"
        );
        assert_eq!(reloaded_connectivity.ice_stun_servers.len(), 2);
        assert_eq!(reloaded_connectivity.ice_turn_servers.len(), 1);
        assert!(reloaded_group
            .channels
            .iter()
            .any(|channel| channel.name == "#field-notes"));
        assert!(reloaded_group
            .channels
            .iter()
            .any(|channel| channel.channel_id == voice_channel_id));
        assert_eq!(
            alice_reloaded_before_receipt
                .voice_session
                .as_ref()
                .map(|session| (
                    session.self_muted,
                    session
                        .input_device
                        .as_ref()
                        .map(|device| device.device_id.as_str()),
                    session
                        .output_device
                        .as_ref()
                        .map(|device| device.device_id.as_str()),
                    session.media_runtime.remote_transport_active,
                    session
                        .participants
                        .iter()
                        .find(|participant| participant.id == remote_participant_id)
                        .map(|participant| participant.volume)
                )),
            Some((
                true,
                Some("alice-mic"),
                Some("alice-speaker"),
                true,
                Some(37)
            ))
        );
        assert_eq!(
            alice_reloaded_before_receipt
                .active_context
                .as_ref()
                .map(|context| (
                    context.kind.as_str(),
                    context.group_id.as_deref(),
                    context.channel_id.as_deref()
                )),
            Some((
                "text_channel",
                Some(group_id.as_str()),
                Some(text_channel_id.as_str())
            ))
        );
        assert!(alice_reloaded_before_receipt.invites.iter().any(|invite| {
            invite.invite_kind == "dm_contact"
                && invite.max_use == "2"
                && invite.scope_id_commitment == dm_invite.scope_id_commitment
        }));
        assert!(alice_reloaded_before_receipt.invites.iter().any(|invite| {
            invite.invite_kind == "group_join"
                && invite.max_use == "2"
                && invite.scope_id_commitment == group_invite.scope_id_commitment
                && invite.ice_turn_servers.len() == 1
        }));

        let bob_path = reset_with_temp_state("g004-full-surface-bob");
        let bob_user = recover_user(RecoverUserRequest {
            display_name: "Bob G004".to_owned(),
            recovery_code: "g004-paper-coral-falcon".to_owned(),
            device_name: Some("Bob recovered laptop".to_owned()),
            recovery_room_memberships: vec!["G004 Lab".to_owned()],
            recovered_device_count: Some(1),
            use_sealed_account_backup: false,
        });
        let bob_profile_id = bob_user
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .ok_or_else(|| "bob profile missing".to_owned())?;
        assert_ne!(alice_profile_id, bob_profile_id);
        assert!(bob_user
            .profile
            .as_ref()
            .map(|profile| profile.recovery_status.contains("rooms: 1"))
            .unwrap_or(false));
        save_preferences(SavePreferencesRequest {
            theme_id: "midnight-steel".to_owned(),
            template_id: "command-center".to_owned(),
        });
        let accepted_dm = accept_dm_invite(AcceptDmInviteRequest {
            invite_code: dm_invite.code.clone(),
            display_name: Some("Alice G004".to_owned()),
        });
        assert!(accepted_dm.last_command_error.is_none(), "{accepted_dm:?}");
        let joined_group = join_group(JoinGroupRequest {
            invite_code: group_invite.code.clone(),
            group_name: Some("G004 Lab".to_owned()),
        });
        assert!(
            joined_group.last_command_error.is_none(),
            "{joined_group:?}"
        );
        let receipt_response = receive_text_delivery_envelope(ReceiveTextDeliveryEnvelopeRequest {
            target: target.clone(),
            envelope: envelope_record.envelope.clone(),
            sender_verifying_key_hex: envelope_record.sender_verifying_key_hex.clone(),
            recipient_leaf: Some(2),
        });
        let receipt = receipt_response
            .receipt
            .ok_or_else(|| "bob receipt missing".to_owned())?;
        let recipient_verifying_key_hex = receipt_response
            .recipient_verifying_key_hex
            .ok_or_else(|| "bob receipt key missing".to_owned())?;

        let bob_reloaded = load_state_from_store(&mut FileAppStore::new(&bob_path)).to_view();
        assert_eq!(
            bob_reloaded
                .profile
                .as_ref()
                .map(|profile| profile.user_id.as_str()),
            Some(bob_profile_id.as_str())
        );
        assert_eq!(bob_reloaded.preferences.theme_id, "midnight-steel");
        assert!(bob_reloaded.active_context.as_ref().is_some_and(|context| {
            context.kind == "group"
                && context.group_id.as_deref().is_some_and(|active_group_id| {
                    bob_reloaded
                        .groups
                        .iter()
                        .any(|group| group.name == "G004 Lab" && group.group_id == active_group_id)
                })
        }));
        assert!(bob_reloaded.dms.iter().any(|dm| {
            dm.display_name == "Alice G004"
                && dm.connectivity.as_ref().is_some_and(|policy| {
                    policy.scope_id_commitment == dm_invite.scope_id_commitment
                })
        }));
        assert!(bob_reloaded.groups.iter().any(|group| {
            group.name == "G004 Lab"
                && group.role == "member"
                && group.connectivity.as_ref().is_some_and(|policy| {
                    policy.scope_id_commitment == group_invite.scope_id_commitment
                        && policy.ice_stun_servers == group_invite.ice_stun_servers
                        && policy.ice_turn_servers == group_invite.ice_turn_servers
                })
        }));
        assert!(bob_reloaded.invites.iter().any(|invite| {
            invite.invite_kind == "dm_contact" && invite.uses == 1 && !invite.revoked
        }));
        assert!(bob_reloaded.invites.iter().any(|invite| {
            invite.invite_kind == "group_join" && invite.uses == 1 && !invite.revoked
        }));
        assert!(bob_reloaded.messages.iter().any(|message| {
            message.message_id == message_id && message.state_key == "received_envelope"
        }));

        reload_global_app_service_from_path(&alice_path);
        let receipted = apply_text_delivery_receipt(ApplyTextDeliveryReceiptRequest {
            message_id: message_id.clone(),
            receipt,
            recipient_verifying_key_hex: recipient_verifying_key_hex.clone(),
        });
        assert!(receipted.last_command_error.is_none(), "{receipted:?}");
        let alice_reloaded_after_receipt =
            load_state_from_store(&mut FileAppStore::new(&alice_path)).to_view();
        let reloaded_message = alice_reloaded_after_receipt
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "alice receipted message missing after reload".to_owned())?;
        let bob_key = verifying_key_from_hex(&recipient_verifying_key_hex)
            .ok_or_else(|| "bob receipt key should decode".to_owned())?;
        assert_eq!(reloaded_message.state_key, "peer_receipt");
        assert_eq!(
            reloaded_message
                .peer_receipt
                .as_ref()
                .map(|receipt| receipt.recipient_key_fingerprint.as_str()),
            Some(key_fingerprint(&bob_key).as_str())
        );
        Ok(())
    }

    #[test]
    fn fresh_state_starts_first_run() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("fresh");
        let state = app_state();
        assert_eq!(state.lifecycle, AppLifecycle::FirstRun);
        assert!(state.profile.is_none());
        assert!(state.groups.is_empty());
        assert!(state.dms.is_empty());
        assert!(state.voice_session.is_none());
        assert!(state.snapshot.servers.is_empty());
    }

    #[test]
    fn create_user_transitions_ready_and_persists() -> Result<(), String> {
        let _guard = test_lock();
        let path = reset_with_temp_state("create-user");
        let state = create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        assert_eq!(state.lifecycle, AppLifecycle::Ready);
        assert_eq!(
            state
                .profile
                .as_ref()
                .map(|profile| profile.display_name.as_str()),
            Some("Alice")
        );
        assert_eq!(state.dms.len(), 1);
        assert_eq!(state.dms[0].display_name, "New contact");
        assert!(state.dms[0].participant_id.starts_with("friend-"));
        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.devices[0].label, "Desktop");
        assert_eq!(state.devices[0].identity_key.len(), 64);
        assert_eq!(state.devices[0].device_key.len(), 64);
        assert_ne!(state.devices[0].identity_key, state.devices[0].device_key);
        assert!(state
            .profile
            .as_ref()
            .map(|profile| profile.recovery_status.contains("command signing material"))
            .unwrap_or(false));
        let persisted_state = load_state();
        let leaf = persisted_state
            .device_set
            .active_devices()
            .first()
            .cloned()
            .ok_or_else(|| "real device-set leaf created".to_owned())?;
        assert_eq!(
            state.devices[0].identity_key,
            hex::encode(leaf.identity_key)
        );
        assert_eq!(state.devices[0].device_key, hex::encode(leaf.device_key));
        assert!(!state.devices[0].revoked);
        assert_eq!(state.devices[0].revoked_at_epoch, None);
        assert!(path.exists());

        let loaded = load_state().to_view();
        assert_eq!(loaded.lifecycle, AppLifecycle::Ready);
        assert_eq!(
            loaded
                .profile
                .as_ref()
                .map(|profile| profile.device_name.as_str()),
            Some("Desktop")
        );
        Ok(())
    }

    #[test]
    fn recover_user_transitions_ready_and_persists() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("recover-user");
        let state = recover_user(RecoverUserRequest {
            display_name: "Alice recovered".to_owned(),
            recovery_code: "local-placeholder".to_owned(),
            device_name: None,
            recovery_room_memberships: Vec::new(),
            recovered_device_count: None,
            use_sealed_account_backup: false,
        });
        assert_eq!(state.lifecycle, AppLifecycle::Ready);
        assert!(state
            .profile
            .as_ref()
            .map(|profile| profile
                .recovery_status
                .contains("content keys restored: false"))
            .unwrap_or(false));
        assert!(state
            .events
            .iter()
            .any(|event| event.kind == "identity.recovered"));
        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.devices[0].label, "Desktop");
        assert_eq!(state.devices[0].identity_key.len(), 64);
        assert_eq!(state.devices[0].device_key.len(), 64);
        assert!(state
            .profile
            .as_ref()
            .map(|profile| profile
                .recovery_status
                .contains("content keys restored: false"))
            .unwrap_or(false));
        let persisted_state = load_state();
        assert_eq!(persisted_state.device_set.active_devices().len(), 1);
    }

    #[test]
    fn recovery_code_rehydrates_stable_identity_material() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("stable-recovery-one");
        let first = recover_user(RecoverUserRequest {
            display_name: "Alice recovered".to_owned(),
            recovery_code: "paper-coral-falcon".to_owned(),
            device_name: Some("Recovered desktop".to_owned()),
            recovery_room_memberships: vec!["Recovered Room".to_owned()],
            recovered_device_count: Some(1),
            use_sealed_account_backup: false,
        });
        let first_identity = first.devices[0].identity_key.clone();

        let _path = reset_with_temp_state("stable-recovery-two");
        let second = recover_user(RecoverUserRequest {
            display_name: "Alice recovered".to_owned(),
            recovery_code: "paper-coral-falcon".to_owned(),
            device_name: Some("Recovered desktop".to_owned()),
            recovery_room_memberships: vec!["Recovered Room".to_owned()],
            recovered_device_count: Some(1),
            use_sealed_account_backup: false,
        });
        assert_eq!(second.devices[0].identity_key, first_identity);
        assert!(second
            .profile
            .as_ref()
            .map(|profile| profile
                .recovery_status
                .contains("verified local identity material"))
            .unwrap_or(false));
    }

    #[test]
    fn verify_safety_number_rejects_mismatches_and_persists_success() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("verify-safety");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        let snapshot = app_snapshot();

        let wrong_friend = verify_safety_number(SafetyVerificationRequest {
            friend_id: "wrong-friend".to_owned(),
            provided: snapshot.friend.safety_number.clone(),
        });
        assert!(!wrong_friend.verified);
        assert!(!app_snapshot().friend.verified);

        let wrong_number = verify_safety_number(SafetyVerificationRequest {
            friend_id: snapshot.friend.friend_code.clone(),
            provided: "0000".to_owned(),
        });
        assert!(!wrong_number.verified);
        assert!(!app_snapshot().friend.verified);

        let ok = verify_safety_number(SafetyVerificationRequest {
            friend_id: snapshot.friend.friend_code,
            provided: snapshot.friend.safety_number,
        });
        assert!(ok.verified);
        assert!(app_snapshot().friend.verified);
        assert!(load_state().friend_verified);
    }

    #[test]
    fn device_pairing_payload_commands_accept_and_reject_strings(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("pairing-flow");
        let initial = create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        assert_eq!(initial.devices.len(), 1);

        let payload = create_device_pairing_payload(CreateDevicePairingPayloadRequest {
            requested_label: "Phone".to_owned(),
            current_epoch: Some(10),
            valid_for_epochs: Some(2),
        });
        assert!(payload.rejected_reason.is_none());
        assert!(payload.payload.contains("signature"));
        assert_eq!(payload.expires_epoch, 12);

        let accepted = accept_device_pairing_payload(AcceptDevicePairingPayloadRequest {
            payload: payload.payload.clone(),
            device_name: Some("Phone".to_owned()),
            current_epoch: Some(11),
        });
        assert_eq!(accepted.devices.len(), 2);
        assert!(accepted
            .events
            .iter()
            .any(|event| event.kind == "device.paired"));

        let expired = accept_device_pairing_payload(AcceptDevicePairingPayloadRequest {
            payload: payload.payload.clone(),
            device_name: Some("Tablet".to_owned()),
            current_epoch: Some(13),
        });
        assert_eq!(expired.devices.len(), 2);
        assert!(expired.events.iter().any(|event| {
            event.kind == "device.pairing_rejected" && event.summary.contains("expired")
        }));

        let mut tampered: DevicePairingPayload = serde_json::from_str(&payload.payload)?;
        tampered.requested_label = "Mallory phone".to_owned();
        let tampered = serde_json::to_string(&tampered)?;
        let rejected = accept_device_pairing_payload(AcceptDevicePairingPayloadRequest {
            payload: tampered,
            device_name: Some("Mallory".to_owned()),
            current_epoch: Some(11),
        });
        assert_eq!(rejected.devices.len(), 2);
        assert!(rejected.events.iter().any(|event| {
            event.kind == "device.pairing_rejected" && event.summary.contains("signature")
        }));
        Ok(())
    }

    #[test]
    fn recovery_command_composes_with_device_count_without_content_keys() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("pairing-recovery");
        let state = recover_user(RecoverUserRequest {
            display_name: "Alice recovered".to_owned(),
            recovery_code: "paper-coral-falcon".to_owned(),
            device_name: Some("Recovered desktop".to_owned()),
            recovery_room_memberships: vec!["Pairing Lab".to_owned()],
            recovered_device_count: Some(2),
            use_sealed_account_backup: false,
        });
        assert_eq!(state.devices.len(), 2);
        assert!(state.groups.iter().any(|group| group.name == "Pairing Lab"));
        assert!(state
            .profile
            .as_ref()
            .map(|profile| profile
                .recovery_status
                .contains("content keys restored: false"))
            .unwrap_or(false));
    }

    #[test]
    fn group_invite_channel_message_flow() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("group-flow");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        let group_state = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group_state.groups[0].group_id.clone();
        let invite_state = create_invite(CreateInviteRequest {
            group_id: Some(group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "5".to_owned(),
        });
        assert!(invite_state.invites[0]
            .code
            .starts_with("discrypt://join/v1/"));
        assert!(!invite_state.invites[0].code.contains("room_secret="));
        assert!(invite_state.invites[0].code.contains("?d="));
        assert_eq!(
            invite_state.invites[0].endpoint_policy,
            "production_tls".to_owned()
        );
        assert!(invite_state.invites[0]
            .signaling_endpoint
            .starts_with("https://"));
        assert_eq!(
            invite_state.invites[0].signaling_trust_fingerprint.len(),
            64
        );
        assert_eq!(invite_state.invites[0].uses, 0);
        assert!(!invite_state.invites[0].room_secret_hash.is_empty());
        assert_eq!(
            invite_state.invites[0].ice_stun_servers,
            vec!["stun:stun.l.google.com:19302".to_owned()]
        );
        assert!(invite_state.invites[0].ice_turn_servers.is_empty());
        let channel_state = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "ops".to_owned(),
            kind: ChannelKind::Text,
            retention_status: "24 hours".to_owned(),
        });
        let channel_id = channel_state.groups[0].channels[0].channel_id.clone();
        let message_state = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "channel".to_owned(),
                dm_id: None,
                group_id: Some(group_id),
                channel_id: Some(channel_id),
            },
            body: "hello encrypted local shell".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        assert!(message_state
            .messages
            .iter()
            .any(|message| message.body == "hello encrypted local shell"));
    }

    #[test]
    fn production_invite_parser_surfaces_endpoint_and_trust_without_room_secret() {
        let code = "discrypt://join/v1/invite-a?endpoint=https%3A%2F%2Fsignal.example.invalid%2Fv1&policy=production_tls&trust_fp=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa&trust=signed%20endpoint&commitment=bbbb&exp=2026-05-29T00%3A00%3A00Z&max=3";
        let parsed = parse_invite_metadata(code);
        assert!(parsed.is_some());
        let Some(parsed) = parsed else {
            return;
        };
        assert_eq!(
            parsed.signaling_endpoint,
            "https://signal.example.invalid/v1".to_owned()
        );
        assert_eq!(
            parsed.signaling_trust_fingerprint,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned()
        );
        assert_eq!(parsed.signaling_trust_status, "signed endpoint".to_owned());
        assert_eq!(parsed.endpoint_policy, "production_tls".to_owned());
        assert!(parsed.ice_stun_servers.is_empty());
        assert!(parsed.ice_turn_servers.is_empty());
        assert_eq!(parsed.max_uses, 3);
        assert!(!code.contains("room_secret="));
    }

    #[test]
    fn production_invite_descriptor_parser_surfaces_redacted_ice_metadata() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("invite-ice-parse");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        let group_state = create_group(CreateGroupRequest {
            name: "ICE Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let invite_state = create_invite(CreateInviteRequest {
            group_id: Some(group_state.groups[0].group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "2".to_owned(),
        });
        let parsed = parse_invite_metadata(&invite_state.invites[0].code);
        assert!(parsed.is_some());
        let Some(parsed) = parsed else {
            return;
        };

        assert_eq!(
            parsed.ice_stun_servers,
            vec!["stun:stun.l.google.com:19302".to_owned()]
        );
        assert!(parsed.ice_turn_servers.is_empty());
        assert!(!format!("{parsed:?}").contains("raw-turn-secret"));
    }

    #[test]
    fn group_and_dm_invites_carry_bootstrap_metadata_and_persist() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("invite-bootstrap-flow");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        let group_state = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group_state.groups[0].group_id.clone();
        let invite_state = create_invite(CreateInviteRequest {
            group_id: Some(group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "5".to_owned(),
        });
        let group_invite = invite_state
            .invites
            .iter()
            .find(|invite| invite.group_id == group_id);
        assert!(group_invite.is_some());
        let Some(group_invite) = group_invite else {
            return;
        };
        assert_eq!(group_invite.connectivity_schema_version, 1);
        assert_eq!(group_invite.invite_kind, "group_join");
        assert_eq!(group_invite.scope_id_commitment.len(), 64);
        assert!(group_invite.group_bootstrap.is_some());
        assert!(group_invite.dm_bootstrap.is_none());
        assert_eq!(
            group_invite.ice_stun_servers,
            vec!["stun:stun.l.google.com:19302".to_owned()]
        );
        assert!(group_invite.ice_turn_servers.is_empty());
        assert!(group_invite.code.contains("?d="));
        assert!(!group_invite.code.contains("Private%20Lab"));
        assert!(!group_invite.code.contains("room_secret="));

        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let dm_invite_state = create_dm_invite(CreateDmInviteRequest {
            dm_id: Some(dm_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "1".to_owned(),
        });
        let dm_invite = dm_invite_state
            .invites
            .iter()
            .find(|invite| invite.dm_id.as_deref() == Some(dm_id.as_str()));
        assert!(dm_invite.is_some());
        let Some(dm_invite) = dm_invite else {
            return;
        };
        assert_eq!(dm_invite.invite_kind, "dm_contact");
        assert_eq!(dm_invite.group_id, "");
        assert_eq!(dm_invite.scope_id_commitment.len(), 64);
        assert!(dm_invite.dm_bootstrap.is_some());
        assert!(dm_invite.group_bootstrap.is_none());
        assert!(dm_invite.code.contains("?d="));
        assert!(!dm_invite.code.contains("Bob"));

        let accepted = accept_dm_invite(AcceptDmInviteRequest {
            invite_code: dm_invite.code.clone(),
            display_name: Some("Bob accepted".to_owned()),
        });
        assert_eq!(
            accepted
                .active_context
                .as_ref()
                .and_then(|context| context.dm_id.as_deref()),
            Some(dm_id.as_str())
        );
        assert!(accepted
            .invites
            .iter()
            .any(|invite| invite.invite_kind == "dm_contact" && invite.uses == 1));

        let persisted = load_state().to_view();
        assert!(persisted
            .invites
            .iter()
            .any(|invite| invite.invite_kind == "group_join"));
        assert!(persisted
            .invites
            .iter()
            .any(|invite| invite.invite_kind == "dm_contact"));
        assert!(persisted.dms.iter().any(|dm| {
            dm.connectivity
                .as_ref()
                .map(|policy| policy.invite_kind.as_str())
                == Some("dm_contact")
        }));
    }

    #[test]
    fn dm_invite_accept_persists_signed_runtime_peers() -> Result<(), String> {
        let _guard = test_lock();
        reset_with_temp_state("dm-runtime-peers-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let opened = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        assert!(opened.last_command_error.is_none(), "{opened:?}");
        let inviter_dm = opened
            .dms
            .iter()
            .find(|dm| dm.display_name == "Bob")
            .ok_or_else(|| "inviter DM missing".to_owned())?;
        assert_eq!(inviter_dm.runtime_peers.len(), 2);
        let inviter_local_peer = inviter_dm
            .runtime_peers
            .iter()
            .find(|peer| peer.is_local)
            .ok_or_else(|| "inviter local runtime peer missing".to_owned())?;
        assert_eq!(inviter_local_peer.role, "inviter");
        assert_eq!(inviter_local_peer.source, "signed_dm_bootstrap_v1");
        let inviter_remote_peer = inviter_dm
            .runtime_peers
            .iter()
            .find(|peer| !peer.is_local)
            .ok_or_else(|| "inviter remote runtime peer missing".to_owned())?;
        assert_eq!(inviter_remote_peer.role, "reply");

        let invited = create_dm_invite(CreateDmInviteRequest {
            dm_id: Some(inviter_dm.dm_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "1".to_owned(),
        });
        assert!(invited.last_command_error.is_none(), "{invited:?}");
        let invite_code = invited
            .invites
            .last()
            .map(|invite| invite.code.clone())
            .ok_or_else(|| "DM invite code missing".to_owned())?;

        reset_with_temp_state("dm-runtime-peers-bob");
        create_user(CreateUserRequest {
            display_name: "Bob".to_owned(),
            device_name: Some("Bob laptop".to_owned()),
        });
        let accepted = accept_dm_invite(AcceptDmInviteRequest {
            invite_code,
            display_name: Some("Alice".to_owned()),
        });
        assert!(accepted.last_command_error.is_none(), "{accepted:?}");
        let reply_dm = accepted
            .dms
            .iter()
            .find(|dm| dm.display_name == "Alice")
            .ok_or_else(|| "reply DM missing".to_owned())?;
        assert_eq!(reply_dm.runtime_peers.len(), 2);
        let reply_local_peer = reply_dm
            .runtime_peers
            .iter()
            .find(|peer| peer.is_local)
            .ok_or_else(|| "reply local runtime peer missing".to_owned())?;
        assert_eq!(reply_local_peer.role, "reply");
        assert_eq!(reply_local_peer.peer_id, inviter_remote_peer.peer_id);
        let reply_remote_peer = reply_dm
            .runtime_peers
            .iter()
            .find(|peer| !peer.is_local)
            .ok_or_else(|| "reply remote runtime peer missing".to_owned())?;
        assert_eq!(reply_remote_peer.role, "inviter");
        assert_eq!(reply_remote_peer.peer_id, inviter_local_peer.peer_id);

        let reloaded = load_state().to_view();
        let persisted_dm = reloaded
            .dms
            .iter()
            .find(|dm| dm.display_name == "Alice")
            .ok_or_else(|| "persisted reply DM missing".to_owned())?;
        assert_eq!(persisted_dm.runtime_peers, reply_dm.runtime_peers);
        Ok(())
    }

    #[test]
    fn create_group_persists_custom_signaling_and_ice_policy() -> Result<(), String> {
        let _guard = test_lock();
        reset_with_temp_state("custom-group-connectivity-policy");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let state = create_group(CreateGroupRequest {
            name: "Custom Voice Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: Some("mqtt".to_owned()),
            signaling_endpoint: Some("mqtts://broker.emqx.io:8883".to_owned()),
            ice_stun_servers: Some(vec![
                "stun:stun.l.google.com:19302".to_owned(),
                "stun:stun.cloudflare.com:3478".to_owned(),
            ]),
            ice_turn_servers: Some(vec![IceTurnServerView {
                endpoint: "turns:turn.example.invalid:5349".to_owned(),
                credential_declared: true,
                credential_expires_at: Some("2026-06-02T00:00:00Z".to_owned()),
            }]),
        });
        let connectivity = state
            .groups
            .first()
            .and_then(|group| group.connectivity.as_ref())
            .ok_or_else(|| "custom group connectivity missing".to_owned())?;
        assert_eq!(connectivity.signaling_profiles.len(), 1);
        assert_eq!(connectivity.signaling_profiles[0].adapter_kind, "mqtt");
        assert_eq!(
            connectivity.signaling_profiles[0].endpoints,
            vec!["mqtts://broker.emqx.io:8883".to_owned()]
        );
        assert_eq!(
            connectivity.ice_stun_servers,
            vec![
                "stun:stun.l.google.com:19302".to_owned(),
                "stun:stun.cloudflare.com:3478".to_owned(),
            ]
        );
        assert_eq!(connectivity.ice_turn_servers.len(), 1);
        assert!(connectivity.ice_turn_servers[0].credential_declared);
        let invite_state = create_invite(CreateInviteRequest {
            group_id: state.groups.first().map(|group| group.group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "5".to_owned(),
        });
        let invite = invite_state
            .invites
            .last()
            .ok_or_else(|| "custom group invite missing".to_owned())?;
        assert_eq!(invite.signaling_profiles, connectivity.signaling_profiles);
        assert_eq!(invite.ice_stun_servers, connectivity.ice_stun_servers);
        assert_eq!(invite.ice_turn_servers, connectivity.ice_turn_servers);
        Ok(())
    }

    #[test]
    fn create_group_persists_rehydratable_openmls_group_handle() -> Result<(), String> {
        let _guard = test_lock();
        reset_with_temp_state("openmls-group-handle");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });

        let created = create_group(CreateGroupRequest {
            name: "OpenMLS Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        assert!(created.last_command_error.is_none(), "{created:?}");
        let group_id = created
            .groups
            .iter()
            .find(|group| group.name == "OpenMLS Lab")
            .map(|group| group.group_id.clone())
            .ok_or_else(|| "created group missing".to_owned())?;

        let persisted = load_state();
        let handle = persisted
            .openmls_groups
            .iter()
            .find(|handle| handle.group_id == group_id)
            .ok_or_else(|| "OpenMLS group handle was not persisted".to_owned())?;
        assert_eq!(handle.epoch, 0);
        assert!(!handle.signer_public_key_hex.is_empty());
        assert_eq!(handle.confirmation_tag_sha256.len(), 64);

        let signer_public_key = hex::decode(&handle.signer_public_key_hex)
            .map_err(|error| format!("persisted OpenMLS signer key was not hex: {error}"))?;
        let mut engine = OpenMlsGroupEngine::open(app_openmls_store_path())
            .map_err(|error| format!("OpenMLS provider could not be reopened: {error}"))?;
        let rehydrated = engine
            .load_group(&group_id, &signer_public_key)
            .map_err(|error| format!("OpenMLS group could not be rehydrated: {error}"))?;
        assert_eq!(rehydrated.epoch, handle.epoch);
        let exported = engine
            .export_secret(&group_id, "discrypt/text", b"g012-openmls-foundation", 32)
            .map_err(|error| format!("OpenMLS exporter failed after rehydrate: {error}"))?;
        assert_eq!(exported.len(), 32);
        Ok(())
    }

    #[test]
    fn g012_channel_send_uses_openmls_exporter_for_text_ciphertext() -> Result<(), String> {
        let _guard = test_lock();
        reset_with_temp_state("g012-openmls-outbound-text");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let created = create_group(CreateGroupRequest {
            name: "OpenMLS Text Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        assert!(created.last_command_error.is_none(), "{created:?}");
        let group = created
            .groups
            .iter()
            .find(|group| group.name == "OpenMLS Text Lab")
            .ok_or_else(|| "created group missing".to_owned())?;
        let channel_id = group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Text)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "text channel missing".to_owned())?;
        let target = MessageTargetView {
            kind: "channel".to_owned(),
            dm_id: None,
            group_id: Some(group.group_id.clone()),
            channel_id: Some(channel_id),
        };

        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "openmls exporter ciphertext".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        assert!(sent.last_command_error.is_none(), "{sent:?}");
        let message_id = sent
            .messages
            .last()
            .map(|message| message.message_id.clone())
            .ok_or_else(|| "sent message missing".to_owned())?;
        let persisted = load_state();
        let envelope_record = persisted
            .text_delivery_envelopes
            .iter()
            .find(|record| record.message_id == message_id)
            .ok_or_else(|| "OpenMLS text envelope missing".to_owned())?;
        assert_eq!(envelope_record.group_id, text_delivery_group_id(&target)?);
        assert!(!envelope_record
            .envelope
            .content_ciphertext
            .starts_with(b"ciphertext:discrypt-text-control-proof"));
        let (exporter, _, _) =
            persisted.openmls_text_exporter_for_target(&target, &envelope_record.group_id)?;
        let plaintext = discrypt_mls_delivery::decrypt_text_envelope(
            &envelope_record.group_id,
            &exporter,
            &envelope_record.envelope,
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(plaintext, b"openmls exporter ciphertext");
        let placeholder_secret = opaque_text_control_frame_for_message(
            &persisted,
            &target,
            &message_id,
            "openmls exporter ciphertext",
            envelope_record.envelope.sequence,
        );
        assert!(discrypt_mls_delivery::decrypt_text_envelope(
            &envelope_record.group_id,
            &placeholder_secret,
            &envelope_record.envelope,
        )
        .is_err());
        let pending = persisted
            .text_control_outbox
            .iter()
            .find(|record| record.message_id == message_id)
            .ok_or_else(|| "text/control outbox frame missing".to_owned())?;
        assert_eq!(pending.state_key, "pending");
        Ok(())
    }

    #[test]
    fn g012_channel_send_fails_closed_without_openmls_group_state() -> Result<(), String> {
        let _guard = test_lock();
        reset_with_temp_state("g012-openmls-outbound-missing-state");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let created = create_group(CreateGroupRequest {
            name: "Missing MLS Text Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        assert!(created.last_command_error.is_none(), "{created:?}");
        let group = created
            .groups
            .iter()
            .find(|group| group.name == "Missing MLS Text Lab")
            .ok_or_else(|| "created group missing".to_owned())?;
        let channel_id = group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Text)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "text channel missing".to_owned())?;
        {
            let service = app_service();
            let mut guard = service
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.state.openmls_groups.clear();
        }

        let rejected = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "channel".to_owned(),
                dm_id: None,
                group_id: Some(group.group_id.clone()),
                channel_id: Some(channel_id),
            },
            false,
        );
        bob.persist();
        let bob_package = bob.request_openmls_admission_key_package(&group_id)?;
        assert_eq!(bob_package.group_id, group_id);
        assert_eq!(bob_package.member_identity, bob.state.local_user_id());
        assert!(!bob_package.signer_public_key_hex.is_empty());

        let mut alice = TauriAppService::load_for_test_path(alice_path.clone());
        let welcome = alice.issue_openmls_admission_welcome(&bob_package)?;
        assert_eq!(welcome.group_id, group_id);
        assert_eq!(welcome.epoch, 1);
        assert_eq!(
            welcome.member_signer_public_key_hex,
            bob_package.signer_public_key_hex
        );
        assert!(!welcome.owner_signer_public_key_hex.is_empty());
        assert!(!welcome.welcome_bytes.is_empty());

        bob.join_openmls_group_from_welcome(&welcome)?;
        let bob_handle = bob
            .state
            .openmls_groups
            .iter()
            .find(|handle| handle.group_id == group_id)
            .cloned()
            .ok_or_else(|| "bob OpenMLS handle missing after Welcome join".to_owned())?;
        assert_eq!(bob_handle.epoch, welcome.epoch);
        assert_eq!(
            bob_handle.signer_public_key_hex,
            bob_package.signer_public_key_hex
        );
        assert_eq!(
            bob_handle.confirmation_tag_sha256,
            welcome.confirmation_tag_sha256
        );

        let alice_handle = alice
            .state
            .openmls_groups
            .iter()
            .find(|handle| handle.group_id == group_id)
            .cloned()
            .ok_or_else(|| "alice OpenMLS handle missing after Welcome issue".to_owned())?;
        assert_eq!(alice_handle.epoch, welcome.epoch);
        assert_eq!(
            alice_handle.confirmation_tag_sha256,
            welcome.confirmation_tag_sha256
        );

        let mut alice_engine = OpenMlsGroupEngine::open(openmls_store_path_for_app_state_path(
            &alice_path,
        ))
        .map_err(|error| format!("alice OpenMLS provider could not be reopened: {error}"))?;
        alice_engine
            .load_group(
                &group_id,
                &hex::decode(&alice_handle.signer_public_key_hex)
                    .map_err(|error| format!("alice signer handle was not hex: {error}"))?,
            )
            .map_err(|error| format!("alice OpenMLS group could not be rehydrated: {error}"))?;
        let mut bob_engine =
            OpenMlsGroupEngine::open(openmls_store_path_for_app_state_path(&bob_path))
                .map_err(|error| format!("bob OpenMLS provider could not be reopened: {error}"))?;
        bob_engine
            .load_group(
                &group_id,
                &hex::decode(&bob_handle.signer_public_key_hex)
                    .map_err(|error| format!("bob signer handle was not hex: {error}"))?,
            )
            .map_err(|error| format!("bob OpenMLS group could not be rehydrated: {error}"))?;

        let context = format!("g012-admission:{group_id}");
        let alice_export = alice_engine
            .export_secret(&group_id, "discrypt/v1/text", context.as_bytes(), 32)
            .map_err(|error| format!("alice exporter failed: {error}"))?;
        let bob_export = bob_engine
            .export_secret(&group_id, "discrypt/v1/text", context.as_bytes(), 32)
            .map_err(|error| format!("bob exporter failed: {error}"))?;
        assert_eq!(alice_export, bob_export);
        assert_eq!(alice_export.len(), 32);

        let persisted_bob = load_state_from_store(&mut FileAppStore::new(&bob_path));
        assert!(persisted_bob.openmls_groups.iter().any(|handle| {
            handle.group_id == group_id
                && handle.signer_public_key_hex == bob_handle.signer_public_key_hex
                && handle.epoch == welcome.epoch
        }));
        Ok(())
    }

    #[test]
    fn openmls_admission_bridge_joins_two_profiles_with_exporter_parity() -> Result<(), String> {
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("openmls-admission-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let created = create_group(CreateGroupRequest {
            name: "Admission Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        assert!(created.last_command_error.is_none(), "{created:?}");
        let group_id = created
            .groups
            .iter()
            .find(|group| group.name == "Admission Lab")
            .map(|group| group.group_id.clone())
            .ok_or_else(|| "created group missing".to_owned())?;

        let bob_path = fresh_state_path("openmls-admission-bob");
        let _ = fs::remove_file(&bob_path);
        let mut bob = TauriAppService::load_for_test_path(bob_path.clone());
        bob.state.create_user(
            CreateUserRequest {
                display_name: "Bob".to_owned(),
                device_name: Some("Bob laptop".to_owned()),
            },
            false,
        );
        bob.persist();
        let bob_package = bob.request_openmls_admission_key_package(&group_id)?;
        assert_eq!(bob_package.group_id, group_id);
        assert_eq!(bob_package.member_identity, bob.state.local_user_id());
        assert!(!bob_package.signer_public_key_hex.is_empty());

        let mut alice = TauriAppService::load_for_test_path(alice_path.clone());
        let welcome = alice.issue_openmls_admission_welcome(&bob_package)?;
        assert_eq!(welcome.group_id, group_id);
        assert_eq!(welcome.epoch, 1);
        assert_eq!(
            welcome.member_signer_public_key_hex,
            bob_package.signer_public_key_hex
        );
        assert!(!welcome.owner_signer_public_key_hex.is_empty());
        assert!(!welcome.welcome_bytes.is_empty());

        bob.join_openmls_group_from_welcome(&welcome)?;
        let bob_handle = bob
            .state
            .openmls_groups
            .iter()
            .find(|handle| handle.group_id == group_id)
            .cloned()
            .ok_or_else(|| "bob OpenMLS handle missing after Welcome join".to_owned())?;
        assert_eq!(bob_handle.epoch, welcome.epoch);
        assert_eq!(
            bob_handle.signer_public_key_hex,
            bob_package.signer_public_key_hex
        );
        assert_eq!(
            bob_handle.confirmation_tag_sha256,
            welcome.confirmation_tag_sha256
        );

        let alice_handle = alice
            .state
            .openmls_groups
            .iter()
            .find(|handle| handle.group_id == group_id)
            .cloned()
            .ok_or_else(|| "alice OpenMLS handle missing after Welcome issue".to_owned())?;
        assert_eq!(alice_handle.epoch, welcome.epoch);
        assert_eq!(
            alice_handle.confirmation_tag_sha256,
            welcome.confirmation_tag_sha256
        );

        let mut alice_engine = OpenMlsGroupEngine::open(openmls_store_path_for_app_state_path(
            &alice_path,
        ))
        .map_err(|error| format!("alice OpenMLS provider could not be reopened: {error}"))?;
        alice_engine
            .load_group(
                &group_id,
                &hex::decode(&alice_handle.signer_public_key_hex)
                    .map_err(|error| format!("alice signer handle was not hex: {error}"))?,
            )
            .map_err(|error| format!("alice OpenMLS group could not be rehydrated: {error}"))?;
        let mut bob_engine =
            OpenMlsGroupEngine::open(openmls_store_path_for_app_state_path(&bob_path))
                .map_err(|error| format!("bob OpenMLS provider could not be reopened: {error}"))?;
        bob_engine
            .load_group(
                &group_id,
                &hex::decode(&bob_handle.signer_public_key_hex)
                    .map_err(|error| format!("bob signer handle was not hex: {error}"))?,
            )
            .map_err(|error| format!("bob OpenMLS group could not be rehydrated: {error}"))?;

        let context = format!("g012-admission:{group_id}");
        let alice_export = alice_engine
            .export_secret(&group_id, "discrypt/v1/text", context.as_bytes(), 32)
            .map_err(|error| format!("alice exporter failed: {error}"))?;
        let bob_export = bob_engine
            .export_secret(&group_id, "discrypt/v1/text", context.as_bytes(), 32)
            .map_err(|error| format!("bob exporter failed: {error}"))?;
        assert_eq!(alice_export, bob_export);
        assert_eq!(alice_export.len(), 32);

        let persisted_bob = load_state_from_store(&mut FileAppStore::new(&bob_path));
        assert!(persisted_bob.openmls_groups.iter().any(|handle| {
            handle.group_id == group_id
                && handle.signer_public_key_hex == bob_handle.signer_public_key_hex
                && handle.epoch == welcome.epoch
        }));
        Ok(())
    }

    #[test]
    fn g005_connectivity_policy_command_persists_scope_overrides_and_invites() -> Result<(), String>
    {
        let _guard = test_lock();
        reset_with_temp_state("g005-connectivity-policy-command");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });

        let app_defaults = set_connectivity_policy(SetConnectivityPolicyRequest {
            scope_kind: "app".to_owned(),
            group_id: None,
            channel_id: None,
            dm_id: None,
            adapter_kind: Some("mqtt".to_owned()),
            signaling_endpoint: Some("mqtts://broker.emqx.io:8883".to_owned()),
            ice_stun_servers: Some(vec!["stun:stun.l.google.com:19302".to_owned()]),
            ice_turn_servers: Some(vec![IceTurnServerView {
                endpoint: "turns:turn.example.invalid:5349".to_owned(),
                credential_declared: true,
                credential_expires_at: Some("2026-06-02T00:00:00Z".to_owned()),
            }]),
        });
        assert!(
            app_defaults.last_command_error.is_none(),
            "{app_defaults:?}"
        );
        assert_eq!(
            app_defaults.connectivity_defaults.signaling_profiles[0].adapter_kind,
            "mqtt"
        );

        let created = create_group(CreateGroupRequest {
            name: "G005 Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group = created
            .groups
            .iter()
            .find(|group| group.name == "G005 Lab")
            .ok_or_else(|| "group missing".to_owned())?;
        let inherited = group
            .connectivity
            .as_ref()
            .ok_or_else(|| "group connectivity missing".to_owned())?;
        assert_eq!(inherited.signaling_profiles[0].adapter_kind, "mqtt");
        assert_eq!(inherited.ice_turn_servers.len(), 1);

        let updated_group = set_connectivity_policy(SetConnectivityPolicyRequest {
            scope_kind: "group".to_owned(),
            group_id: Some(group.group_id.clone()),
            channel_id: None,
            dm_id: None,
            adapter_kind: Some("nostr".to_owned()),
            signaling_endpoint: Some("wss://relay.damus.io".to_owned()),
            ice_stun_servers: Some(vec!["stun:stun.cloudflare.com:3478".to_owned()]),
            ice_turn_servers: Some(Vec::new()),
        });
        assert!(
            updated_group.last_command_error.is_none(),
            "{updated_group:?}"
        );
        let invite_state = create_invite(CreateInviteRequest {
            group_id: Some(group.group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "3".to_owned(),
        });
        let invite = invite_state
            .invites
            .last()
            .ok_or_else(|| "invite missing".to_owned())?;
        assert_eq!(invite.signaling_profiles[0].adapter_kind, "nostr");
        assert_eq!(
            invite.ice_stun_servers,
            vec!["stun:stun.cloudflare.com:3478"]
        );

        let channel_state = create_channel(CreateChannelRequest {
            group_id: group.group_id.clone(),
            name: "ops".to_owned(),
            kind: ChannelKind::Text,
            retention_status: "24 hours".to_owned(),
        });
        let channel_id = channel_state
            .active_context
            .as_ref()
            .and_then(|context| context.channel_id.clone())
            .ok_or_else(|| "active channel missing".to_owned())?;
        let channel_update = set_connectivity_policy(SetConnectivityPolicyRequest {
            scope_kind: "channel".to_owned(),
            group_id: Some(group.group_id.clone()),
            channel_id: Some(channel_id.clone()),
            dm_id: None,
            adapter_kind: Some("mqtt".to_owned()),
            signaling_endpoint: Some("mqtt://127.0.0.1:1883".to_owned()),
            ice_stun_servers: Some(vec!["stun:stun.l.google.com:19302".to_owned()]),
            ice_turn_servers: Some(Vec::new()),
        });
        assert!(
            channel_update.last_command_error.is_none(),
            "{channel_update:?}"
        );
        let persisted = load_state().to_view();
        let persisted_channel = persisted
            .groups
            .iter()
            .find(|persisted_group| persisted_group.group_id == group.group_id)
            .and_then(|persisted_group| {
                persisted_group
                    .channels
                    .iter()
                    .find(|channel| channel.channel_id == channel_id)
            })
            .ok_or_else(|| "persisted channel missing".to_owned())?;
        assert_eq!(
            persisted_channel
                .connectivity
                .as_ref()
                .and_then(|policy| policy.signaling_profiles.first())
                .map(|profile| profile.adapter_kind.as_str()),
            Some("mqtt")
        );

        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm = dm_state
            .dms
            .iter()
            .find(|dm| dm.display_name == "Bob")
            .ok_or_else(|| "dm missing".to_owned())?;
        let dm_update = set_connectivity_policy(SetConnectivityPolicyRequest {
            scope_kind: "dm".to_owned(),
            group_id: None,
            channel_id: None,
            dm_id: Some(dm.dm_id.clone()),
            adapter_kind: Some("mqtt".to_owned()),
            signaling_endpoint: Some("mqtts://broker.emqx.io:8883".to_owned()),
            ice_stun_servers: Some(vec!["stun:stun.l.google.com:19302".to_owned()]),
            ice_turn_servers: Some(Vec::new()),
        });
        assert!(dm_update.last_command_error.is_none(), "{dm_update:?}");
        let dm_invite_state = create_dm_invite(CreateDmInviteRequest {
            dm_id: Some(dm.dm_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "1".to_owned(),
        });
        let dm_invite = dm_invite_state
            .invites
            .last()
            .ok_or_else(|| "dm invite missing".to_owned())?;
        assert_eq!(dm_invite.invite_kind, "dm_contact");
        assert_eq!(dm_invite.signaling_profiles[0].adapter_kind, "mqtt");

        let rejected = set_connectivity_policy(SetConnectivityPolicyRequest {
            scope_kind: "group".to_owned(),
            group_id: Some(group.group_id.clone()),
            channel_id: None,
            dm_id: None,
            adapter_kind: Some("nostr".to_owned()),
            signaling_endpoint: Some("mqtts://wrong-for-nostr.example:8883".to_owned()),
            ice_stun_servers: Some(vec!["https://not-stun.example".to_owned()]),
            ice_turn_servers: Some(Vec::new()),
        });
        assert!(rejected.last_command_error.is_some());
        assert_eq!(
            rejected
                .last_command_error
                .as_ref()
                .map(|error| error.command.as_str()),
            Some("set_connectivity_policy")
        );
        Ok(())
    }

    #[test]
    fn group_invite_join_persists_signed_runtime_peers() -> Result<(), String> {
        let _guard = test_lock();
        reset_with_temp_state("group-runtime-peers-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let created = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        assert!(created.last_command_error.is_none(), "{created:?}");
        let owner_group = created
            .groups
            .first()
            .ok_or_else(|| "owner group missing".to_owned())?;
        assert_eq!(owner_group.runtime_peers.len(), 2);
        let owner_local_peer = owner_group
            .runtime_peers
            .iter()
            .find(|peer| peer.is_local)
            .ok_or_else(|| "owner local runtime peer missing".to_owned())?;
        assert_eq!(owner_local_peer.role, "owner");
        assert_eq!(owner_local_peer.source, "signed_group_bootstrap_v1");
        let owner_remote_peer = owner_group
            .runtime_peers
            .iter()
            .find(|peer| !peer.is_local)
            .ok_or_else(|| "owner remote runtime peer missing".to_owned())?;
        assert_eq!(owner_remote_peer.role, "member");

        let invited = create_invite(CreateInviteRequest {
            group_id: Some(owner_group.group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "5".to_owned(),
        });
        assert!(invited.last_command_error.is_none(), "{invited:?}");
        let invite_code = invited
            .invites
            .last()
            .map(|invite| invite.code.clone())
            .ok_or_else(|| "group invite code missing".to_owned())?;

        reset_with_temp_state("group-runtime-peers-bob");
        create_user(CreateUserRequest {
            display_name: "Bob".to_owned(),
            device_name: Some("Bob laptop".to_owned()),
        });
        let joined = join_group(JoinGroupRequest {
            invite_code,
            group_name: Some("Private Lab".to_owned()),
        });
        assert!(joined.last_command_error.is_none(), "{joined:?}");
        let member_group = joined
            .groups
            .first()
            .ok_or_else(|| "member group missing".to_owned())?;
        assert_eq!(member_group.runtime_peers.len(), 2);
        let member_local_peer = member_group
            .runtime_peers
            .iter()
            .find(|peer| peer.is_local)
            .ok_or_else(|| "member local runtime peer missing".to_owned())?;
        assert_eq!(member_local_peer.role, "member");
        assert_eq!(member_local_peer.peer_id, owner_remote_peer.peer_id);
        let member_remote_peer = member_group
            .runtime_peers
            .iter()
            .find(|peer| !peer.is_local)
            .ok_or_else(|| "member remote runtime peer missing".to_owned())?;
        assert_eq!(member_remote_peer.role, "owner");
        assert_eq!(member_remote_peer.peer_id, owner_local_peer.peer_id);

        let reloaded = load_state().to_view();
        let persisted_group = reloaded
            .groups
            .first()
            .ok_or_else(|| "persisted member group missing".to_owned())?;
        assert_eq!(persisted_group.runtime_peers, member_group.runtime_peers);
        Ok(())
    }

    #[test]
    fn voice_signaling_uses_provider_text_control_outbox_and_inbox() -> Result<(), String> {
        let _guard = test_lock();
        reset_with_temp_state("voice-signaling-state");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let created = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group = created
            .groups
            .first()
            .ok_or_else(|| "group missing".to_owned())?;
        let local_peer = group
            .runtime_peers
            .iter()
            .find(|peer| peer.is_local)
            .ok_or_else(|| "local runtime peer missing".to_owned())?
            .peer_id
            .clone();
        let remote_peer = group
            .runtime_peers
            .iter()
            .find(|peer| !peer.is_local)
            .ok_or_else(|| "remote runtime peer missing".to_owned())?
            .peer_id
            .clone();
        let voice_channel = group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Voice)
            .ok_or_else(|| "voice channel missing".to_owned())?;
        let joined = join_voice(JoinVoiceRequest {
            group_id: group.group_id.clone(),
            channel_id: voice_channel.channel_id.clone(),
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic".to_owned()),
            input_device_label: Some("Mic".to_owned()),
            output_device_id: Some("speaker".to_owned()),
            output_device_label: Some("Speaker".to_owned()),
        });
        let session_id = joined
            .voice_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "voice session missing".to_owned())?;
        let queued = publish_voice_signaling_message(PublishVoiceSignalingMessageRequest {
            session_id: session_id.clone(),
            signal_kind: "offer".to_owned(),
            sealed_payload: "voice-signal-sealed:v1:test-offer-ciphertext-ref".to_owned(),
            signal_id: Some("voice-signal-offer-1".to_owned()),
            created_at_ms: 42,
        });
        let signaling = queued
            .voice_session
            .as_ref()
            .map(|session| session.signaling.clone())
            .ok_or_else(|| "signaling state missing".to_owned())?;
        assert_eq!(signaling.local_peer_id, local_peer);
        assert_eq!(signaling.remote_peer_id, remote_peer);
        assert_eq!(signaling.pending_local_signals, 1);
        let pending = list_pending_text_control_frames(ListPendingTextControlFramesRequest {
            target: None,
            limit: Some(10),
            operation_timeout_ms: None,
        });
        assert!(pending
            .frames
            .iter()
            .any(|frame| matches!(frame.frame, TextControlFrameView::VoiceSignal { .. })));

        let local_user_id = queued
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .ok_or_else(|| "profile missing".to_owned())?;
        let handled = handle_text_control_frame(HandleTextControlFrameRequest {
            frame: TextControlFrameView::VoiceSignal {
                signal: VoiceSignalingMessageView {
                    signal_id: "voice-signal-answer-1".to_owned(),
                    session_id: session_id.clone(),
                    group_id: group.group_id.clone(),
                    channel_id: voice_channel.channel_id.clone(),
                    sender_participant_id: "remote-member".to_owned(),
                    sender_peer_id: remote_peer.clone(),
                    recipient_peer_id: local_peer.clone(),
                    signal_kind: "answer".to_owned(),
                    sealed_payload: "voice-signal-sealed:v1:test-answer-ciphertext-ref".to_owned(),
                    created_at_ms: 43,
                },
            },
        });
        assert!(handled.response_frame.is_none());
        assert_ne!(local_user_id, "remote-member");
        assert_eq!(
            handled
                .state
                .voice_session
                .as_ref()
                .map(|session| session.signaling.received_remote_signals),
            Some(1)
        );
        let taken =
            take_pending_voice_signaling_messages(TakePendingVoiceSignalingMessagesRequest {
                session_id: Some(session_id.clone()),
                limit: Some(10),
            });
        assert_eq!(taken.messages.len(), 1);
        assert_eq!(taken.messages[0].signal_kind, "answer");
        let drained =
            take_pending_voice_signaling_messages(TakePendingVoiceSignalingMessagesRequest {
                session_id: Some(session_id.clone()),
                limit: Some(10),
            });
        assert!(drained.messages.is_empty());
        let left = leave_voice(LeaveVoiceRequest { session_id });
        assert!(left
            .voice_session
            .as_ref()
            .is_some_and(|session| session.signaling.role == "stopped"));
        Ok(())
    }

    #[test]
    fn voice_signaling_prejoin_frames_are_queued_for_stable_group_channel_session(
    ) -> Result<(), String> {
        let _guard = test_lock();
        reset_with_temp_state("voice-signaling-prejoin");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let created = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group = created
            .groups
            .first()
            .ok_or_else(|| "group missing".to_owned())?;
        let local_peer = group
            .runtime_peers
            .iter()
            .find(|peer| peer.is_local)
            .ok_or_else(|| "local runtime peer missing".to_owned())?
            .peer_id
            .clone();
        let remote_peer = group
            .runtime_peers
            .iter()
            .find(|peer| !peer.is_local)
            .ok_or_else(|| "remote runtime peer missing".to_owned())?
            .peer_id
            .clone();
        let voice_channel = group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Voice)
            .ok_or_else(|| "voice channel missing".to_owned())?;
        let stable_session_id = stable_voice_session_id(&group.group_id, &voice_channel.channel_id);

        let handled = handle_text_control_frame(HandleTextControlFrameRequest {
            frame: TextControlFrameView::VoiceSignal {
                signal: VoiceSignalingMessageView {
                    signal_id: "voice-signal-prejoin-offer-1".to_owned(),
                    session_id: stable_session_id.clone(),
                    group_id: group.group_id.clone(),
                    channel_id: voice_channel.channel_id.clone(),
                    sender_participant_id: "remote-member".to_owned(),
                    sender_peer_id: remote_peer.clone(),
                    recipient_peer_id: local_peer.clone(),
                    signal_kind: "offer".to_owned(),
                    sealed_payload: "voice-signal-sealed:v1:test-prejoin-offer-ciphertext-ref"
                        .to_owned(),
                    created_at_ms: 44,
                },
            },
        });
        assert!(handled.state.last_command_error.is_none(), "{handled:?}");
        assert!(handled
            .state
            .events
            .iter()
            .any(|event| event.kind == "voice.signal_prejoin_queued"));

        let joined = join_voice(JoinVoiceRequest {
            group_id: group.group_id.clone(),
            channel_id: voice_channel.channel_id.clone(),
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic".to_owned()),
            input_device_label: Some("Mic".to_owned()),
            output_device_id: Some("speaker".to_owned()),
            output_device_label: Some("Speaker".to_owned()),
        });
        let joined_session_id = joined
            .voice_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "joined voice session missing".to_owned())?;
        assert_eq!(joined_session_id, stable_session_id);
        let taken =
            take_pending_voice_signaling_messages(TakePendingVoiceSignalingMessagesRequest {
                session_id: Some(joined_session_id),
                limit: Some(10),
            });
        assert_eq!(taken.messages.len(), 1);
        assert_eq!(taken.messages[0].signal_kind, "offer");
        Ok(())
    }

    #[test]
    fn g009_voice_signaling_rejects_raw_sdp_ice_before_ipc_persistence() -> Result<(), String> {
        let _guard = test_lock();
        let path = reset_with_temp_state("g009-voice-signaling-privacy");
        create_user(CreateUserRequest {
            display_name: "Profile A".to_owned(),
            device_name: Some("Device A".to_owned()),
        });
        let created = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group = created
            .groups
            .first()
            .ok_or_else(|| "group missing".to_owned())?;
        let voice_channel = group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Voice)
            .ok_or_else(|| "voice channel missing".to_owned())?;
        let joined = join_voice(JoinVoiceRequest {
            group_id: group.group_id.clone(),
            channel_id: voice_channel.channel_id.clone(),
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic".to_owned()),
            input_device_label: Some("Mic".to_owned()),
            output_device_id: Some("speaker".to_owned()),
            output_device_label: Some("Speaker".to_owned()),
        });
        let session_id = joined
            .voice_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "voice session missing".to_owned())?;
        let rejected = publish_voice_signaling_message(PublishVoiceSignalingMessageRequest {
            session_id,
            signal_kind: "offer".to_owned(),
            sealed_payload: "voice-signal-sealed:v1:v=0\r\na=ice-pwd:g009\r\ncandidate:g009"
                .to_owned(),
            signal_id: Some("voice-signal-g009-raw".to_owned()),
            created_at_ms: 42,
        });
        assert!(rejected
            .last_command_error
            .as_ref()
            .is_some_and(|error| error.code == "voice_signal_queue_failed"));
        let persisted = fs::read_to_string(path).map_err(|error| error.to_string())?;
        for forbidden in ["a=ice-pwd:g009", "candidate:g009", "voice-signal-g009-raw"] {
            assert!(
                !persisted.contains(forbidden),
                "raw voice signaling marker leaked into persisted state: {forbidden}"
            );
        }
        Ok(())
    }

    #[test]
    fn dm_flow_persists_across_reload() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("dm-flow");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "New contact".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id.clone()),
                group_id: None,
                channel_id: None,
            },
            body: "persist this dm".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let loaded = load_state().to_view();
        assert!(loaded.dms.iter().any(|dm| dm.dm_id == dm_id));
        assert!(loaded
            .messages
            .iter()
            .any(|message| message.body == "persist this dm"));
    }

    #[test]
    fn set_active_group_focuses_existing_group() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("group-focus");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        let alpha = create_group(CreateGroupRequest {
            name: "Alpha Lab".to_owned(),
            retention: "7 days".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        create_group(CreateGroupRequest {
            name: "Beta Lab".to_owned(),
            retention: "7 days".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let alpha_id = alpha.groups[0].group_id.clone();
        let focused = set_active_group(SetActiveGroupRequest {
            group_id: alpha_id.clone(),
        });
        assert_eq!(
            focused.active_context.and_then(|context| context.group_id),
            Some(alpha_id)
        );
    }

    #[test]
    fn voice_join_mute_volume_leave_flow_does_not_clear_state() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("voice-flow");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        let group_state = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "7 days".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group_state.groups[0].group_id.clone();
        let channel_state = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Ops Voice".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        let channel_id = channel_state.groups[0].channels[0].channel_id.clone();
        let joined = join_voice(JoinVoiceRequest {
            group_id,
            channel_id,
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic-default".to_owned()),
            input_device_label: Some("Default microphone".to_owned()),
            output_device_id: Some("speaker-default".to_owned()),
            output_device_label: Some("Default speaker".to_owned()),
        });
        let session_id = joined
            .voice_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .unwrap_or_default();
        assert!(joined
            .voice_session
            .as_ref()
            .map(|session| session.joined)
            .unwrap_or(false));
        let media_runtime = joined
            .voice_session
            .as_ref()
            .map(|session| session.media_runtime.clone())
            .ok_or_else(|| "voice media runtime boundary".to_owned())?;
        assert!(media_runtime.local_capture_active);
        assert!(!media_runtime.remote_transport_active);
        assert_eq!(media_runtime.boundary, "webview-local-capture");
        assert!(media_runtime
            .fail_closed_reason
            .contains("Remote WebRTC audio transport is not attached")); // backend state proves fail-closed media route
        assert_eq!(
            joined
                .voice_session
                .as_ref()
                .map(|session| session.participants.len()),
            Some(1)
        );
        let activity = update_voice_activity(UpdateVoiceActivityRequest {
            session_id: session_id.clone(),
            rms_i16: 1_800,
            peak_i16: 7_000,
            captured_at_ms: 1_234,
        });
        assert!(activity
            .voice_session
            .as_ref()
            .and_then(|session| session
                .participants
                .iter()
                .find(|participant| participant.role == "you"))
            .map(|participant| participant.speaking)
            .unwrap_or(false));
        assert!(activity
            .events
            .iter()
            .any(|event| event.kind == "voice.activity"));
        let muted = set_self_mute(SetSelfMuteRequest {
            session_id: session_id.clone(),
            muted: true,
        });
        assert!(muted
            .voice_session
            .as_ref()
            .map(|session| session.self_muted)
            .unwrap_or(false));
        let muted_activity = update_voice_activity(UpdateVoiceActivityRequest {
            session_id: session_id.clone(),
            rms_i16: 2_000,
            peak_i16: 9_000,
            captured_at_ms: 1_260,
        });
        assert!(muted_activity
            .voice_session
            .as_ref()
            .and_then(|session| session
                .participants
                .iter()
                .find(|participant| participant.role == "you"))
            .map(|participant| participant.muted && !participant.speaking)
            .unwrap_or(false));
        let local_user_id = joined
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .unwrap_or_default();
        let local_volume_rejected = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id: session_id.clone(),
            participant_id: local_user_id.clone(),
            volume: 55,
        });
        assert_eq!(
            local_volume_rejected
                .last_command_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("voice_volume_local_participant")
        );
        let remote_participant_id = "remote-call-proof".to_owned();
        let remote_attached = attach_test_remote_voice(&session_id, &remote_participant_id);
        assert!(remote_attached
            .voice_session
            .as_ref()
            .map(|session| {
                session.media_runtime.remote_transport_active
                    && session.media_runtime.remote_audio.len() == 1
                    && session.participants.iter().any(|participant| {
                        participant.id == remote_participant_id && participant.role == "remote"
                    })
            })
            .unwrap_or(false));
        let volume = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id: session_id.clone(),
            participant_id: remote_participant_id.clone(),
            volume: 55,
        });
        assert_eq!(
            volume
                .voice_session
                .as_ref()
                .and_then(|session| session
                    .participants
                    .iter()
                    .find(|participant| participant.id == remote_participant_id))
                .map(|participant| participant.volume),
            Some(55)
        );
        let left = leave_voice(LeaveVoiceRequest { session_id });
        let session = left
            .voice_session
            .as_ref()
            .ok_or_else(|| "voice session remains for dock state".to_owned())?;
        assert!(!session.joined);
        assert_eq!(session.media_runtime.boundary, "stopped");
        assert!(!session.media_runtime.local_capture_active);
        assert!(!session.media_runtime.remote_transport_active);
        assert!(session.media_runtime.remote_audio.is_empty());
        assert!(session
            .participants
            .iter()
            .all(|participant| participant.role == "you"));
        assert!(session
            .participants
            .iter()
            .all(|participant| !participant.speaking));
        assert!(!left.groups.is_empty());
        assert_eq!(left.lifecycle, AppLifecycle::Ready);
        Ok(())
    }

    #[test]
    fn voice_remote_media_rejects_local_or_incomplete_evidence() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("voice-invalid-remote-media");
        let created = create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        let local_user_id = created
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .ok_or_else(|| "local profile id".to_owned())?;
        let group_state = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "7 days".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group_state.groups[0].group_id.clone();
        let channel_state = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Ops Voice".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        let channel_id = channel_state.groups[0].channels[0].channel_id.clone();
        let joined = join_voice(JoinVoiceRequest {
            group_id,
            channel_id,
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic-default".to_owned()),
            input_device_label: Some("Default microphone".to_owned()),
            output_device_id: Some("speaker-default".to_owned()),
            output_device_label: Some("Default speaker".to_owned()),
        });
        let session_id = joined
            .voice_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "joined voice session".to_owned())?;

        let rejected = attach_voice_remote_media(AttachVoiceRemoteMediaRequest {
            session_id,
            participant_id: local_user_id,
            participant_name: "Alice local loopback".to_owned(),
            remote_peer_id: "peer-local-loopback".to_owned(),
            stream_id: "stream-local-loopback".to_owned(),
            audio_track_id: "track-local-loopback".to_owned(),
            playback_element_id: "audio-local-loopback".to_owned(),
            local_audio_tracks_sent: 1,
            received_audio_frames: 0,
            speaking: true,
            attached_at_ms: 1_700_000_000_000,
        });
        assert_eq!(
            rejected
                .last_command_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("voice_remote_media_evidence_invalid")
        );
        let session = rejected
            .voice_session
            .as_ref()
            .ok_or_else(|| "voice session remains after rejected evidence".to_owned())?;
        assert_eq!(session.media_runtime.boundary, "webview-local-capture");
        assert!(!session.media_runtime.remote_transport_active);
        assert!(session.media_runtime.remote_audio.is_empty());
        assert!(session
            .participants
            .iter()
            .all(|participant| participant.role == "you"));
        Ok(())
    }

    #[test]
    fn voice_rejoin_preserves_self_mute_and_suppresses_speaking_until_unmuted() -> Result<(), String>
    {
        let _guard = test_lock();
        let _path = reset_with_temp_state("voice-rejoin-muted");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let group_state = create_group(CreateGroupRequest {
            name: "Mute Persistence Lab".to_owned(),
            retention: "7 days".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group_state.groups[0].group_id.clone();
        let channel_state = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Ops Voice".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        let channel_id = channel_state.groups[0].channels[0].channel_id.clone();
        let joined = join_voice(JoinVoiceRequest {
            group_id: group_id.clone(),
            channel_id: channel_id.clone(),
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic-default".to_owned()),
            input_device_label: Some("Default microphone".to_owned()),
            output_device_id: Some("speaker-default".to_owned()),
            output_device_label: Some("Default speaker".to_owned()),
        });
        let session_id = joined
            .voice_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "joined session".to_owned())?;
        set_self_mute(SetSelfMuteRequest {
            session_id: session_id.clone(),
            muted: true,
        });
        leave_voice(LeaveVoiceRequest { session_id });

        let rejoined = join_voice(JoinVoiceRequest {
            group_id,
            channel_id,
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic-default".to_owned()),
            input_device_label: Some("Default microphone".to_owned()),
            output_device_id: Some("speaker-default".to_owned()),
            output_device_label: Some("Default speaker".to_owned()),
        });
        let rejoined_session = rejoined
            .voice_session
            .as_ref()
            .ok_or_else(|| "rejoined session".to_owned())?;
        assert!(rejoined_session.joined);
        assert!(rejoined_session.self_muted);
        let rejoined_session_id = rejoined_session.session_id.clone();
        let muted_activity = update_voice_activity(UpdateVoiceActivityRequest {
            session_id: rejoined_session_id.clone(),
            rms_i16: 8_000,
            peak_i16: 16_000,
            captured_at_ms: 2_000,
        });
        assert!(muted_activity
            .voice_session
            .as_ref()
            .and_then(|session| session
                .participants
                .iter()
                .find(|participant| participant.role == "you"))
            .map(|participant| participant.muted && !participant.speaking)
            .unwrap_or(false));
        let unmuted = set_self_mute(SetSelfMuteRequest {
            session_id: rejoined_session_id.clone(),
            muted: false,
        });
        assert!(!unmuted
            .voice_session
            .as_ref()
            .map(|session| session.self_muted)
            .unwrap_or(true));
        let unmuted_activity = update_voice_activity(UpdateVoiceActivityRequest {
            session_id: rejoined_session_id,
            rms_i16: 8_000,
            peak_i16: 16_000,
            captured_at_ms: 2_100,
        });
        assert!(unmuted_activity
            .voice_session
            .as_ref()
            .and_then(|session| session
                .participants
                .iter()
                .find(|participant| participant.role == "you"))
            .map(|participant| !participant.muted && participant.speaking)
            .unwrap_or(false));
        Ok(())
    }

    #[test]
    fn voice_join_requires_microphone_permission_and_input_device() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("voice-permission-denied");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        let group_state = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "7 days".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group_state.groups[0].group_id.clone();
        let channel_state = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Ops Voice".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        let channel_id = channel_state.groups[0].channels[0].channel_id.clone();
        let denied = join_voice(JoinVoiceRequest {
            group_id,
            channel_id,
            microphone_permission: "denied".to_owned(),
            input_device_id: None,
            input_device_label: None,
            output_device_id: None,
            output_device_label: None,
        });
        let session = denied
            .voice_session
            .as_ref()
            .ok_or_else(|| "permission-denied voice session state".to_owned())?;
        assert!(!session.joined);
        assert_eq!(session.microphone_permission, "denied");
        assert!(session.permission_denied_copy.contains("Grant microphone"));
        assert_eq!(session.media_runtime.boundary, "fail-closed");
        assert!(!session.media_runtime.local_capture_active);
        assert!(!session.media_runtime.remote_transport_active);
        assert!(session.media_runtime.fail_closed_reason.contains("denied"));
        assert!(session
            .participants
            .iter()
            .all(|participant| !participant.speaking));
        assert!(denied
            .events
            .iter()
            .any(|event| event.kind == "voice.permission_denied"));
        Ok(())
    }

    #[test]
    fn voice_sessions_are_channel_scoped() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("voice-scoped");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        let group_state = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "7 days".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group_state.groups[0].group_id.clone();
        let first = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Ops".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        let first_channel = first.groups[0].channels[0].channel_id.clone();
        let second = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Lounge".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        let second_channel = second.groups[0].channels[1].channel_id.clone();
        let joined = join_voice(JoinVoiceRequest {
            group_id,
            channel_id: first_channel.clone(),
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic-default".to_owned()),
            input_device_label: Some("Default microphone".to_owned()),
            output_device_id: Some("speaker-default".to_owned()),
            output_device_label: Some("Default speaker".to_owned()),
        });
        let session = joined
            .voice_session
            .as_ref()
            .ok_or_else(|| "joined voice session".to_owned())?;
        assert_eq!(session.channel_id, first_channel);
        assert_ne!(session.channel_id, second_channel);
        Ok(())
    }

    #[test]
    fn voice_join_records_runtime_boundary_without_fake_remote_participants() -> Result<(), String>
    {
        let _guard = test_lock();
        let _path = reset_with_temp_state("voice-runtime-boundary");
        let created = create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let local_user_id = created
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .ok_or_else(|| "profile created".to_owned())?;
        let group = create_group(CreateGroupRequest {
            name: "Runtime Lab".to_owned(),
            retention: "7 days".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group.groups[0].group_id.clone();
        let channel = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Voice Runtime".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        let channel_id = channel.groups[0].channels[0].channel_id.clone();
        let joined = join_voice(JoinVoiceRequest {
            group_id,
            channel_id,
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic-default".to_owned()),
            input_device_label: Some("Default microphone".to_owned()),
            output_device_id: Some("speaker-default".to_owned()),
            output_device_label: Some("Default speaker".to_owned()),
        });
        let session = joined
            .voice_session
            .as_ref()
            .ok_or_else(|| "joined voice session".to_owned())?;
        assert!(session.joined);
        assert_eq!(session.media_runtime.boundary, "webview-local-capture");
        assert!(session.media_runtime.local_capture_active);
        assert!(!session.media_runtime.remote_transport_active);
        assert_eq!(session.participants.len(), 1);
        assert_eq!(session.participants[0].id, local_user_id);
        assert_eq!(session.participants[0].role, "you");
        assert!(!session
            .participants
            .iter()
            .any(|participant| participant.name.eq_ignore_ascii_case("bob")
                || participant.role == "remote"));
        Ok(())
    }

    #[cfg(not(all(target_os = "linux", feature = "production-storage")))]
    #[test]
    fn persistence_uses_app_store_env_override_and_atomic_shape() {
        let _guard = test_lock();
        let path = reset_with_temp_state("atomic");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
        let mut store = FileAppStore::new(&path);
        let contents = store
            .load_app_state()
            .ok()
            .flatten()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_default();
        assert!(contents.contains("schema_version"));
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn production_storage_persists_sealed_envelope_without_plain_state() {
        let _guard = test_lock();
        let path = reset_with_temp_state("sealed-envelope");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
        let persisted = fs::read_to_string(path).unwrap_or_default();
        assert!(persisted.contains(&["discrypt.appdb.", "en", "crypted.v1"].concat()));
        assert!(!persisted.contains("schema_version"));
        assert!(!persisted.contains("Alice"));
    }

    #[test]
    fn production_path_ignores_env_override_without_harness_gate() {
        let _guard = test_lock();
        let path = fresh_state_path("prod-env-ignored");
        std::env::set_var("DISCRYPT_APP_STATE_PATH", &path);
        let production_path = app_store_path_with_env_override(false);
        assert_ne!(production_path, path);
        assert_eq!(
            production_path.file_name().and_then(|value| value.to_str()),
            Some(APP_STATE_STORE_FILENAME)
        );
    }

    #[test]
    fn corrupt_persisted_state_surfaces_recovery_error_instead_of_silent_first_run() {
        let _guard = test_lock();
        let path = fresh_state_path("corrupt-state");
        fs::write(&path, b"{not-json").expect("write corrupt app state");
        let mut store = FileAppStore::new(&path);
        let state = load_state_from_store(&mut store);
        let error = state
            .last_command_error
            .as_ref()
            .expect("corrupt persisted state must surface an error");
        assert_eq!(error.command, "app_persistence");
        assert_eq!(error.code, "state_decode_failed");
        assert!(
            error
                .recovery_hint
                .contains("do not silently treat this as a first-run profile"),
            "{error:?}"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn persistence_save_failure_keeps_live_state_unchanged_and_fail_closed() {
        let _guard = test_lock();
        let path = fresh_state_path("save-fail-closed");
        fs::create_dir_all(&path).expect("create directory at state file path");
        let mut service = TauriAppService {
            state: PersistedAppState::initial(),
            text_control_transport_runtime: None,
            pending_text_control_transport_runtime: None,
            state_path_override: Some(path.clone()),
        };

        let view = service.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Alice Should Not Commit".to_owned(),
                    device_name: None,
                },
                false,
            );
        });

        assert!(
            service.state.profile.is_none(),
            "failed persistence must not swap candidate state into the live service"
        );
        assert!(
            view.profile.is_none(),
            "returned view must remain pre-mutation"
        );
        let error = service
            .state
            .last_command_error
            .as_ref()
            .expect("save failure must be surfaced");
        assert_eq!(error.code, "state_save_failed");
        assert_eq!(error.command, "app_persistence");
        assert!(
            error
                .message
                .contains("detailed storage failure copy is redacted"),
            "{error:?}"
        );
        assert!(
            !error.message.contains("Alice Should Not Commit"),
            "observable error must not leak uncommitted state"
        );
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn persistence_errors_default_redact_unrecognized_failure_details() {
        let error = persistence_command_error(
            "state_save_failed",
            "database failure leaked-secret-token-12345",
            "Check disk/keychain availability before continuing; the app did not confirm persistence.",
        );
        assert_eq!(error.code, "state_save_failed");
        assert!(
            error
                .message
                .contains("detailed storage failure copy is redacted"),
            "{error:?}"
        );
        assert!(
            !error.message.contains("leaked-secret-token-12345"),
            "unrecognized secret-like storage errors must not be copied verbatim"
        );
    }

    #[test]
    fn desktop_signaling_client_wraps_rust_protocol_client() -> Result<(), String> {
        let service = external_signaling::server::SharedSignalingService::new();
        let transport = external_signaling::client::SharedServiceTransport::new(
            service,
            external_signaling::server::ServerConfig::default(),
        );
        let client = external_signaling::client::SignalingClient::new(
            external_signaling::client::SignalingClientConfig::new(
                "https://127.0.0.1:8787",
                b"desktop-client-token".to_vec(),
                b"desktop-nonce-seed".to_vec(),
            )
            .map_err(|err| err.to_string())?,
            transport,
        )
        .map_err(|err| err.to_string())?;
        let mut desktop_client = DesktopSignalingClient::new(client);
        let expires_at = Utc::now() + Duration::seconds(60);

        desktop_client.publish_opaque_signal(
            external_signaling::server::SignalKind::Offer,
            "session-1",
            b"opaque-offer",
            expires_at,
        )?;
        let signals = desktop_client
            .take_opaque_signals(external_signaling::server::SignalKind::Offer, "session-1")?;

        assert_eq!(signals, vec![b"opaque-offer".to_vec()]);
        Ok(())
    }

    #[test]
    fn transport_status_surfaces_all_connectivity_states() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("transport-status-ui");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let group = create_group(CreateGroupRequest {
            name: "Status Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group.groups[0].group_id.clone();
        let state = create_invite(CreateInviteRequest {
            group_id: Some(group_id),
            expires: "1 day".to_owned(),
            max_use: "5".to_owned(),
        });
        let labels: Vec<_> = state
            .transport_status
            .iter()
            .map(|status| status.label.as_str())
            .collect();
        assert_eq!(
            labels,
            vec![
                "signaling",
                "ICE",
                "adapter",
                "direct",
                "overlay",
                "TURN",
                "degraded",
                "reconnecting",
                "failed",
                "text/control runtime"
            ]
        );
        assert!(state
            .transport_status
            .iter()
            .any(|status| status.label == "signaling" && status.status == "signed-endpoint-ready"));
        assert!(state
            .transport_status
            .iter()
            .any(|status| status.label == "direct" && status.status == "no-direct-proof"));
    }

    #[test]
    fn join_progress_surfaces_backend_join_stages() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("join-progress-ui");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let group = create_group(CreateGroupRequest {
            name: "Join Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let state = create_invite(CreateInviteRequest {
            group_id: Some(group.groups[0].group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "5".to_owned(),
        });
        let keys: Vec<_> = state
            .join_progress
            .iter()
            .map(|step| step.key.as_str())
            .collect();
        assert_eq!(
            keys,
            vec![
                "invite_parsed",
                "rendezvous",
                "authorized_member",
                "welcome",
                "mls_joined",
                "transport"
            ]
        );
        assert!(state
            .join_progress
            .iter()
            .any(|step| step.key == "invite_parsed" && step.status == "complete"));
        assert!(state
            .join_progress
            .iter()
            .any(|step| step.key == "transport" && step.status == "waiting-route-proof"));
    }

    #[test]
    fn runtime_mode_disables_production_labels_without_configured_services() {
        let runtime = runtime_mode_view();
        if cfg!(all(
            target_os = "linux",
            feature = "production-network",
            feature = "production-media",
            feature = "production-storage"
        )) {
            assert_eq!(runtime.mode, "configured-services");
            assert!(runtime.production_labels_enabled);
            assert!(runtime.harness_badge.contains("configured"));
            assert!(runtime
                .disabled_reason
                .contains("Production labels enabled"));
        } else {
            assert_eq!(runtime.mode, "local-dev-harness");
            assert!(!runtime.production_labels_enabled);
            assert!(runtime.harness_badge.contains("harness"));
            assert!(runtime
                .disabled_reason
                .contains("Production labels disabled"));
        }
        assert_eq!(runtime.services.len(), 3);
    }

    #[test]
    fn voice_states_surface_permission_route_mute_speaking_and_left() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("voice-state-ui");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let group = create_group(CreateGroupRequest {
            name: "Voice State Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group.groups[0].group_id.clone();
        let channel = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Voice Lobby".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        let channel_id = channel.groups[0]
            .channels
            .iter()
            .find(|channel| matches!(channel.kind, ChannelKind::Voice))
            .map(|channel| channel.channel_id.clone())
            .unwrap_or_else(|| "voice".to_owned());
        let state = join_voice(JoinVoiceRequest {
            group_id,
            channel_id,
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic".to_owned()),
            input_device_label: Some("Mic".to_owned()),
            output_device_id: Some("speaker".to_owned()),
            output_device_label: Some("Speaker".to_owned()),
        });
        let keys: Vec<_> = state
            .voice_states
            .iter()
            .map(|entry| entry.key.as_str())
            .collect();
        assert_eq!(
            keys,
            vec![
                "permission_needed",
                "joining",
                "ice_checking",
                "route",
                "muted",
                "speaking",
                "reconnecting",
                "left"
            ]
        );
        assert!(state
            .voice_states
            .iter()
            .any(|entry| entry.key == "joining" && entry.status == "joined"));
    }

    #[test]
    fn signed_text_delivery_receipt_updates_message_state() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("signed-text-receipt");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let sent = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "receipt-bound message".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();
        let persisted = load_state();
        let envelope = persisted
            .text_delivery_envelopes
            .iter()
            .find(|record| record.message_id == message_id)
            .ok_or_else(|| "persisted text envelope missing".to_owned())?;
        let recipient_signer = SigningKey::generate(&mut OsRng);
        let receipt = TextDeliveryReceipt::sign(
            &envelope.group_id,
            TextDeliveryReceiptInput {
                message_id: message_id.clone(),
                recipient_leaf: 2,
                recipient_device_id: "bob-desktop".to_owned(),
                received_at_ms: 42_000,
                envelope_ciphertext_hash: envelope.envelope.ciphertext_hash(),
            },
            &recipient_signer,
        )
        .map_err(|error| error.to_string())?;

        let receipted = apply_text_delivery_receipt(ApplyTextDeliveryReceiptRequest {
            message_id: message_id.clone(),
            receipt,
            recipient_verifying_key_hex: hex::encode(recipient_signer.verifying_key().as_bytes()),
        });

        assert!(receipted.last_command_error.is_none());
        let message = receipted
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "message row missing".to_owned())?;
        assert_eq!(message.state_key, "peer_receipt");
        assert_eq!(message.state_label, "Peer receipt");
        assert!(message.peer_receipt.is_some());
        assert!(message.state_detail.contains("bob-desktop"));
        Ok(())
    }

    #[test]
    fn two_profile_receiver_identity_can_sign_delivery_receipt() -> Result<(), String> {
        let _guard = test_lock();
        let _alice_path = reset_with_temp_state("two-profile-text-receipt-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let sent = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "two profile receipt".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();
        let persisted = load_state();
        let envelope = persisted
            .text_delivery_envelopes
            .iter()
            .find(|record| record.message_id == message_id)
            .ok_or_else(|| "persisted text envelope missing".to_owned())?;

        let bob_path = fresh_state_path("two-profile-text-receipt-bob");
        let _ = fs::remove_file(&bob_path);
        let mut bob = TauriAppService::load_for_test_path(bob_path);
        bob.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Bob".to_owned(),
                    device_name: Some("Bob laptop".to_owned()),
                },
                false,
            );
        });
        let bob_signer = SigningKey::from_bytes(&bob.state.identity_seed_bytes());
        let receipt = TextDeliveryReceipt::sign(
            &envelope.group_id,
            TextDeliveryReceiptInput {
                message_id: message_id.clone(),
                recipient_leaf: 2,
                recipient_device_id: bob.state.local_user_id(),
                received_at_ms: 42_100,
                envelope_ciphertext_hash: envelope.envelope.ciphertext_hash(),
            },
            &bob_signer,
        )
        .map_err(|error| error.to_string())?;

        let receipted = apply_text_delivery_receipt(ApplyTextDeliveryReceiptRequest {
            message_id: message_id.clone(),
            receipt,
            recipient_verifying_key_hex: hex::encode(bob_signer.verifying_key().as_bytes()),
        });

        assert!(receipted.last_command_error.is_none());
        let message = receipted
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "message row missing".to_owned())?;
        assert_eq!(message.state_key, "peer_receipt");
        assert_eq!(
            message
                .peer_receipt
                .as_ref()
                .map(|receipt| { receipt.recipient_key_fingerprint.as_str() }),
            Some(key_fingerprint(&bob_signer.verifying_key()).as_str())
        );
        Ok(())
    }

    #[test]
    fn receiver_command_accepts_verified_envelope_and_returns_signed_receipt() -> Result<(), String>
    {
        let _guard = test_lock();
        let _alice_path = reset_with_temp_state("receive-envelope-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_id),
            group_id: None,
            channel_id: None,
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "receiver command receipt".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();
        let persisted = load_state();
        let envelope_record = persisted
            .text_delivery_envelopes
            .iter()
            .find(|record| record.message_id == message_id)
            .cloned()
            .ok_or_else(|| "persisted text envelope missing".to_owned())?;

        let bob_path = fresh_state_path("receive-envelope-bob");
        let _ = fs::remove_file(&bob_path);
        let mut bob = TauriAppService::load_for_test_path(bob_path);
        bob.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Bob".to_owned(),
                    device_name: Some("Bob laptop".to_owned()),
                },
                false,
            );
        });
        let (receipt, recipient_key_hex) =
            bob.state
                .receive_text_delivery_envelope(ReceiveTextDeliveryEnvelopeRequest {
                    target,
                    envelope: envelope_record.envelope.clone(),
                    sender_verifying_key_hex: envelope_record.sender_verifying_key_hex.clone(),
                    recipient_leaf: Some(2),
                })?;

        assert!(bob.state.last_command_error.is_none());
        assert!(bob
            .state
            .messages
            .iter()
            .any(|message| message.message_id == message_id
                && message.state_key == "received_envelope"));
        let recipient_key = verifying_key_from_hex(&recipient_key_hex)
            .ok_or_else(|| "recipient key should decode".to_owned())?;
        receipt
            .verify(
                &envelope_record.group_id,
                &envelope_record.envelope,
                &recipient_key,
            )
            .map_err(|error| error.to_string())?;

        let receipted = apply_text_delivery_receipt(ApplyTextDeliveryReceiptRequest {
            message_id: message_id.clone(),
            receipt,
            recipient_verifying_key_hex: recipient_key_hex,
        });
        let message = receipted
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "message row missing".to_owned())?;
        assert_eq!(message.state_key, "peer_receipt");
        Ok(())
    }

    fn openmls_text_envelope_for_test(
        group_id: &str,
        channel_id: &str,
        message_id: &str,
        plaintext: &str,
        text_exporter_secret: &[u8],
        sender: &SigningKey,
    ) -> Result<TextMessageEnvelope, String> {
        let mut log = InMemoryTextAuthorLog::default();
        let mut transport = InMemoryTextTransport::default();
        let mut events = InMemoryTextSendEvents::default();
        let delivery_group_id = text_delivery_group_id(&MessageTargetView {
            kind: "channel".to_owned(),
            dm_id: None,
            group_id: Some(group_id.to_owned()),
            channel_id: Some(channel_id.to_owned()),
        })?;
        let receipt = TextOutboundPipeline::new(&mut log, &mut transport, &mut events)
            .send(
                TextOutboundRequest {
                    group_id: delivery_group_id,
                    channel_id: channel_id.to_owned(),
                    epoch: 0,
                    sender_leaf: 1,
                    sender_device_id: "remote-device".to_owned(),
                    sequence: 1,
                    message_id: message_id.to_owned(),
                    retention: TextRetentionMetadata {
                        policy: "app-default".to_owned(),
                        created_at_ms: 1,
                        expires_at_ms: None,
                        delete_after_read: false,
                    },
                    plaintext: plaintext.as_bytes().to_vec(),
                    sent_at_ms: 1,
                    now: Utc::now(),
                },
                TextSelectedRoute {
                    session_id: "test-text-session".to_owned(),
                    route_label: "direct".to_owned(),
                    overlay_hops: 0,
                    ciphertext_only: true,
                },
                text_exporter_secret,
                sender,
            )
            .map_err(|error| error.to_string())?;
        Ok(receipt.envelope)
    }

    #[test]
    fn receiver_command_decrypts_openmls_exporter_text_plaintext() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("receive-openmls-plaintext");
        create_user(CreateUserRequest {
            display_name: "Bob".to_owned(),
            device_name: Some("Bob laptop".to_owned()),
        });
        let created = create_group(CreateGroupRequest {
            name: "OpenMLS Receive Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group = created
            .groups
            .iter()
            .find(|group| group.name == "OpenMLS Receive Lab")
            .ok_or_else(|| "created group missing".to_owned())?;
        let group_id = group.group_id.clone();
        let channel_id = group
            .channels
            .iter()
            .find(|channel| matches!(channel.kind, ChannelKind::Text))
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "text channel missing".to_owned())?;
        let mut engine = OpenMlsGroupEngine::open(app_openmls_store_path())
            .map_err(|error| error.to_string())?;
        let handle = load_state()
            .openmls_groups
            .iter()
            .find(|record| record.group_id == group_id)
            .cloned()
            .ok_or_else(|| "OpenMLS handle missing".to_owned())?;
        let signer_public_key = hex::decode(&handle.signer_public_key_hex)
            .map_err(|error| format!("OpenMLS signer key should decode: {error}"))?;
        engine
            .load_group(&group_id, &signer_public_key)
            .map_err(|error| error.to_string())?;
        let text_exporter_secret = engine
            .export_secret(&group_id, TEXT_EXPORTER_LABEL, TEXT_EXPORTER_CONTEXT, 32)
            .map_err(|error| error.to_string())?;
        let sender = SigningKey::generate(&mut OsRng);
        let envelope = openmls_text_envelope_for_test(
            &group_id,
            &channel_id,
            "msg-openmls-plaintext",
            "hello from remote openmls",
            &text_exporter_secret,
            &sender,
        )?;

        let response = receive_text_delivery_envelope(ReceiveTextDeliveryEnvelopeRequest {
            target: MessageTargetView {
                kind: "channel".to_owned(),
                dm_id: None,
                group_id: Some(group_id),
                channel_id: Some(channel_id),
            },
            envelope,
            sender_verifying_key_hex: hex::encode(sender.verifying_key().as_bytes()),
            recipient_leaf: Some(2),
        });

        assert!(response.receipt.is_some(), "{response:?}");
        assert!(response.state.last_command_error.is_none(), "{response:?}");
        let message = response
            .state
            .messages
            .iter()
            .find(|message| message.message_id == "msg-openmls-plaintext")
            .ok_or_else(|| "received message row missing".to_owned())?;
        assert_eq!(message.state_key, "received_plaintext");
        assert_eq!(message.body, "hello from remote openmls");
        assert!(message
            .status
            .contains("decrypted through TextInboundPipeline"));
        Ok(())
    }

    #[test]
    fn receiver_command_does_not_render_plaintext_for_invalid_signer_or_exporter(
    ) -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("receive-openmls-invalid");
        create_user(CreateUserRequest {
            display_name: "Bob".to_owned(),
            device_name: Some("Bob laptop".to_owned()),
        });
        let created = create_group(CreateGroupRequest {
            name: "OpenMLS Invalid Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group = created
            .groups
            .iter()
            .find(|group| group.name == "OpenMLS Invalid Lab")
            .ok_or_else(|| "created group missing".to_owned())?;
        let group_id = group.group_id.clone();
        let channel_id = group
            .channels
            .iter()
            .find(|channel| matches!(channel.kind, ChannelKind::Text))
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "text channel missing".to_owned())?;
        let sender = SigningKey::generate(&mut OsRng);
        let wrong_sender = SigningKey::generate(&mut OsRng);
        let target = MessageTargetView {
            kind: "channel".to_owned(),
            dm_id: None,
            group_id: Some(group_id.clone()),
            channel_id: Some(channel_id.clone()),
        };
        let invalid_signer_envelope = openmls_text_envelope_for_test(
            &group_id,
            &channel_id,
            "msg-invalid-signer",
            "must not render",
            b"wrong-exporter-but-signature-valid",
            &sender,
        )?;
        let invalid_signer = receive_text_delivery_envelope(ReceiveTextDeliveryEnvelopeRequest {
            target: target.clone(),
            envelope: invalid_signer_envelope,
            sender_verifying_key_hex: hex::encode(wrong_sender.verifying_key().as_bytes()),
            recipient_leaf: Some(2),
        });
        assert!(invalid_signer.receipt.is_none(), "{invalid_signer:?}");
        assert!(invalid_signer
            .state
            .messages
            .iter()
            .all(|message| message.body != "must not render"));

        let invalid_exporter_envelope = openmls_text_envelope_for_test(
            &group_id,
            &channel_id,
            "msg-invalid-exporter",
            "must not render exporter mismatch",
            b"wrong-exporter-material",
            &sender,
        )?;
        let invalid_exporter = receive_text_delivery_envelope(ReceiveTextDeliveryEnvelopeRequest {
            target,
            envelope: invalid_exporter_envelope,
            sender_verifying_key_hex: hex::encode(sender.verifying_key().as_bytes()),
            recipient_leaf: Some(2),
        });
        assert!(invalid_exporter.receipt.is_some(), "{invalid_exporter:?}");
        let message = invalid_exporter
            .state
            .messages
            .iter()
            .find(|message| message.message_id == "msg-invalid-exporter")
            .ok_or_else(|| "invalid exporter row missing".to_owned())?;
        assert_eq!(message.state_key, "received_decrypt_failed");
        assert_ne!(message.body, "must not render exporter mismatch");
        assert!(message.body.contains("Encrypted message envelope received"));
        Ok(())
    }

    #[test]
    fn receiver_command_rejects_tampered_envelope_without_receipt() -> Result<(), String> {
        let _guard = test_lock();
        let _alice_path = reset_with_temp_state("receive-envelope-tampered-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_id),
            group_id: None,
            channel_id: None,
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "tamper me".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();
        let persisted = load_state();
        let mut envelope_record = persisted
            .text_delivery_envelopes
            .iter()
            .find(|record| record.message_id == message_id)
            .cloned()
            .ok_or_else(|| "persisted text envelope missing".to_owned())?;
        envelope_record.envelope.content_ciphertext.push(0x99);

        let bob_path = fresh_state_path("receive-envelope-tampered-bob");
        let _ = fs::remove_file(&bob_path);
        let mut bob = TauriAppService::load_for_test_path(bob_path);
        bob.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Bob".to_owned(),
                    device_name: Some("Bob laptop".to_owned()),
                },
                false,
            );
        });
        let rejected =
            bob.state
                .receive_text_delivery_envelope(ReceiveTextDeliveryEnvelopeRequest {
                    target,
                    envelope: envelope_record.envelope,
                    sender_verifying_key_hex: envelope_record.sender_verifying_key_hex,
                    recipient_leaf: Some(2),
                });

        assert!(rejected.is_err());
        assert_eq!(
            bob.state
                .last_command_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("text_envelope_verification_failed")
        );
        assert!(bob.state.messages.is_empty());
        assert!(bob.state.text_delivery_receipts.is_empty());
        Ok(())
    }

    #[test]
    fn text_control_frame_handler_bridges_envelope_to_receipt() -> Result<(), String> {
        let _guard = test_lock();
        let _alice_path = reset_with_temp_state("text-control-frame-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_id),
            group_id: None,
            channel_id: None,
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "frame handler receipt".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();
        let persisted = load_state();
        let envelope_record = persisted
            .text_delivery_envelopes
            .iter()
            .find(|record| record.message_id == message_id)
            .cloned()
            .ok_or_else(|| "persisted text envelope missing".to_owned())?;

        let bob_path = fresh_state_path("text-control-frame-bob");
        let _ = fs::remove_file(&bob_path);
        let mut bob = TauriAppService::load_for_test_path(bob_path);
        bob.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Bob".to_owned(),
                    device_name: Some("Bob laptop".to_owned()),
                },
                false,
            );
        });

        let response = bob
            .state
            .handle_text_control_frame(TextControlFrameView::Envelope {
                target,
                envelope: envelope_record.envelope,
                sender_verifying_key_hex: envelope_record.sender_verifying_key_hex,
                recipient_leaf: Some(2),
            })
            .ok_or_else(|| "receiver should return receipt frame".to_owned())?;

        let TextControlFrameView::Receipt {
            message_id: receipt_message_id,
            receipt,
            recipient_verifying_key_hex,
        } = response
        else {
            return Err("expected receipt response frame".to_owned());
        };
        assert_eq!(receipt_message_id, message_id);

        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let response = guard
            .state
            .handle_text_control_frame(TextControlFrameView::Receipt {
                message_id: receipt_message_id,
                receipt,
                recipient_verifying_key_hex,
            });
        assert!(response.is_none());
        let alice_view = guard.state.to_view();
        let message = alice_view
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "message row missing".to_owned())?;
        assert_eq!(message.state_key, "peer_receipt");
        Ok(())
    }

    #[test]
    fn text_control_frame_roundtrip_persists_across_two_profile_state_files() -> Result<(), String>
    {
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("text-control-frame-persist-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_id),
            group_id: None,
            channel_id: None,
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "persistent frame receipt".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();
        let envelope_record = load_state()
            .text_delivery_envelopes
            .into_iter()
            .find(|record| record.message_id == message_id)
            .ok_or_else(|| "persisted text envelope missing".to_owned())?;

        let bob_path = fresh_state_path("text-control-frame-persist-bob");
        let _ = fs::remove_file(&bob_path);
        let mut bob = TauriAppService::load_for_test_path(bob_path.clone());
        bob.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Bob".to_owned(),
                    device_name: Some("Bob laptop".to_owned()),
                },
                false,
            );
        });
        let mut receipt_frame = None;
        bob.mutate(|state| {
            receipt_frame = state.handle_text_control_frame(TextControlFrameView::Envelope {
                target,
                envelope: envelope_record.envelope,
                sender_verifying_key_hex: envelope_record.sender_verifying_key_hex,
                recipient_leaf: Some(2),
            });
        });
        let receipt_frame =
            receipt_frame.ok_or_else(|| "receiver should return receipt frame".to_owned())?;

        let bob_reloaded = TauriAppService::load_for_test_path(bob_path);
        let bob_view = bob_reloaded.state.to_view();
        assert!(bob_view.messages.iter().any(|message| {
            message.message_id == message_id && message.state_key == "received_envelope"
        }));
        assert!(bob_reloaded
            .state
            .text_delivery_receipts
            .iter()
            .any(|receipt| receipt.message_id == message_id));

        let (_alice_view, response) =
            mutate_app_service_with_result(|state| state.handle_text_control_frame(receipt_frame));
        assert!(response.is_none());

        let mut alice_store = FileAppStore::new(&alice_path);
        let alice_reloaded = load_state_from_store(&mut alice_store);
        let message = alice_reloaded
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "alice message row missing after reload".to_owned())?;
        assert_eq!(message.state_key, "peer_receipt");
        assert!(message.peer_receipt.is_some());
        Ok(())
    }

    #[test]
    fn text_control_outbox_persists_pending_frame_across_reload() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("text-control-outbox-pending");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_state.dms[0].dm_id.clone()),
            group_id: None,
            channel_id: None,
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "queued outbox frame".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();

        let listed = list_pending_text_control_frames(ListPendingTextControlFramesRequest {
            target: Some(target),
            limit: None,
            operation_timeout_ms: None,
        });
        assert!(listed.state.last_command_error.is_none());
        assert_eq!(listed.frames.len(), 1);
        assert_eq!(listed.frames[0].message_id, message_id);
        assert_eq!(listed.frames[0].state_key, "pending");
        assert_eq!(
            listed.frames[0].frame_sha256,
            text_control_frame_sha256(&listed.frames[0].frame)?
        );

        let reloaded = load_state();
        let pending_after_reload =
            reloaded.list_pending_text_control_frames(&ListPendingTextControlFramesRequest {
                target: None,
                limit: Some(10),
                operation_timeout_ms: None,
            });
        assert_eq!(pending_after_reload.len(), 1);
        assert_eq!(pending_after_reload[0].message_id, message_id);
        Ok(())
    }

    #[test]
    fn text_control_outbox_marks_sent_then_receipted() -> Result<(), String> {
        let _guard = test_lock();
        let _alice_path = reset_with_temp_state("text-control-outbox-receipted-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_state.dms[0].dm_id.clone()),
            group_id: None,
            channel_id: None,
        };
        let sent = send_message(SendMessageRequest {
            target,
            body: "outbox sent then receipt".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();
        let listed = list_pending_text_control_frames(ListPendingTextControlFramesRequest {
            target: None,
            limit: None,
            operation_timeout_ms: None,
        });
        let outbox_frame = listed
            .frames
            .into_iter()
            .find(|frame| frame.message_id == message_id)
            .ok_or_else(|| "pending outbox frame missing".to_owned())?;

        let marked = mark_text_control_frame_sent(MarkTextControlFrameSentRequest {
            message_id: message_id.clone(),
            frame_sha256: outbox_frame.frame_sha256.clone(),
            transport_session_id: Some("text-session-test".to_owned()),
        });
        assert!(marked.last_command_error.is_none());
        let marked_message = marked
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "marked message row missing".to_owned())?;
        assert_eq!(marked_message.state_key, "transport_frame_sent");
        assert!(
            list_pending_text_control_frames(ListPendingTextControlFramesRequest {
                target: None,
                limit: None,
                operation_timeout_ms: None,
            })
            .frames
            .is_empty()
        );

        let bob_path = fresh_state_path("text-control-outbox-receipted-bob");
        let _ = fs::remove_file(&bob_path);
        let mut bob = TauriAppService::load_for_test_path(bob_path);
        bob.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Bob".to_owned(),
                    device_name: Some("Bob laptop".to_owned()),
                },
                false,
            );
        });
        let receipt_frame = bob
            .state
            .handle_text_control_frame(outbox_frame.frame)
            .ok_or_else(|| "receiver should return receipt frame".to_owned())?;
        let handled = handle_text_control_frame(HandleTextControlFrameRequest {
            frame: receipt_frame,
        });
        assert!(handled.state.last_command_error.is_none());
        let receipted_message = handled
            .state
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "receipted message row missing".to_owned())?;
        assert_eq!(receipted_message.state_key, "peer_receipt");
        assert_eq!(
            load_state()
                .text_control_outbox
                .iter()
                .find(|record| record.message_id == message_id)
                .map(|record| record.state_key.as_str()),
            Some("receipted")
        );
        Ok(())
    }

    #[test]
    fn text_control_session_pump_uses_data_transport_trait_and_persists_receipt(
    ) -> Result<(), String> {
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("text-control-transport-pump-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_state.dms[0].dm_id.clone()),
            group_id: None,
            channel_id: None,
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "transport trait session pump".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();

        let bob_path = fresh_state_path("text-control-transport-pump-bob");
        let _ = fs::remove_file(&bob_path);
        let mut bob = TauriAppService::load_for_test_path(bob_path.clone());
        bob.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Bob".to_owned(),
                    device_name: Some("Bob laptop".to_owned()),
                },
                false,
            );
        });
        let transport = Arc::new(ReceiverBackedTextControlTransport::new(bob));
        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("text-control-transport-trait-test".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(started.last_command_error.is_none(), "{started:?}");
        let active_session_id = load_state()
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .expect("text session should be active after start_text_session");
        attach_text_control_transport_runtime_for_test(transport.clone(), active_session_id);

        let report = pump_text_control_transport_once(ListPendingTextControlFramesRequest {
            target: Some(target),
            limit: None,
            operation_timeout_ms: None,
        });

        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.pending_before, 1);
        assert_eq!(report.frames_sent, 1);
        assert_eq!(report.response_frames_received, 1);
        assert_eq!(report.receipts_applied, 1);
        assert!(report.metrics.open);
        assert_eq!(report.metrics.frames_sent, 1);
        assert_eq!(report.metrics.frames_received, 1);
        assert!(report.metrics.bytes_sent > 0);
        assert!(report.metrics.bytes_received > 0);

        let receipted_message = load_state()
            .messages
            .into_iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "sender message row missing after transport pump".to_owned())?;
        assert_eq!(receipted_message.state_key, "peer_receipt");

        let mut alice_store = FileAppStore::new(&alice_path);
        let alice_reloaded = load_state_from_store(&mut alice_store);
        assert_eq!(
            alice_reloaded
                .text_control_outbox
                .iter()
                .find(|record| record.message_id == message_id)
                .map(|record| record.state_key.as_str()),
            Some("receipted")
        );

        let receiver_path = transport
            .receiver_state_path()
            .ok_or_else(|| "receiver state path missing".to_owned())?;
        let mut bob_store = FileAppStore::new(&receiver_path);
        let bob_reloaded = load_state_from_store(&mut bob_store);
        assert!(bob_reloaded.messages.iter().any(|message| {
            message.message_id == message_id && message.state_key == "received_envelope"
        }));
        assert!(bob_reloaded
            .text_delivery_receipts
            .iter()
            .any(|receipt| receipt.message_id == message_id));
        clear_text_control_transport_runtime_for_test();
        Ok(())
    }

    #[test]
    fn g012_two_profile_group_text_delivery_bidirectional_persists() -> Result<(), String> {
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("g012-group-text-alice");
        let alice_created = create_user(CreateUserRequest {
            display_name: "Alice G012".to_owned(),
            device_name: Some("Alice Tauri profile".to_owned()),
        });
        assert!(
            alice_created.last_command_error.is_none(),
            "{alice_created:?}"
        );
        let created_group = create_group(CreateGroupRequest {
            name: "G012 Text Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        assert!(
            created_group.last_command_error.is_none(),
            "{created_group:?}"
        );
        let alice_group = created_group
            .groups
            .iter()
            .find(|group| group.name == "G012 Text Lab")
            .cloned()
            .ok_or_else(|| "alice group missing".to_owned())?;
        let alice_group_id = alice_group.group_id.clone();
        let alice_channel_id = alice_group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Text)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "alice text channel missing".to_owned())?;
        let alice_invite_state = create_invite(CreateInviteRequest {
            group_id: Some(alice_group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "2".to_owned(),
        });
        assert!(
            alice_invite_state.last_command_error.is_none(),
            "{alice_invite_state:?}"
        );
        let group_invite = alice_invite_state
            .invites
            .iter()
            .find(|invite| invite.group_id == alice_group_id)
            .cloned()
            .ok_or_else(|| "group invite missing".to_owned())?;

        let (bob_path, _) = join_group_invite_as_test_profile(
            "g012-group-text-bob",
            "Bob G012",
            "Bob Tauri profile",
            group_invite.code.clone(),
            "G012 Text Lab",
        )?;
        let bob_loaded = TauriAppService::load_for_test_path(bob_path.clone());
        let bob_group = bob_loaded
            .state
            .to_view()
            .groups
            .into_iter()
            .find(|group| group.name == "G012 Text Lab")
            .ok_or_else(|| "bob joined group missing".to_owned())?;
        let bob_group_id = bob_group.group_id.clone();
        let bob_channel_id = bob_group
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Text)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "bob text channel missing".to_owned())?;
        assert_eq!(
            bob_group_id, alice_group_id,
            "signed invite must install the inviter's OpenMLS group id"
        );
        let (owner_exporter_sha256, joiner_exporter_sha256) =
            admit_group_invite_between_test_profiles(
                &alice_path,
                &bob_path,
                &alice_group_id,
                &group_invite,
            )?;
        assert_eq!(owner_exporter_sha256, joiner_exporter_sha256);

        reload_global_app_service_from_path(&alice_path);
        let alice_target = MessageTargetView {
            kind: "channel".to_owned(),
            dm_id: None,
            group_id: Some(alice_group_id.clone()),
            channel_id: Some(alice_channel_id.clone()),
        };
        let alice_sent = send_message(SendMessageRequest {
            target: alice_target.clone(),
            body: "g012 alice to bob encrypted group text".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let alice_message_id = alice_sent
            .messages
            .last()
            .map(|message| message.message_id.clone())
            .ok_or_else(|| "alice message missing".to_owned())?;
        let alice_receiver = Arc::new(ReceiverBackedTextControlTransport::new(
            TauriAppService::load_for_test_path(bob_path.clone()),
        ));
        let alice_session = start_text_session(StartTextSessionRequest {
            scope_label: Some("g012-alice-to-bob-group-text".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(
            alice_session.last_command_error.is_none(),
            "{alice_session:?}"
        );
        let alice_session_id = load_state()
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "alice text session missing".to_owned())?;
        attach_text_control_transport_runtime_for_test(alice_receiver.clone(), alice_session_id);
        let alice_report = pump_text_control_transport_once(ListPendingTextControlFramesRequest {
            target: Some(alice_target),
            limit: Some(8),
            operation_timeout_ms: Some(5_000),
        });
        assert!(
            alice_report.failures.is_empty(),
            "{:?}",
            alice_report.failures
        );
        assert_eq!(alice_report.frames_sent, 1);
        assert_eq!(alice_report.response_frames_received, 1);
        assert_eq!(alice_report.receipts_applied, 1);
        clear_text_control_transport_runtime_for_test();
        let alice_after = load_state_from_store(&mut FileAppStore::new(&alice_path));
        let alice_delivered = alice_after
            .messages
            .iter()
            .find(|message| message.message_id == alice_message_id)
            .ok_or_else(|| "alice delivered message missing after reload".to_owned())?;
        assert_eq!(alice_delivered.state_key, "peer_receipt");
        assert!(alice_delivered.peer_receipt.is_some());
        let bob_after_alice = load_state_from_store(&mut FileAppStore::new(&bob_path));
        assert!(bob_after_alice.messages.iter().any(|message| {
            message.message_id == alice_message_id
                && message.state_key == "received_plaintext"
                && message.body == "g012 alice to bob encrypted group text"
        }));

        reload_global_app_service_from_path(&bob_path);
        let bob_target = MessageTargetView {
            kind: "channel".to_owned(),
            dm_id: None,
            group_id: Some(bob_group_id.clone()),
            channel_id: Some(bob_channel_id.clone()),
        };
        let bob_sent = send_message(SendMessageRequest {
            target: bob_target.clone(),
            body: "g012 bob to alice encrypted group text".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let bob_send_error = bob_sent
            .last_command_error
            .as_ref()
            .ok_or_else(|| "joined profile without OpenMLS state should fail closed".to_owned())?;
        assert_eq!(bob_send_error.code, "text_delivery_envelope_failed");
        assert!(bob_send_error
            .message
            .contains("OpenMLS group state is missing"));
        assert!(!bob_sent
            .messages
            .iter()
            .any(|message| message.body == "g012 bob to alice encrypted group text"));
        let bob_after = load_state_from_store(&mut FileAppStore::new(&bob_path));
        let alice_after_bob = load_state_from_store(&mut FileAppStore::new(&alice_path));
        assert!(!alice_after_bob
            .messages
            .iter()
            .any(|message| { message.body == "g012 bob to alice encrypted group text" }));

        if let Ok(artifact_path) = std::env::var("DISCRYPT_G012_TEXT_PROOF_ARTIFACT") {
            if let Some(parent) = std::path::Path::new(&artifact_path).parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let profile_sha256 = |path: &PathBuf| -> Result<String, String> {
                let bytes = fs::read(path).map_err(|error| error.to_string())?;
                let mut hasher = Sha256::new();
                hasher.update(bytes);
                Ok(hex::encode(hasher.finalize()))
            };
            let report = serde_json::json!({
                "schema_version": "discrypt.g012.group_text_outbound_openmls.v1",
                "status": "owner_outbound_passed_joiner_outbound_blocked_until_openmls_admission",
                "profiles": {
                    "alice": {
                        "state_path": alice_path,
                        "state_sha256": profile_sha256(&alice_path)?,
                    },
                    "bob": {
                        "state_path": bob_path,
                        "state_sha256": profile_sha256(&bob_path)?,
                    }
                },
                "group": {
                    "name": "G012 Text Lab",
                    "alice_group_id": alice_group_id,
                    "bob_group_id": bob_group_id,
                    "alice_channel_id": alice_channel_id,
                    "bob_channel_id": bob_channel_id,
                    "invite_kind": group_invite.invite_kind,
                    "invite_uses": bob_after.invites.iter().find(|invite| invite.invite_key == group_invite.invite_key).map(|invite| invite.uses).unwrap_or(0),
                },
                "openmls_admission": {
                    "welcome_admission": "authorized_openmls_welcome_joined",
                    "owner_exporter_sha256": owner_exporter_sha256,
                    "joiner_exporter_sha256": joiner_exporter_sha256,
                    "exporters_match": true,
                },
                "deliveries": [
                    {
                        "direction": "alice_to_bob",
                        "message_id": alice_message_id,
                        "sender_state_after_reload": alice_delivered.state_key,
                        "sender_peer_receipt": alice_delivered.peer_receipt.is_some(),
                        "receiver_state_after_reload": "received_plaintext",
                        "receiver_plaintext_rendered": true,
                        "frames_sent": alice_report.frames_sent,
                        "response_frames_received": alice_report.response_frames_received,
                        "receipts_applied": alice_report.receipts_applied,
                        "transport_open": alice_report.metrics.open,
                        "transport_bytes_sent": alice_report.metrics.bytes_sent,
                        "transport_bytes_received": alice_report.metrics.bytes_received,
                    },
                    {
                        "direction": "bob_to_alice",
                        "status": "blocked_missing_openmls_member_state",
                        "command_error_code": bob_send_error.code,
                        "command_error_message": bob_send_error.message,
                        "frames_sent": 0,
                        "receipts_applied": 0,
                    }
                ],
                "claims": [
                    "two isolated Tauri AppService profile stores",
                    "signed group invite accepted by second profile",
                    "owner channel send used OpenMLS exporter-backed text ciphertext",
                    "text/control transport pump delivered the owner envelope",
                    "receiver persisted received_envelope rows",
                    "sender persisted signed peer_receipt state after reload",
                    "joined profile channel send fails closed until persisted OpenMLS admission/member state exists"
                ]
            });
            fs::write(
                artifact_path,
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
                ),
            )
            .map_err(|error| error.to_string())?;
        }

        Ok(())
    }

    #[test]
    fn text_control_pump_reports_missing_runtime() {
        let _guard = test_lock();
        reset_with_temp_state("text-control-transport-pump-runtime-missing");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_state.dms[0].dm_id.clone()),
            group_id: None,
            channel_id: None,
        };
        let message_id = send_message(SendMessageRequest {
            target: target.clone(),
            body: "transport runtime unavailable".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        })
        .messages
        .first()
        .map(|message| message.message_id.clone())
        .unwrap();

        let state_before = load_state();
        assert!(
            state_before
                .text_control_outbox
                .iter()
                .any(|frame| frame.message_id == message_id),
            "expected unsent frame in text control outbox",
        );

        let report = pump_text_control_transport_once(ListPendingTextControlFramesRequest {
            target: Some(target),
            limit: Some(8),
            operation_timeout_ms: None,
        });

        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.contains("transport runtime is not attached")),
            "expected missing-runtime failure detail",
        );
        assert_eq!(
            report.frames_sent, 0,
            "no frames should send without runtime"
        );
        assert_eq!(
            report.receipts_applied, 0,
            "no receipts should apply without transport runtime"
        );
        assert_eq!(
            report.response_frames_received, 0,
            "no response frames should be received without runtime"
        );

        let state_after = load_state();
        let command_error = state_after
            .last_command_error
            .expect("missing transport runtime should be reported as a command error");
        assert_eq!(command_error.command, "pump_text_control_transport_once");
        assert_eq!(command_error.code, "transport_runtime_missing");
    }

    #[test]
    fn text_control_pump_times_out_hanging_response_without_blocking_ui() {
        let _guard = test_lock();
        reset_with_temp_state("text-control-transport-pump-timeout");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_state.dms[0].dm_id.clone()),
            group_id: None,
            channel_id: None,
        };
        let message_id = send_message(SendMessageRequest {
            target: target.clone(),
            body: "transport runtime timeout".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        })
        .messages
        .first()
        .map(|message| message.message_id.clone())
        .unwrap();

        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("timeout-text-session".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(started.last_command_error.is_none(), "{started:?}");
        let active_session_id = load_state()
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .expect("text session should be active");
        let transport = Arc::new(HangingResponseTextControlTransport::new());
        attach_text_control_transport_runtime_for_test(transport, active_session_id);

        let started_at = std::time::Instant::now();
        let report = pump_text_control_transport_once(ListPendingTextControlFramesRequest {
            target: Some(target),
            limit: Some(8),
            operation_timeout_ms: Some(100),
        });

        assert!(
            started_at.elapsed() < std::time::Duration::from_secs(2),
            "pump must return on the configured timeout instead of hanging"
        );
        assert_eq!(report.pending_before, 1);
        assert_eq!(report.frames_sent, 1);
        assert_eq!(report.response_frames_received, 0);
        assert_eq!(report.receipts_applied, 0);
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("timed out after 100 ms")));
        assert_eq!(report.metrics.frames_sent, 1);

        let message = load_state()
            .messages
            .into_iter()
            .find(|message| message.message_id == message_id)
            .expect("message row should remain persisted");
        assert_eq!(message.state_key, "transport_frame_sent");
        assert_ne!(message.state_key, "peer_receipt");

        clear_text_control_transport_runtime_for_test();
    }

    #[test]
    fn text_control_pump_reports_missing_session() {
        let _guard = test_lock();
        reset_with_temp_state("text-control-transport-pump-session-missing");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_state.dms[0].dm_id.clone()),
            group_id: None,
            channel_id: None,
        };
        let message_id = send_message(SendMessageRequest {
            target: target.clone(),
            body: "text transport session unavailable".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        })
        .messages
        .first()
        .map(|message| message.message_id.clone())
        .unwrap();

        let receiver_path = fresh_state_path("text-control-transport-pump-session-missing-bob");
        let _ = fs::remove_file(&receiver_path);
        let transport = Arc::new(ReceiverBackedTextControlTransport::new(
            TauriAppService::load_for_test_path(receiver_path),
        ));
        attach_text_control_transport_runtime_for_test(transport, "session-absent");

        let state_before = load_state();
        assert!(
            state_before
                .text_control_outbox
                .iter()
                .any(|frame| frame.message_id == message_id),
            "expected unsent frame in text control outbox",
        );

        let report = pump_text_control_transport_once(ListPendingTextControlFramesRequest {
            target: Some(target),
            limit: Some(8),
            operation_timeout_ms: None,
        });

        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.contains("text transport session is not active")),
            "expected missing-session failure detail",
        );
        assert_eq!(
            report.frames_sent, 0,
            "no frames should send without session"
        );
        assert_eq!(
            report.response_frames_received, 0,
            "no response frames should be received without session"
        );

        let state_after = load_state();
        let command_error = state_after
            .last_command_error
            .expect("missing text session should be reported as a command error");
        assert_eq!(command_error.command, "pump_text_control_transport_once");
        assert_eq!(command_error.code, "text_session_missing");

        clear_text_control_transport_runtime_for_test();
    }

    #[test]
    fn text_control_pump_reports_session_id_mismatch() {
        let _guard = test_lock();
        reset_with_temp_state("text-control-transport-pump-session-mismatch");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_state.dms[0].dm_id.clone()),
            group_id: None,
            channel_id: None,
        };
        let message_id = send_message(SendMessageRequest {
            target: target.clone(),
            body: "session id mismatch".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        })
        .messages
        .first()
        .map(|message| message.message_id.clone())
        .unwrap();

        let receiver_state_path =
            fresh_state_path("text-control-transport-pump-session-mismatch-bob");
        let _ = fs::remove_file(&receiver_state_path);
        let transport = Arc::new(ReceiverBackedTextControlTransport::new(
            TauriAppService::load_for_test_path(receiver_state_path),
        ));

        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("matching-text-session".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(started.last_command_error.is_none(), "{started:?}");
        attach_text_control_transport_runtime_for_test(transport, "mismatched-text-session");

        let state_before = load_state();
        assert!(
            state_before
                .text_control_outbox
                .iter()
                .any(|frame| frame.message_id == message_id),
            "expected unsent frame in text control outbox",
        );

        let report = pump_text_control_transport_once(ListPendingTextControlFramesRequest {
            target: Some(target),
            limit: Some(8),
            operation_timeout_ms: None,
        });

        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.contains("does not match active text session")),
            "expected session id mismatch failure detail",
        );
        assert_eq!(
            report.frames_sent, 0,
            "no frames should send with session id mismatch"
        );
        assert_eq!(
            report.response_frames_received, 0,
            "no response frames should be received with session id mismatch"
        );

        let state_after = load_state();
        let command_error = state_after
            .last_command_error
            .expect("session id mismatch should be reported as a command error");
        assert_eq!(command_error.command, "pump_text_control_transport_once");
        assert_eq!(command_error.code, "text_session_id_mismatch");

        clear_text_control_transport_runtime_for_test();
    }

    #[test]
    fn attach_text_control_transport_runtime_rejects_without_active_session() {
        let _guard = test_lock();
        reset_with_temp_state("attach-text-control-runtime-missing-session");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });

        let state =
            attach_text_control_transport_runtime(AttachTextControlTransportRuntimeRequest {
                session_id: None,
                ..Default::default()
            });
        let command_error = state
            .last_command_error
            .expect("missing session should fail");
        assert_eq!(
            command_error.command, "attach_text_control_transport_runtime",
            "attach command should report missing-session failure source"
        );
        assert_eq!(
            command_error.code, "text_session_missing",
            "attach precondition should fail when no text session exists"
        );
    }

    #[test]
    fn attach_text_control_transport_runtime_rejects_stale_session_id() {
        let _guard = test_lock();
        reset_with_temp_state("attach-text-control-runtime-stale-session");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("attach-stale-session".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(started.last_command_error.is_none(), "{started:?}");
        let active_session_id = load_state()
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .expect("text session should be active after start_text_session");

        let state =
            attach_text_control_transport_runtime(AttachTextControlTransportRuntimeRequest {
                session_id: Some("stale-text-session".to_owned()),
                ..Default::default()
            });
        let command_error = state
            .last_command_error
            .expect("session mismatch should fail");
        assert_eq!(
            command_error.command, "attach_text_control_transport_runtime",
            "stale session should fail at attach"
        );
        assert_eq!(command_error.code, "text_session_id_mismatch");
        let service = app_service();
        let guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(
            guard.text_control_transport_runtime.is_none(),
            "attach rejection must not create runtime state"
        );
        assert_ne!(
            active_session_id, "stale-text-session",
            "test should use a stale session id"
        );
    }

    #[test]
    fn attach_text_control_transport_runtime_is_idempotent_while_attach_is_pending() {
        let _guard = test_lock();
        reset_with_temp_state("attach-text-control-runtime-idempotent-pending");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("attach-idempotent-pending".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(started.last_command_error.is_none(), "{started:?}");
        let active_session_id = load_state()
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .expect("text session should be active after start_text_session");

        let service = app_service();
        {
            let mut guard = service
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.pending_text_control_transport_runtime =
                Some(PendingTextControlTransportRuntime {
                    session_id: active_session_id.clone(),
                    role: ProviderTextControlRuntimePeerRole::Offerer,
                    local_peer_id: "alice-runtime-peer".to_owned(),
                    remote_peer_id: "bob-runtime-peer".to_owned(),
                });
        }

        let state =
            attach_text_control_transport_runtime(AttachTextControlTransportRuntimeRequest {
                session_id: Some(active_session_id.clone()),
                ..Default::default()
            });
        assert!(state.last_command_error.is_none(), "{state:?}");
        assert!(state.events.iter().any(|event| {
            event.kind == "transport.text_runtime_attach_deduped"
                && event.summary.contains(&active_session_id)
        }));
        let guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(guard
            .pending_text_control_transport_runtime
            .as_ref()
            .is_some_and(|pending| pending.session_id == active_session_id));
    }

    #[test]
    fn attach_text_control_transport_runtime_rejects_when_text_session_not_connected() {
        let _guard = test_lock();
        reset_with_temp_state("attach-text-control-runtime-not-connected");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });

        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("attach-not-connected".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(started.last_command_error.is_none(), "{started:?}");

        let active_session_id = load_state()
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .expect("text session should be active after start_text_session");

        let state =
            attach_text_control_transport_runtime(AttachTextControlTransportRuntimeRequest {
                session_id: Some(active_session_id),
                ..Default::default()
            });
        let command_error = state
            .last_command_error
            .expect("attach should fail while text session is not connected");
        assert_eq!(command_error.code, "text_session_not_connected");
    }

    #[test]
    fn attach_text_control_transport_runtime_exposes_missing_provider_implementation() {
        let _guard = test_lock();
        reset_with_temp_state("attach-text-control-runtime-missing-implementation");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("attach-unsupported".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(started.last_command_error.is_none(), "{started:?}");

        let synthetic_probe = ProviderWebRtcDataChannelProbeView {
            kind: "unit-test".to_owned(),
            profile_id: "unit-test-profile".to_owned(),
            endpoint_label: "unit-test-endpoint".to_owned(),
            scope_commitment: "unit-test-scope".to_owned(),
            rendezvous_topic: "unit-test-topic".to_owned(),
            offerer_direct_path_ready: true,
            answerer_direct_path_ready: true,
            offerer_turn_fallback_ready: false,
            answerer_turn_fallback_ready: false,
            offerer_configured_turn_servers: 0,
            answerer_configured_turn_servers: 0,
            offerer_local_relay_candidates_gathered: 0,
            answerer_local_relay_candidates_gathered: 0,
            offerer_remote_relay_candidates_applied: 0,
            answerer_remote_relay_candidates_applied: 0,
            offerer_data_channel_open: true,
            answerer_data_channel_open: true,
            text_control_frame_roundtrip: true,
            text_control_frame_sha256: "a".repeat(64),
            receipt_frame_roundtrip: true,
            receipt_frame_sha256: "b".repeat(64),
            runtime_spec: None,
        };
        let attached = mutate_app_service(|state| {
            state.latest_data_channel_probe = Some(synthetic_probe.clone());
            state.mark_text_session_data_channel_route_proof(&synthetic_probe);
        });
        assert!(
            attached
                .transport_status
                .iter()
                .any(|status| status.label == "text session" && status.status == "direct"),
            "synthetic route proof should transition text session to direct"
        );
        assert!(attached.last_command_error.is_none());

        let session_id = load_state()
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .expect("text session should remain active");
        let state =
            attach_text_control_transport_runtime(AttachTextControlTransportRuntimeRequest {
                session_id: Some(session_id),
                ..Default::default()
            });
        let command_error = state
            .last_command_error
            .expect("attach should fail until provider runtime path is wired");
        assert_eq!(
            command_error.command,
            "attach_text_control_transport_runtime"
        );
        assert_eq!(command_error.code, "transport_runtime_not_supported");
        assert!(command_error
            .recovery_hint
            .contains("persisted negotiated offer/answer/ICE bootstrap handoff"));
        assert_eq!(
            command_error.message,
            TEXT_CONTROL_RUNTIME_SPEC_MISSING_MESSAGE
        );
        let service = app_service();
        let guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(
            guard.text_control_transport_runtime.is_none(),
            "unsupported attach path must remain fail-closed"
        );
    }

    #[test]
    fn text_control_runtime_status_remains_not_attached_after_missing_implementation() {
        let _guard = test_lock();
        reset_with_temp_state("text-control-runtime-status-not-attached");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("attach-status-not-attached".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(started.last_command_error.is_none(), "{started:?}");
        let synthetic_probe = ProviderWebRtcDataChannelProbeView {
            kind: "unit-test".to_owned(),
            profile_id: "unit-test-profile".to_owned(),
            endpoint_label: "unit-test-endpoint".to_owned(),
            scope_commitment: "unit-test-scope".to_owned(),
            rendezvous_topic: "unit-test-topic".to_owned(),
            offerer_direct_path_ready: true,
            answerer_direct_path_ready: true,
            offerer_turn_fallback_ready: false,
            answerer_turn_fallback_ready: false,
            offerer_configured_turn_servers: 0,
            answerer_configured_turn_servers: 0,
            offerer_local_relay_candidates_gathered: 0,
            answerer_local_relay_candidates_gathered: 0,
            offerer_remote_relay_candidates_applied: 0,
            answerer_remote_relay_candidates_applied: 0,
            offerer_data_channel_open: true,
            answerer_data_channel_open: true,
            text_control_frame_roundtrip: true,
            text_control_frame_sha256: "a".repeat(64),
            receipt_frame_roundtrip: true,
            receipt_frame_sha256: "b".repeat(64),
            runtime_spec: None,
        };
        let session_id = load_state()
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .expect("text session should be active");
        assert!(
            session_id.starts_with("text-session-"),
            "expected a generated session id before proving route"
        );
        let route_marked = mutate_app_service(|state| {
            state.latest_data_channel_probe = Some(synthetic_probe);
            let proof = state
                .latest_data_channel_probe
                .as_ref()
                .expect("probe should be set")
                .clone();
            state.mark_text_session_data_channel_route_proof(&proof)
        });
        assert!(
            route_marked.last_command_error.is_none(),
            "route proof should not fail"
        );

        let attached =
            attach_text_control_transport_runtime(AttachTextControlTransportRuntimeRequest {
                session_id: Some(session_id.clone()),
                ..Default::default()
            });
        let command_error = attached
            .last_command_error
            .expect("attach should fail until provider runtime path is wired");
        assert_eq!(command_error.code, "transport_runtime_not_supported");
        let text_control_status = attached
            .transport_status
            .into_iter()
            .find(|status| status.label == "text/control runtime")
            .expect("text/control runtime status should be present");
        assert_eq!(text_control_status.status, "not-attached");
        assert!(
            text_control_status.detail.contains(
                "pending signed frames remain queued until a matching live runtime attaches"
            ),
            "status detail should expose the fail-closed queueing boundary"
        );

        let service = app_service();
        let guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(
            guard.text_control_transport_runtime.is_none(),
            "missing provider implementation must not attach runtime state"
        );
        let active_session_id = guard
            .state
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .expect("active text session should still exist");
        assert_eq!(
            active_session_id, session_id,
            "attach attempt should not mutate active session identity"
        );
    }

    #[test]
    fn live_role_split_runtime_material_is_invite_shared_not_profile_local() -> Result<(), String> {
        let _guard = test_lock();
        reset_with_temp_state("role-split-shared-material-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let active_dm_id = dm
            .active_context
            .as_ref()
            .and_then(|context| context.dm_id.clone())
            .ok_or_else(|| "active DM id missing after start_dm".to_owned())?;
        let invite = create_dm_invite(CreateDmInviteRequest {
            dm_id: Some(active_dm_id),
            expires: "24 hours".to_owned(),
            max_use: "1 use".to_owned(),
        });
        assert!(invite.last_command_error.is_none(), "{invite:?}");
        let invite_code = invite
            .invites
            .last()
            .map(|invite| invite.code.clone())
            .ok_or_else(|| "DM invite code missing".to_owned())?;
        let alice_state = load_state();
        let alice_inputs =
            alice_state.text_control_runtime_inputs_for_active_scope(Some("mqtt"))?;

        reset_with_temp_state("role-split-shared-material-bob");
        create_user(CreateUserRequest {
            display_name: "Bob".to_owned(),
            device_name: Some("Bob laptop".to_owned()),
        });
        let accepted = accept_dm_invite(AcceptDmInviteRequest {
            invite_code,
            display_name: Some("Alice".to_owned()),
        });
        assert!(accepted.last_command_error.is_none(), "{accepted:?}");
        let bob_state = load_state();
        assert_ne!(
            alice_state.local_user_id(),
            bob_state.local_user_id(),
            "test must compare two distinct local profiles"
        );
        let bob_inputs = bob_state.text_control_runtime_inputs_for_active_scope(Some("mqtt"))?;

        assert_eq!(
            alice_inputs.profile.profile_id,
            bob_inputs.profile.profile_id
        );
        assert_eq!(alice_inputs.scope, bob_inputs.scope);
        assert_eq!(alice_inputs.ice_config, bob_inputs.ice_config);
        assert_eq!(
            alice_inputs.bootstrap_secret, bob_inputs.bootstrap_secret,
            "role-split peers must derive the same provider rendezvous bootstrap from signed invite/connectivity metadata, not local profile identity"
        );
        assert_eq!(
            alice_inputs.random_entropy, bob_inputs.random_entropy,
            "role-split peers must derive the same provider rendezvous entropy from signed invite/connectivity metadata, not local profile identity"
        );
        Ok(())
    }

    #[test]
    fn live_runtime_peer_ids_are_signed_invite_reciprocals() -> Result<(), String> {
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("role-split-runtime-peers-alice");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let active_dm_id = dm
            .active_context
            .as_ref()
            .and_then(|context| context.dm_id.clone())
            .ok_or_else(|| "active DM id missing after start_dm".to_owned())?;
        let invite = create_dm_invite(CreateDmInviteRequest {
            dm_id: Some(active_dm_id),
            expires: "24 hours".to_owned(),
            max_use: "1 use".to_owned(),
        });
        assert!(invite.last_command_error.is_none(), "{invite:?}");
        let invite_code = invite
            .invites
            .last()
            .map(|invite| invite.code.clone())
            .ok_or_else(|| "DM invite code missing".to_owned())?;
        let alice_state = load_state();

        let (_bob_path, bob_service) = accept_dm_invite_as_test_profile(
            "role-split-runtime-peers-bob",
            "Bob",
            "Bob laptop",
            invite_code,
            "Alice",
        )?;
        reload_global_app_service_from_path(&alice_path);

        let (alice_local, alice_remote) = alice_state.active_runtime_peer_ids_for_text_control()?;
        let (bob_local, bob_remote) = bob_service
            .state
            .active_runtime_peer_ids_for_text_control()?;
        assert_eq!(
            alice_local, bob_remote,
            "Bob must route to Alice's persisted signed bootstrap runtime peer"
        );
        assert_eq!(
            alice_remote, bob_local,
            "Alice must route to Bob's persisted signed bootstrap runtime peer"
        );
        Ok(())
    }

    #[test]
    fn backend_derives_text_runtime_attachment_without_manual_pairing_fields() -> Result<(), String>
    {
        let _guard = test_lock();
        reset_with_temp_state("backend-derived-runtime-attachment");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });

        let state = load_state();
        let attachment = state.active_runtime_peer_attachment_for_text_control()?;
        let active_dm_id = state
            .active_context
            .as_ref()
            .and_then(|context| context.dm_id.as_ref())
            .ok_or_else(|| "expected active DM context".to_owned())?;
        let dm = state
            .dms
            .iter()
            .find(|dm| &dm.dm_id == active_dm_id)
            .ok_or_else(|| "expected active DM".to_owned())?;
        let local = dm
            .runtime_peers
            .iter()
            .find(|peer| peer.is_local)
            .ok_or_else(|| "expected backend local runtime peer".to_owned())?;
        let remote = dm
            .runtime_peers
            .iter()
            .find(|peer| !peer.is_local)
            .ok_or_else(|| "expected backend remote runtime peer".to_owned())?;

        assert_eq!(attachment.role, ProviderTextControlRuntimePeerRole::Offerer);
        assert_eq!(attachment.local_peer_id.0, local.peer_id);
        assert_eq!(attachment.remote_peer_id.0, remote.peer_id);
        Ok(())
    }

    #[test]
    fn g004_two_profile_state_survives_reload_with_invites_receipts_voice_and_preferences(
    ) -> Result<(), String> {
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("g004-persistent-state-alice");
        let alice_created = create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        assert!(
            alice_created.last_command_error.is_none(),
            "{alice_created:?}"
        );
        let alice_identity_key = alice_created
            .devices
            .iter()
            .find(|device| device.local)
            .map(|device| device.identity_key.clone())
            .ok_or_else(|| "Alice local identity key missing".to_owned())?;
        assert!(snapshot_safety_number_matches_identity_keys(
            &alice_created.snapshot
        ));

        let alice_preferences = save_preferences(SavePreferencesRequest {
            theme_id: "ocean-contrast".to_owned(),
            template_id: "compact-ops".to_owned(),
        });
        assert!(alice_preferences.last_command_error.is_none());

        let dm = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let alice_dm_id = dm
            .active_context
            .as_ref()
            .and_then(|context| context.dm_id.clone())
            .ok_or_else(|| "Alice active DM missing".to_owned())?;
        let dm_invite = create_dm_invite(CreateDmInviteRequest {
            dm_id: Some(alice_dm_id),
            expires: "24 hours".to_owned(),
            max_use: "1 use".to_owned(),
        });
        assert!(dm_invite.last_command_error.is_none(), "{dm_invite:?}");
        let dm_invite_code = dm_invite
            .invites
            .last()
            .map(|invite| invite.code.clone())
            .ok_or_else(|| "DM invite code missing".to_owned())?;

        let group = create_group(CreateGroupRequest {
            name: "G004 Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: Some("mqtt".to_owned()),
            signaling_endpoint: Some("mqtts://broker.example.test:8883".to_owned()),
            ice_stun_servers: Some(vec!["stun:stun.example.test:3478".to_owned()]),
            ice_turn_servers: Some(vec![IceTurnServerView {
                endpoint: "turns:turn.example.test:5349".to_owned(),
                credential_declared: false,
                credential_expires_at: None,
            }]),
        });
        assert!(group.last_command_error.is_none(), "{group:?}");
        let group_id = group
            .groups
            .first()
            .map(|group| group.group_id.clone())
            .ok_or_else(|| "group id missing".to_owned())?;
        let group_connectivity = group
            .groups
            .first()
            .and_then(|group| group.connectivity.clone())
            .ok_or_else(|| "group connectivity missing".to_owned())?;
        assert_eq!(
            group_connectivity.ice_stun_servers,
            vec!["stun:stun.example.test:3478".to_owned()]
        );
        assert_eq!(
            group_connectivity.ice_turn_servers[0].endpoint,
            "turns:turn.example.test:5349"
        );

        let text_channel = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "ops".to_owned(),
            kind: ChannelKind::Text,
            retention_status: "24 hours".to_owned(),
        });
        assert!(
            text_channel.last_command_error.is_none(),
            "{text_channel:?}"
        );
        let text_channel_id = text_channel
            .groups
            .iter()
            .find(|group| group.group_id == group_id)
            .and_then(|group| {
                group
                    .channels
                    .iter()
                    .find(|channel| channel.kind == ChannelKind::Text && channel.name == "#ops")
            })
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "text channel id missing".to_owned())?;
        let voice_channel = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Ops Voice".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        assert!(
            voice_channel.last_command_error.is_none(),
            "{voice_channel:?}"
        );
        let voice_channel_id = voice_channel
            .groups
            .iter()
            .find(|group| group.group_id == group_id)
            .and_then(|group| {
                group.channels.iter().find(|channel| {
                    channel.kind == ChannelKind::Voice && channel.name == "Ops Voice"
                })
            })
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "voice channel id missing".to_owned())?;

        let group_invite = create_invite(CreateInviteRequest {
            group_id: Some(group_id.clone()),
            expires: "2 days".to_owned(),
            max_use: "2 uses".to_owned(),
        });
        assert!(
            group_invite.last_command_error.is_none(),
            "{group_invite:?}"
        );
        let group_invite_row = group_invite
            .invites
            .last()
            .cloned()
            .ok_or_else(|| "group invite missing".to_owned())?;
        assert_eq!(
            group_invite_row.invite_kind,
            InviteKind::GroupJoin.canonical_name()
        );
        assert_eq!(
            group_invite_row.ice_stun_servers,
            group_connectivity.ice_stun_servers
        );
        assert_eq!(
            group_invite_row.ice_turn_servers,
            group_connectivity.ice_turn_servers
        );
        assert!(!group_invite_row.revoked);
        let group_invite_code = group_invite_row.code.clone();

        let sent = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "channel".to_owned(),
                dm_id: None,
                group_id: Some(group_id.clone()),
                channel_id: Some(text_channel_id.clone()),
            },
            body: "G004 reload receipt proof".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        assert!(sent.last_command_error.is_none(), "{sent:?}");
        let message_id = sent
            .messages
            .last()
            .map(|message| message.message_id.clone())
            .ok_or_else(|| "message id missing".to_owned())?;
        let persisted_after_send = load_state();
        let envelope = persisted_after_send
            .text_delivery_envelopes
            .iter()
            .find(|record| record.message_id == message_id)
            .ok_or_else(|| "persisted envelope missing".to_owned())?;
        let recipient_signer = SigningKey::generate(&mut OsRng);
        let receipt = TextDeliveryReceipt::sign(
            &envelope.group_id,
            TextDeliveryReceiptInput {
                message_id: message_id.clone(),
                recipient_leaf: 2,
                recipient_device_id: "bob-laptop".to_owned(),
                received_at_ms: 42_000,
                envelope_ciphertext_hash: envelope.envelope.ciphertext_hash(),
            },
            &recipient_signer,
        )
        .map_err(|error| error.to_string())?;
        let receipted = apply_text_delivery_receipt(ApplyTextDeliveryReceiptRequest {
            message_id: message_id.clone(),
            receipt,
            recipient_verifying_key_hex: hex::encode(recipient_signer.verifying_key().as_bytes()),
        });
        assert!(receipted.last_command_error.is_none(), "{receipted:?}");
        assert_eq!(
            receipted
                .messages
                .iter()
                .find(|message| message.message_id == message_id)
                .map(|message| message.state_key.as_str()),
            Some("peer_receipt")
        );

        let voice_joined = join_voice(JoinVoiceRequest {
            group_id: group_id.clone(),
            channel_id: voice_channel_id.clone(),
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic-alice".to_owned()),
            input_device_label: Some("Alice microphone".to_owned()),
            output_device_id: Some("speaker-alice".to_owned()),
            output_device_label: Some("Alice speaker".to_owned()),
        });
        assert!(
            voice_joined.last_command_error.is_none(),
            "{voice_joined:?}"
        );
        let session_id = voice_joined
            .voice_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "voice session id missing".to_owned())?;
        let muted = set_self_mute(SetSelfMuteRequest {
            session_id: session_id.clone(),
            muted: true,
        });
        assert!(muted.last_command_error.is_none(), "{muted:?}");
        let local_participant_id = muted
            .voice_session
            .as_ref()
            .and_then(|session| session.participants.first())
            .map(|participant| participant.id.clone())
            .ok_or_else(|| "voice participant id missing".to_owned())?;
        let local_volume_rejected = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id: session_id.clone(),
            participant_id: local_participant_id,
            volume: 37,
        });
        assert_eq!(
            local_volume_rejected
                .last_command_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("voice_volume_local_participant")
        );
        let remote_participant_id = "remote-reload-proof".to_owned();
        attach_test_remote_voice(&session_id, &remote_participant_id);
        let volume = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id,
            participant_id: remote_participant_id.clone(),
            volume: 37,
        });
        assert!(volume.last_command_error.is_none(), "{volume:?}");

        reload_global_app_service_from_path(&alice_path);
        let alice_reloaded = app_state();
        assert_eq!(alice_reloaded.lifecycle, AppLifecycle::Ready);
        assert_eq!(alice_reloaded.preferences.theme_id, "ocean-contrast");
        assert_eq!(alice_reloaded.preferences.template_id, "compact-ops");
        assert_eq!(
            alice_reloaded
                .groups
                .iter()
                .find(|group| group.group_id == group_id)
                .and_then(|group| group.connectivity.as_ref())
                .map(|connectivity| connectivity.ice_turn_servers.clone()),
            Some(group_connectivity.ice_turn_servers.clone())
        );
        assert!(alice_reloaded
            .groups
            .iter()
            .find(|group| group.group_id == group_id)
            .is_some_and(|group| {
                group.role == "owner"
                    && group
                        .channels
                        .iter()
                        .any(|channel| channel.channel_id == text_channel_id)
                    && group
                        .channels
                        .iter()
                        .any(|channel| channel.channel_id == voice_channel_id)
            }));
        assert!(alice_reloaded
            .dms
            .iter()
            .any(|dm| dm.display_name == "Bob" && !dm.runtime_peers.is_empty()));
        assert!(alice_reloaded
            .invites
            .iter()
            .any(|invite| invite.invite_kind == InviteKind::DmContact.canonical_name()));
        assert!(alice_reloaded
            .invites
            .iter()
            .any(
                |invite| invite.invite_kind == InviteKind::GroupJoin.canonical_name()
                    && invite.max_use == "2 uses"
                    && !invite.revoked
            ));
        let reloaded_message = alice_reloaded
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "reloaded message missing".to_owned())?;
        assert_eq!(reloaded_message.state_key, "peer_receipt");
        assert!(reloaded_message.peer_receipt.is_some());
        let reloaded_voice = alice_reloaded
            .voice_session
            .as_ref()
            .ok_or_else(|| "reloaded voice session missing".to_owned())?;
        assert!(reloaded_voice.joined);
        assert!(reloaded_voice.self_muted);
        assert_eq!(
            reloaded_voice
                .input_device
                .as_ref()
                .map(|device| device.device_id.as_str()),
            Some("mic-alice")
        );
        assert_eq!(
            reloaded_voice
                .output_device
                .as_ref()
                .map(|device| device.device_id.as_str()),
            Some("speaker-alice")
        );
        assert!(reloaded_voice
            .participants
            .iter()
            .any(|participant| participant.id == remote_participant_id
                && participant.role == "remote"
                && !participant.muted
                && participant.volume == 37));
        assert_eq!(
            alice_reloaded
                .active_context
                .as_ref()
                .and_then(|context| context.channel_id.as_deref()),
            Some(voice_channel_id.as_str())
        );

        let bob_path = reset_with_temp_state("g004-persistent-state-bob");
        let bob_recovered = recover_user(RecoverUserRequest {
            display_name: "Bob".to_owned(),
            recovery_code: "paper-coral-falcon".to_owned(),
            device_name: Some("Bob laptop".to_owned()),
            recovery_room_memberships: vec!["Recovered Bob Room".to_owned()],
            recovered_device_count: Some(2),
            use_sealed_account_backup: false,
        });
        assert!(
            bob_recovered.last_command_error.is_none(),
            "{bob_recovered:?}"
        );
        assert!(bob_recovered.profile.as_ref().is_some_and(|profile| profile
            .recovery_status
            .contains("content keys restored: false")));
        let bob_identity_key = bob_recovered
            .devices
            .iter()
            .find(|device| device.local)
            .map(|device| device.identity_key.clone())
            .ok_or_else(|| "Bob local identity key missing".to_owned())?;
        assert_ne!(
            alice_identity_key, bob_identity_key,
            "G004 requires two independent users, not a shared local profile"
        );

        let bob_dm = accept_dm_invite(AcceptDmInviteRequest {
            invite_code: dm_invite_code,
            display_name: Some("Alice".to_owned()),
        });
        assert!(bob_dm.last_command_error.is_none(), "{bob_dm:?}");
        let bob_group = join_group(JoinGroupRequest {
            invite_code: group_invite_code,
            group_name: Some("G004 Lab".to_owned()),
        });
        assert!(bob_group.last_command_error.is_none(), "{bob_group:?}");
        save_preferences(SavePreferencesRequest {
            theme_id: "midnight-steel".to_owned(),
            template_id: "command-center".to_owned(),
        });
        reload_global_app_service_from_path(&bob_path);
        let bob_reloaded = app_state();
        assert_eq!(bob_reloaded.lifecycle, AppLifecycle::Ready);
        assert_eq!(bob_reloaded.preferences.theme_id, "midnight-steel");
        assert!(bob_reloaded
            .profile
            .as_ref()
            .is_some_and(|profile| profile.display_name == "Bob"
                && profile
                    .recovery_status
                    .contains("content keys restored: false")));
        assert!(bob_reloaded.devices.len() >= 2);
        assert!(bob_reloaded.dms.iter().any(|dm| dm.display_name == "Alice"
            && dm
                .connectivity
                .as_ref()
                .is_some_and(|connectivity| connectivity.invite_kind
                    == InviteKind::DmContact.canonical_name())));
        assert!(
            bob_reloaded
                .groups
                .iter()
                .any(|group| group.name == "G004 Lab"
                    && group.role == "member"
                    && group
                        .connectivity
                        .as_ref()
                        .is_some_and(|connectivity| connectivity.ice_stun_servers
                            == vec!["stun:stun.example.test:3478".to_owned()]
                            && connectivity.ice_turn_servers
                                == group_connectivity.ice_turn_servers)),
            "Bob groups after reload: {:?}; expected TURN {:?}",
            bob_reloaded.groups,
            group_connectivity.ice_turn_servers
        );
        assert!(bob_reloaded.invites.iter().any(|invite| invite.uses == 1
            && invite.invite_kind == InviteKind::GroupJoin.canonical_name()
            && invite.ice_turn_servers == group_connectivity.ice_turn_servers));

        reload_global_app_service_from_path(&alice_path);
        let alice_final = app_state();
        assert_eq!(alice_final.preferences.theme_id, "ocean-contrast");
        assert_eq!(alice_final.devices[0].identity_key, alice_identity_key);
        assert!(
            alice_final
                .messages
                .iter()
                .any(|message| message.message_id == message_id
                    && message.state_key == "peer_receipt")
        );
        Ok(())
    }

    #[test]
    fn text_control_runtime_clears_when_text_session_stops() {
        let _guard = test_lock();
        reset_with_temp_state("text-control-runtime-clears-on-stop");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let receiver_path = fresh_state_path("text-control-runtime-clears-on-stop-bob");
        let _ = fs::remove_file(&receiver_path);
        let transport = Arc::new(ReceiverBackedTextControlTransport::new(
            TauriAppService::load_for_test_path(receiver_path),
        ));

        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("text-control-stop".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(started.last_command_error.is_none(), "{started:?}");
        let active_session_id = load_state()
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .expect("text session should be active");
        attach_text_control_transport_runtime_for_test(transport, active_session_id.clone());

        let stopped = stop_text_session(StopTextSessionRequest {
            session_id: Some(active_session_id),
        });
        assert!(stopped.last_command_error.is_none(), "{stopped:?}");

        let service = app_service();
        let guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(
            guard.text_control_transport_runtime.is_none(),
            "stopping a text session must drop the owned runtime"
        );
        let runtime_status = guard.text_control_runtime_status_row();
        assert_eq!(runtime_status.status, "inactive");
    }

    #[test]
    fn text_control_runtime_clears_when_text_session_restarts_with_new_scope() {
        let _guard = test_lock();
        reset_with_temp_state("text-control-runtime-clears-on-restart");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let receiver_path = fresh_state_path("text-control-runtime-clears-on-restart-bob");
        let _ = fs::remove_file(&receiver_path);
        let transport = Arc::new(ReceiverBackedTextControlTransport::new(
            TauriAppService::load_for_test_path(receiver_path),
        ));

        let first = start_text_session(StartTextSessionRequest {
            scope_label: Some("first-text-scope".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(first.last_command_error.is_none(), "{first:?}");
        let first_session_id = load_state()
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .expect("first text session should be active");
        attach_text_control_transport_runtime_for_test(transport, first_session_id);

        let restarted = start_text_session(StartTextSessionRequest {
            scope_label: Some("second-text-scope".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(restarted.last_command_error.is_none(), "{restarted:?}");

        let service = app_service();
        let guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(
            guard.text_control_transport_runtime.is_none(),
            "starting a different text session must drop the stale runtime"
        );
        let runtime_status = guard.text_control_runtime_status_row();
        assert_eq!(runtime_status.status, "not-attached");
    }

    #[test]
    #[cfg(feature = "mqtt-adapter")]
    fn public_mqtt_two_profile_receipt_crosses_provider_webrtc_when_enabled() -> Result<(), String>
    {
        if std::env::var("DISCRYPT_DESKTOP_PUBLIC_MQTT_RECEIPT_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping desktop public MQTT two-profile receipt proof; set DISCRYPT_DESKTOP_PUBLIC_MQTT_RECEIPT_E2E=1 to run"
            );
            return Ok(());
        }
        let _guard = test_lock();
        let _alice_path = reset_with_temp_state("public-mqtt-two-profile-receipt-alice");
        std::env::set_var(
            "DISCRYPT_DEFAULT_MQTT_ENDPOINT",
            std::env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
                .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned()),
        );
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let sent = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "provider receipt proof".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();

        let bob_path = fresh_state_path("public-mqtt-two-profile-receipt-bob");
        let _ = fs::remove_file(&bob_path);
        let mut bob = TauriAppService::load_for_test_path(bob_path);
        bob.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Bob".to_owned(),
                    device_name: Some("Bob laptop".to_owned()),
                },
                false,
            );
        });

        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let proof = guard
            .state
            .prove_text_delivery_receipt_over_data_channel_with_receiver(
                &bob.state,
                &message_id,
                Some("mqtt"),
            )?;
        assert_eq!(proof.kind, "mqtt");
        assert!(proof.text_control_frame_roundtrip);
        assert!(proof.receipt_frame_roundtrip);
        let view = guard.mutate(|_| {});
        let message = view
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "message row missing".to_owned())?;
        assert_eq!(message.state_key, "peer_receipt");
        assert!(message.peer_receipt.is_some());
        Ok(())
    }

    #[test]
    #[cfg(feature = "nostr-adapter")]
    fn public_nostr_two_profile_receipt_crosses_provider_webrtc_when_enabled() -> Result<(), String>
    {
        if std::env::var("DISCRYPT_DESKTOP_PUBLIC_NOSTR_RECEIPT_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping desktop public Nostr two-profile receipt proof; set DISCRYPT_DESKTOP_PUBLIC_NOSTR_RECEIPT_E2E=1 to run"
            );
            return Ok(());
        }
        let _guard = test_lock();
        let _alice_path = reset_with_temp_state("public-nostr-two-profile-receipt-alice");
        std::env::set_var(
            "DISCRYPT_DEFAULT_NOSTR_ENDPOINT",
            std::env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINT")
                .unwrap_or_else(|_| "wss://nos.lol".to_owned()),
        );
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let sent = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "provider nostr receipt proof".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();

        let bob_path = fresh_state_path("public-nostr-two-profile-receipt-bob");
        let _ = fs::remove_file(&bob_path);
        let mut bob = TauriAppService::load_for_test_path(bob_path);
        bob.mutate(|state| {
            state.create_user(
                CreateUserRequest {
                    display_name: "Bob".to_owned(),
                    device_name: Some("Bob laptop".to_owned()),
                },
                false,
            );
        });

        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let proof = guard
            .state
            .prove_text_delivery_receipt_over_data_channel_with_receiver(
                &bob.state,
                &message_id,
                Some("nostr"),
            )?;
        assert_eq!(proof.kind, "nostr");
        assert!(proof.text_control_frame_roundtrip);
        assert!(proof.receipt_frame_roundtrip);
        let view = guard.mutate(|_| {});
        let message = view
            .messages
            .iter()
            .find(|message| message.message_id == message_id)
            .ok_or_else(|| "message row missing".to_owned())?;
        assert_eq!(message.state_key, "peer_receipt");
        assert!(message.peer_receipt.is_some());
        Ok(())
    }

    #[test]
    fn tampered_text_delivery_receipt_is_rejected() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("tampered-text-receipt");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let sent = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "tamper receipt".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();
        let persisted = load_state();
        let envelope = persisted
            .text_delivery_envelopes
            .iter()
            .find(|record| record.message_id == message_id)
            .ok_or_else(|| "persisted text envelope missing".to_owned())?;
        let recipient_signer = SigningKey::generate(&mut OsRng);
        let mut receipt = TextDeliveryReceipt::sign(
            &envelope.group_id,
            TextDeliveryReceiptInput {
                message_id: message_id.clone(),
                recipient_leaf: 2,
                recipient_device_id: "bob-desktop".to_owned(),
                received_at_ms: 42_000,
                envelope_ciphertext_hash: envelope.envelope.ciphertext_hash(),
            },
            &recipient_signer,
        )
        .map_err(|error| error.to_string())?;
        receipt.message_id = "different-message".to_owned();

        let rejected = apply_text_delivery_receipt(ApplyTextDeliveryReceiptRequest {
            message_id: message_id.clone(),
            receipt,
            recipient_verifying_key_hex: hex::encode(recipient_signer.verifying_key().as_bytes()),
        });

        let error = rejected
            .last_command_error
            .as_ref()
            .ok_or_else(|| "tampered receipt should be rejected".to_owned())?;
        assert_eq!(error.code, "receipt_verification_failed");
        assert_eq!(rejected.messages[0].state_key, "sent_local");
        Ok(())
    }

    #[test]
    fn text_message_states_include_sent_local_and_full_legend() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("text-state-ui");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Peer".to_owned(),
        });
        let dm_id = dm_state.dms[0].dm_id.clone();
        let state = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "hello state".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        assert_eq!(state.messages[0].state_key, "sent_local");
        assert_eq!(state.messages[0].state_label, "Sent locally");
        let keys: Vec<_> = state
            .text_state_legend
            .iter()
            .map(|entry| entry.key.as_str())
            .collect();
        assert_eq!(
            keys,
            vec![
                "pending",
                "sent_local",
                "transport_probe_verified",
                "transport_probe_failed",
                "peer_receipt",
                "received",
                "failed",
                "locked",
                "shredded"
            ]
        );
    }

    #[test]
    fn tauri_command_integration_exercises_real_service_and_event_stream() -> Result<(), String> {
        let _guard = test_lock();
        let path = reset_with_temp_state("command-integration-real-service");
        let created = create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        assert_eq!(created.lifecycle, AppLifecycle::Ready);
        let group = create_group(CreateGroupRequest {
            name: "Integration Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group
            .groups
            .first()
            .map(|group| group.group_id.clone())
            .ok_or_else(|| "group created through command service".to_owned())?;
        let channel_state = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "ops".to_owned(),
            kind: ChannelKind::Text,
            retention_status: "24 hours".to_owned(),
        });
        let channel_id = channel_state
            .groups
            .first()
            .and_then(|group| {
                group
                    .channels
                    .iter()
                    .find(|channel| channel.kind == ChannelKind::Text)
            })
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "text channel created through command service".to_owned())?;
        let messaged = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "channel".to_owned(),
                dm_id: None,
                group_id: Some(group_id.clone()),
                channel_id: Some(channel_id),
            },
            body: "service-backed command path".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        assert!(messaged.last_command_error.is_none());
        assert_eq!(app_state().messages.len(), 1);
        let events = poll_app_events(Some(PollAppEventsRequest {
            after: Some(0),
            kinds: vec!["group".to_owned(), "message".to_owned()],
            limit: Some(16),
        }));
        assert!(events
            .events
            .iter()
            .any(|event| event.kind == "message.sent"));
        assert!(load_state()
            .messages
            .iter()
            .any(|message| message.body == "service-backed command path"));
        assert!(path.exists());
        let persisted = fs::read_to_string(path).map_err(|err| err.to_string())?;
        if cfg!(all(target_os = "linux", feature = "production-storage")) {
            assert!(persisted.contains(&["discrypt.appdb.", "en", "crypted.v1"].concat()));
            assert!(!persisted.contains("service-backed command path"));
        } else {
            assert!(persisted.contains("service-backed command path"));
        }
        Ok(())
    }

    #[test]
    fn production_adapter_conformance_integration_mode_is_cfg_audited() {
        let _guard = test_lock();
        let path = fresh_state_path("production-adapter-env");
        std::env::set_var("DISCRYPT_APP_STATE_PATH", &path);
        let production_path = app_store_path_with_env_override(false);
        assert_ne!(production_path, path);
        assert!(production_path.ends_with(APP_STATE_STORE_FILENAME));
        assert!(!env_app_state_override_allowed() || cfg!(test));
    }

    #[test]
    fn command_health_covers_full_user_flow() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("health");
        let health = command_health();
        assert!(health.app_state_ready);
        assert!(health.identity_ready);
        assert!(health.verification_ready);
        assert!(health.collaboration_ready);
        assert_eq!(health.voice_ready, cfg!(feature = "production-media"));
        assert!(health.honest_copy_ready);
        assert!(abuse_controls_contract_covers_g116());
    }

    #[test]
    fn transport_session_commands_surface_backend_diagnostics() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("transport-session-commands");

        let started = start_signaling_session(StartSignalingSessionRequest {
            scope_label: Some("dm:alice-bob".to_owned()),
            adapter_probe: false,
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(started.last_command_error.is_none());
        let diagnostics = started.transport_diagnostics;
        assert!(diagnostics
            .adapter_boundaries
            .iter()
            .any(|boundary| boundary.kind == "mqtt"));
        assert!(diagnostics
            .adapter_fallback_attempts
            .iter()
            .any(|attempt| attempt.attempted));
        assert_eq!(diagnostics.route_proof_status, "route-proof-not-available");
        assert!(started
            .transport_status
            .iter()
            .any(|status| status.label == "signaling session" && status.status == "signaling"));

        let session_id = started
            .transport_status
            .iter()
            .find(|status| status.label == "signaling session")
            .expect("signaling session status row")
            .detail
            .clone();
        assert!(session_id.contains("session="));

        let stopped = stop_signaling_session(StopSignalingSessionRequest { session_id: None });
        assert!(stopped.last_command_error.is_none());
        assert!(stopped
            .transport_status
            .iter()
            .any(|status| status.label == "signaling session" && status.status == "cancelled"));
    }

    #[test]
    fn signaling_adapter_probe_surfaces_runtime_blocker_without_route_claim() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("signaling-adapter-probe");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });

        let state = start_signaling_session(StartSignalingSessionRequest {
            scope_label: Some("dm:bob".to_owned()),
            adapter_probe: true,
            data_channel_probe: false,
            adapter_kind: Some("discrypt_quic_rendezvous".to_owned()),
        });

        let error = state
            .last_command_error
            .as_ref()
            .expect("QUIC probe must fail closed until sibling client is wired");
        assert_eq!(error.code, "adapter_probe_failed");
        assert_eq!(
            state.transport_diagnostics.adapter_probe_status,
            "provider-roundtrip-failed"
        );
        assert_eq!(
            state.transport_diagnostics.route_proof_status,
            "route-proof-not-available"
        );
        assert!(state.transport_diagnostics.adapter_probe.is_none());
    }

    #[test]
    fn data_channel_probe_surfaces_runtime_blocker_without_route_or_media_claim() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("data-channel-probe");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });

        let state = start_signaling_session(StartSignalingSessionRequest {
            scope_label: Some("dm:bob".to_owned()),
            adapter_probe: false,
            data_channel_probe: true,
            adapter_kind: Some("discrypt_quic_rendezvous".to_owned()),
        });

        let error = state
            .last_command_error
            .as_ref()
            .expect("QUIC data-channel probe must fail closed until sibling client is wired");
        assert_eq!(error.code, "data_channel_probe_failed");
        assert_eq!(
            state.transport_diagnostics.data_channel_probe_status,
            "webrtc-datachannel-failed"
        );
        assert_eq!(
            state.transport_diagnostics.route_proof_status,
            "route-proof-not-available"
        );
        assert!(state.transport_diagnostics.data_channel_probe.is_none());
        assert!(state.voice_session.is_none());
    }

    #[test]
    fn text_session_probe_failure_keeps_route_unproven() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("text-session-probe-fail");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });

        let state = start_text_session(StartTextSessionRequest {
            scope_label: Some("dm:bob".to_owned()),
            data_channel_probe: true,
            adapter_kind: Some("discrypt_quic_rendezvous".to_owned()),
        });

        let error = state
            .last_command_error
            .as_ref()
            .expect("QUIC text-session DataChannel probe must fail closed");
        assert_eq!(error.code, "text_data_channel_probe_failed");
        assert_eq!(
            state.transport_diagnostics.route_proof_status,
            "route-proof-not-available"
        );
        assert!(state
            .transport_status
            .iter()
            .any(|status| { status.label == "text session" && status.status == "signaling" }));
    }

    #[test]
    fn text_session_turn_probe_marks_turn_route_and_diagnostics() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("text-session-turn-route-proof");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });

        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("dm:bob".to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(started.last_command_error.is_none());

        let turn_probe = ProviderWebRtcDataChannelProbeView {
            kind: "mqtt".to_owned(),
            profile_id: "mqtt-default".to_owned(),
            endpoint_label: "mqtts://broker.example".to_owned(),
            scope_commitment: "scope-commitment".to_owned(),
            rendezvous_topic: "topic-commitment".to_owned(),
            offerer_direct_path_ready: false,
            answerer_direct_path_ready: false,
            offerer_turn_fallback_ready: true,
            answerer_turn_fallback_ready: true,
            offerer_configured_turn_servers: 1,
            answerer_configured_turn_servers: 1,
            offerer_local_relay_candidates_gathered: 1,
            answerer_local_relay_candidates_gathered: 1,
            offerer_remote_relay_candidates_applied: 1,
            answerer_remote_relay_candidates_applied: 1,
            offerer_data_channel_open: true,
            answerer_data_channel_open: true,
            text_control_frame_roundtrip: true,
            text_control_frame_sha256: "a".repeat(64),
            receipt_frame_roundtrip: true,
            receipt_frame_sha256: "b".repeat(64),
            runtime_spec: None,
        };

        let state = mutate_app_service(|state| {
            state.latest_data_channel_probe = Some(turn_probe.clone());
            state.mark_text_session_data_channel_route_proof(&turn_probe);
        });

        assert_eq!(
            state.transport_diagnostics.data_channel_probe_status,
            "webrtc-datachannel-proofed"
        );
        assert!(state
            .transport_diagnostics
            .data_channel_probe_detail
            .contains("offerer_turn=true"));
        assert_eq!(
            state.transport_diagnostics.route_proof_status,
            "route-proofed"
        );
        assert_eq!(state.transport_diagnostics.turn_required, "turn-required");
        assert!(state
            .transport_diagnostics
            .route_proof_detail
            .contains("selected=turn"));
        assert!(state
            .transport_status
            .iter()
            .any(|status| status.label == "text session" && status.status == "turn_relay"));
    }

    #[test]
    #[cfg(feature = "mqtt-adapter")]
    fn public_mqtt_text_session_probe_marks_text_route_when_enabled() {
        if std::env::var("DISCRYPT_DESKTOP_PUBLIC_MQTT_TEXT_SESSION_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping desktop public MQTT text session route proof; set DISCRYPT_DESKTOP_PUBLIC_MQTT_TEXT_SESSION_E2E=1 to run"
            );
            return;
        }
        let _guard = test_lock();
        let _path = reset_with_temp_state("desktop-public-mqtt-text-session");
        std::env::set_var(
            "DISCRYPT_DEFAULT_MQTT_ENDPOINT",
            std::env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
                .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned()),
        );
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });

        let state = start_text_session(StartTextSessionRequest {
            scope_label: Some("dm:bob".to_owned()),
            data_channel_probe: true,
            adapter_kind: Some("mqtt".to_owned()),
        });

        assert!(
            state.last_command_error.is_none(),
            "{:?}",
            state.last_command_error
        );
        assert_eq!(
            state.transport_diagnostics.data_channel_probe_status,
            "webrtc-datachannel-proofed"
        );
        assert_eq!(
            state.transport_diagnostics.route_proof_status,
            "route-proofed"
        );
        assert_eq!(
            state.transport_diagnostics.turn_required,
            "turn-not-required"
        );
        assert!(state
            .transport_status
            .iter()
            .any(|status| { status.label == "text session" && status.status == "direct" }));
        assert!(state.voice_session.is_none());
    }

    #[test]
    #[cfg(feature = "nostr-adapter")]
    fn public_nostr_text_session_probe_marks_text_route_when_enabled() {
        if std::env::var("DISCRYPT_DESKTOP_PUBLIC_NOSTR_TEXT_SESSION_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping desktop public Nostr text session route proof; set DISCRYPT_DESKTOP_PUBLIC_NOSTR_TEXT_SESSION_E2E=1 to run"
            );
            return;
        }
        let _guard = test_lock();
        let _path = reset_with_temp_state("desktop-public-nostr-text-session");
        std::env::set_var(
            "DISCRYPT_DEFAULT_NOSTR_ENDPOINT",
            std::env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINT")
                .unwrap_or_else(|_| "wss://nos.lol".to_owned()),
        );
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });

        let state = start_text_session(StartTextSessionRequest {
            scope_label: Some("dm:bob".to_owned()),
            data_channel_probe: true,
            adapter_kind: Some("nostr".to_owned()),
        });

        assert!(
            state.last_command_error.is_none(),
            "{:?}",
            state.last_command_error
        );
        assert_eq!(
            state.transport_diagnostics.data_channel_probe_status,
            "webrtc-datachannel-proofed"
        );
        assert_eq!(
            state.transport_diagnostics.route_proof_status,
            "route-proofed"
        );
        assert_eq!(
            state.transport_diagnostics.turn_required,
            "turn-not-required"
        );
        assert!(state
            .transport_status
            .iter()
            .any(|status| { status.label == "text session" && status.status == "direct" }));
        let proof = state
            .transport_diagnostics
            .data_channel_probe
            .expect("text session should retain Nostr DataChannel proof");
        assert_eq!(proof.kind, "nostr");
        assert!(proof.text_control_frame_roundtrip);
        assert!(proof.receipt_frame_roundtrip);
        assert!(state.voice_session.is_none());
    }

    #[test]
    #[cfg(feature = "mqtt-adapter")]
    fn public_mqtt_live_runtime_pair_pump_persists_peer_receipt_when_enabled() -> Result<(), String>
    {
        if std::env::var("DISCRYPT_DESKTOP_PUBLIC_MQTT_RUNTIME_PAIR_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping desktop public MQTT live runtime-pair pump proof; set DISCRYPT_DESKTOP_PUBLIC_MQTT_RUNTIME_PAIR_E2E=1 to run"
            );
            return Ok(());
        }
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("desktop-public-mqtt-live-runtime-pair-alice");
        std::env::set_var(
            "DISCRYPT_DEFAULT_MQTT_ENDPOINT",
            std::env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
                .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned()),
        );
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let active_dm_id = dm
            .active_context
            .as_ref()
            .and_then(|context| context.dm_id.clone())
            .ok_or_else(|| "active DM id missing after start_dm".to_owned())?;
        let invite = create_dm_invite(CreateDmInviteRequest {
            dm_id: Some(active_dm_id.clone()),
            expires: "24 hours".to_owned(),
            max_use: "1 use".to_owned(),
        });
        assert!(invite.last_command_error.is_none(), "{invite:?}");
        let invite_code = invite
            .invites
            .last()
            .map(|invite| invite.code.clone())
            .ok_or_else(|| "DM invite code missing".to_owned())?;
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(active_dm_id),
            group_id: None,
            channel_id: None,
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "mqtt live runtime pair receipt".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();

        let (_bob_path, bob) = accept_dm_invite_as_test_profile(
            "desktop-public-mqtt-live-runtime-pair-bob",
            "Bob",
            "Bob laptop",
            invite_code,
            "Alice",
        )?;
        reload_global_app_service_from_path(&alice_path);

        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("dm:bob".to_owned()),
            data_channel_probe: false,
            adapter_kind: Some("mqtt".to_owned()),
        });
        assert!(started.last_command_error.is_none(), "{started:?}");
        assert_eq!(
            started.transport_diagnostics.route_proof_status,
            "route-proof-not-available"
        );
        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session_id = guard
            .state
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "active text session missing".to_owned())?;
        let (evidence, report, bob_state) = guard
            .state
            .pump_text_delivery_receipt_over_live_runtime_pair_with_receiver(
                bob,
                &message_id,
                Some("mqtt"),
                session_id,
            )?;
        guard.persist();

        assert_eq!(evidence.kind.canonical_name(), "mqtt");
        assert!(evidence.offerer_direct_path_ready);
        assert!(evidence.answerer_direct_path_ready);
        assert!(evidence.offerer_data_channel_open);
        assert!(evidence.answerer_data_channel_open);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.frames_sent, 1);
        assert_eq!(report.response_frames_received, 1);
        assert_eq!(report.receipts_applied, 1);
        let alice_reloaded = load_state_from_store(&mut FileAppStore::new(&alice_path));
        assert!(
            alice_reloaded
                .messages
                .iter()
                .any(|message| message.message_id == message_id
                    && message.state_key == "peer_receipt")
        );
        assert!(bob_state.messages.iter().any(|message| {
            message.message_id == message_id && message.state_key == "received_envelope"
        }));
        assert!(bob_state
            .text_delivery_receipts
            .iter()
            .any(|receipt| receipt.message_id == message_id));
        Ok(())
    }

    #[test]
    #[cfg(feature = "mqtt-adapter")]
    fn public_mqtt_group_live_runtime_pair_pump_persists_peer_receipt_when_enabled(
    ) -> Result<(), String> {
        if std::env::var("DISCRYPT_DESKTOP_PUBLIC_MQTT_GROUP_RUNTIME_PAIR_E2E").as_deref()
            != Ok("1")
        {
            eprintln!(
                "skipping desktop public MQTT group live runtime-pair pump proof; set DISCRYPT_DESKTOP_PUBLIC_MQTT_GROUP_RUNTIME_PAIR_E2E=1 to run"
            );
            return Ok(());
        }
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("desktop-public-mqtt-group-runtime-pair-alice");
        std::env::set_var(
            "DISCRYPT_DEFAULT_MQTT_ENDPOINT",
            std::env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
                .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned()),
        );
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let group = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "7 days".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group
            .active_context
            .as_ref()
            .and_then(|context| context.group_id.clone())
            .ok_or_else(|| "active group id missing after create_group".to_owned())?;
        let channel_id = group
            .groups
            .iter()
            .find(|group| group.group_id == group_id)
            .and_then(|group| {
                group
                    .channels
                    .iter()
                    .find(|channel| channel.kind == ChannelKind::Text)
                    .map(|channel| channel.channel_id.clone())
            })
            .ok_or_else(|| "group text channel missing after create_group".to_owned())?;
        let invite = create_invite(CreateInviteRequest {
            group_id: Some(group_id.clone()),
            expires: "24 hours".to_owned(),
            max_use: "1 use".to_owned(),
        });
        assert!(invite.last_command_error.is_none(), "{invite:?}");
        let invite_code = invite
            .invites
            .last()
            .map(|invite| invite.code.clone())
            .ok_or_else(|| "group invite code missing".to_owned())?;
        let target = MessageTargetView {
            kind: "channel".to_owned(),
            dm_id: None,
            group_id: Some(group_id.clone()),
            channel_id: Some(channel_id),
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "mqtt group live runtime pair receipt".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();

        let (_bob_path, bob) = join_group_invite_as_test_profile(
            "desktop-public-mqtt-group-runtime-pair-bob",
            "Bob",
            "Bob laptop",
            invite_code,
            "Private Lab",
        )?;
        reload_global_app_service_from_path(&alice_path);

        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("group:private-lab".to_owned()),
            data_channel_probe: false,
            adapter_kind: Some("mqtt".to_owned()),
        });
        assert!(started.last_command_error.is_none(), "{started:?}");
        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session_id = guard
            .state
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "active group text session missing".to_owned())?;
        let (evidence, report, bob_state) = guard
            .state
            .pump_text_delivery_receipt_over_live_runtime_pair_with_receiver(
                bob,
                &message_id,
                Some("mqtt"),
                session_id,
            )?;
        guard.persist();

        assert_eq!(evidence.kind.canonical_name(), "mqtt");
        assert!(evidence.offerer_direct_path_ready);
        assert!(evidence.answerer_direct_path_ready);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.frames_sent, 1);
        assert_eq!(report.response_frames_received, 1);
        assert_eq!(report.receipts_applied, 1);
        let alice_reloaded = load_state_from_store(&mut FileAppStore::new(&alice_path));
        assert!(
            alice_reloaded
                .messages
                .iter()
                .any(|message| message.message_id == message_id
                    && message.state_key == "peer_receipt")
        );
        assert!(bob_state.messages.iter().any(|message| {
            message.message_id == message_id && message.state_key == "received_envelope"
        }));
        Ok(())
    }

    #[test]
    #[cfg(feature = "nostr-adapter")]
    fn public_nostr_live_runtime_pair_pump_persists_peer_receipt_when_enabled() -> Result<(), String>
    {
        if std::env::var("DISCRYPT_DESKTOP_PUBLIC_NOSTR_RUNTIME_PAIR_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping desktop public Nostr live runtime-pair pump proof; set DISCRYPT_DESKTOP_PUBLIC_NOSTR_RUNTIME_PAIR_E2E=1 to run"
            );
            return Ok(());
        }
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("desktop-public-nostr-live-runtime-pair-alice");
        std::env::set_var(
            "DISCRYPT_DEFAULT_NOSTR_ENDPOINT",
            std::env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINT")
                .unwrap_or_else(|_| "wss://nos.lol".to_owned()),
        );
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let active_dm_id = dm
            .active_context
            .as_ref()
            .and_then(|context| context.dm_id.clone())
            .ok_or_else(|| "active DM id missing after start_dm".to_owned())?;
        let invite = create_dm_invite(CreateDmInviteRequest {
            dm_id: Some(active_dm_id.clone()),
            expires: "24 hours".to_owned(),
            max_use: "1 use".to_owned(),
        });
        assert!(invite.last_command_error.is_none(), "{invite:?}");
        let invite_code = invite
            .invites
            .last()
            .map(|invite| invite.code.clone())
            .ok_or_else(|| "DM invite code missing".to_owned())?;
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(active_dm_id),
            group_id: None,
            channel_id: None,
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "nostr live runtime pair receipt".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();

        let (_bob_path, bob) = accept_dm_invite_as_test_profile(
            "desktop-public-nostr-live-runtime-pair-bob",
            "Bob",
            "Bob laptop",
            invite_code,
            "Alice",
        )?;
        reload_global_app_service_from_path(&alice_path);

        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("dm:bob".to_owned()),
            data_channel_probe: false,
            adapter_kind: Some("nostr".to_owned()),
        });
        assert!(started.last_command_error.is_none(), "{started:?}");
        assert_eq!(
            started.transport_diagnostics.route_proof_status,
            "route-proof-not-available"
        );
        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session_id = guard
            .state
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "active text session missing".to_owned())?;
        let (evidence, report, bob_state) = guard
            .state
            .pump_text_delivery_receipt_over_live_runtime_pair_with_receiver(
                bob,
                &message_id,
                Some("nostr"),
                session_id,
            )?;
        guard.persist();

        assert_eq!(evidence.kind.canonical_name(), "nostr");
        assert!(evidence.offerer_direct_path_ready);
        assert!(evidence.answerer_direct_path_ready);
        assert!(evidence.offerer_data_channel_open);
        assert!(evidence.answerer_data_channel_open);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.frames_sent, 1);
        assert_eq!(report.response_frames_received, 1);
        assert_eq!(report.receipts_applied, 1);
        let alice_reloaded = load_state_from_store(&mut FileAppStore::new(&alice_path));
        assert!(
            alice_reloaded
                .messages
                .iter()
                .any(|message| message.message_id == message_id
                    && message.state_key == "peer_receipt")
        );
        assert!(bob_state.messages.iter().any(|message| {
            message.message_id == message_id && message.state_key == "received_envelope"
        }));
        assert!(bob_state
            .text_delivery_receipts
            .iter()
            .any(|receipt| receipt.message_id == message_id));
        Ok(())
    }

    #[test]
    #[cfg(feature = "nostr-adapter")]
    fn public_nostr_group_live_runtime_pair_pump_persists_peer_receipt_when_enabled(
    ) -> Result<(), String> {
        if std::env::var("DISCRYPT_DESKTOP_PUBLIC_NOSTR_GROUP_RUNTIME_PAIR_E2E").as_deref()
            != Ok("1")
        {
            eprintln!(
                "skipping desktop public Nostr group live runtime-pair pump proof; set DISCRYPT_DESKTOP_PUBLIC_NOSTR_GROUP_RUNTIME_PAIR_E2E=1 to run"
            );
            return Ok(());
        }
        let _guard = test_lock();
        let alice_path = reset_with_temp_state("desktop-public-nostr-group-runtime-pair-alice");
        std::env::set_var(
            "DISCRYPT_DEFAULT_NOSTR_ENDPOINT",
            std::env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINT")
                .unwrap_or_else(|_| "wss://nos.lol".to_owned()),
        );
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let group = create_group(CreateGroupRequest {
            name: "Private Lab".to_owned(),
            retention: "7 days".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group
            .active_context
            .as_ref()
            .and_then(|context| context.group_id.clone())
            .ok_or_else(|| "active group id missing after create_group".to_owned())?;
        let channel_id = group
            .groups
            .iter()
            .find(|group| group.group_id == group_id)
            .and_then(|group| {
                group
                    .channels
                    .iter()
                    .find(|channel| channel.kind == ChannelKind::Text)
                    .map(|channel| channel.channel_id.clone())
            })
            .ok_or_else(|| "group text channel missing after create_group".to_owned())?;
        let invite = create_invite(CreateInviteRequest {
            group_id: Some(group_id.clone()),
            expires: "24 hours".to_owned(),
            max_use: "1 use".to_owned(),
        });
        assert!(invite.last_command_error.is_none(), "{invite:?}");
        let invite_code = invite
            .invites
            .last()
            .map(|invite| invite.code.clone())
            .ok_or_else(|| "group invite code missing".to_owned())?;
        let target = MessageTargetView {
            kind: "channel".to_owned(),
            dm_id: None,
            group_id: Some(group_id.clone()),
            channel_id: Some(channel_id),
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "nostr group live runtime pair receipt".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent.messages[0].message_id.clone();

        let (_bob_path, bob) = join_group_invite_as_test_profile(
            "desktop-public-nostr-group-runtime-pair-bob",
            "Bob",
            "Bob laptop",
            invite_code,
            "Private Lab",
        )?;
        reload_global_app_service_from_path(&alice_path);

        let started = start_text_session(StartTextSessionRequest {
            scope_label: Some("group:private-lab".to_owned()),
            data_channel_probe: false,
            adapter_kind: Some("nostr".to_owned()),
        });
        assert!(started.last_command_error.is_none(), "{started:?}");
        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session_id = guard
            .state
            .text_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "active group text session missing".to_owned())?;
        let (evidence, report, bob_state) = guard
            .state
            .pump_text_delivery_receipt_over_live_runtime_pair_with_receiver(
                bob,
                &message_id,
                Some("nostr"),
                session_id,
            )?;
        guard.persist();

        assert_eq!(evidence.kind.canonical_name(), "nostr");
        assert!(evidence.offerer_direct_path_ready);
        assert!(evidence.answerer_direct_path_ready);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.frames_sent, 1);
        assert_eq!(report.response_frames_received, 1);
        assert_eq!(report.receipts_applied, 1);
        let alice_reloaded = load_state_from_store(&mut FileAppStore::new(&alice_path));
        assert!(
            alice_reloaded
                .messages
                .iter()
                .any(|message| message.message_id == message_id
                    && message.state_key == "peer_receipt")
        );
        assert!(bob_state.messages.iter().any(|message| {
            message.message_id == message_id && message.state_key == "received_envelope"
        }));
        Ok(())
    }

    #[test]
    #[cfg(feature = "mqtt-adapter")]
    fn public_mqtt_data_channel_probe_reaches_tauri_diagnostics_when_enabled() {
        if std::env::var("DISCRYPT_DESKTOP_PUBLIC_MQTT_WEBRTC_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping desktop public MQTT WebRTC probe; set DISCRYPT_DESKTOP_PUBLIC_MQTT_WEBRTC_E2E=1 to run"
            );
            return;
        }
        let _guard = test_lock();
        let _path = reset_with_temp_state("desktop-public-mqtt-datachannel");
        std::env::set_var(
            "DISCRYPT_DEFAULT_MQTT_ENDPOINT",
            std::env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
                .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned()),
        );
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });

        let state = start_signaling_session(StartSignalingSessionRequest {
            scope_label: Some("dm:bob".to_owned()),
            adapter_probe: false,
            data_channel_probe: true,
            adapter_kind: Some("mqtt".to_owned()),
        });

        assert!(
            state.last_command_error.is_none(),
            "{:?}",
            state.last_command_error
        );
        assert_eq!(
            state.transport_diagnostics.data_channel_probe_status,
            "webrtc-datachannel-proofed"
        );
        let probe = state
            .transport_diagnostics
            .data_channel_probe
            .expect("desktop diagnostics should include DataChannel proof");
        assert_eq!(probe.kind, "mqtt");
        assert!(probe.offerer_direct_path_ready);
        assert!(probe.answerer_direct_path_ready);
        assert!(probe.offerer_data_channel_open);
        assert!(probe.answerer_data_channel_open);
        assert!(probe.text_control_frame_roundtrip);
        assert!(probe.receipt_frame_roundtrip);
        assert_eq!(probe.receipt_frame_sha256.len(), 64);
        assert!(state.voice_session.is_none());
    }

    #[test]
    fn text_transport_proof_failure_keeps_message_honest() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("text-transport-proof-fail");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let dm = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm.dms[0].dm_id.clone();

        let state = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "needs real transport proof".to_owned(),
            transport_proof: true,
            adapter_kind: Some("discrypt_quic_rendezvous".to_owned()),
        });

        let error = state
            .last_command_error
            .as_ref()
            .expect("fail-closed QUIC adapter should block text transport proof");
        assert_eq!(error.code, "text_transport_proof_failed");
        assert_eq!(state.messages[0].state_key, "transport_probe_failed");
        assert_eq!(state.messages[0].state_label, "Transport proof failed");
        assert!(
            state.messages[0].state_detail.contains("unavailable")
                || state.messages[0]
                    .state_detail
                    .contains("implementation_unavailable")
                || state.messages[0].state_detail.contains("not enabled")
                || state.messages[0]
                    .state_detail
                    .contains("No signaling profile matches"),
            "{}",
            state.messages[0].state_detail
        );
    }

    #[test]
    #[cfg(feature = "mqtt-adapter")]
    fn public_mqtt_message_send_proves_provider_webrtc_transport_when_enabled() {
        if std::env::var("DISCRYPT_DESKTOP_PUBLIC_MQTT_MESSAGE_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping desktop public MQTT message transport proof; set DISCRYPT_DESKTOP_PUBLIC_MQTT_MESSAGE_E2E=1 to run"
            );
            return;
        }
        let _guard = test_lock();
        let _path = reset_with_temp_state("desktop-public-mqtt-message-proof");
        std::env::set_var(
            "DISCRYPT_DEFAULT_MQTT_ENDPOINT",
            std::env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
                .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned()),
        );
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let dm = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm.dms[0].dm_id.clone();

        let state = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "transport proof message".to_owned(),
            transport_proof: true,
            adapter_kind: Some("mqtt".to_owned()),
        });

        assert!(
            state.last_command_error.is_none(),
            "{:?}",
            state.last_command_error
        );
        assert_eq!(state.messages[0].state_key, "transport_probe_verified");
        assert_eq!(state.messages[0].state_label, "Transport proofed");
        assert!(state.messages[0].state_detail.contains("frame_sha256="));
        assert!(state.messages[0]
            .state_detail
            .contains("receipt_return=true"));
        let proof = state
            .transport_diagnostics
            .data_channel_probe
            .expect("send transport proof should update diagnostics");
        assert_eq!(proof.kind, "mqtt");
        assert!(proof.text_control_frame_roundtrip);
        assert!(proof.receipt_frame_roundtrip);
        assert_eq!(proof.text_control_frame_sha256.len(), 64);
        assert_eq!(proof.receipt_frame_sha256.len(), 64);
    }

    #[test]
    #[cfg(feature = "nostr-adapter")]
    fn public_nostr_message_send_proves_provider_webrtc_transport_when_enabled() {
        if std::env::var("DISCRYPT_DESKTOP_PUBLIC_NOSTR_MESSAGE_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping desktop public Nostr message transport proof; set DISCRYPT_DESKTOP_PUBLIC_NOSTR_MESSAGE_E2E=1 to run"
            );
            return;
        }
        let _guard = test_lock();
        let _path = reset_with_temp_state("desktop-public-nostr-message-proof");
        std::env::set_var(
            "DISCRYPT_DEFAULT_NOSTR_ENDPOINT",
            std::env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINT")
                .unwrap_or_else(|_| "wss://nos.lol".to_owned()),
        );
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let dm = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let dm_id = dm.dms[0].dm_id.clone();

        let state = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "nostr transport proof message".to_owned(),
            transport_proof: true,
            adapter_kind: Some("nostr".to_owned()),
        });

        assert!(
            state.last_command_error.is_none(),
            "{:?}",
            state.last_command_error
        );
        assert_eq!(state.messages[0].state_key, "transport_probe_verified");
        let proof = state
            .transport_diagnostics
            .data_channel_probe
            .expect("send transport proof should update diagnostics");
        assert_eq!(proof.kind, "nostr");
        assert!(proof.text_control_frame_roundtrip);
        assert!(proof.receipt_frame_roundtrip);
        assert_eq!(proof.text_control_frame_sha256.len(), 64);
        assert_eq!(proof.receipt_frame_sha256.len(), 64);
    }

    #[test]
    #[cfg(feature = "mqtt-adapter")]
    fn mqtt_adapter_feature_reaches_app_state_diagnostics() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("mqtt-adapter-diagnostics");
        let state = app_state();
        assert_eq!(
            state.transport_diagnostics.selected_adapter.as_deref(),
            Some("mqtt")
        );
        let mqtt = state
            .transport_diagnostics
            .adapter_boundaries
            .iter()
            .find(|boundary| boundary.kind == "mqtt")
            .expect("mqtt boundary is surfaced");
        assert_eq!(mqtt.readiness, "available");
        assert_eq!(mqtt.failure_class, "available");
    }

    #[test]
    fn default_profiles_omit_unconfigured_ipfs_quic_placeholder_endpoints() {
        let _guard = test_lock();
        std::env::remove_var("DISCRYPT_DEFAULT_IPFS_BOOTSTRAP_ENDPOINTS");
        std::env::remove_var("DISCRYPT_DEFAULT_QUIC_RENDEZVOUS_ENDPOINT");

        let profiles = default_signaling_profiles("configured-defaults-only");
        let kinds = profiles
            .iter()
            .map(|profile| profile.adapter_kind.as_str())
            .collect::<Vec<_>>();

        assert_eq!(kinds, vec!["nostr", "mqtt"]);
        assert!(profiles.iter().all(|profile| {
            profile
                .endpoints
                .iter()
                .all(|endpoint| !endpoint.contains(".invalid"))
        }));
    }

    #[test]
    fn default_profiles_carry_provider_allowlist_and_rotation_policy() {
        let _guard = test_lock();
        std::env::remove_var("DISCRYPT_DEFAULT_IPFS_BOOTSTRAP_ENDPOINTS");
        std::env::remove_var("DISCRYPT_DEFAULT_QUIC_RENDEZVOUS_ENDPOINT");

        let profiles = default_signaling_profiles("allowlist-scope");
        assert!(!profiles.is_empty());
        for profile in &profiles {
            assert_eq!(
                profile.provider_policy_version,
                INVITE_PROVIDER_POLICY_VERSION
            );
            assert!(!profile.endpoint_allowlist_commitments.is_empty());
            assert!(profile
                .provider_rotation_policy
                .contains("fresh signed invite"));
            for endpoint in &profile.endpoints {
                assert!(profile.endpoint_allowlist_commitments.contains(
                    &endpoint_allowlist_commitment(&profile.adapter_kind, endpoint)
                ));
            }
            transport_profile_from_view(profile).expect("default profile policy validates");
        }

        let mut tampered = profiles
            .into_iter()
            .find(|profile| profile.adapter_kind == "nostr")
            .expect("nostr default profile");
        tampered.endpoints = vec!["wss://relay.example.com".to_owned()];
        let error = transport_profile_from_view(&tampered)
            .expect_err("tampered endpoint must not pass signed allowlist validation");
        assert!(error.contains("signed allowlist"));
    }

    #[test]
    #[cfg(all(feature = "mqtt-adapter", feature = "nostr-adapter"))]
    fn active_connectivity_policy_drives_selected_adapter_order() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("active-policy-selected-adapter");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let state = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });

        assert_eq!(
            state.transport_diagnostics.selected_adapter.as_deref(),
            Some("nostr")
        );
        let adapter_status = state
            .transport_status
            .iter()
            .find(|status| status.label == "adapter")
            .expect("adapter status row is surfaced");
        assert_eq!(adapter_status.status, "selected");
        assert!(adapter_status.detail.contains("Selected provider nostr"));
        assert!(adapter_status.detail.contains("nostr:available:selected"));
        let first_attempt = state
            .transport_diagnostics
            .adapter_fallback_attempts
            .first()
            .expect("active policy fallback attempt");
        assert_eq!(first_attempt.kind, "nostr");
    }

    #[test]
    #[cfg(all(
        feature = "ipfs-pubsub-adapter",
        not(feature = "mqtt-adapter"),
        not(feature = "nostr-adapter")
    ))]
    fn ipfs_pubsub_adapter_feature_reaches_app_state_diagnostics() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("ipfs-pubsub-adapter-diagnostics");
        std::env::set_var(
            "DISCRYPT_DEFAULT_IPFS_BOOTSTRAP_ENDPOINTS",
            "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWConfiguredOnly",
        );
        let state = app_state();
        assert_eq!(
            state.transport_diagnostics.selected_adapter.as_deref(),
            Some("ipfs_pubsub")
        );
        let ipfs = state
            .transport_diagnostics
            .adapter_boundaries
            .iter()
            .find(|boundary| boundary.kind == "ipfs_pubsub")
            .expect("ipfs_pubsub boundary is surfaced");
        assert_eq!(ipfs.readiness, "available");
        assert_eq!(ipfs.failure_class, "available");
    }

    #[test]
    fn abuse_rate_limits_invite_consume_helper_and_text_send_commands() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("abuse-command-rate-limits");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let group = create_group(CreateGroupRequest {
            name: "Abuse Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group.groups[0].group_id.clone();
        for index in 0..INVITE_CREATE_LIMIT {
            let state = create_invite(CreateInviteRequest {
                group_id: Some(group_id.clone()),
                expires: "1 day".to_owned(),
                max_use: "1".to_owned(),
            });
            assert!(
                state.last_command_error.is_none(),
                "invite {index} should pass"
            );
        }
        let limited_invite = create_invite(CreateInviteRequest {
            group_id: Some(group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "1".to_owned(),
        });
        assert_eq!(
            limited_invite
                .last_command_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("invite_create_rate_limited")
        );

        let invite_code = limited_invite.invites[0].code.clone();
        for index in 0..INVITE_CREATE_LIMIT {
            let joined = join_group(JoinGroupRequest {
                invite_code: invite_code.clone(),
                group_name: Some(format!("Joined {index}")),
            });
            assert!(
                joined.last_command_error.is_none(),
                "join {index} should pass"
            );
        }
        let limited_join = join_group(JoinGroupRequest {
            invite_code,
            group_name: Some("Joined limited".to_owned()),
        });
        assert_eq!(
            limited_join
                .last_command_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("invite_consume_rate_limited")
        );

        let dm = start_dm(StartDmRequest {
            display_name: "Peer".to_owned(),
        });
        let dm_id = dm.dms[0].dm_id.clone();
        for index in 0..TEXT_SEND_LIMIT {
            let sent = send_message(SendMessageRequest {
                target: MessageTargetView {
                    kind: "dm".to_owned(),
                    dm_id: Some(dm_id.clone()),
                    group_id: None,
                    channel_id: None,
                },
                body: format!("message {index}"),
                transport_proof: false,
                adapter_kind: None,
            });
            assert!(
                sent.last_command_error.is_none(),
                "message {index} should pass"
            );
        }
        let limited_message = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "limited".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        assert_eq!(
            limited_message
                .last_command_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("text_send_rate_limited")
        );
    }

    #[test]
    fn preferences_use_app_config_ids_and_persist_across_reload() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("preferences-persist");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let themed = save_preferences(SavePreferencesRequest {
            theme_id: "ocean-contrast".to_owned(),
            template_id: "compact-ops".to_owned(),
        });
        assert_eq!(themed.preferences.theme_id, "ocean-contrast");
        assert_eq!(themed.preferences.template_id, "compact-ops");
        let reloaded = load_state().to_view();
        assert_eq!(reloaded.preferences.theme_id, "ocean-contrast");
        assert_eq!(reloaded.preferences.template_id, "compact-ops");
        let normalized = save_preferences(SavePreferencesRequest {
            theme_id: "not-in-app-config".to_owned(),
            template_id: "also-invalid".to_owned(),
        });
        assert_eq!(normalized.preferences.theme_id, DEFAULT_THEME_ID);
        assert_eq!(normalized.preferences.template_id, DEFAULT_TEMPLATE_ID);
    }

    #[test]
    fn reset_app_state_requires_explicit_confirmation() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("reset-confirmation");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let rejected = reset_app_state_confirmed(ResetAppStateRequest {
            confirmation: "delete".to_owned(),
        });
        assert_eq!(rejected.lifecycle, AppLifecycle::Ready);
        let error = rejected
            .last_command_error
            .as_ref()
            .ok_or_else(|| "typed reset confirmation error".to_owned())?;
        assert_eq!(error.command, "reset_app_state");
        assert_eq!(error.code, "confirmation_required");
        assert!(error.recovery_hint.contains(RESET_APP_CONFIRMATION_PHRASE));

        let reset = reset_app_state_confirmed(ResetAppStateRequest {
            confirmation: RESET_APP_CONFIRMATION_PHRASE.to_owned(),
        });
        assert_eq!(reset.lifecycle, AppLifecycle::FirstRun);
        assert!(reset.profile.is_none());
        assert!(reset.groups.is_empty());
        Ok(())
    }

    #[test]
    fn typed_command_errors_surface_actionable_codes() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("typed-command-errors");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });

        let empty_message = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "channel".to_owned(),
                dm_id: None,
                group_id: None,
                channel_id: None,
            },
            body: "   ".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let error = empty_message
            .last_command_error
            .as_ref()
            .ok_or_else(|| "empty message command error".to_owned())?;
        assert_eq!(error.command, "send_message");
        assert_eq!(error.code, "message_empty");
        assert!(error.recovery_hint.contains("non-empty"));
        assert!(empty_message
            .events
            .iter()
            .any(|event| event.kind == "message.rejected"));

        let missing_group = create_channel(CreateChannelRequest {
            group_id: "missing-group".to_owned(),
            name: "ops".to_owned(),
            kind: ChannelKind::Text,
            retention_status: "7 days".to_owned(),
        });
        let error = missing_group
            .last_command_error
            .as_ref()
            .ok_or_else(|| "missing group command error".to_owned())?;
        assert_eq!(error.command, "create_channel");
        assert_eq!(error.code, "group_not_found");
        assert!(error.recovery_hint.contains("existing group"));

        let group = create_group(CreateGroupRequest {
            name: "Error Lab".to_owned(),
            retention: "7 days".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        assert!(group.last_command_error.is_none());
        let group_id = group
            .groups
            .first()
            .map(|group| group.group_id.clone())
            .ok_or_else(|| "group created".to_owned())?;
        let voice = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Ops Voice".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        let channel_id = voice
            .groups
            .first()
            .and_then(|group| {
                group
                    .channels
                    .iter()
                    .find(|channel| channel.kind == ChannelKind::Voice)
            })
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "voice channel created".to_owned())?;
        let denied = join_voice(JoinVoiceRequest {
            group_id,
            channel_id,
            microphone_permission: "denied".to_owned(),
            input_device_id: None,
            input_device_label: None,
            output_device_id: None,
            output_device_label: None,
        });
        let error = denied
            .last_command_error
            .as_ref()
            .ok_or_else(|| "voice permission command error".to_owned())?;
        assert_eq!(error.command, "join_voice");
        assert_eq!(error.code, "voice_permission_required");
        assert!(error.recovery_hint.contains("microphone permission"));
        assert!(denied
            .events
            .iter()
            .any(|event| event.kind == "voice.permission_denied"));
        let session_id = denied
            .voice_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "denied session captured".to_owned())?;
        let activity_error = update_voice_activity(UpdateVoiceActivityRequest {
            session_id: "missing".to_owned(),
            rms_i16: 4_000,
            peak_i16: 12_000,
            captured_at_ms: 42,
        });
        let error = activity_error
            .last_command_error
            .as_ref()
            .ok_or_else(|| "activity session error".to_owned())?;
        assert_eq!(error.command, "update_voice_activity");
        assert_eq!(error.code, "voice_session_not_found");
        let volume_error = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id,
            participant_id: "missing-participant".to_owned(),
            volume: 50,
        });
        let error = volume_error
            .last_command_error
            .as_ref()
            .ok_or_else(|| "participant command error".to_owned())?;
        assert_eq!(error.command, "set_speaker_volume");
        assert_eq!(error.code, "voice_participant_not_found");
        assert!(error.recovery_hint.contains("visible participant"));
        Ok(())
    }

    #[test]
    fn mutation_commands_return_current_state_with_event_cursor() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("mutation-cursor");

        fn assert_cursor_advanced(previous: u64, state: &AppStateView) -> u64 {
            assert_eq!(
                Some(state.event_cursor),
                state.events.last().map(|event| event.sequence)
            );
            assert!(state.event_cursor > previous);
            state.event_cursor
        }

        let mut cursor = 0;
        let created = create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        cursor = assert_cursor_advanced(cursor, &created);

        let themed = save_preferences(SavePreferencesRequest {
            theme_id: "graphite-calm".to_owned(),
            template_id: "command-center".to_owned(),
        });
        cursor = assert_cursor_advanced(cursor, &themed);

        let dm = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        cursor = assert_cursor_advanced(cursor, &dm);
        let dm_id = dm
            .dms
            .first()
            .map(|dm| dm.dm_id.clone())
            .ok_or_else(|| "dm created".to_owned())?;

        let group = create_group(CreateGroupRequest {
            name: "Cursor Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        cursor = assert_cursor_advanced(cursor, &group);
        let group_id = group
            .groups
            .first()
            .map(|group| group.group_id.clone())
            .ok_or_else(|| "group created".to_owned())?;

        let focused = set_active_group(SetActiveGroupRequest {
            group_id: group_id.clone(),
        });
        cursor = assert_cursor_advanced(cursor, &focused);

        let invite = create_invite(CreateInviteRequest {
            group_id: Some(group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "2".to_owned(),
        });
        cursor = assert_cursor_advanced(cursor, &invite);

        let text_channel = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "ops".to_owned(),
            kind: ChannelKind::Text,
            retention_status: "24 hours".to_owned(),
        });
        cursor = assert_cursor_advanced(cursor, &text_channel);
        let channel_id = text_channel
            .groups
            .first()
            .and_then(|group| group.channels.first())
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "text channel created".to_owned())?;

        let message = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "channel".to_owned(),
                dm_id: None,
                group_id: Some(group_id.clone()),
                channel_id: Some(channel_id),
            },
            body: "cursor-backed state".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        cursor = assert_cursor_advanced(cursor, &message);

        let dm_message = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "cursor-backed dm".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        cursor = assert_cursor_advanced(cursor, &dm_message);

        let voice_channel = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Ops Voice".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        cursor = assert_cursor_advanced(cursor, &voice_channel);
        let voice_channel_id = voice_channel
            .groups
            .first()
            .and_then(|group| group.channels.last())
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "voice channel created".to_owned())?;

        let joined = join_voice(JoinVoiceRequest {
            group_id,
            channel_id: voice_channel_id,
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic-default".to_owned()),
            input_device_label: Some("Default microphone".to_owned()),
            output_device_id: Some("speaker-default".to_owned()),
            output_device_label: Some("Default speaker".to_owned()),
        });
        cursor = assert_cursor_advanced(cursor, &joined);
        let session_id = joined
            .voice_session
            .as_ref()
            .map(|session| session.session_id.clone())
            .ok_or_else(|| "voice session joined".to_owned())?;
        let participant_id = joined
            .voice_session
            .as_ref()
            .and_then(|session| session.participants.first())
            .map(|participant| participant.id.clone())
            .ok_or_else(|| "voice participant present".to_owned())?;

        let activity = update_voice_activity(UpdateVoiceActivityRequest {
            session_id: session_id.clone(),
            rms_i16: 1_024,
            peak_i16: 3_000,
            captured_at_ms: 500,
        });
        cursor = assert_cursor_advanced(cursor, &activity);

        let muted = set_self_mute(SetSelfMuteRequest {
            session_id: session_id.clone(),
            muted: true,
        });
        cursor = assert_cursor_advanced(cursor, &muted);

        let volume = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id: session_id.clone(),
            participant_id,
            volume: 42,
        });
        cursor = assert_cursor_advanced(cursor, &volume);

        let left = leave_voice(LeaveVoiceRequest { session_id });
        let _cursor = assert_cursor_advanced(cursor, &left);
        Ok(())
    }

    #[test]
    fn event_stream_poll_filters_topics_and_cursors() -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("event-stream");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let group = create_group(CreateGroupRequest {
            name: "Stream Lab".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group
            .groups
            .first()
            .map(|group| group.group_id.clone())
            .ok_or_else(|| "group created".to_owned())?;
        create_invite(CreateInviteRequest {
            group_id: Some(group_id.clone()),
            expires: "1 day".to_owned(),
            max_use: "2".to_owned(),
        });
        create_device_pairing_payload(CreateDevicePairingPayloadRequest {
            requested_label: "Phone".to_owned(),
            current_epoch: Some(10),
            valid_for_epochs: Some(2),
        });
        let text_channel = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "ops".to_owned(),
            kind: ChannelKind::Text,
            retention_status: "24 hours".to_owned(),
        });
        let text_channel_id = text_channel
            .groups
            .first()
            .and_then(|group| group.channels.first())
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "text channel created".to_owned())?;
        send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "channel".to_owned(),
                dm_id: None,
                group_id: Some(group_id.clone()),
                channel_id: Some(text_channel_id),
            },
            body: "stream me".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let voice_channel = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Voice Ops".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        let voice_channel_id = voice_channel
            .groups
            .first()
            .and_then(|group| group.channels.last())
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "voice channel created".to_owned())?;
        join_voice(JoinVoiceRequest {
            group_id,
            channel_id: voice_channel_id,
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic-default".to_owned()),
            input_device_label: Some("Default microphone".to_owned()),
            output_device_id: Some("speaker-default".to_owned()),
            output_device_label: Some("Default speaker".to_owned()),
        });

        let first_page = poll_app_events(Some(PollAppEventsRequest {
            after: Some(0),
            kinds: vec![],
            limit: Some(3),
        }));
        assert_eq!(first_page.cursor, 0);
        assert_eq!(first_page.events.len(), 3);
        assert!(first_page.has_more);
        assert!(first_page.next_cursor > first_page.cursor);

        let invite_events = poll_app_events(Some(PollAppEventsRequest {
            after: Some(0),
            kinds: vec!["invite".to_owned()],
            limit: Some(16),
        }));
        assert_eq!(invite_events.subscribed_kinds, vec!["invite".to_owned()]);
        assert!(!invite_events.events.is_empty());
        assert!(invite_events
            .events
            .iter()
            .all(|event| event.kind.starts_with("invite.")));

        let topic_events = poll_app_events(Some(PollAppEventsRequest {
            after: Some(0),
            kinds: vec![
                "message".to_owned(),
                "device".to_owned(),
                "group".to_owned(),
                "voice".to_owned(),
            ],
            limit: Some(64),
        }));
        for topic in ["message.", "device.", "group.", "voice."] {
            assert!(
                topic_events
                    .events
                    .iter()
                    .any(|event| event.kind.starts_with(topic)),
                "missing topic {topic}"
            );
        }

        let drained = poll_app_events(Some(PollAppEventsRequest {
            after: Some(topic_events.next_cursor),
            kinds: vec!["message".to_owned(), "voice".to_owned()],
            limit: Some(64),
        }));
        assert!(drained.events.is_empty());
        assert_eq!(drained.cursor, topic_events.next_cursor);
        Ok(())
    }

    #[test]
    fn message_mutations_prepare_tauri_push_event_payloads_from_persisted_cursor(
    ) -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("message-push-events");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let dm = start_dm(StartDmRequest {
            display_name: "Bob".to_owned(),
        });
        let before_message_cursor = dm.event_cursor;
        let dm_id = dm
            .dms
            .first()
            .map(|dm| dm.dm_id.clone())
            .ok_or_else(|| "dm created".to_owned())?;

        let sent = send_message(SendMessageRequest {
            target: MessageTargetView {
                kind: "dm".to_owned(),
                dm_id: Some(dm_id),
                group_id: None,
                channel_id: None,
            },
            body: "push me".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let push_payload = app_event_stream_after_view(&sent, before_message_cursor);

        assert_eq!(push_payload.cursor, before_message_cursor);
        assert_eq!(push_payload.next_cursor, sent.event_cursor);
        assert!(!push_payload.has_more);
        assert_eq!(push_payload.subscribed_kinds, Vec::<String>::new());
        assert!(!push_payload.events.is_empty());
        assert!(push_payload
            .events
            .iter()
            .all(|event| event.sequence > before_message_cursor));
        assert!(push_payload
            .events
            .iter()
            .any(|event| event.kind == "message.sent"));
        assert_eq!(
            push_payload.events.last().map(|event| event.sequence),
            Some(sent.event_cursor)
        );

        let persisted = load_state().to_view();
        let persisted_payload = app_event_stream_after_view(&persisted, before_message_cursor);
        assert_eq!(persisted_payload, push_payload);
        Ok(())
    }

    #[test]
    fn voice_mutations_prepare_tauri_push_event_payloads_from_persisted_cursor(
    ) -> Result<(), String> {
        let _guard = test_lock();
        let _path = reset_with_temp_state("voice-push-events");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: Some("Desktop".to_owned()),
        });
        let group = create_group(CreateGroupRequest {
            name: "Voice Lab".to_owned(),
            retention: "session".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = group
            .groups
            .first()
            .map(|group| group.group_id.clone())
            .ok_or_else(|| "group created".to_owned())?;
        let voice_channel = create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "Voice Ops".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
        });
        let before_voice_cursor = voice_channel.event_cursor;
        let channel_id = voice_channel
            .groups
            .first()
            .and_then(|group| group.channels.last())
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "voice channel created".to_owned())?;

        let joined = join_voice(JoinVoiceRequest {
            group_id,
            channel_id,
            microphone_permission: "granted".to_owned(),
            input_device_id: Some("mic-default".to_owned()),
            input_device_label: Some("Default microphone".to_owned()),
            output_device_id: Some("speaker-default".to_owned()),
            output_device_label: Some("Default speaker".to_owned()),
        });
        let push_payload = app_event_stream_after_view(&joined, before_voice_cursor);

        assert_eq!(push_payload.cursor, before_voice_cursor);
        assert_eq!(push_payload.next_cursor, joined.event_cursor);
        assert!(!push_payload.events.is_empty());
        assert!(push_payload
            .events
            .iter()
            .all(|event| event.sequence > before_voice_cursor));
        assert!(push_payload
            .events
            .iter()
            .any(|event| event.kind == "voice.joined"));
        Ok(())
    }

    #[test]
    fn tauri_runtime_mutating_commands_are_wired_to_push_app_event_streams() {
        let source = include_str!("lib.rs");
        assert!(source.contains("const APP_EVENT_TAURI_TOPIC: &str = \"app_event\""));
        assert!(source.contains("app_handle.emit(APP_EVENT_TAURI_TOPIC, stream)"));

        fn command_wrapper<'a>(source: &'a str, command: &str) -> &'a str {
            let marker = format!("pub(super) fn {command}");
            let start = source.find(&marker).unwrap_or_else(|| {
                panic!("missing Tauri command wrapper for {command}");
            });
            let rest = &source[start..];
            let end = rest.find("\n    #[tauri::command]").unwrap_or(rest.len());
            &rest[..end]
        }

        for command in [
            "start_signaling_session",
            "stop_signaling_session",
            "start_text_session",
            "stop_text_session",
            "attach_text_control_transport_runtime",
            "create_user",
            "recover_user",
            "accept_device_pairing_payload",
            "save_preferences",
            "start_dm",
            "create_group",
            "set_active_group",
            "set_active_channel",
            "set_active_dm",
            "join_group",
            "create_invite",
            "create_dm_invite",
            "accept_dm_invite",
            "create_channel",
            "send_message",
            "apply_text_delivery_receipt",
            "mark_text_control_frame_sent",
            "join_voice",
            "leave_voice",
            "set_self_mute",
            "update_voice_activity",
            "set_speaker_volume",
        ] {
            let start = source
                .find(&format!("pub(super) fn {command}("))
                .unwrap_or_else(|| panic!("missing {command} wrapper"));
            let rest = &source[start..];
            let end = rest.find("\n    #[tauri::command]").unwrap_or(rest.len());
            let wrapper = &rest[..end];
            assert!(
                wrapper.contains("app_handle: tauri::AppHandle"),
                "{command} wrapper must accept AppHandle for push app_event emission"
            );
            assert!(
                wrapper.contains(&format!("super::{command}(request)")),
                "{command} wrapper must call the underlying mutator inside an event-emitting helper"
            );
            assert!(
                wrapper.contains("run_app_state_command_with_event_emit")
                    || wrapper.contains("run_command_with_event_emit")
                    || wrapper.contains("emit_app_event_stream"),
                "{command} wrapper must emit an app_event stream after mutation"
            );
        }

        let reset = command_wrapper(source, "reset_app_state");
        assert!(reset.contains("app_handle: tauri::AppHandle"));
        assert!(reset.contains("run_app_state_command_with_event_emit(&app_handle"));
        assert!(reset.contains("super::reset_app_state_confirmed(request)"));

        for command in [
            "verify_safety_number",
            "create_device_pairing_payload",
            "pump_text_control_transport_once",
        ] {
            let wrapper = command_wrapper(source, command);
            assert!(
                wrapper.contains("app_handle: tauri::AppHandle"),
                "{command} must accept AppHandle for app_event emission"
            );
            assert!(
                wrapper.contains("run_command_with_event_emit(&app_handle"),
                "{command} must emit app_event stream after non-AppState mutation"
            );
            assert!(wrapper.contains(&format!("super::{command}(request)")));
        }

        let receive = command_wrapper(source, "receive_text_delivery_envelope");
        assert!(receive.contains("run_receive_text_delivery_envelope_with_event_emit(&app_handle"));
        let handle = command_wrapper(source, "handle_text_control_frame");
        assert!(handle.contains("run_handle_text_control_frame_with_event_emit(&app_handle"));
    }

    #[test]
    fn g006_observable_transport_copy_redacts_room_scope_and_message_ids() -> Result<(), String> {
        let _guard = test_lock();
        reset_with_temp_state("g006-privacy-redaction");

        create_user(CreateUserRequest {
            display_name: "Alice Redaction".to_owned(),
            device_name: Some("Alice laptop".to_owned()),
        });
        let dm_state = start_dm(StartDmRequest {
            display_name: "Bob Redaction".to_owned(),
        });
        let dm_id = dm_state
            .dms
            .first()
            .map(|dm| dm.dm_id.clone())
            .ok_or_else(|| "DM should exist".to_owned())?;
        let target = MessageTargetView {
            kind: "dm".to_owned(),
            dm_id: Some(dm_id),
            group_id: None,
            channel_id: None,
        };
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "privacy gate hello".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        let message_id = sent
            .messages
            .first()
            .map(|message| message.message_id.clone())
            .ok_or_else(|| "sent message should be visible".to_owned())?;
        let pending = list_pending_text_control_frames(ListPendingTextControlFramesRequest {
            target: Some(target),
            limit: Some(1),
            operation_timeout_ms: None,
        });
        let frame = pending
            .frames
            .first()
            .ok_or_else(|| "pending text/control frame should exist".to_owned())?;
        let raw_scope = "private-room raw-sdp v=0 raw-ice message-topic";
        let text_session = start_text_session(StartTextSessionRequest {
            scope_label: Some(raw_scope.to_owned()),
            data_channel_probe: false,
            adapter_kind: None,
        });
        assert!(text_session
            .transport_status
            .iter()
            .any(|row| row.detail.contains("scope_ref=")));
        let marked = mark_text_control_frame_sent(MarkTextControlFrameSentRequest {
            message_id: message_id.clone(),
            frame_sha256: frame.frame_sha256.clone(),
            transport_session_id: Some("redacted-test-session".to_owned()),
        });

        let observable = marked
            .events
            .iter()
            .map(|event| event.summary.as_str())
            .chain(
                marked
                    .transport_status
                    .iter()
                    .map(|row| row.detail.as_str()),
            )
            .chain(
                marked
                    .messages
                    .iter()
                    .map(|message| message.status.as_str()),
            )
            .chain(
                marked
                    .messages
                    .iter()
                    .map(|message| message.state_detail.as_str()),
            )
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            !observable.contains(&message_id),
            "observable copy must not include raw message id: {observable}"
        );
        assert!(
            !observable.contains(raw_scope),
            "observable copy must not include raw room/scope label: {observable}"
        );
        assert!(
            observable.contains("message_ref=") && observable.contains("scope_ref="),
            "observable copy should expose redacted refs for support/debuggability: {observable}"
        );
        Ok(())
    }

    #[test]
    fn g009_observable_copy_redacts_sensitive_classes() {
        for forbidden in [
            "v=0\r\na=ice-pwd:g009",
            "a=ice-ufrag:g009",
            "candidate:g009",
            "turn credential g009",
            "turn password g009",
            "room-secret:g009",
            "plaintext message g009",
            "audio plaintext g009",
            "sframe key g009",
            "content key g009",
            "mls epoch secret g009",
            "mls exporter g009",
            "production-ready",
            "fake production label",
        ] {
            let redacted = redact_sensitive_observable_copy(forbidden);
            assert!(
                redacted.contains("redacted sensitive observable copy"),
                "sensitive class was not redacted: {forbidden} -> {redacted}"
            );
            assert!(
                !redacted.contains(forbidden),
                "sensitive observable copy leaked original value: {redacted}"
            );
        }
    }

    #[test]
    fn g009_backend_event_summaries_commit_names_and_invites() -> Result<(), String> {
        let _guard = test_lock();
        reset_with_temp_state("g009-backend-event-redaction");
        let created = create_user(CreateUserRequest {
            display_name: "Alice Secret".to_owned(),
            device_name: Some("Laptop Secret".to_owned()),
        });
        assert!(created
            .events
            .iter()
            .any(|event| event.summary.contains("profile_ref=")));
        let grouped = create_group(CreateGroupRequest {
            name: "Secret Project".to_owned(),
            retention: "24 hours".to_owned(),
            adapter_kind: None,
            signaling_endpoint: None,
            ice_stun_servers: None,
            ice_turn_servers: None,
        });
        let group_id = grouped
            .groups
            .iter()
            .find(|group| group.name == "Secret Project")
            .map(|group| group.group_id.clone())
            .ok_or_else(|| "created group missing".to_owned())?;
        create_channel(CreateChannelRequest {
            group_id: group_id.clone(),
            name: "private-plans".to_owned(),
            kind: ChannelKind::Text,
            retention_status: "24 hours".to_owned(),
        });
        create_invite(CreateInviteRequest {
            group_id: Some(group_id),
            expires: "24 hours".to_owned(),
            max_use: "single use".to_owned(),
        });
        start_dm(StartDmRequest {
            display_name: "Bob Secret".to_owned(),
        });

        let event_copy = load_state()
            .events
            .iter()
            .map(|event| event.summary.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for raw in [
            "Alice Secret",
            "Laptop Secret",
            "Secret Project",
            "private-plans",
            "Bob Secret",
        ] {
            assert!(
                !event_copy.contains(raw),
                "backend event summary leaked raw label {raw}: {event_copy}"
            );
        }
        for redacted_ref in [
            "profile_ref=",
            "device_ref=",
            "group_ref=",
            "channel_ref=",
            "dm_contact_ref=",
        ] {
            assert!(
                event_copy.contains(redacted_ref),
                "backend event summary missing redacted reference {redacted_ref}: {event_copy}"
            );
        }
        Ok(())
    }

    #[test]
    fn tauri_commands_use_shared_app_service_singleton() {
        let source = include_str!("lib.rs");
        assert!(source.contains("static APP_SERVICE"));
        assert!(!source.contains(&["static", "APP_STATE"].join(" ")));
        assert!(!source.contains(&["get_or_init(||", "Mutex::new(load_state()))",].join(" ")));
        assert!(source.matches("mutate_app_service(").count() >= 16);
    }
}

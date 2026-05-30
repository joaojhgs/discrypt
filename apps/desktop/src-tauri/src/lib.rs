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
    INVITE_CONNECTIVITY_SCHEMA_VERSION,
};
use discrypt_core::{
    app_snapshot as core_app_snapshot, identity_recovery_verification_smoke,
    snapshot_safety_number_matches_identity_keys, AppSnapshot, ChannelKind,
    ChannelView as SnapshotChannelView, DeviceView, MessageView as SnapshotMessageView,
    SafetyVerificationRequest, SafetyVerificationResult, SecurityCopyView, ServerView,
};
use discrypt_media::{
    MicrophonePermissionState, VoiceDeviceDescriptor, VoiceDeviceKind, VoiceDeviceSelection,
};
use discrypt_transport::{
    required_provider_adapter_boundaries, TransportRoute, TransportSession, TransportSessionSnapshot,
    TransportSessionState,
};
use discrypt_mls_core::{
    verifying_key_from_hex, DeviceLeaf, DevicePairingPayload, DeviceSet, DeviceStatus, FriendCode,
    Identity, SafetyNumber,
};
#[cfg(all(target_os = "linux", feature = "production-storage"))]
use discrypt_storage::EncryptedAppDb;
#[cfg(not(all(target_os = "linux", feature = "production-storage")))]
use discrypt_storage::FileAppStore;
#[cfg(all(target_os = "linux", feature = "production-storage", not(test)))]
use discrypt_storage::LinuxOsKeychain;
use discrypt_storage::{
    recover_account, recovery_code_material, seal_account_backup, AccountRecovery, AppStore,
    RecoveryCodeVerifier, RecoveryMaterial,
};
#[cfg(all(test, target_os = "linux", feature = "production-storage"))]
use discrypt_storage::{AppDbKeychain, AppStoreError};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(all(test, target_os = "linux", feature = "production-storage"))]
use std::collections::BTreeMap;
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
const ADMISSION_HELPER_ATTEMPT_LIMIT: u32 = 5;
const SIGNALING_ACTION_LIMIT: u32 = 60;
const ABUSE_WINDOW_SECONDS: i64 = 60;

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
    /// Local speaker volume 0-100.
    pub volume: u8,
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

/// Request to create a local-first group/server.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    /// Group display name.
    pub name: String,
    /// Default retention label for new text channels.
    pub retention: String,
}

/// Request to focus an existing group from the server rail.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetActiveGroupRequest {
    /// Existing group id.
    pub group_id: String,
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

/// Request to set self mute state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetSelfMuteRequest {
    /// Session id.
    pub session_id: String,
    /// Whether the local participant is muted.
    pub muted: bool,
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
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportDiagnosticsView {
    /// Required adapter boundaries and their readiness labels.
    pub adapter_boundaries: Vec<SignalingAdapterBoundaryView>,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BackendTransportMode {
    Signaling,
    Text,
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PersistedAppState {
    schema_version: u32,
    lifecycle: AppLifecycle,
    profile: Option<UserProfileView>,
    preferences: UiPreferencesView,
    dms: Vec<DirectConversationView>,
    groups: Vec<GroupView>,
    active_context: Option<ActiveContextView>,
    messages: Vec<MessageView>,
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
    abuse: PersistedAbuseState,
    friend_verified: bool,
    next_sequence: u64,
}

static APP_SERVICE: OnceLock<Mutex<TauriAppService>> = OnceLock::new();

/// Shared command-facing app service used by Tauri IPC wrappers.
#[derive(Debug)]
struct TauriAppService {
    state: PersistedAppState,
}

impl TauriAppService {
    fn load() -> Self {
        Self {
            state: load_state(),
        }
    }

    fn read<T>(&self, read: impl FnOnce(&PersistedAppState) -> T) -> T {
        read(&self.state)
    }

    fn mutate(&mut self, update: impl FnOnce(&mut PersistedAppState)) -> AppStateView {
        self.state.last_command_error = None;
        update(&mut self.state);
        self.persist();
        self.state.to_view()
    }

    fn persist(&self) {
        persist_state(&self.state);
    }
}

/// Tauri command: return the transitional compatibility snapshot for older clients.
pub fn app_snapshot() -> AppSnapshot {
    with_state(|state| state.to_snapshot())
}

/// Tauri command: return the full command-backed app state for the React shell.
pub fn app_state() -> AppStateView {
    with_state(|state| state.to_view())
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
                    format!("Pairing payload created for {requested_label}"),
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

/// Tauri command: start or focus a direct-message conversation.
pub fn start_dm(request: StartDmRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let display_name =
            normalize_label(&request.display_name, &core_app_snapshot().friend.alias);
        let dm_id = stable_id("dm", &display_name, state.next_sequence);
        if !state.dms.iter().any(|dm| dm.display_name == display_name) {
            let participant_id = stable_id("participant", &display_name, state.next_sequence);
            state.dms.push(DirectConversationView {
                dm_id: dm_id.clone(),
                participant_id: participant_id.clone(),
                display_name: display_name.clone(),
                local_only_copy: "Local harness-backed DM; no remote delivery is claimed"
                    .to_owned(),
                connectivity: Some(dm_connectivity_policy(&dm_id, &participant_id)),
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
        state.push_event("dm.started", format!("Opened local DM with {display_name}"));
    })
}

/// Tauri command: create a local-first group and make it active.
pub fn create_group(request: CreateGroupRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let name = normalize_label(&request.name, "private lab");
        let group_id = stable_id("group", &name, state.next_sequence);
        if !state.groups.iter().any(|group| group.name == name) {
            state.groups.push(GroupView {
                group_id: group_id.clone(),
                name: name.clone(),
                role: "owner".to_owned(),
                channels: default_group_channels(state.next_sequence),
                connectivity: Some(group_connectivity_policy(&group_id)),
            });
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
        state.push_event("group.created", format!("Created group {name}"));
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
            state.push_event("group.focused", format!("Focused group {}", group.name));
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
                format!("Opened {group_name} from local invite"),
            );
            return;
        }
        let parsed_invite = parse_invite_metadata(&invite_code);
        let name = request
            .group_name
            .map(|value| normalize_label(&value, "joined enclave"))
            .unwrap_or_else(|| parse_invite_group_name(&invite_code));
        let group_id = stable_id("group", &name, state.next_sequence);
        if !state.groups.iter().any(|group| group.name == name) {
            state.groups.push(GroupView {
                group_id: group_id.clone(),
                name: name.clone(),
                role: "member".to_owned(),
                channels: default_group_channels(state.next_sequence),
                connectivity: parsed_invite
                    .as_ref()
                    .map(|parsed| parsed.connectivity.clone())
                    .or_else(|| Some(group_connectivity_policy(&group_id))),
            });
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
        state.push_event("group.joined", format!("Joined {name} via {invite_code}"));
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
        let expires = normalize_label(&request.expires, "Invite expires and can be revoked");
        let max_use = normalize_label(&request.max_use, "Max-use is enforced before MLS admission");
        let expires_at = invite_expiration_horizon(&expires);
        let descriptor_expires_at = Utc::now() + invite_expiration_duration(&expires);
        let max_uses = parse_max_uses(&max_use);
        let invite_key = Uuid::new_v4().to_string();
        let room_secret = format!("room-secret:{}:{}:{}", group_id, invite_key, sequence);
        let signaling_endpoint = default_signaling_endpoint();
        let signaling_trust_fingerprint = signaling_fingerprint_for_endpoint(&signaling_endpoint);
        let signaling_metadata = InviteSignalingMetadata::new(
            signaling_endpoint.clone(),
            InviteEndpointPolicy::ProductionTls,
            InviteTrustMetadata::new(
                signaling_trust_fingerprint.clone(),
                "signed endpoint fingerprint; verify before MLS Welcome",
            )
            .unwrap_or_else(|_| InviteSignalingMetadata::default_production().trust),
        )
        .unwrap_or_else(|_| InviteSignalingMetadata::default_production());
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
        let ice_config = descriptor.ice_server_config_at(None, Utc::now()).ok();
        let invite_code = match production_invite_link(&descriptor, expires_at.as_str(), max_uses) {
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
            ice_stun_servers: ice_config
                .as_ref()
                .map(ice_stun_server_views)
                .unwrap_or_default(),
            ice_turn_servers: ice_config
                .as_ref()
                .map(ice_turn_server_views)
                .unwrap_or_default(),
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
        state.push_event("invite.created", format!("Invite created for {group_name}"));
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
        let signaling_metadata = InviteSignalingMetadata::new(
            signaling_endpoint.clone(),
            InviteEndpointPolicy::ProductionTls,
            InviteTrustMetadata::new(
                signaling_trust_fingerprint.clone(),
                "signed endpoint fingerprint; verify before DM accept",
            )
            .unwrap_or_else(|_| InviteSignalingMetadata::default_production().trust),
        )
        .unwrap_or_else(|_| InviteSignalingMetadata::default_production());
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
        let ice_config = descriptor.ice_server_config_at(None, Utc::now()).ok();
        let invite_code = match production_invite_link(&descriptor, expires_at.as_str(), max_uses) {
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
            ice_stun_servers: ice_config
                .as_ref()
                .map(ice_stun_server_views)
                .unwrap_or_default(),
            ice_turn_servers: ice_config
                .as_ref()
                .map(ice_turn_server_views)
                .unwrap_or_default(),
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
            format!("DM contact invite created for {}", dm.display_name),
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
            state.dms.push(DirectConversationView {
                dm_id: dm_id.clone(),
                participant_id,
                display_name: display_name.clone(),
                local_only_copy: "DM contact opened from signed invite metadata; remote delivery is not claimed until backend receipt proof".to_owned(),
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
            format!("Opened DM contact {display_name}"),
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
                format!("Created channel {}", channel.name),
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
        let message = MessageView {
            message_id: format!("msg-{sequence}"),
            target: request.target,
            author_id: state.local_user_id(),
            author,
            body: body.to_owned(),
            status: "local encrypted author log; remote delivery/read receipts not claimed without signed receipt".to_owned(),
            state_key: "sent_local".to_owned(),
            state_label: "Sent locally".to_owned(),
            state_detail: default_text_state_detail(),
            sent_at: format!("local-{sequence}"),
        };
        state.messages.push(message);
        state.push_event(
            "message.sent",
            "Message appended to local encrypted timeline; remote delivery/read receipts are not claimed",
        );
    })
}

/// Tauri command: join a voice channel.
pub fn join_voice(request: JoinVoiceRequest) -> AppStateView {
    mutate_app_service(|state| {
        state.ensure_ready_profile();
        let session_id = stable_id("voice", &request.channel_id, state.next_sequence);
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
            participants: default_voice_participants(&local_user_id, false),
            route_copy: if capture_allowed {
                "Local capture permission and device selection are ready; encrypted media transport remains gated by media-frame E2E; speaking indicators wait for media audio-level/VAD events".to_owned()
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
            state.push_event("voice.joined", format!("Joined voice session {session_id}"));
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
        if let Some(session) = &mut state.voice_session {
            if session.session_id == request.session_id {
                session.joined = false;
                session.status_copy =
                    "Not joined; command-backed local voice controls are idle".to_owned();
                for participant in &mut session.participants {
                    participant.speaking = false;
                }
                state.push_event("voice.left", "Left command-backed local voice session");
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

/// Tauri command: persist a participant speaker volume.
pub fn set_speaker_volume(request: SetSpeakerVolumeRequest) -> AppStateView {
    mutate_app_service(|state| {
        if let Some(session) = &mut state.voice_session {
            if session.session_id == request.session_id {
                let volume = request.volume.min(100);
                if let Some(participant) = session
                    .participants
                    .iter_mut()
                    .find(|participant| participant.id == request.participant_id)
                {
                    participant.volume = volume;
                    let name = participant.name.clone();
                    state.push_event("voice.volume", format!("Set {name} volume to {volume}"));
                } else {
                    state.push_command_error(
                        "voice.volume_rejected",
                        "set_speaker_volume",
                        "voice_participant_not_found",
                        "No matching voice participant for speaker volume",
                        "Choose a visible participant from the voice member list",
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
    pub(super) fn create_user(request: CreateUserRequest) -> AppStateView {
        super::create_user(request)
    }

    #[tauri::command]
    pub(super) fn recover_user(request: RecoverUserRequest) -> AppStateView {
        super::recover_user(request)
    }

    #[tauri::command]
    pub(super) fn verify_safety_number(
        request: SafetyVerificationRequest,
    ) -> SafetyVerificationResult {
        super::verify_safety_number(request)
    }

    #[tauri::command]
    pub(super) fn create_device_pairing_payload(
        request: CreateDevicePairingPayloadRequest,
    ) -> DevicePairingPayloadView {
        super::create_device_pairing_payload(request)
    }

    #[tauri::command]
    pub(super) fn accept_device_pairing_payload(
        request: AcceptDevicePairingPayloadRequest,
    ) -> AppStateView {
        super::accept_device_pairing_payload(request)
    }

    #[tauri::command]
    pub(super) fn save_preferences(request: SavePreferencesRequest) -> AppStateView {
        super::save_preferences(request)
    }

    #[tauri::command]
    pub(super) fn start_dm(request: StartDmRequest) -> AppStateView {
        super::start_dm(request)
    }

    #[tauri::command]
    pub(super) fn create_group(request: CreateGroupRequest) -> AppStateView {
        super::create_group(request)
    }

    #[tauri::command]
    pub(super) fn set_active_group(request: SetActiveGroupRequest) -> AppStateView {
        super::set_active_group(request)
    }

    #[tauri::command]
    pub(super) fn join_group(request: JoinGroupRequest) -> AppStateView {
        super::join_group(request)
    }

    #[tauri::command]
    pub(super) fn create_invite(request: CreateInviteRequest) -> AppStateView {
        super::create_invite(request)
    }

    #[tauri::command]
    pub(super) fn create_dm_invite(request: CreateDmInviteRequest) -> AppStateView {
        super::create_dm_invite(request)
    }

    #[tauri::command]
    pub(super) fn accept_dm_invite(request: AcceptDmInviteRequest) -> AppStateView {
        super::accept_dm_invite(request)
    }

    #[tauri::command]
    pub(super) fn create_channel(request: CreateChannelRequest) -> AppStateView {
        super::create_channel(request)
    }

    #[tauri::command]
    pub(super) fn send_message(request: SendMessageRequest) -> AppStateView {
        super::send_message(request)
    }

    #[tauri::command]
    pub(super) fn join_voice(request: JoinVoiceRequest) -> AppStateView {
        super::join_voice(request)
    }

    #[tauri::command]
    pub(super) fn leave_voice(request: LeaveVoiceRequest) -> AppStateView {
        super::leave_voice(request)
    }

    #[tauri::command]
    pub(super) fn set_self_mute(request: SetSelfMuteRequest) -> AppStateView {
        super::set_self_mute(request)
    }

    #[tauri::command]
    pub(super) fn set_speaker_volume(request: SetSpeakerVolumeRequest) -> AppStateView {
        super::set_speaker_volume(request)
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
    pub(super) fn reset_app_state(request: ResetAppStateRequest) -> AppStateView {
        super::reset_app_state_confirmed(request)
    }
}

/// Run the native Tauri shell with the command surface registered for frontend IPC.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::<tauri::Wry>::default()
        .invoke_handler(tauri::generate_handler![
            ipc_commands::app_snapshot,
            ipc_commands::app_state,
            ipc_commands::create_user,
            ipc_commands::recover_user,
            ipc_commands::verify_safety_number,
            ipc_commands::create_device_pairing_payload,
            ipc_commands::accept_device_pairing_payload,
            ipc_commands::save_preferences,
            ipc_commands::start_dm,
            ipc_commands::create_group,
            ipc_commands::set_active_group,
            ipc_commands::join_group,
            ipc_commands::create_invite,
            ipc_commands::create_dm_invite,
            ipc_commands::accept_dm_invite,
            ipc_commands::create_channel,
            ipc_commands::send_message,
            ipc_commands::join_voice,
            ipc_commands::leave_voice,
            ipc_commands::set_self_mute,
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
            active_context: None,
            messages: Vec::new(),
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
            abuse: PersistedAbuseState::default(),
            friend_verified: false,
            next_sequence: 2,
        }
    }

    fn to_view(&self) -> AppStateView {
        AppStateView {
            schema_version: self.schema_version,
            lifecycle: self.lifecycle.clone(),
            profile: self.profile.clone(),
            preferences: self.preferences.clone(),
            dms: self.dms.clone(),
            groups: self.groups.clone(),
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
            join_progress: self.join_progress(),
            text_state_legend: text_state_legend(),
            voice_states: self.voice_states(),
            runtime_mode: runtime_mode_view(),
            snapshot: self.to_snapshot(),
        }
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
        vec![
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
            TransportStatusView {
                label: "direct".to_owned(),
                status: if voice_joined {
                    "media-gated"
                } else {
                    "no-direct-proof"
                }
                .to_owned(),
                detail: "Direct path is only shown as connected after transport/session state proves it; this command state has no direct route proof yet".to_owned(),
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
                status: last_error.map(|_| "attention").unwrap_or("clear").to_owned(),
                detail: last_error
                    .map(|error| format!("Last command issue {}: {}", error.code, error.message))
                    .unwrap_or_else(|| "No degraded command state is currently reported".to_owned()),
            },
            TransportStatusView {
                label: "reconnecting".to_owned(),
                status: "idle".to_owned(),
                detail: "Reconnect orchestration is displayed only when event state reports reconnect attempts".to_owned(),
            },
            TransportStatusView {
                label: "failed".to_owned(),
                status: last_error.map(|_| "last-command-error").unwrap_or("clear").to_owned(),
                detail: last_error
                    .map(|error| error.recovery_hint.clone())
                    .unwrap_or_else(|| "No failed transport command is currently reported".to_owned()),
            },
        ]
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
                status_copy: "Not joined; command-backed local voice controls are idle".to_owned(),
                route_copy:
                    "Local voice controls only; network media route is not connected in this build"
                        .to_owned(),
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
            self.dms.push(DirectConversationView {
                dm_id: dm_id.clone(),
                participant_id: participant_id.clone(),
                display_name: friend.alias,
                local_only_copy: "Local DM seeded from a generated friend-code/QR payload; no remote delivery is claimed".to_owned(),
                connectivity: Some(dm_connectivity_policy(&dm_id, &participant_id)),
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
            format!("Profile ready for {display_name} on {device_name}"),
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
                "Account continuity restored with verified local identity material for {} room(s) and {} device(s); content keys restored: {}",
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
            self.groups.push(GroupView {
                group_id: group_id.clone(),
                name: room_name,
                role: "member".to_owned(),
                channels: default_group_channels(self.next_sequence),
                connectivity: Some(group_connectivity_policy(&group_id)),
            });
        }
    }

    fn push_event(&mut self, kind: impl Into<String>, summary: impl Into<String>) {
        let event = AppEventView {
            sequence: self.next_sequence,
            kind: kind.into(),
            summary: summary.into(),
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
            message: message.into(),
            recovery_hint: recovery_hint.into(),
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
    if let Ok(Some(bytes)) = store.load_app_state() {
        if let Ok(state) = serde_json::from_slice::<PersistedAppState>(&bytes) {
            if state.schema_version == APP_STATE_SCHEMA_VERSION {
                return state;
            }
        }
    }
    PersistedAppState::initial()
}

fn persist_state(state: &PersistedAppState) {
    if let Ok(encoded) = serde_json::to_vec_pretty(state) {
        let mut store = app_store();
        let _ = store.save_app_state(&encoded);
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

fn env_app_state_override_allowed() -> bool {
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
        },
        ChannelStateView {
            channel_id: stable_id("channel", "Voice Lobby", sequence),
            name: "Voice Lobby".to_owned(),
            kind: ChannelKind::Voice,
            retention_status: "session".to_owned(),
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

fn default_adapter_endpoint(kind: &InviteSignalingAdapterKind) -> String {
    match kind {
        InviteSignalingAdapterKind::Mqtt => "wss://mqtt.discrypt.invalid/mqtt".to_owned(),
        InviteSignalingAdapterKind::Nostr => "wss://nostr.discrypt.invalid".to_owned(),
        InviteSignalingAdapterKind::IpfsPubsub => {
            "https://ipfs.discrypt.invalid/bootstrap/pubsub".to_owned()
        }
        InviteSignalingAdapterKind::DiscryptQuicRendezvous => {
            "quic://signaling.discrypt.invalid:443/rendezvous".to_owned()
        }
    }
}

fn default_ice_stun_servers() -> Vec<String> {
    vec!["stun:stun.l.google.com:19302".to_owned()]
}

fn default_redacted_turn_servers() -> Vec<IceTurnServerView> {
    Vec::new()
}

fn default_signaling_profiles(scope_commitment: &str) -> Vec<SignalingProfileView> {
    [
        InviteSignalingAdapterKind::Mqtt,
        InviteSignalingAdapterKind::Nostr,
        InviteSignalingAdapterKind::IpfsPubsub,
        InviteSignalingAdapterKind::DiscryptQuicRendezvous,
    ]
    .into_iter()
    .map(|kind| {
        let adapter_kind = profile_kind_name(&kind);
        let endpoint = default_adapter_endpoint(&kind);
        SignalingProfileView {
            profile_id: format!("{adapter_kind}-default"),
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
            capabilities: vec![
                "presence_ttl".to_owned(),
                "trickle_ice".to_owned(),
                "broadcast_control".to_owned(),
                "health_telemetry".to_owned(),
            ],
        }
    })
    .collect()
}

fn group_connectivity_policy(group_id: &str) -> ConnectivityPolicyView {
    let scope_id_commitment = hash_commitment("discrypt-group-scope-commitment-v1", &[group_id]);
    ConnectivityPolicyView {
        connectivity_schema_version: INVITE_CONNECTIVITY_SCHEMA_VERSION,
        invite_kind: InviteKind::GroupJoin.canonical_name().to_owned(),
        scope_id_commitment: scope_id_commitment.clone(),
        signaling_profiles: default_signaling_profiles(&scope_id_commitment),
        ice_stun_servers: default_ice_stun_servers(),
        ice_turn_servers: default_redacted_turn_servers(),
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
) -> Result<String, String> {
    let descriptor_bytes = serde_json::to_vec(descriptor)
        .map_err(|error| format!("Could not encode signed invite descriptor: {error}"))?;
    let encoded_descriptor = URL_SAFE_NO_PAD.encode(descriptor_bytes);
    Ok(format!(
        "discrypt://join/v1/{}?d={encoded_descriptor}&exp={}&max={max_uses}",
        descriptor.invite_id,
        url_component(expires_at)
    ))
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
    use super::*;
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
        assert!(persisted.dms.iter().any(|dm| dm
            .connectivity
            .as_ref()
            .map(|policy| policy.invite_kind.as_str())
            == Some("dm_contact")));
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
        });
        create_group(CreateGroupRequest {
            name: "Beta Lab".to_owned(),
            retention: "7 days".to_owned(),
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
        let muted = set_self_mute(SetSelfMuteRequest {
            session_id: session_id.clone(),
            muted: true,
        });
        assert!(muted
            .voice_session
            .as_ref()
            .map(|session| session.self_muted)
            .unwrap_or(false));
        let local_user_id = joined
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .unwrap_or_default();
        let volume = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id: session_id.clone(),
            participant_id: local_user_id.clone(),
            volume: 55,
        });
        assert_eq!(
            volume
                .voice_session
                .as_ref()
                .and_then(|session| session
                    .participants
                    .iter()
                    .find(|participant| participant.id == local_user_id))
                .map(|participant| participant.volume),
            Some(55)
        );
        let left = leave_voice(LeaveVoiceRequest { session_id });
        let session = left
            .voice_session
            .as_ref()
            .ok_or_else(|| "voice session remains for dock state".to_owned())?;
        assert!(!session.joined);
        assert!(session
            .participants
            .iter()
            .all(|participant| !participant.speaking));
        assert!(!left.groups.is_empty());
        assert_eq!(left.lifecycle, AppLifecycle::Ready);
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
                "direct",
                "overlay",
                "TURN",
                "degraded",
                "reconnecting",
                "failed"
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
    fn tauri_commands_use_shared_app_service_singleton() {
        let source = include_str!("lib.rs");
        assert!(source.contains("static APP_SERVICE"));
        assert!(!source.contains(&["static", "APP_STATE"].join(" ")));
        assert!(!source.contains(&["get_or_init(||", "Mutex::new(load_state()))",].join(" ")));
        assert!(source.matches("mutate_app_service(").count() >= 16);
    }
}

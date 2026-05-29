//! Tauri command surface and local-first app-state service for the native discrypt shell.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use chrono::{Duration, Utc};
use discrypt_core::{
    app_snapshot as core_app_snapshot, generated_device_view, identity_recovery_verification_smoke,
    safety_number_for_identity_hex_and_friend_code, snapshot_safety_number_matches_identity_keys,
    AppSnapshot, ChannelKind, ChannelView as SnapshotChannelView, DeviceView,
    MessageView as SnapshotMessageView, SafetyVerificationRequest, SafetyVerificationResult,
    SecurityCopyView, ServerView,
};
use discrypt_mls_core::{DeviceLeaf, DevicePairingPayload, DeviceSet, DeviceStatus, Identity};
#[cfg(not(all(target_os = "linux", feature = "production-storage")))]
use discrypt_storage::FileAppStore;
use discrypt_storage::{
    recover_account, recovery_code_material, seal_account_backup, AccountRecovery, AppStore,
    RecoveryCodeVerifier, RecoveryMaterial,
};
#[cfg(all(target_os = "linux", feature = "production-storage"))]
use discrypt_storage::{EncryptedAppDb, LinuxOsKeychain};
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use uuid::Uuid;

const APP_STATE_SCHEMA_VERSION: u32 = 1;
const APP_STATE_STORE_FILENAME: &str = "app-state.discrypt-store";
const DEFAULT_THEME_ID: &str = "graphite-calm";
const DEFAULT_TEMPLATE_ID: &str = "command-center";

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
    /// Honest local-only recovery posture.
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
    /// Deterministic local timestamp/counter label.
    pub sent_at: String,
}

/// Command-backed invite row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InviteView {
    /// Stable invite identifier for the local command surface.
    pub invite_id: String,
    /// Opaque invite key embedded in the link.
    #[serde(default)]
    pub invite_key: String,
    /// Group id this invite targets.
    pub group_id: String,
    /// User-pastable invite code/URL.
    pub code: String,
    /// Hash of the room secret; the plan requires secret-derived admission, not incremental ids.
    #[serde(default)]
    pub room_secret_hash: String,
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
    /// Whether the participant is currently speaking.
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
    /// Participant roster.
    pub participants: Vec<VoiceParticipantView>,
    /// Honest route/status copy.
    pub route_copy: String,
    /// Honest media/session status copy.
    pub status_copy: String,
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
    friend_verified: bool,
    next_sequence: u64,
}

static APP_STATE: OnceLock<Mutex<PersistedAppState>> = OnceLock::new();

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
    mutate_state(|state| state.create_user(request, false))
}

/// Tauri command: recover account continuity and unlock the shell without content keys.
pub fn recover_user(request: RecoverUserRequest) -> AppStateView {
    mutate_state(|state| {
        let recovery = account_recovery_from_request(&request);
        state.create_user(
            CreateUserRequest {
                display_name: request.display_name.clone(),
                device_name: request.device_name.clone(),
            },
            true,
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
    let state = APP_STATE.get_or_init(|| Mutex::new(load_state()));
    let mut guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.ensure_ready_profile();
    let identity = guard.local_identity();
    guard.ensure_device_set(&identity);
    let requested_label = normalize_label(&request.requested_label, "paired device");
    let current_epoch = request.current_epoch.unwrap_or(guard.next_sequence);
    let valid_for_epochs = request.valid_for_epochs.unwrap_or(3).max(1);
    let authorizing_device_id = guard
        .device_set
        .active_devices()
        .first()
        .map(|device| device.device_id);
    let view = if let Some(authorizing_device_id) = authorizing_device_id {
        match guard.device_set.create_pairing_payload(
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
                guard.push_event(
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
                guard.push_event("device.pairing_rejected", message.clone());
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
        guard.push_event("device.pairing_rejected", message);
        DevicePairingPayloadView {
            payload: String::new(),
            authorizing_device_id: String::new(),
            requested_label,
            expires_epoch: current_epoch,
            rejected_reason: Some(message.to_owned()),
        }
    };
    persist_state(&guard);
    view
}

/// Tauri command: accept a signed pasteable pairing payload and add the new device row.
pub fn accept_device_pairing_payload(request: AcceptDevicePairingPayloadRequest) -> AppStateView {
    mutate_state(|state| {
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
                state.push_event(
                    "device.pairing_rejected",
                    format!("Pairing rejected: {error}"),
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
        mutate_state(|state| {
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
    mutate_state(|state| {
        state.preferences = UiPreferencesView {
            theme_id: normalize_label(&request.theme_id, DEFAULT_THEME_ID),
            template_id: normalize_label(&request.template_id, DEFAULT_TEMPLATE_ID),
        };
        state.push_event("preferences.saved", "Theme/template preferences saved");
    })
}

/// Tauri command: start or focus a direct-message conversation.
pub fn start_dm(request: StartDmRequest) -> AppStateView {
    mutate_state(|state| {
        state.ensure_ready_profile();
        let display_name =
            normalize_label(&request.display_name, &core_app_snapshot().friend.alias);
        let dm_id = stable_id("dm", &display_name, state.next_sequence);
        if !state.dms.iter().any(|dm| dm.display_name == display_name) {
            state.dms.push(DirectConversationView {
                dm_id: dm_id.clone(),
                participant_id: stable_id("participant", &display_name, state.next_sequence),
                display_name: display_name.clone(),
                local_only_copy: "Local harness-backed DM; no remote delivery is claimed"
                    .to_owned(),
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
    mutate_state(|state| {
        state.ensure_ready_profile();
        let name = normalize_label(&request.name, "private lab");
        let group_id = stable_id("group", &name, state.next_sequence);
        if !state.groups.iter().any(|group| group.name == name) {
            state.groups.push(GroupView {
                group_id: group_id.clone(),
                name: name.clone(),
                role: "owner".to_owned(),
                channels: default_group_channels(state.next_sequence),
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
    mutate_state(|state| {
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
            state.push_event("group.focus_missing", "Requested group does not exist");
        }
    })
}

/// Tauri command: join a local-first group from an invite.
pub fn join_group(request: JoinGroupRequest) -> AppStateView {
    mutate_state(|state| {
        state.ensure_ready_profile();
        let invite_code = normalize_label(&request.invite_code, "manual invite");
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
        state.push_event("group.joined", format!("Joined {name} via {invite_code}"));
    })
}

/// Tauri command: create an invite for the active group.
pub fn create_invite(request: CreateInviteRequest) -> AppStateView {
    mutate_state(|state| {
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
            state.push_event("invite.rejected", "No group exists for invite creation");
            return;
        };
        let sequence = state.next_sequence;
        let group_name = state
            .groups
            .iter()
            .find(|group| group.group_id == group_id)
            .map(|group| group.name.clone())
            .unwrap_or_else(|| "group".to_owned());
        let invite_key = Uuid::new_v4().to_string();
        let room_secret = format!("room-secret:{}:{}:{}", group_id, invite_key, sequence);
        let room_secret_hash = hex::encode(Sha256::digest(room_secret.as_bytes()));
        let room_secret_token = &room_secret_hash[..32];
        let expires = normalize_label(&request.expires, "Invite expires and can be revoked");
        let max_use = normalize_label(&request.max_use, "Max-use is enforced before MLS admission");
        let expires_at = invite_expiration_horizon(&expires);
        let max_uses = parse_max_uses(&max_use);
        let invite = InviteView {
            invite_id: format!("invite-{invite_key}"),
            invite_key: invite_key.clone(),
            group_id: group_id.clone(),
            code: format!(
                "discrypt://join/v1/{invite_key}?room_secret={room_secret_token}&exp={expires_at}&max={max_uses}"
            ),
            room_secret_hash,
            expires,
            expires_at,
            max_use,
            uses: 0,
            revoked: false,
            admission_copy: "Final admission still requires an authorized MLS Welcome/add; the room-secret link alone is insufficient"
                .to_owned(),
        };
        state.invites.push(invite);
        state.push_event("invite.created", format!("Invite created for {group_name}"));
    })
}

/// Tauri command: create a channel in a group.
pub fn create_channel(request: CreateChannelRequest) -> AppStateView {
    mutate_state(|state| {
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
            state.push_event("channel.rejected", "No matching group for channel creation");
        }
    })
}

/// Tauri command: append a message to a local timeline.
pub fn send_message(request: SendMessageRequest) -> AppStateView {
    mutate_state(|state| {
        state.ensure_ready_profile();
        let body = request.body.trim();
        if body.is_empty() {
            state.push_event("message.rejected", "Empty message was not sent");
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
            status: "local encrypted author log; socket delivery not claimed".to_owned(),
            sent_at: format!("local-{sequence}"),
        };
        state.messages.push(message);
        state.push_event(
            "message.sent",
            "Message appended to local encrypted timeline facade",
        );
    })
}

/// Tauri command: join a voice channel.
pub fn join_voice(request: JoinVoiceRequest) -> AppStateView {
    mutate_state(|state| {
        state.ensure_ready_profile();
        let session_id = stable_id("voice", &request.channel_id, state.next_sequence);
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
            joined: true,
            self_muted,
            participants: default_voice_participants(&local_user_id, !self_muted),
            route_copy:
                "Local voice controls only; network media route is not connected in this build"
                    .to_owned(),
            status_copy:
                "Voice session state joined locally; real audio-frame media remains release-gated"
                    .to_owned(),
        });
        state.active_context = Some(ActiveContextView {
            kind: "voice_channel".to_owned(),
            group_id: Some(request.group_id),
            channel_id: Some(request.channel_id),
            dm_id: None,
        });
        state.push_event("voice.joined", format!("Joined voice session {session_id}"));
    })
}

/// Tauri command: leave a voice session.
pub fn leave_voice(request: LeaveVoiceRequest) -> AppStateView {
    mutate_state(|state| {
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
                state.push_event(
                    "voice.leave_ignored",
                    "Leave request did not match active session",
                );
            }
        } else {
            state.push_event("voice.leave_ignored", "No active voice session to leave");
        }
    })
}

/// Tauri command: persist local self-mute state.
pub fn set_self_mute(request: SetSelfMuteRequest) -> AppStateView {
    mutate_state(|state| {
        let local_user_id = state.local_user_id();
        if let Some(session) = &mut state.voice_session {
            if session.session_id == request.session_id {
                session.self_muted = request.muted;
                for participant in &mut session.participants {
                    if participant.id == local_user_id {
                        participant.muted = request.muted;
                        participant.speaking = session.joined && !request.muted;
                    }
                }
                let summary = if request.muted {
                    "Self muted"
                } else {
                    "Self unmuted"
                };
                state.push_event("voice.self_mute", summary);
            }
        }
    })
}

/// Tauri command: persist a participant speaker volume.
pub fn set_speaker_volume(request: SetSpeakerVolumeRequest) -> AppStateView {
    mutate_state(|state| {
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
                }
            }
        }
    })
}

/// Tauri command: return recent command-backed app events for polling clients.
pub fn poll_app_events() -> Vec<AppEventView> {
    with_state(|state| state.events.clone())
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
    let verification_snapshot = if state.snapshot.devices.is_empty() {
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
            .contains("does not claim anonymity");
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

/// Reset the persisted app state. Intended only for tests/dev smoke.
pub fn reset_app_state() -> AppStateView {
    let state = APP_STATE.get_or_init(|| Mutex::new(load_state()));
    let mut guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = PersistedAppState::initial();
    persist_state(&guard);
    guard.to_view()
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
    pub(super) fn poll_app_events() -> Vec<AppEventView> {
        super::poll_app_events()
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
    pub(super) fn reset_app_state() -> AppStateView {
        super::reset_app_state()
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
            snapshot: self.to_snapshot(),
        }
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
            }
        } else {
            discrypt_core::VoiceSessionView {
                joined: false,
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
        let display_name = normalize_label(&request.display_name, "Alice");
        let device_name = request
            .device_name
            .map(|value| normalize_label(&value, "Desktop"))
            .unwrap_or_else(|| "Desktop".to_owned());
        let user_id = stable_id("user", &display_name, self.next_sequence);
        self.profile = Some(UserProfileView {
            user_id,
            display_name: display_name.clone(),
            device_name: device_name.clone(),
            recovery_status: if recovered {
                "Recovered locally from placeholder code; no cloud or cross-device history recovery claimed"
                    .to_owned()
            } else {
                "New local profile; recovery export remains a local placeholder".to_owned()
            },
        });
        self.lifecycle = AppLifecycle::Ready;
        let base_device = core_app_snapshot().devices.into_iter().next();
        self.devices = vec![DeviceView {
            device_id: slugify(&device_name),
            label: device_name.clone(),
            leaf_index: 1,
            identity_key: base_device
                .as_ref()
                .map(|device| device.identity_key.clone())
                .unwrap_or_default(),
            device_key: base_device
                .as_ref()
                .map(|device| device.device_key.clone())
                .unwrap_or_default(),
            local: true,
            authorized: true,
            revoked: false,
            added_at_epoch: 1,
            revoked_at_epoch: None,
        }];
        if self.dms.is_empty() {
            let friend = core_app_snapshot().friend;
            let dm_id = stable_id("dm", &friend.friend_code, self.next_sequence);
            self.dms.push(DirectConversationView {
                dm_id: dm_id.clone(),
                participant_id: participant_id_from_friend_code(&friend.friend_code),
                display_name: friend.alias,
                local_only_copy: "Local DM seeded from a generated friend-code/QR payload; no remote delivery is claimed".to_owned(),
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
                "Account continuity restored for {} room(s) and {} device(s); content keys restored: {}",
                recovery.room_memberships.len(),
                recovery.device_count,
                recovery.content_keys_restored
            );
        }

        let local_device = self.devices.first().cloned().unwrap_or_else(|| DeviceView {
            device_id: "desktop".to_owned(),
            leaf_index: 1,
            local: true,
            authorized: true,
        });
        self.devices = vec![local_device];
        for index in 2..=recovery.device_count.max(1) {
            let device_id = format!("recovered-device-{index}");
            if !self
                .devices
                .iter()
                .any(|device| device.device_id == device_id)
            {
                self.devices.push(DeviceView {
                    device_id: device_id.clone(),
                    label: format!("Recovered device {index}"),
                    leaf_index: index as u32,
                    identity_key: local_device.identity_key.clone(),
                    device_key: format!("recovered-device-key-{index}"),
                    local: false,
                    authorized: true,
                    revoked: false,
                    added_at_epoch: index as u64,
                    revoked_at_epoch: None,
                });
            }
        }

        for room in &recovery.room_memberships {
            let room_name = normalize_label(room, "recovered room");
            if self.groups.iter().any(|group| group.name == room_name) {
                continue;
            }
            let group_id = stable_id("group", &room_name, self.next_sequence);
            self.groups.push(GroupView {
                group_id,
                name: room_name,
                role: "member".to_owned(),
                channels: default_group_channels(self.next_sequence),
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

    fn local_user_id(&self) -> String {
        self.profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .unwrap_or_else(|| "local-profile-pending".to_owned())
    }
}

fn with_state<T>(read: impl FnOnce(&PersistedAppState) -> T) -> T {
    let state = APP_STATE.get_or_init(|| Mutex::new(load_state()));
    let guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    read(&guard)
}

fn mutate_state(update: impl FnOnce(&mut PersistedAppState)) -> AppStateView {
    let state = APP_STATE.get_or_init(|| Mutex::new(load_state()));
    let mut guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    update(&mut guard);
    persist_state(&guard);
    guard.to_view()
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

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn app_store() -> EncryptedAppDb<LinuxOsKeychain> {
    EncryptedAppDb::new(app_store_path(), LinuxOsKeychain::discrypt_app_db())
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
    let lower = label.to_ascii_lowercase();
    let now = Utc::now();
    let expires_at = if lower.contains("hour") || lower.contains("1 h") {
        now + Duration::hours(1)
    } else if lower.contains("day") || lower.contains("24") || lower.contains("1 d") {
        now + Duration::days(1)
    } else if lower.contains("30") {
        now + Duration::days(30)
    } else if lower.contains("90") {
        now + Duration::days(90)
    } else {
        now + Duration::days(7)
    };
    expires_at.to_rfc3339()
}

fn parse_max_uses(label: &str) -> u32 {
    label
        .split(|ch: char| !ch.is_ascii_digit())
        .find_map(|part| part.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(5)
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
    fn create_user_transitions_ready_and_persists() {
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
            .map(|profile| {
                profile
                    .recovery_status
                    .contains("content keys restored: false")
            })
            .unwrap_or(false));
        assert!(state
            .events
            .iter()
            .any(|event| event.kind == "identity.recovered"));
        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.devices[0].label, "Desktop");
        assert_eq!(state.devices[0].identity_key.len(), 64);
        assert_eq!(state.devices[0].device_key.len(), 64);
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
        assert!(invite_state.invites[0].code.contains("room_secret="));
        assert_eq!(invite_state.invites[0].uses, 0);
        assert!(!invite_state.invites[0].room_secret_hash.is_empty());
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
    fn command_health_covers_full_user_flow() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("health");
        let health = command_health();
        assert!(health.app_state_ready);
        assert!(health.identity_ready);
        assert!(health.verification_ready);
        assert!(health.collaboration_ready);
        assert!(!health.voice_ready);
        assert!(health.honest_copy_ready);
    }
}

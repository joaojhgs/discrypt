//! Tauri command surface and local-first app-state service for the native discrypt shell.
use discrypt_core::{
    app_snapshot as core_app_snapshot, verify_safety_number as core_verify_safety_number,
    AppSnapshot, ChannelKind, ChannelView as SnapshotChannelView, DeviceView,
    MessageView as SnapshotMessageView, SafetyVerificationRequest, SafetyVerificationResult,
    SecurityCopyView, ServerView,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const APP_STATE_SCHEMA_VERSION: u32 = 1;
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
    /// Group id this invite targets.
    pub group_id: String,
    /// User-pastable invite code/URL.
    pub code: String,
    /// Expiry label.
    pub expires: String,
    /// Maximum-use label.
    pub max_use: String,
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
    /// Whether this participant is muted.
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

/// Request to recover a local user profile placeholder.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoverUserRequest {
    /// Display name.
    pub display_name: String,
    /// Local recovery phrase/code placeholder.
    pub recovery_code: String,
    /// Optional device label.
    pub device_name: Option<String>,
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

/// Command result for local E2E/smoke execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandHealth {
    /// Compatibility flag for older harnesses that expected snapshot readiness.
    pub snapshot_ready: bool,
    /// Compatibility flag for older harnesses that expected safety verification readiness.
    pub verification_ready: bool,
    /// Canonical app-state command is available.
    pub app_state_ready: bool,
    /// Identity lifecycle commands are available.
    pub identity_ready: bool,
    /// Group/channel/message/invite commands are available.
    pub collaboration_ready: bool,
    /// Voice join/leave/mute/volume commands are available.
    pub voice_ready: bool,
    /// Honest security copy is present.
    pub honest_copy_ready: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    security_copy: SecurityCopyView,
    events: Vec<AppEventView>,
    friend_verified: bool,
    next_sequence: u64,
}

static APP_STATE: OnceLock<Result<Mutex<PersistedAppState>, String>> = OnceLock::new();

/// Tauri command: return the transitional compatibility snapshot for older clients.
pub fn app_snapshot() -> AppSnapshot {
    with_state(|state| state.to_snapshot())
}

/// Tauri command: return the full command-backed app state for the React shell.
pub fn app_state() -> Result<AppStateView, String> {
    with_state(PersistedAppState::to_view)
}

/// Tauri command: create a new local user profile.
pub fn create_user(request: CreateUserRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        let display_name = normalize_label(&request.display_name, "Alice");
        let device_name = normalize_label(&request.device_name, "this device");
        state.user = Some(UserIdentityView {
            user_id: stable_id("user", &display_name),
            display_name: display_name.clone(),
            device_name,
            recovery_hint: "Local recovery placeholder created on this device; QR/cross-device recovery is not enabled yet.".to_owned(),
        });
        state.lifecycle = LifecycleStage::Ready;
        ensure_default_collaboration_state(state, &display_name);
        state.push_event(
            "identity.created",
            format!("Created local user {display_name}"),
        );
        Ok(())
    })
}

/// Tauri command: recover/select an existing local user placeholder.
pub fn recover_user(request: RecoverUserRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        let display_name = normalize_label(&request.display_name, "Recovered user");
        let device_name = normalize_label(&request.device_name, "recovered device");
        let code = normalize_label(&request.recovery_code, "manual-local-recovery");
        state.user = Some(UserIdentityView {
            user_id: stable_id("user", &format!("{display_name}-{code}")),
            display_name: display_name.clone(),
            device_name,
            recovery_hint: "Recovered a local profile placeholder only. This build does not claim QR, backup, or cross-device content-key recovery.".to_owned(),
        });
        state.lifecycle = LifecycleStage::Ready;
        ensure_default_collaboration_state(state, &display_name);
        state.push_event(
            "identity.recovered",
            "Recovered local user placeholder with honest copy",
        );
        Ok(())
    })
}

/// Tauri command: create a new local user and unlock the shell.
pub fn create_user(request: CreateUserRequest) -> AppStateView {
    mutate_state(|state| state.create_user(request, false))
}

/// Tauri command: recover an existing local user placeholder and unlock the shell.
pub fn recover_user(request: RecoverUserRequest) -> AppStateView {
    mutate_state(|state| {
        state.create_user(
            CreateUserRequest {
                display_name: request.display_name,
                device_name: request.device_name,
            },
            true,
        );
        state.push_event(
            "identity.recovered",
            format!(
                "Local recovery placeholder accepted; code length {} was not treated as cloud/key recovery",
                request.recovery_code.trim().len()
            ),
        );
    })
}

/// Tauri command: verify a user-confirmed safety-number comparison and persist success.
pub fn verify_safety_number(
    request: SafetyVerificationRequest,
) -> Result<SafetyVerificationResult, String> {
    let result = core_verify_safety_number(request);
    if result.verified {
        mutate_state(|state| {
            state.friend_verified = true;
            state.push_event(
                "friend.verified",
                "Safety number verified and persisted for this profile",
            );
        });
    }
    Ok(result)
}

/// Tauri command: save theme/template preferences.
pub fn save_preferences(request: SavePreferencesRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        state.preferences = UiPreferencesView {
            theme_id: normalize_label(&request.theme_id, DEFAULT_THEME_ID),
            template_id: normalize_label(&request.template_id, DEFAULT_TEMPLATE_ID),
        };
        state.push_event("preferences.saved", "Theme/template preferences saved");
        Ok(())
    })
}

/// Tauri command: start or open a direct-message timeline.
pub fn start_dm(request: StartDmRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        require_identity(state)?;
        let peer_label = normalize_label(&request.peer_label, "Bob");
        let dm_id = stable_id("dm", &peer_label);
        if !state.dms.iter().any(|dm| dm.dm_id == dm_id) {
            state.dms.push(DmView {
                dm_id: dm_id.clone(),
                peer_label: peer_label.clone(),
            });
        }
        state.active_dm_id = Some(dm_id);
        state.push_event("dm.opened", format!("Opened DM with {peer_label}"));
        Ok(())
    })
}

/// Tauri command: start or focus a direct-message conversation.
pub fn start_dm(request: StartDmRequest) -> AppStateView {
    mutate_state(|state| {
        state.ensure_ready_profile();
        let display_name = normalize_label(&request.display_name, "Bob");
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
pub fn create_group(request: CreateGroupRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        state.ensure_ready_profile();
        let name = normalize_label(&request.name, "private lab");
        let group_id = stable_id("group", &name, state.next_sequence);
        if !state.groups.iter().any(|group| group.name == name) {
            state.groups.push(GroupView {
                group_id: group_id.clone(),
                name: name.clone(),
                role: "owner".to_owned(),
                channels: Vec::new(),
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

/// Tauri command: join a local-first group from an invite.
pub fn join_group(request: JoinGroupRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        state.ensure_ready_profile();
        let invite_code = normalize_label(&request.invite_code, "manual invite");
        let name = request
            .group_name
            .map(|value| normalize_label(&value, "joined enclave"))
            .unwrap_or_else(|| {
                if invite_code.contains("enclave") {
                    "joined enclave".to_owned()
                } else {
                    "joined group".to_owned()
                }
            });
        let group_id = stable_id("group", &name, state.next_sequence);
        if !state.groups.iter().any(|group| group.name == name) {
            state.groups.push(GroupView {
                group_id: group_id.clone(),
                name: name.clone(),
                role: "member".to_owned(),
                channels: Vec::new(),
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

/// Tauri command: create a channel in a group.
pub fn create_channel(request: CreateChannelRequest) -> Result<AppStateView, String> {
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
        let invite = InviteView {
            invite_id: format!("invite-{sequence}"),
            group_id: group_id.clone(),
            code: format!("discrypt://join/{sequence}-{group_name}"),
            expires: normalize_label(&request.expires, "Invite expires and can be revoked"),
            max_use: normalize_label(&request.max_use, "Max-use is enforced before MLS admission"),
            admission_copy: "Final admission still requires an authorized MLS Welcome/add"
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
            return Err("message body must not be empty".to_owned());
        }
        validate_message_target(state, &request.target)?;
        let sequence = state.next_sequence;
        let author = state
            .profile
            .as_ref()
            .map(|profile| profile.display_name.clone())
            .unwrap_or_else(|| "Alice".to_owned());
        let message = MessageView {
            message_id: format!("msg-{sequence}"),
            target: request.target,
            author_id: "local-user".to_owned(),
            author,
            body: body.to_owned(),
            status: "local encrypted-message facade persisted; relay/network delivery not claimed"
                .to_owned(),
            sent_at: format!("local-{sequence}"),
        });
        state.push_event(
            "message.sent",
            "Message persisted to local encrypted timeline facade",
        );
        Ok(())
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
        state.voice_session = Some(VoiceSessionView {
            session_id: session_id.clone(),
            group_id: request.group_id.clone(),
            channel_id: request.channel_id.clone(),
            joined: true,
            self_muted,
            participants: default_voice_participants(!self_muted),
            route_copy: "STUN → peer relay overlay → TURN; route is harness-backed".to_owned(),
            status_copy: "Voice session state joined; real audio-frame media remains release-gated"
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
                    "Not joined; transport/media unavailable until real adapter gates pass"
                        .to_owned();
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
pub fn set_self_mute(request: SetSelfMuteRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        if let Some(session) = &mut state.voice_session {
            if session.session_id == request.session_id {
                session.self_muted = request.muted;
                for participant in &mut session.participants {
                    if participant.id == "local-user" {
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
pub fn set_speaker_volume(request: SetSpeakerVolumeRequest) -> Result<AppStateView, String> {
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
        state.push_event("voice.volume", format!("Set speaker volume to {volume}"));
        Ok(())
    })
}

/// Tauri command: return recent command-backed app events for polling clients.
pub fn poll_app_events() -> Result<Vec<AppEventView>, String> {
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
    let honest_copy_ready = deletion_warning().contains("pending on offline devices")
        && metadata_warning().contains("does not claim anonymity");
    CommandHealth {
        snapshot_ready: state.snapshot.schema_version >= APP_STATE_SCHEMA_VERSION,
        verification_ready: true,
        app_state_ready: state.schema_version == APP_STATE_SCHEMA_VERSION,
        identity_ready: matches!(
            state.lifecycle,
            AppLifecycle::FirstRun | AppLifecycle::Ready
        ),
        collaboration_ready: true,
        voice_ready: true,
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
    pub(super) fn app_snapshot() -> Result<AppSnapshot, String> {
        super::app_snapshot()
    }

    #[tauri::command]
    pub(super) fn app_state() -> Result<AppStateView, String> {
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
    ) -> Result<SafetyVerificationResult, String> {
        super::verify_safety_number(request)
    }

    #[tauri::command]
    pub(super) fn save_preferences(
        request: SavePreferencesRequest,
    ) -> Result<AppStateView, String> {
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
    pub(super) fn join_group(request: JoinGroupRequest) -> Result<AppStateView, String> {
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
    pub(super) fn send_message(request: SendMessageRequest) -> Result<AppStateView, String> {
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
    pub(super) fn set_self_mute(request: SetSelfMuteRequest) -> Result<AppStateView, String> {
        super::set_self_mute(request)
    }

    #[tauri::command]
    pub(super) fn set_speaker_volume(
        request: SetSpeakerVolumeRequest,
    ) -> Result<AppStateView, String> {
        super::set_speaker_volume(request)
    }

    #[tauri::command]
    pub(super) fn poll_app_events() -> Result<Vec<AppEventView>, String> {
        super::poll_app_events()
    }

    #[tauri::command]
    pub(super) fn deletion_warning() -> Result<String, String> {
        super::deletion_warning()
    }

    #[tauri::command]
    pub(super) fn metadata_warning() -> Result<String, String> {
        super::metadata_warning()
    }

    #[tauri::command]
    pub(super) fn command_health() -> Result<CommandHealth, String> {
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
            ipc_commands::save_preferences,
            ipc_commands::start_dm,
            ipc_commands::create_group,
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
        if let Some(profile) = &self.profile {
            snapshot.friend.alias = profile.display_name.clone();
        }
        snapshot.devices = self.devices.clone();
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
                participants: default_voice_participants(false)
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
                status_copy: "Not joined; voice session is optional and shell-safe".to_owned(),
                route_copy: "Route copy is harness-backed until socket/media adapter E2E passes"
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
        self.devices = vec![DeviceView {
            device_id: slugify(&device_name),
            leaf_index: 1,
            local: true,
            authorized: true,
        }];
        if self.dms.is_empty() {
            self.dms.push(DirectConversationView {
                dm_id: "dm-bob".to_owned(),
                participant_id: "bob".to_owned(),
                display_name: "Bob".to_owned(),
                local_only_copy: "Default local DM fixture; no remote delivery is claimed"
                    .to_owned(),
            });
        }
        self.active_context = self.active_context.clone().or(Some(ActiveContextView {
            kind: "dm".to_owned(),
            group_id: None,
            channel_id: None,
            dm_id: Some("dm-bob".to_owned()),
        }));
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
}

fn with_state<T>(read: impl FnOnce(&PersistedAppState) -> T) -> Result<T, String> {
    let state = state_mutex()?;
    let guard = state
        .lock()
        .map_err(|_| "app state lock poisoned".to_owned())?;
    Ok(read(&guard))
}

fn mutate_state(update: impl FnOnce(&mut PersistedAppState)) -> AppStateView {
    let state = APP_STATE.get_or_init(|| Mutex::new(load_state()));
    let mut guard = state
        .lock()
        .map_err(|_| "app state lock poisoned".to_owned())?;
    update(&mut guard)?;
    persist_state(&guard)?;
    Ok(guard.to_view())
}

fn state_mutex() -> Result<&'static Mutex<PersistedAppState>, String> {
    APP_STATE
        .get_or_init(|| load_state().map(Mutex::new))
        .as_ref()
        .map_err(Clone::clone)
}

fn load_state() -> Result<PersistedAppState, String> {
    let path = state_path();
    if let Ok(contents) = fs::read_to_string(path) {
        if let Ok(state) = serde_json::from_str::<PersistedAppState>(&contents) {
            if state.schema_version == APP_STATE_SCHEMA_VERSION {
                return state;
            }
        }
    };
    normalize_loaded_state(&mut state);
    persist_state(&state)?;
    Ok(state)
}

fn persist_state(state: &PersistedAppState) -> Result<(), String> {
    let path = state_path();
    atomic_write_json(&path, state)
        .map_err(|error| format!("failed to persist app state at {}: {error}", path.display()))
}

fn atomic_write_json(path: &Path, state: &PersistedAppState) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(encoded) = serde_json::to_string_pretty(state) {
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, encoded).is_ok() {
            let _ = fs::rename(tmp, path);
        }
    }
    let encoded = serde_json::to_vec_pretty(state)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, encoded)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn state_path() -> PathBuf {
    if let Some(path) = std::env::var_os("DISCRYPT_APP_STATE_PATH") {
        return PathBuf::from(path);
    }
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(data_home)
            .join("discrypt")
            .join("app-state.json");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("discrypt")
            .join("app-state.json");
    }
    PathBuf::from("discrypt-app-state.json")
}

fn default_voice_participants(local_speaking: bool) -> Vec<VoiceParticipantView> {
    vec![
        VoiceParticipantView {
            id: "local-user".to_owned(),
            name: "You".to_owned(),
            role: "you".to_owned(),
            speaking: local_speaking,
            muted: false,
            volume: 82,
        },
        VoiceParticipantView {
            id: "bob".to_owned(),
            name: "Bob".to_owned(),
            role: "friend".to_owned(),
            speaking: false,
            muted: false,
            volume: 68,
        },
        VoiceParticipantView {
            id: "ops".to_owned(),
            name: "Ops relay".to_owned(),
            role: "route".to_owned(),
            speaking: false,
            muted: true,
            volume: 38,
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

fn normalize_channel_name(value: &str, kind: ChannelKind) -> String {
    let trimmed = normalize_label(value.trim_start_matches('#'), "general");
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
        });
        assert_eq!(state.lifecycle, AppLifecycle::Ready);
        assert!(state
            .profile
            .as_ref()
            .map(|profile| profile.recovery_status.contains("placeholder"))
            .unwrap_or(false));
        assert!(state
            .events
            .iter()
            .any(|event| event.kind == "identity.recovered"));
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
        assert!(invite_state.invites[0].code.starts_with("discrypt://join/"));
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
            display_name: "Bob".to_owned(),
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
        let volume = set_speaker_volume(SetSpeakerVolumeRequest {
            session_id: session_id.clone(),
            participant_id: "bob".to_owned(),
            volume: 55,
        });
        assert_eq!(
            volume
                .voice_session
                .as_ref()
                .and_then(|session| session
                    .participants
                    .iter()
                    .find(|participant| participant.id == "bob"))
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
    fn persistence_uses_env_override_and_atomic_shape() {
        let _guard = test_lock();
        let path = reset_with_temp_state("atomic");
        create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: None,
        });
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
        let contents = fs::read_to_string(path).unwrap_or_default();
        assert!(contents.contains("schema_version"));
    }

    #[test]
    fn command_health_covers_full_user_flow() {
        let _guard = test_lock();
        let _path = reset_with_temp_state("health");
        let health = command_health();
        assert!(health.app_state_ready);
        assert!(health.identity_ready);
        assert!(health.collaboration_ready);
        assert!(health.voice_ready);
        assert!(health.honest_copy_ready);
    }
}

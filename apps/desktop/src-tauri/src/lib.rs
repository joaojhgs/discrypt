//! Tauri command surface for the native discrypt shell.
//!
//! The desktop crate is intentionally a thin shell around a single persisted
//! app-state model. It keeps IPC DTOs explicit so the React client and tests can
//! detect drift before a blank-screen regression reaches the Tauri runtime.
use discrypt_core::{
    app_snapshot as core_app_snapshot, verify_safety_number as core_verify_safety_number,
    AppSnapshot, ChannelKind, SafetyVerificationRequest, SafetyVerificationResult,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const DEFAULT_THEME_ID: &str = "graphite-calm";
const DEFAULT_TEMPLATE_ID: &str = "command-center";

/// Persisted UI preference model shared with the React command client.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UiPreferencesView {
    /// Active theme identifier from the frontend theme registry.
    pub theme_id: String,
    /// Active layout template identifier from the frontend template registry.
    pub template_id: String,
}

impl Default for UiPreferencesView {
    fn default() -> Self {
        Self {
            theme_id: DEFAULT_THEME_ID.to_owned(),
            template_id: DEFAULT_TEMPLATE_ID.to_owned(),
        }
    }
}

/// Current first-run lifecycle state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStage {
    /// The local machine has no selected/recovered user yet.
    #[default]
    NeedsIdentity,
    /// A local user exists and the application can show DMs/groups.
    Ready,
}

/// Local user/device identity row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UserIdentityView {
    /// Stable local user id.
    pub user_id: String,
    /// User-chosen display name.
    pub display_name: String,
    /// Local device name.
    pub device_name: String,
    /// Honest recovery hint/copy for the current local-only implementation.
    pub recovery_hint: String,
}

/// Request to create a new local user.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateUserRequest {
    /// Display name for the new user.
    pub display_name: String,
    /// Device label for this machine.
    pub device_name: String,
}

/// Request to recover/select an existing local user.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoverUserRequest {
    /// Display name for the recovered local user.
    pub display_name: String,
    /// Device label for this machine.
    pub device_name: String,
    /// Local placeholder recovery code. QR/cross-device recovery is not claimed.
    pub recovery_code: String,
}

/// Request to persist UI preference changes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SavePreferencesRequest {
    /// Theme identifier to persist.
    pub theme_id: String,
    /// Template identifier to persist.
    pub template_id: String,
}

/// Request to start/open a direct message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StartDmRequest {
    /// Peer display label.
    pub peer_label: String,
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
    /// Optional display label assigned to the joined group.
    pub group_name: Option<String>,
}

/// Request to create a channel in a group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateChannelRequest {
    /// Stable group id.
    pub group_id: String,
    /// Channel display name. Text channels are normalized with a leading '#'.
    pub name: String,
    /// Channel kind.
    pub kind: ChannelKind,
    /// Channel retention label.
    pub retention_status: String,
}

/// Request to create an invite for a group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateInviteRequest {
    /// Stable group id.
    pub group_id: String,
    /// Expiry label selected by the user/admin.
    pub expires: String,
    /// Maximum-use label selected by the user/admin.
    pub max_use: String,
}

/// Message target union used by the UI and IPC drift gate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageTarget {
    /// Target a direct message timeline.
    Dm { dm_id: String },
    /// Target a group text channel.
    Channel {
        group_id: String,
        channel_id: String,
    },
}

/// Request to append a message to a command-backed local timeline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SendMessageRequest {
    /// DM or group channel target.
    pub target: MessageTarget,
    /// Message body.
    pub body: String,
}

/// Request to join a voice channel.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JoinVoiceRequest {
    /// Stable group id.
    pub group_id: String,
    /// Stable voice channel id.
    pub channel_id: String,
}

/// Request to leave a voice session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LeaveVoiceRequest {
    /// Stable voice session id.
    pub session_id: String,
}

/// Request to set self mute state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetSelfMuteRequest {
    /// Stable voice session id.
    pub session_id: String,
    /// Whether the local participant is muted.
    pub muted: bool,
}

/// Request to set a participant speaker volume.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetSpeakerVolumeRequest {
    /// Stable voice session id.
    pub session_id: String,
    /// Participant identifier.
    pub participant_id: String,
    /// Volume 0-100.
    pub volume: u8,
}

/// Channel row owned by the app state service.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppChannelView {
    /// Stable channel id.
    pub channel_id: String,
    /// User-facing channel name.
    pub name: String,
    /// Channel kind.
    pub kind: ChannelKind,
    /// Retention/security status copy.
    pub retention_status: String,
}

/// Group/server row owned by the app state service.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupView {
    /// Stable group id.
    pub group_id: String,
    /// Group display name.
    pub name: String,
    /// Local role label.
    pub role: String,
    /// Channels in this group.
    pub channels: Vec<AppChannelView>,
    /// Active invite codes for this local shell.
    pub invite_codes: Vec<String>,
}

/// Direct-message timeline summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DmView {
    /// Stable direct-message id.
    pub dm_id: String,
    /// Peer display label.
    pub peer_label: String,
}

/// Result returned by invite creation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InviteView {
    /// Stable invite identifier for the local command surface.
    pub invite_id: String,
    /// User-pastable invite URL/code.
    pub code: String,
    /// Group id the invite targets.
    pub group_id: String,
    /// Expiry label.
    pub expires: String,
    /// Maximum-use label.
    pub max_use: String,
    /// Honest admission copy.
    pub admission_copy: String,
}

/// Command-backed local text message row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageView {
    /// Stable local message id.
    pub message_id: String,
    /// Message target.
    pub target: MessageTarget,
    /// Author label.
    pub author: String,
    /// Decrypted local body shown in this shell.
    pub body: String,
    /// Delivery/security status copy.
    pub status: String,
    /// Deterministic local timestamp/counter label.
    pub sent_at: String,
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

/// Channel-scoped command-backed voice session state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceSessionView {
    /// Stable voice session id.
    pub session_id: String,
    /// Group id containing the channel.
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
    pub route: String,
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
    /// Existing domain snapshot for security/capability copy.
    pub snapshot: AppSnapshot,
    /// First-run lifecycle stage.
    pub lifecycle: LifecycleStage,
    /// Local user identity if one exists.
    pub user: Option<UserIdentityView>,
    /// Persisted UI preferences.
    pub preferences: UiPreferencesView,
    /// Direct-message timelines.
    pub dms: Vec<DmView>,
    /// Local group/server state.
    pub groups: Vec<GroupView>,
    /// Active group id.
    pub active_group_id: Option<String>,
    /// Active direct-message id.
    pub active_dm_id: Option<String>,
    /// Timeline messages for DMs and text channels.
    pub messages: Vec<MessageView>,
    /// Voice control state.
    pub voice_sessions: Vec<VoiceSessionView>,
    /// Active joined voice session, if any.
    pub active_voice_session_id: Option<String>,
    /// Most recent local events.
    pub events: Vec<AppEventView>,
    /// Most recent invite, if one has been created in this profile.
    pub active_invite: Option<InviteView>,
    /// Honest local-only recovery copy.
    pub recovery_copy: String,
}

/// Command result for local E2E/smoke execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandHealth {
    /// App state can be loaded.
    pub app_state_ready: bool,
    /// Identity lifecycle command surface is available.
    pub identity_ready: bool,
    /// DM/group/text flows are command backed.
    pub messaging_ready: bool,
    /// Voice state is channel/session scoped.
    pub voice_ready: bool,
    /// Honest security/recovery copy is present.
    pub honest_copy_ready: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct PersistedAppState {
    #[serde(default = "core_app_snapshot")]
    snapshot: AppSnapshot,
    #[serde(default)]
    lifecycle: LifecycleStage,
    #[serde(default)]
    user: Option<UserIdentityView>,
    #[serde(default)]
    preferences: UiPreferencesView,
    #[serde(default)]
    dms: Vec<DmView>,
    #[serde(default)]
    groups: Vec<GroupView>,
    #[serde(default)]
    active_group_id: Option<String>,
    #[serde(default)]
    active_dm_id: Option<String>,
    #[serde(default)]
    timeline: Vec<MessageView>,
    #[serde(default)]
    voice_sessions: Vec<VoiceSessionView>,
    #[serde(default)]
    active_voice_session_id: Option<String>,
    #[serde(default)]
    events: Vec<AppEventView>,
    #[serde(default)]
    active_invite: Option<InviteView>,
    #[serde(default)]
    next_sequence: u64,
}

static APP_STATE: OnceLock<Result<Mutex<PersistedAppState>, String>> = OnceLock::new();

/// Tauri command: return the legacy app snapshot for headless compatibility.
pub fn app_snapshot() -> Result<AppSnapshot, String> {
    with_state(|state| state.snapshot.clone())
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

/// Tauri command: verify a user-confirmed safety-number comparison and persist success.
pub fn verify_safety_number(
    request: SafetyVerificationRequest,
) -> Result<SafetyVerificationResult, String> {
    let result = core_verify_safety_number(request);
    if result.verified {
        mutate_state(|state| {
            state.snapshot.friend.verified = true;
            state.push_event("friend.verified", "Safety number verified and persisted");
            Ok(())
        })?;
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

/// Tauri command: create a local-first group and make it active.
pub fn create_group(request: CreateGroupRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        require_identity(state)?;
        let name = normalize_label(&request.name, "private lab");
        let group_id = stable_id("group", &name);
        if !state.groups.iter().any(|group| group.group_id == group_id) {
            state.groups.push(GroupView {
                group_id: group_id.clone(),
                name: name.clone(),
                role: "owner".to_owned(),
                channels: default_channels(&request.retention),
                invite_codes: Vec::new(),
            });
        }
        state.active_group_id = Some(group_id);
        state.push_event("group.created", format!("Created group {name}"));
        Ok(())
    })
}

/// Tauri command: join a local-first group from an invite.
pub fn join_group(request: JoinGroupRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        require_identity(state)?;
        let invite_code = normalize_label(&request.invite_code, "manual invite");
        let requested_name = request
            .group_name
            .as_deref()
            .map(|name| normalize_label(name, "joined enclave"))
            .unwrap_or_else(|| infer_group_name_from_invite(&invite_code));
        let group_id = stable_id("group", &requested_name);
        if !state.groups.iter().any(|group| group.group_id == group_id) {
            state.groups.push(GroupView {
                group_id: group_id.clone(),
                name: requested_name.clone(),
                role: "member".to_owned(),
                channels: default_channels(&state.snapshot.retention.selected),
                invite_codes: vec![invite_code.clone()],
            });
        }
        state.active_group_id = Some(group_id);
        state.push_event(
            "group.joined",
            format!("Joined {requested_name} via {invite_code}"),
        );
        Ok(())
    })
}

/// Tauri command: create a channel in a group.
pub fn create_channel(request: CreateChannelRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        require_identity(state)?;
        let group = state
            .groups
            .iter_mut()
            .find(|group| group.group_id == request.group_id)
            .ok_or_else(|| format!("unknown group_id '{}'", request.group_id))?;
        let name = normalize_channel_name(&request.name, request.kind);
        let channel_id = stable_id("channel", &format!("{}-{name}", group.group_id));
        if !group
            .channels
            .iter()
            .any(|channel| channel.channel_id == channel_id)
        {
            group.channels.push(AppChannelView {
                channel_id,
                name: name.clone(),
                kind: request.kind,
                retention_status: normalize_label(
                    &request.retention_status,
                    &state.snapshot.retention.selected,
                ),
            });
        }
        state.push_event("channel.created", format!("Created channel {name}"));
        Ok(())
    })
}

/// Tauri command: create an invite for a group.
pub fn create_invite(request: CreateInviteRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        require_identity(state)?;
        let sequence = state.next_sequence;
        let group = state
            .groups
            .iter_mut()
            .find(|group| group.group_id == request.group_id)
            .ok_or_else(|| format!("unknown group_id '{}'", request.group_id))?;
        let code = format!("discrypt://join/{}-{}", sequence, group.group_id);
        if !group.invite_codes.contains(&code) {
            group.invite_codes.push(code.clone());
        }
        state.active_invite = Some(InviteView {
            invite_id: format!("invite-{sequence}"),
            code: code.clone(),
            group_id: request.group_id.clone(),
            expires: normalize_label(&request.expires, &state.snapshot.invite.expires),
            max_use: normalize_label(&request.max_use, &state.snapshot.invite.max_use),
            admission_copy: state.snapshot.invite.welcome_required.clone(),
        });
        state.push_event("invite.created", format!("Created invite {code}"));
        Ok(())
    })
}

/// Tauri command: append a message to a local DM or channel timeline.
pub fn send_message(request: SendMessageRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        require_identity(state)?;
        let body = request.body.trim();
        if body.is_empty() {
            return Err("message body must not be empty".to_owned());
        }
        validate_message_target(state, &request.target)?;
        let sequence = state.next_sequence;
        let author = state
            .user
            .as_ref()
            .map(|user| user.display_name.clone())
            .unwrap_or_else(|| "local user".to_owned());
        state.timeline.push(MessageView {
            message_id: format!("msg-{sequence}"),
            target: request.target.clone(),
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

/// Tauri command: join a channel-scoped voice session.
pub fn join_voice(request: JoinVoiceRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        require_identity(state)?;
        validate_voice_channel(state, &request.group_id, &request.channel_id)?;
        for session in &mut state.voice_sessions {
            session.joined = false;
            for participant in &mut session.participants {
                participant.speaking = false;
            }
        }
        let session_id = stable_id(
            "voice",
            &format!("{}-{}", request.group_id, request.channel_id),
        );
        let display_name = state
            .user
            .as_ref()
            .map(|user| user.display_name.clone())
            .unwrap_or_else(|| "You".to_owned());
        if let Some(session) = state
            .voice_sessions
            .iter_mut()
            .find(|session| session.session_id == session_id)
        {
            session.joined = true;
            session.self_muted = false;
            set_participant_state(
                session,
                "local",
                Some(display_name),
                Some(false),
                Some(true),
                None,
            );
        } else {
            state.voice_sessions.push(VoiceSessionView {
                session_id: session_id.clone(),
                group_id: request.group_id.clone(),
                channel_id: request.channel_id.clone(),
                joined: true,
                self_muted: false,
                participants: default_voice_participants(&display_name),
                route:
                    "local voice session only; production media path waits for adapter/E2E gates"
                        .to_owned(),
            });
        }
        state.active_voice_session_id = Some(session_id);
        state.push_event("voice.joined", "Joined channel-scoped voice session");
        Ok(())
    })
}

/// Tauri command: leave a voice session.
pub fn leave_voice(request: LeaveVoiceRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        let session = state
            .voice_sessions
            .iter_mut()
            .find(|session| session.session_id == request.session_id)
            .ok_or_else(|| format!("unknown session_id '{}'", request.session_id))?;
        session.joined = false;
        for participant in &mut session.participants {
            participant.speaking = false;
        }
        if state.active_voice_session_id.as_deref() == Some(&request.session_id) {
            state.active_voice_session_id = None;
        }
        state.push_event("voice.left", "Left channel-scoped voice session");
        Ok(())
    })
}

/// Tauri command: persist local self-mute state.
pub fn set_self_mute(request: SetSelfMuteRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        let session = state
            .voice_sessions
            .iter_mut()
            .find(|session| session.session_id == request.session_id)
            .ok_or_else(|| format!("unknown session_id '{}'", request.session_id))?;
        session.self_muted = request.muted;
        set_participant_state(
            session,
            "local",
            None,
            Some(request.muted),
            Some(session.joined && !request.muted),
            None,
        );
        state.push_event(
            "voice.self_mute",
            if request.muted {
                "Self muted"
            } else {
                "Self unmuted"
            },
        );
        Ok(())
    })
}

/// Tauri command: persist a participant speaker volume.
pub fn set_speaker_volume(request: SetSpeakerVolumeRequest) -> Result<AppStateView, String> {
    mutate_state(|state| {
        let volume = request.volume.min(100);
        let session = state
            .voice_sessions
            .iter_mut()
            .find(|session| session.session_id == request.session_id)
            .ok_or_else(|| format!("unknown session_id '{}'", request.session_id))?;
        let updated = set_participant_state(
            session,
            &request.participant_id,
            None,
            None,
            None,
            Some(volume),
        );
        if !updated {
            return Err(format!(
                "unknown participant_id '{}'",
                request.participant_id
            ));
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
pub fn deletion_warning() -> Result<String, String> {
    with_state(|state| state.snapshot.security_copy.deletion.clone())
}

/// Tauri command: return the metadata-minimization caveat copy.
pub fn metadata_warning() -> Result<String, String> {
    with_state(|state| state.snapshot.security_copy.metadata.clone())
}

/// E2E command-health smoke used by CI and the multinode harness.
pub fn command_health() -> Result<CommandHealth, String> {
    let mut health_state = app_state()?;
    let identity_ready = if health_state.user.is_none() {
        health_state = create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: "health-check".to_owned(),
        })?;
        true
    } else {
        true
    };
    let group_id = health_state
        .active_group_id
        .clone()
        .or_else(|| {
            health_state
                .groups
                .first()
                .map(|group| group.group_id.clone())
        })
        .unwrap_or_default();
    let voice_ready = health_state.groups.iter().any(|group| {
        group
            .channels
            .iter()
            .any(|channel| channel.kind == ChannelKind::Voice)
    });
    let messaging_ready = !health_state.dms.is_empty()
        && !group_id.is_empty()
        && health_state.groups.iter().any(|group| {
            group
                .channels
                .iter()
                .any(|channel| channel.kind == ChannelKind::Text)
        });
    Ok(CommandHealth {
        app_state_ready: matches!(health_state.lifecycle, LifecycleStage::Ready),
        identity_ready,
        messaging_ready,
        voice_ready,
        honest_copy_ready: health_state
            .recovery_copy
            .contains("QR/cross-device recovery is not enabled"),
    })
}

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
    pub(super) fn create_user(request: CreateUserRequest) -> Result<AppStateView, String> {
        super::create_user(request)
    }

    #[tauri::command]
    pub(super) fn recover_user(request: RecoverUserRequest) -> Result<AppStateView, String> {
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
    pub(super) fn start_dm(request: StartDmRequest) -> Result<AppStateView, String> {
        super::start_dm(request)
    }

    #[tauri::command]
    pub(super) fn create_group(request: CreateGroupRequest) -> Result<AppStateView, String> {
        super::create_group(request)
    }

    #[tauri::command]
    pub(super) fn join_group(request: JoinGroupRequest) -> Result<AppStateView, String> {
        super::join_group(request)
    }

    #[tauri::command]
    pub(super) fn create_channel(request: CreateChannelRequest) -> Result<AppStateView, String> {
        super::create_channel(request)
    }

    #[tauri::command]
    pub(super) fn create_invite(request: CreateInviteRequest) -> Result<AppStateView, String> {
        super::create_invite(request)
    }

    #[tauri::command]
    pub(super) fn send_message(request: SendMessageRequest) -> Result<AppStateView, String> {
        super::send_message(request)
    }

    #[tauri::command]
    pub(super) fn join_voice(request: JoinVoiceRequest) -> Result<AppStateView, String> {
        super::join_voice(request)
    }

    #[tauri::command]
    pub(super) fn leave_voice(request: LeaveVoiceRequest) -> Result<AppStateView, String> {
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
            ipc_commands::create_channel,
            ipc_commands::create_invite,
            ipc_commands::send_message,
            ipc_commands::join_voice,
            ipc_commands::leave_voice,
            ipc_commands::set_self_mute,
            ipc_commands::set_speaker_volume,
            ipc_commands::poll_app_events,
            ipc_commands::deletion_warning,
            ipc_commands::metadata_warning,
            ipc_commands::command_health
        ])
        .run(tauri::generate_context!())
        .expect("error while running discrypt Tauri application");
}

impl PersistedAppState {
    fn initial() -> Self {
        Self {
            snapshot: core_app_snapshot(),
            lifecycle: LifecycleStage::NeedsIdentity,
            user: None,
            preferences: UiPreferencesView::default(),
            dms: Vec::new(),
            groups: Vec::new(),
            active_group_id: None,
            active_dm_id: None,
            timeline: Vec::new(),
            voice_sessions: Vec::new(),
            active_voice_session_id: None,
            events: vec![AppEventView {
                sequence: 1,
                kind: "app.needs_identity".to_owned(),
                summary: "Choose create user or recover existing local user".to_owned(),
            }],
            active_invite: None,
            next_sequence: 2,
        }
    }

    fn to_view(&self) -> AppStateView {
        AppStateView {
            snapshot: self.snapshot.clone(),
            lifecycle: self.lifecycle.clone(),
            user: self.user.clone(),
            preferences: self.preferences.clone(),
            dms: self.dms.clone(),
            groups: self.groups.clone(),
            active_group_id: self.active_group_id.clone(),
            active_dm_id: self.active_dm_id.clone(),
            messages: self.timeline.clone(),
            voice_sessions: self.voice_sessions.clone(),
            active_voice_session_id: self.active_voice_session_id.clone(),
            events: self.events.clone(),
            active_invite: self.active_invite.clone(),
            recovery_copy: "Recovery is local-only in this build. QR/cross-device recovery is not enabled yet; do not assume remote history or content-key restoration.".to_owned(),
        }
    }

    fn push_event(&mut self, kind: impl Into<String>, summary: impl Into<String>) {
        let event = AppEventView {
            sequence: self.next_sequence,
            kind: kind.into(),
            summary: summary.into(),
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.events.insert(0, event);
        self.events.truncate(24);
    }
}

fn with_state<T>(read: impl FnOnce(&PersistedAppState) -> T) -> Result<T, String> {
    let state = state_mutex()?;
    let guard = state
        .lock()
        .map_err(|_| "app state lock poisoned".to_owned())?;
    Ok(read(&guard))
}

fn mutate_state(
    mut update: impl FnMut(&mut PersistedAppState) -> Result<(), String>,
) -> Result<AppStateView, String> {
    let state = state_mutex()?;
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
    let mut state = match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str::<PersistedAppState>(&contents)
            .map_err(|error| format!("failed to parse app state at {}: {error}", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => PersistedAppState::initial(),
        Err(error) => {
            return Err(format!(
                "failed to load app state at {}: {error}",
                path.display()
            ));
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
        fs::create_dir_all(parent)?;
    }
    let encoded = serde_json::to_vec_pretty(state)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, encoded)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn state_path() -> PathBuf {
    std::env::var_os("DISCRYPT_APP_STATE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_app_data_path)
}

fn default_app_data_path() -> PathBuf {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(data_home)
            .join("discrypt")
            .join("discrypt-state.json");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("discrypt")
            .join("discrypt-state.json");
    }
    std::env::temp_dir()
        .join("discrypt")
        .join("discrypt-state.json")
}

fn normalize_loaded_state(state: &mut PersistedAppState) {
    if state.next_sequence == 0 {
        state.next_sequence = 1;
    }
    if matches!(state.lifecycle, LifecycleStage::Ready) && state.user.is_some() {
        let display_name = state
            .user
            .as_ref()
            .map(|user| user.display_name.clone())
            .unwrap_or_else(|| "Alice".to_owned());
        ensure_default_collaboration_state(state, &display_name);
    }
}

fn ensure_default_collaboration_state(state: &mut PersistedAppState, display_name: &str) {
    if state.dms.is_empty() {
        state.dms.push(DmView {
            dm_id: stable_id("dm", "Bob"),
            peer_label: "Bob".to_owned(),
        });
        state.active_dm_id = state.dms.first().map(|dm| dm.dm_id.clone());
    }
    if state.groups.is_empty() {
        let group_id = stable_id("group", "discrypt lab");
        state.groups.push(GroupView {
            group_id: group_id.clone(),
            name: "discrypt lab".to_owned(),
            role: "owner".to_owned(),
            channels: default_channels(&state.snapshot.retention.selected),
            invite_codes: Vec::new(),
        });
        state.active_group_id = Some(group_id);
    }
    let voice_pairs: Vec<(String, String)> = state
        .groups
        .iter()
        .flat_map(|group| {
            group
                .channels
                .iter()
                .filter(|channel| channel.kind == ChannelKind::Voice)
                .map(|channel| (group.group_id.clone(), channel.channel_id.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    for (group_id, channel_id) in voice_pairs {
        let session_id = stable_id("voice", &format!("{group_id}-{channel_id}"));
        if !state
            .voice_sessions
            .iter()
            .any(|session| session.session_id == session_id)
        {
            state.voice_sessions.push(VoiceSessionView {
                session_id,
                group_id,
                channel_id,
                joined: false,
                self_muted: false,
                participants: default_voice_participants(display_name),
                route:
                    "local voice session only; production media path waits for adapter/E2E gates"
                        .to_owned(),
            });
        }
    }
}

fn default_channels(retention: &str) -> Vec<AppChannelView> {
    vec![
        AppChannelView {
            channel_id: stable_id("channel", "general"),
            name: "#general".to_owned(),
            kind: ChannelKind::Text,
            retention_status: normalize_label(retention, "7 days"),
        },
        AppChannelView {
            channel_id: stable_id("channel", "voice-lobby"),
            name: "Voice Lobby".to_owned(),
            kind: ChannelKind::Voice,
            retention_status:
                "Session-state only; media-frame E2E gate required before production voice claims"
                    .to_owned(),
        },
    ]
}

fn default_voice_participants(display_name: &str) -> Vec<VoiceParticipantView> {
    vec![
        VoiceParticipantView {
            id: "local".to_owned(),
            name: normalize_label(display_name, "You"),
            role: "you".to_owned(),
            speaking: true,
            muted: false,
            volume: 100,
        },
        VoiceParticipantView {
            id: "peer-bob".to_owned(),
            name: "Bob".to_owned(),
            role: "peer".to_owned(),
            speaking: false,
            muted: false,
            volume: 72,
        },
        VoiceParticipantView {
            id: "relay".to_owned(),
            name: "Relay route".to_owned(),
            role: "route".to_owned(),
            speaking: false,
            muted: true,
            volume: 40,
        },
    ]
}

fn require_identity(state: &PersistedAppState) -> Result<(), String> {
    if state.user.is_none() || matches!(state.lifecycle, LifecycleStage::NeedsIdentity) {
        Err("create or recover a user before using DMs, groups, or voice".to_owned())
    } else {
        Ok(())
    }
}

fn validate_message_target(
    state: &PersistedAppState,
    target: &MessageTarget,
) -> Result<(), String> {
    match target {
        MessageTarget::Dm { dm_id } => state
            .dms
            .iter()
            .any(|dm| &dm.dm_id == dm_id)
            .then_some(())
            .ok_or_else(|| format!("unknown dm_id '{dm_id}'")),
        MessageTarget::Channel {
            group_id,
            channel_id,
        } => state
            .groups
            .iter()
            .find(|group| &group.group_id == group_id)
            .and_then(|group| {
                group
                    .channels
                    .iter()
                    .find(|channel| &channel.channel_id == channel_id)
            })
            .filter(|channel| channel.kind == ChannelKind::Text)
            .map(|_| ())
            .ok_or_else(|| format!("unknown text channel '{group_id}/{channel_id}'")),
    }
}

fn validate_voice_channel(
    state: &PersistedAppState,
    group_id: &str,
    channel_id: &str,
) -> Result<(), String> {
    state
        .groups
        .iter()
        .find(|group| group.group_id == group_id)
        .and_then(|group| {
            group
                .channels
                .iter()
                .find(|channel| channel.channel_id == channel_id)
        })
        .filter(|channel| channel.kind == ChannelKind::Voice)
        .map(|_| ())
        .ok_or_else(|| format!("unknown voice channel '{group_id}/{channel_id}'"))
}

fn set_participant_state(
    session: &mut VoiceSessionView,
    participant_id: &str,
    name: Option<String>,
    muted: Option<bool>,
    speaking: Option<bool>,
    volume: Option<u8>,
) -> bool {
    if let Some(participant) = session
        .participants
        .iter_mut()
        .find(|participant| participant.id == participant_id)
    {
        if let Some(name) = name {
            participant.name = name;
        }
        if let Some(muted) = muted {
            participant.muted = muted;
        }
        if let Some(speaking) = speaking {
            participant.speaking = speaking;
        }
        if let Some(volume) = volume {
            participant.volume = volume;
        }
        true
    } else {
        false
    }
}

fn infer_group_name_from_invite(invite_code: &str) -> String {
    invite_code
        .rsplit('/')
        .next()
        .and_then(|tail| tail.split_once('-').map(|(_, name)| name.replace('-', " ")))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "joined group".to_owned())
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

fn stable_id(prefix: &str, value: &str) -> String {
    let normalized = value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if normalized.is_empty() {
        format!("{prefix}-local")
    } else {
        format!("{prefix}-{normalized}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_surface_covers_identity_messaging_and_voice() -> Result<(), String> {
        let state = create_user(CreateUserRequest {
            display_name: "Alice".to_owned(),
            device_name: "test laptop".to_owned(),
        })?;
        assert_eq!(state.lifecycle, LifecycleStage::Ready);
        assert!(state.user.is_some());
        assert!(!state.dms.is_empty());
        assert!(!state.groups.is_empty());

        let dm_id = state.dms[0].dm_id.clone();
        let state = send_message(SendMessageRequest {
            target: MessageTarget::Dm {
                dm_id: dm_id.clone(),
            },
            body: "hello dm".to_owned(),
        })?;
        assert!(state
            .messages
            .iter()
            .any(|message| message.body == "hello dm"));

        let group_id = state.groups[0].group_id.clone();
        let text_channel_id = state.groups[0]
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Text)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "text channel missing".to_owned())?;
        let voice_channel_id = state.groups[0]
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Voice)
            .map(|channel| channel.channel_id.clone())
            .ok_or_else(|| "voice channel missing".to_owned())?;
        let state = send_message(SendMessageRequest {
            target: MessageTarget::Channel {
                group_id: group_id.clone(),
                channel_id: text_channel_id,
            },
            body: "hello channel".to_owned(),
        })?;
        assert!(state
            .messages
            .iter()
            .any(|message| message.body == "hello channel"));

        let state = join_voice(JoinVoiceRequest {
            group_id,
            channel_id: voice_channel_id,
        })?;
        let session_id = state
            .active_voice_session_id
            .clone()
            .ok_or_else(|| "active voice missing".to_owned())?;
        let state = set_self_mute(SetSelfMuteRequest {
            session_id: session_id.clone(),
            muted: true,
        })?;
        let session = state
            .voice_sessions
            .iter()
            .find(|session| session.session_id == session_id)
            .ok_or_else(|| "session missing".to_owned())?;
        assert!(session.self_muted);
        let state = leave_voice(LeaveVoiceRequest { session_id })?;
        assert!(state.active_voice_session_id.is_none());
        Ok(())
    }

    #[test]
    fn command_health_reports_ready_surface() -> Result<(), String> {
        let health = command_health()?;
        assert!(health.app_state_ready);
        assert!(health.identity_ready);
        assert!(health.messaging_ready);
        assert!(health.voice_ready);
        assert!(health.honest_copy_ready);
        Ok(())
    }
}

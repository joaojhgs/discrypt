//! Tauri command surface for the native discrypt shell.
use discrypt_core::{
    app_snapshot as core_app_snapshot, verify_safety_number as core_verify_safety_number,
    AppSnapshot, ChannelKind, ChannelView, SafetyVerificationRequest, SafetyVerificationResult,
    ServerView,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

/// Persisted UI preference model shared with the React command client.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UiPreferencesView {
    /// Active theme identifier from the frontend theme registry.
    pub theme_id: String,
    /// Active layout template identifier from the frontend template registry.
    pub template_id: String,
}

/// Request to persist UI preference changes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SavePreferencesRequest {
    /// Theme identifier to persist.
    pub theme_id: String,
    /// Template identifier to persist.
    pub template_id: String,
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
    pub group_name: String,
}

/// Request to create a channel in the active group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateChannelRequest {
    /// Channel display name. Text channels are normalized with a leading '#'.
    pub name: String,
    /// Channel kind.
    pub kind: ChannelKind,
    /// Channel retention label.
    pub retention_status: String,
}

/// Request to create an invite for the active group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateInviteRequest {
    /// Expiry label selected by the user/admin.
    pub expires: String,
    /// Maximum-use label selected by the user/admin.
    pub max_use: String,
}

/// Result returned by invite creation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InviteView {
    /// Stable invite identifier for the local command surface.
    pub invite_id: String,
    /// User-pastable invite code.
    pub code: String,
    /// Expiry label.
    pub expires: String,
    /// Maximum-use label.
    pub max_use: String,
    /// Honest admission copy.
    pub admission_copy: String,
}

/// Request to append a message to a command-backed local timeline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SendMessageRequest {
    /// Channel name the message belongs to.
    pub channel_name: String,
    /// Message body.
    pub body: String,
}

/// Command-backed local text message row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageView {
    /// Stable local message id.
    pub message_id: String,
    /// Channel name.
    pub channel_name: String,
    /// Author label.
    pub author: String,
    /// Decrypted local body shown in this shell.
    pub body: String,
    /// Delivery/security status copy.
    pub status: String,
    /// Deterministic local timestamp/counter label.
    pub sent_at: String,
}

/// Request to set self mute state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetSelfMuteRequest {
    /// Whether the local participant is muted.
    pub muted: bool,
}

/// Request to set a participant speaker volume.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetSpeakerVolumeRequest {
    /// Participant identifier.
    pub participant_id: String,
    /// Volume 0-100.
    pub volume: u8,
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

/// Command-backed voice session state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceStateView {
    /// Voice room label.
    pub room: String,
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
    /// Existing domain snapshot for setup/security copy.
    pub snapshot: AppSnapshot,
    /// Persisted UI preferences.
    pub preferences: UiPreferencesView,
    /// Local-first group status: current/created/joined.
    pub group_status: String,
    /// Message timelines for text channels.
    pub messages: Vec<MessageView>,
    /// Voice control state.
    pub voice: VoiceStateView,
    /// Most recent local events.
    pub events: Vec<AppEventView>,
    /// Most recent invite, if one has been created in this profile.
    pub active_invite: Option<InviteView>,
}

/// Command result for local E2E/smoke execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandHealth {
    /// Snapshot command returned all required UI flows.
    pub snapshot_ready: bool,
    /// Safety-number verification command accepts exact backend-owned matches.
    pub verification_ready: bool,
    /// Honest security copy is present for deletion and metadata claims.
    pub honest_copy_ready: bool,
    /// Mutation commands and persisted UI preferences are available.
    pub app_state_ready: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct PersistedAppState {
    snapshot: AppSnapshot,
    preferences: UiPreferencesView,
    group_status: String,
    messages: Vec<MessageView>,
    voice: VoiceStateView,
    events: Vec<AppEventView>,
    active_invite: Option<InviteView>,
    next_sequence: u64,
}

static APP_STATE: OnceLock<Mutex<PersistedAppState>> = OnceLock::new();

/// Tauri command: return the initial app snapshot for the React shell.
pub fn app_snapshot() -> AppSnapshot {
    with_state(|state| state.snapshot.clone())
}

/// Tauri command: return the full command-backed app state for the React shell.
pub fn app_state() -> AppStateView {
    with_state(|state| state.to_view())
}

/// Tauri command: verify a user-confirmed safety-number comparison and persist success.
pub fn verify_safety_number(request: SafetyVerificationRequest) -> SafetyVerificationResult {
    let result = core_verify_safety_number(request);
    if result.verified {
        mutate_state(|state| {
            state.snapshot.friend.verified = true;
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
            theme_id: request.theme_id.clone(),
            template_id: request.template_id.clone(),
        };
        state.push_event("preferences.saved", "Theme/template preferences saved");
    })
}

/// Tauri command: create a local-first group and make it active.
pub fn create_group(request: CreateGroupRequest) -> AppStateView {
    mutate_state(|state| {
        let name = normalize_label(&request.name, "private lab");
        state.snapshot.servers = vec![ServerView {
            name: name.clone(),
            role: "owner".to_owned(),
            channels: vec![ChannelView {
                name: "#general".to_owned(),
                kind: ChannelKind::Text,
                retention_status: normalize_label(
                    &request.retention,
                    &state.snapshot.retention.selected,
                ),
            }],
        }];
        state.group_status = "created".to_owned();
        state.push_event("group.created", format!("Created group {name}"));
    })
}

/// Tauri command: join a local-first group from an invite.
pub fn join_group(request: JoinGroupRequest) -> AppStateView {
    mutate_state(|state| {
        let name = normalize_label(&request.group_name, "joined enclave");
        let invite_code = normalize_label(&request.invite_code, "manual invite");
        state.snapshot.servers = vec![ServerView {
            name: name.clone(),
            role: "member".to_owned(),
            channels: vec![ChannelView {
                name: "#general".to_owned(),
                kind: ChannelKind::Text,
                retention_status: state.snapshot.retention.selected.clone(),
            }],
        }];
        state.group_status = "joined".to_owned();
        state.push_event("group.joined", format!("Joined {name} via {invite_code}"));
    })
}

/// Tauri command: create a channel in the active group.
pub fn create_channel(request: CreateChannelRequest) -> AppStateView {
    mutate_state(|state| {
        let channel = ChannelView {
            name: normalize_channel_name(&request.name, request.kind),
            kind: request.kind,
            retention_status: normalize_label(
                &request.retention_status,
                &state.snapshot.retention.selected,
            ),
        };
        if let Some(server) = state.snapshot.servers.first_mut() {
            if !server
                .channels
                .iter()
                .any(|existing| existing.name == channel.name)
            {
                let name = channel.name.clone();
                server.channels.push(channel);
                state.push_event("channel.created", format!("Created channel {name}"));
            }
        }
    })
}

/// Tauri command: create an invite for the active group.
pub fn create_invite(request: CreateInviteRequest) -> AppStateView {
    mutate_state(|state| {
        let sequence = state.next_sequence;
        let group_name = state
            .snapshot
            .servers
            .first()
            .map(|server| server.name.clone())
            .unwrap_or_else(|| "discrypt lab".to_owned());
        let invite = InviteView {
            invite_id: format!("invite-{sequence}"),
            code: format!("discrypt://join/{sequence}-{group_name}"),
            expires: normalize_label(&request.expires, &state.snapshot.invite.expires),
            max_use: normalize_label(&request.max_use, &state.snapshot.invite.max_use),
            admission_copy: state.snapshot.invite.welcome_required.clone(),
        };
        state.active_invite = Some(invite);
        state.push_event(
            "invite.created",
            "Invite created with MLS Welcome admission gate",
        );
    })
}

/// Tauri command: append a message to the active local timeline.
pub fn send_message(request: SendMessageRequest) -> AppStateView {
    mutate_state(|state| {
        let body = request.body.trim();
        if body.is_empty() {
            state.push_event("message.rejected", "Empty message was not sent");
            return;
        }
        let sequence = state.next_sequence;
        let message = MessageView {
            message_id: format!("msg-{sequence}"),
            channel_name: normalize_channel_name(&request.channel_name, ChannelKind::Text),
            author: "Alice".to_owned(),
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

/// Tauri command: join the active voice room.
pub fn join_voice() -> AppStateView {
    mutate_state(|state| {
        state.voice.joined = true;
        if let Some(alice) = state
            .voice
            .participants
            .iter_mut()
            .find(|participant| participant.id == "alice")
        {
            alice.speaking = !alice.muted;
        }
        state.push_event("voice.joined", "Joined command-backed local voice session");
    })
}

/// Tauri command: leave the active voice room.
pub fn leave_voice() -> AppStateView {
    mutate_state(|state| {
        state.voice.joined = false;
        for participant in &mut state.voice.participants {
            participant.speaking = false;
        }
        state.push_event("voice.left", "Left command-backed local voice session");
    })
}

/// Tauri command: persist local self-mute state.
pub fn set_self_mute(request: SetSelfMuteRequest) -> AppStateView {
    mutate_state(|state| {
        state.voice.self_muted = request.muted;
        if let Some(alice) = state
            .voice
            .participants
            .iter_mut()
            .find(|participant| participant.id == "alice")
        {
            alice.muted = request.muted;
            alice.speaking = state.voice.joined && !request.muted;
        }
        let summary = if request.muted {
            "Self muted"
        } else {
            "Self unmuted"
        };
        state.push_event("voice.self_mute", summary);
    })
}

/// Tauri command: persist a participant speaker volume.
pub fn set_speaker_volume(request: SetSpeakerVolumeRequest) -> AppStateView {
    mutate_state(|state| {
        let volume = request.volume.min(100);
        if let Some(participant) = state
            .voice
            .participants
            .iter_mut()
            .find(|participant| participant.id == request.participant_id)
        {
            participant.volume = volume;
            let name = participant.name.clone();
            state.push_event("voice.volume", format!("Set {name} volume to {volume}"));
        }
    })
}

/// Tauri command: return recent command-backed app events for polling clients.
pub fn poll_app_events() -> Vec<AppEventView> {
    with_state(|state| state.events.clone())
}

/// Tauri command: return the mandatory cooperative-deletion warning copy.
pub fn deletion_warning() -> String {
    app_snapshot().security_copy.deletion
}

/// Tauri command: return the metadata-minimization caveat copy.
pub fn metadata_warning() -> String {
    app_snapshot().security_copy.metadata
}

/// E2E command-health smoke used by CI and the multinode harness.
pub fn command_health() -> CommandHealth {
    let snapshot = app_snapshot();
    let verification = verify_safety_number(SafetyVerificationRequest {
        friend_id: snapshot.friend.friend_code.clone(),
        provided: snapshot.friend.safety_number.clone(),
    });
    let state = app_state();
    CommandHealth {
        snapshot_ready: snapshot.schema_version >= 1
            && snapshot.devices.len() >= 2
            && snapshot
                .servers
                .iter()
                .any(|server| !server.channels.is_empty()),
        verification_ready: verification.verified,
        honest_copy_ready: deletion_warning().contains("pending on offline devices")
            && metadata_warning().contains("does not claim anonymity"),
        app_state_ready: !state.preferences.theme_id.is_empty()
            && !state.preferences.template_id.is_empty()
            && !state.voice.participants.is_empty(),
    }
}

/// Tauri IPC wrappers live in a child module because Tauri 2.11 command macros
/// export helper macros at crate root for visible commands. Keeping wrappers out
/// of the crate root avoids helper-name collisions while the public functions
/// above remain directly testable without the Tauri runtime feature.
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
    pub(super) fn verify_safety_number(
        request: SafetyVerificationRequest,
    ) -> SafetyVerificationResult {
        super::verify_safety_number(request)
    }

    #[tauri::command]
    pub(super) fn save_preferences(request: SavePreferencesRequest) -> AppStateView {
        super::save_preferences(request)
    }

    #[tauri::command]
    pub(super) fn create_group(request: CreateGroupRequest) -> AppStateView {
        super::create_group(request)
    }

    #[tauri::command]
    pub(super) fn join_group(request: JoinGroupRequest) -> AppStateView {
        super::join_group(request)
    }

    #[tauri::command]
    pub(super) fn create_channel(request: CreateChannelRequest) -> AppStateView {
        super::create_channel(request)
    }

    #[tauri::command]
    pub(super) fn create_invite(request: CreateInviteRequest) -> AppStateView {
        super::create_invite(request)
    }

    #[tauri::command]
    pub(super) fn send_message(request: SendMessageRequest) -> AppStateView {
        super::send_message(request)
    }

    #[tauri::command]
    pub(super) fn join_voice() -> AppStateView {
        super::join_voice()
    }

    #[tauri::command]
    pub(super) fn leave_voice() -> AppStateView {
        super::leave_voice()
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
}

/// Run the native Tauri shell with the command surface registered for frontend IPC.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::<tauri::Wry>::default()
        .invoke_handler(tauri::generate_handler![
            ipc_commands::app_snapshot,
            ipc_commands::app_state,
            ipc_commands::verify_safety_number,
            ipc_commands::save_preferences,
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
        let snapshot = core_app_snapshot();
        Self {
            voice: VoiceStateView {
                room: "Voice Lobby".to_owned(),
                joined: false,
                self_muted: false,
                route: snapshot.voice.route.clone(),
                participants: vec![
                    VoiceParticipantView {
                        id: "alice".to_owned(),
                        name: "Alice".to_owned(),
                        role: "you".to_owned(),
                        speaking: false,
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
                ],
            },
            snapshot,
            preferences: UiPreferencesView {
                theme_id: "graphite-calm".to_owned(),
                template_id: "command-center".to_owned(),
            },
            group_status: "current".to_owned(),
            messages: vec![MessageView {
                message_id: "msg-1".to_owned(),
                channel_name: "#general".to_owned(),
                author: "system".to_owned(),
                body: "Welcome to the command-backed local timeline.".to_owned(),
                status: "fixture seed; no network delivery claim".to_owned(),
                sent_at: "local-1".to_owned(),
            }],
            events: vec![AppEventView {
                sequence: 1,
                kind: "app.ready".to_owned(),
                summary: "Command-backed app state initialized".to_owned(),
            }],
            active_invite: None,
            next_sequence: 2,
        }
    }

    fn to_view(&self) -> AppStateView {
        AppStateView {
            snapshot: self.snapshot.clone(),
            preferences: self.preferences.clone(),
            group_status: self.group_status.clone(),
            messages: self.messages.clone(),
            voice: self.voice.clone(),
            events: self.events.clone(),
            active_invite: self.active_invite.clone(),
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
        let overflow = self.events.len().saturating_sub(24);
        if overflow > 0 {
            self.events.drain(0..overflow);
        }
    }
}

fn with_state<T>(read: impl FnOnce(&PersistedAppState) -> T) -> T {
    let state = APP_STATE.get_or_init(|| Mutex::new(load_state()));
    let guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    read(&guard)
}

fn mutate_state(mut update: impl FnMut(&mut PersistedAppState)) -> AppStateView {
    let state = APP_STATE.get_or_init(|| Mutex::new(load_state()));
    let mut guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    update(&mut guard);
    persist_state(&guard);
    guard.to_view()
}

fn load_state() -> PersistedAppState {
    let path = state_path();
    if let Ok(contents) = fs::read_to_string(path) {
        if let Ok(state) = serde_json::from_str::<PersistedAppState>(&contents) {
            return state;
        }
    }
    PersistedAppState::initial()
}

fn persist_state(state: &PersistedAppState) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(encoded) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, encoded);
    }
}

fn state_path() -> PathBuf {
    std::env::var_os("DISCRYPT_APP_STATE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("discrypt-app-state.json"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_surface_covers_snapshot_verification_and_honest_copy() {
        let health = command_health();
        assert!(health.snapshot_ready);
        assert!(health.verification_ready);
        assert!(health.honest_copy_ready);
        assert!(health.app_state_ready);
    }

    #[test]
    fn command_mutations_back_ui_state() {
        let state = save_preferences(SavePreferencesRequest {
            theme_id: "midnight-steel".to_owned(),
            template_id: "compact-ops".to_owned(),
        });
        assert_eq!(state.preferences.theme_id, "midnight-steel");
        assert_eq!(state.preferences.template_id, "compact-ops");

        let state = create_group(CreateGroupRequest {
            name: "private lab".to_owned(),
            retention: "24 hours".to_owned(),
        });
        assert_eq!(state.group_status, "created");
        assert_eq!(state.snapshot.servers[0].name, "private lab");

        let state = create_channel(CreateChannelRequest {
            name: "ops-room".to_owned(),
            kind: ChannelKind::Text,
            retention_status: "24 hours".to_owned(),
        });
        assert!(state.snapshot.servers[0]
            .channels
            .iter()
            .any(|channel| channel.name == "#ops-room"));

        let state = send_message(SendMessageRequest {
            channel_name: "#ops-room".to_owned(),
            body: "command backed".to_owned(),
        });
        assert!(state
            .messages
            .iter()
            .any(|message| message.body == "command backed"));

        let state = join_voice();
        assert!(state.voice.joined);
        let state = set_self_mute(SetSelfMuteRequest { muted: true });
        assert!(state.voice.self_muted);
        let state = set_speaker_volume(SetSpeakerVolumeRequest {
            participant_id: "bob".to_owned(),
            volume: 55,
        });
        assert_eq!(
            state
                .voice
                .participants
                .iter()
                .find(|participant| participant.id == "bob")
                .map(|participant| participant.volume),
            Some(55)
        );
    }
}

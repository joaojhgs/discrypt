//! Tauri command surface for the native discrypt shell.
use discrypt_core::{
    app_snapshot as core_app_snapshot, verify_safety_number as core_verify_safety_number,
    AppService, AppSnapshot, ChannelKind, CreateChannelRequest, CreateGroupRequest,
    JoinGroupRequest, SafetyVerificationRequest, SafetyVerificationResult, SavePreferencesRequest,
    SelfMuteRequest, SendMessageRequest, SpeakerVolumeRequest,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use storage::FileAppStore;

/// Command result for local E2E/smoke execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandHealth {
    /// Snapshot command returned all required UI flows.
    pub snapshot_ready: bool,
    /// Safety-number verification command accepts exact backend-owned matches.
    pub verification_ready: bool,
    /// Honest security copy is present for deletion and metadata claims.
    pub honest_copy_ready: bool,
    /// Mutation commands for groups/channels/preferences/text/voice are available.
    pub command_coverage_ready: bool,
    /// AppService can persist mutations and reload them from the AppStore boundary.
    pub persistence_ready: bool,
}

type DesktopService = AppService<FileAppStore>;

static APP_SERVICE: OnceLock<Result<Mutex<DesktopService>, String>> = OnceLock::new();

fn default_store_path() -> PathBuf {
    std::env::var_os("DISCRYPT_APP_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| std::env::temp_dir())
                .join(".discrypt")
                .join("app-state.json")
        })
}

fn service_cell() -> &'static Result<Mutex<DesktopService>, String> {
    APP_SERVICE.get_or_init(|| {
        AppService::load_or_seed(FileAppStore::new(default_store_path()))
            .map(Mutex::new)
            .map_err(|error| error.to_string())
    })
}

fn with_service<T>(
    mut f: impl FnMut(&mut DesktopService) -> Result<T, String>,
) -> Result<T, String> {
    let mutex = service_cell().as_ref().map_err(Clone::clone)?;
    let mut service = mutex
        .lock()
        .map_err(|_| "app service lock poisoned".to_owned())?;
    f(&mut service)
}

/// Tauri command: return the current persisted app snapshot for the React shell.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[must_use]
pub fn app_snapshot() -> AppSnapshot {
    with_service(|service| Ok(service.snapshot())).unwrap_or_else(|_| core_app_snapshot())
}

/// Tauri command: return the full command-backed app state for the React shell.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[must_use]
pub fn app_state() -> AppStateView {
    with_state(|state| state.to_view())
}

/// Tauri command: verify a user-confirmed safety-number comparison and persist success.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[must_use]
pub fn verify_safety_number(request: SafetyVerificationRequest) -> SafetyVerificationResult {
    with_service(|service| {
        service
            .verify_safety_number(request.clone())
            .map_err(|error| error.to_string())
    })
    .unwrap_or_else(|_| core_verify_safety_number(request))
}

/// Tauri command: create and persist a group shell.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn create_group(request: CreateGroupRequest) -> Result<AppSnapshot, String> {
    with_service(|service| {
        service
            .create_group(request.clone())
            .map_err(|error| error.to_string())
    })
}

/// Tauri command: join and persist an admitted group shell.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn join_group(request: JoinGroupRequest) -> Result<AppSnapshot, String> {
    with_service(|service| {
        service
            .join_group(request.clone())
            .map_err(|error| error.to_string())
    })
}

/// Tauri command: create and persist a channel.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn create_channel(request: CreateChannelRequest) -> Result<AppSnapshot, String> {
    with_service(|service| {
        service
            .create_channel(request.clone())
            .map_err(|error| error.to_string())
    })
}

/// Tauri command: save UI preferences.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn save_preferences(request: SavePreferencesRequest) -> Result<AppSnapshot, String> {
    with_service(|service| {
        service
            .save_preferences(request.clone())
            .map_err(|error| error.to_string())
    })
}

/// Tauri command: join voice session state.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn join_voice() -> Result<AppSnapshot, String> {
    with_service(|service| service.join_voice().map_err(|error| error.to_string()))
}

/// Tauri command: leave voice session state.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn leave_voice() -> Result<AppSnapshot, String> {
    with_service(|service| service.leave_voice().map_err(|error| error.to_string()))
}

/// Tauri command: persist local mute state.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn set_self_mute(request: SelfMuteRequest) -> Result<AppSnapshot, String> {
    with_service(|service| {
        service
            .set_self_mute(request.clone())
            .map_err(|error| error.to_string())
    })
}

/// Tauri command: persist speaker volume.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn set_speaker_volume(request: SpeakerVolumeRequest) -> Result<AppSnapshot, String> {
    with_service(|service| {
        service
            .set_speaker_volume(request.clone())
            .map_err(|error| error.to_string())
    })
}

/// Tauri command: append a local-first text message.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn send_message(request: SendMessageRequest) -> Result<AppSnapshot, String> {
    with_service(|service| {
        service
            .send_message(request.clone())
            .map_err(|error| error.to_string())
    })
}

/// Tauri command: return the mandatory cooperative-deletion warning copy.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[must_use]
pub fn deletion_warning() -> String {
    app_snapshot().security_copy.deletion
}

/// Tauri command: return the metadata-minimization caveat copy.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[must_use]
pub fn metadata_warning() -> String {
    app_snapshot().security_copy.metadata
}

/// E2E command-health smoke used by CI and the multinode harness.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[must_use]
pub fn command_health() -> CommandHealth {
    let snapshot = app_snapshot();
    let verification = verify_safety_number(SafetyVerificationRequest {
        friend_id: snapshot.friend.friend_code.clone(),
        provided: snapshot.friend.safety_number.clone(),
    });
    let persistence_ready = persistence_smoke();
    let command_coverage_ready = command_coverage_smoke();
    CommandHealth {
        snapshot_ready: snapshot.schema_version == 2
            && snapshot.devices.len() >= 2
            && snapshot
                .servers
                .iter()
                .any(|server| !server.channels.is_empty())
            && !snapshot.voice_session.participants.is_empty()
            && !snapshot.preferences.theme_id.is_empty(),
        verification_ready: verification.verified,
        honest_copy_ready: deletion_warning().contains("pending on offline devices")
            && metadata_warning().contains("does not claim anonymity"),
        command_coverage_ready,
        persistence_ready,
    }
}

fn command_coverage_smoke() -> bool {
    let Ok(mut service) = discrypt_core::in_memory_app_service() else {
        return false;
    };
    let Ok(snapshot) = service.create_group(CreateGroupRequest {
        name: "health group".to_owned(),
        retention: "7 days".to_owned(),
    }) else {
        return false;
    };
    if !snapshot
        .servers
        .iter()
        .any(|server| server.name == "health group")
    {
        return false;
    }
    let Ok(snapshot) = service.create_channel(CreateChannelRequest {
        server_name: "health group".to_owned(),
        name: "health".to_owned(),
        kind: ChannelKind::Text,
    }) else {
        return false;
    };
    snapshot.servers.iter().any(|server| {
        server.name == "health group"
            && server
                .channels
                .iter()
                .any(|channel| channel.name == "#health")
    }) && service
        .save_preferences(SavePreferencesRequest {
            theme_id: "ocean-contrast".to_owned(),
            template_id: "compact-ops".to_owned(),
        })
        .is_ok()
        && service.join_voice().is_ok()
        && service
            .set_self_mute(SelfMuteRequest { muted: true })
            .is_ok()
        && service
            .set_speaker_volume(SpeakerVolumeRequest {
                participant_id: "bob".to_owned(),
                volume: 55,
            })
            .is_ok()
        && service
            .send_message(SendMessageRequest {
                channel: "#health".to_owned(),
                body: "health".to_owned(),
            })
            .is_ok()
}

fn persistence_smoke() -> bool {
    let path = std::env::temp_dir().join(format!(
        "discrypt-desktop-health-{}-{}.json",
        std::process::id(),
        "store"
    ));
    let _ = std::fs::remove_file(&path);
    let mut first = match AppService::load_or_seed(FileAppStore::new(&path)) {
        Ok(service) => service,
        Err(_) => return false,
    };
    if first
        .create_channel(CreateChannelRequest {
            server_name: "discrypt lab".to_owned(),
            name: "persisted".to_owned(),
            kind: ChannelKind::Text,
        })
        .is_err()
    {
        return false;
    }
    let second = match AppService::load_or_seed(FileAppStore::new(&path)) {
        Ok(service) => service,
        Err(_) => return false,
    };
    let ready = second.snapshot().servers.iter().any(|server| {
        server
            .channels
            .iter()
            .any(|channel| channel.name == "#persisted")
    });
    let _ = std::fs::remove_file(path);
    ready
}

/// Build and type-check the Tauri command handler registration.
#[cfg(feature = "tauri-runtime")]
#[must_use]
pub fn command_handler<R: tauri::Runtime>(
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app_snapshot,
        app_state,
        verify_safety_number,
        create_group,
        join_group,
        create_channel,
        save_preferences,
        join_voice,
        leave_voice,
        set_self_mute,
        set_speaker_volume,
        send_message,
        deletion_warning,
        metadata_warning,
        command_health
    ]
}

/// Run the native Tauri shell with the command surface registered for frontend IPC.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(command_handler())
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
    fn command_surface_covers_snapshot_verification_honest_copy_and_mutations() {
        let health = command_health();
        assert!(health.snapshot_ready);
        assert!(health.verification_ready);
        assert!(health.honest_copy_ready);
        assert!(health.command_coverage_ready);
        assert!(health.persistence_ready);
    }
}

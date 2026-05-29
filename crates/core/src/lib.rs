//! Domain orchestration facade for Tauri commands and headless E2E tests.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use admission::Invite;
use mls_core::{DeviceSet, GroupState, Identity};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use storage::{recover_account, AppStore, AppStoreError, MemoryAppStore, RecoveryMaterial};
use thiserror::Error;

/// Room summary returned to UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoomSummary {
    /// Stable room identifier.
    pub room_id: String,
    /// Current MLS epoch facade.
    pub epoch: u64,
    /// Current member count facade.
    pub members: usize,
}

/// Friend/safety-number row for verification UX.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FriendView {
    /// Local display label.
    pub alias: String,
    /// Friend-code/QR payload preview.
    pub friend_code: String,
    /// Pairwise safety number that must be verified out of band.
    pub safety_number: String,
    /// Whether the user has explicitly verified this safety number.
    pub verified: bool,
}

/// Device-management row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeviceView {
    /// Device id shown in transparency notices.
    pub device_id: String,
    /// Human label for the device.
    pub label: String,
    /// MLS leaf index for this device.
    pub leaf_index: u32,
    /// Account identity public key associated with this device.
    pub identity_key: String,
    /// Per-device public key for this device leaf.
    pub device_key: String,
    /// Whether this device is current/local.
    pub local: bool,
    /// Whether this device is authorized by an existing device.
    pub authorized: bool,
    /// Whether this device has been revoked.
    pub revoked: bool,
    /// Epoch where this device was added.
    pub added_at_epoch: u64,
    /// Epoch where this device was revoked, if any.
    pub revoked_at_epoch: Option<u64>,
}

/// UI channel kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChannelKind {
    /// Text channel.
    Text,
    /// Voice channel.
    Voice,
}

/// Discord-style server/channel row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChannelView {
    /// Channel name.
    pub name: String,
    /// Channel kind.
    pub kind: ChannelKind,
    /// Retention status for channel history.
    pub retention_status: String,
}

/// Discord-style server view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServerView {
    /// Server/room label.
    pub name: String,
    /// Channels visible in the sidebar.
    pub channels: Vec<ChannelView>,
    /// Role label from governance state.
    pub role: String,
}

/// Invite flow status shown to users/admins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InviteFlowView {
    /// Invite expiry copy.
    pub expires: String,
    /// Max-use copy.
    pub max_use: String,
    /// Password-gate posture copy.
    pub password_gate: String,
    /// Final MLS Welcome/add gate copy.
    pub welcome_required: String,
}

/// Retention settings shown in UX.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetentionSettingsView {
    /// Available retention presets.
    pub presets: Vec<String>,
    /// Current default setting.
    pub selected: String,
    /// Warning for unlimited/never-lock.
    pub unlimited_warning: String,
    /// Shorten/lengthen semantic copy.
    pub transition_copy: String,
}

/// Voice-room posture copy shown in UX.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceRoomView {
    /// Current route label.
    pub route: String,
    /// Relay security copy.
    pub relay_copy: String,
    /// Android path copy.
    pub android_path: String,
}

/// Command-backed voice participant row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceParticipantView {
    /// Stable participant id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Participant role/copy.
    pub role: String,
    /// Whether audio-level events currently mark this participant as speaking.
    pub speaking: bool,
    /// Whether this participant is muted.
    pub muted: bool,
    /// Per-speaker output volume.
    pub volume: u8,
}

/// Command-backed voice session state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceSessionView {
    /// Whether the local user has joined the voice room session state.
    pub joined: bool,
    /// Participants emitted by the app service.
    pub participants: Vec<VoiceParticipantView>,
    /// Honest status copy scoped to command-backed session state.
    pub status_copy: String,
    /// Honest route/media copy scoped to the current adapter readiness.
    pub route_copy: String,
}

/// Persisted UI preferences.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreferencesView {
    /// Active theme id from the UI config contract.
    pub theme_id: String,
    /// Active layout template id from the UI config contract.
    pub template_id: String,
}

/// Timeline message state shown by the UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageView {
    /// Stable message id.
    pub id: String,
    /// Channel name for the local-first timeline.
    pub channel: String,
    /// Author display name.
    pub author: String,
    /// Plaintext body as allowed by the current retention state.
    pub body: String,
    /// Retention/security state: plaintext, locked, or shredded.
    pub state: String,
}

/// Connectivity status shown in UX.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectivityView {
    /// Ordered fallback chain copy.
    pub fallback_chain: String,
    /// Metadata posture copy.
    pub metadata_copy: String,
    /// Push wake posture copy.
    pub push_copy: String,
}

/// Required security/deletion copy surfaced in release UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecurityCopyView {
    /// Metadata caveat.
    pub metadata: String,
    /// Cooperative deletion caveat.
    pub deletion: String,
    /// Malicious-recipient caveat.
    pub malicious_member: String,
}

/// Snapshot returned by the Tauri command surface and consumed by the React shell.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppSnapshot {
    /// Serialized command contract schema version.
    pub schema_version: u32,
    /// Friend and explicit verification state.
    pub friend: FriendView,
    /// Device-management rows.
    pub devices: Vec<DeviceView>,
    /// Server/channel navigation rows.
    pub servers: Vec<ServerView>,
    /// Invite/admission flow copy.
    pub invite: InviteFlowView,
    /// Retention settings model.
    pub retention: RetentionSettingsView,
    /// Voice-room posture copy.
    pub voice: VoiceRoomView,
    /// Command-backed voice session state.
    pub voice_session: VoiceSessionView,
    /// Persisted UI preferences.
    pub preferences: PreferencesView,
    /// Local-first persisted timeline messages.
    pub messages: Vec<MessageView>,
    /// Command/event activity feed.
    pub activity_feed: Vec<String>,
    /// Connectivity/signaling/push posture.
    pub connectivity: ConnectivityView,
    /// Mandatory security copy.
    pub security_copy: SecurityCopyView,
}

/// Safety-number verification request from UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SafetyVerificationRequest {
    /// Friend/device identifier whose expected safety number is backend-owned.
    pub friend_id: String,
    /// User-confirmed safety number from QR/out-of-band comparison.
    pub provided: String,
}

/// Safety-number verification response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SafetyVerificationResult {
    /// Whether the values match exactly.
    pub verified: bool,
    /// User-facing result copy.
    pub message: String,
}

/// End-to-end identity/recovery verification evidence for local harnesses.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IdentityRecoveryVerification {
    /// Two independently generated profiles derive the same pairwise safety number.
    pub two_profiles_verify_safety_numbers: bool,
    /// A second own device is paired under the same account identity with a distinct device key.
    pub second_device_paired: bool,
    /// Account-continuity recovery succeeds without restoring content keys.
    pub recovery_without_content_keys: bool,
    /// A compromised device is revoked with structured revocation metadata.
    pub compromised_device_revoked: bool,
}

/// Group creation request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    /// Group name.
    pub name: String,
    /// Default retention label.
    pub retention: String,
}

/// Join group request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JoinGroupRequest {
    /// Invite code/link entered by the user.
    pub invite_code: String,
}

/// Channel creation request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateChannelRequest {
    /// Server name.
    pub server_name: String,
    /// Channel name.
    pub name: String,
    /// Channel kind.
    pub kind: ChannelKind,
}

/// Preference save request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SavePreferencesRequest {
    /// Theme id.
    pub theme_id: String,
    /// Template id.
    pub template_id: String,
}

/// Speaker volume update request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpeakerVolumeRequest {
    /// Participant id.
    pub participant_id: String,
    /// Volume 0..100.
    pub volume: u8,
}

/// Self mute request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelfMuteRequest {
    /// New mute state.
    pub muted: bool,
}

/// Text-message send request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SendMessageRequest {
    /// Channel name.
    pub channel: String,
    /// Message body.
    pub body: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct AppState {
    snapshot: AppSnapshot,
    next_message_sequence: u64,
}

const SNAPSHOT_SCHEMA_VERSION: u32 = 2;
const DEFAULT_THEME_ID: &str = "graphite-calm";
const DEFAULT_TEMPLATE_ID: &str = "command-center";
static PROCESS_IDENTITY_STATE: OnceLock<AppState> = OnceLock::new();

/// App-service errors surfaced through commands.
#[derive(Debug, Error)]
pub enum AppServiceError {
    /// Storage boundary failed.
    #[error("app store error: {0}")]
    Store(#[from] AppStoreError),
    /// JSON state failed to serialize or deserialize.
    #[error("app state serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// User or UI supplied invalid command data.
    #[error("invalid command: {0}")]
    InvalidCommand(String),
}

/// Stateful local-first application service used by Tauri commands and tests.
#[derive(Debug)]
pub struct AppService<S: AppStore> {
    store: S,
    state: AppState,
}

impl<S: AppStore> AppService<S> {
    /// Load app state from a store or seed a deterministic local-first profile.
    pub fn load_or_seed(mut store: S) -> Result<Self, AppServiceError> {
        let state = match store.load_app_state()? {
            Some(bytes) => serde_json::from_slice(&bytes)?,
            None => seed_state(),
        };
        let mut service = Self { store, state };
        service.persist()?;
        Ok(service)
    }

    /// Return the current command-backed snapshot.
    #[must_use]
    pub fn snapshot(&self) -> AppSnapshot {
        self.state.snapshot.clone()
    }

    /// Verify an out-of-band safety-number comparison and persist the result.
    pub fn verify_safety_number(
        &mut self,
        request: SafetyVerificationRequest,
    ) -> Result<SafetyVerificationResult, AppServiceError> {
        let verified = request.friend_id == self.state.snapshot.friend.friend_code
            && request.provided == self.state.snapshot.friend.safety_number;
        let message = if verified {
            self.state.snapshot.friend.verified = true;
            self.push_activity("Safety number verified through command-backed comparison");
            "Safety number verified; MITM risk accepted by explicit user comparison".to_owned()
        } else {
            self.push_activity("Safety number mismatch rejected by backend command");
            "Safety number mismatch; do not trust this device or DM".to_owned()
        };
        self.persist()?;
        Ok(SafetyVerificationResult { verified, message })
    }

    /// Create and persist a local-first group/server.
    pub fn create_group(
        &mut self,
        request: CreateGroupRequest,
    ) -> Result<AppSnapshot, AppServiceError> {
        let name = normalize_label(&request.name, "private lab");
        if !self
            .state
            .snapshot
            .servers
            .iter()
            .any(|server| server.name == name)
        {
            self.state.snapshot.servers.insert(
                0,
                ServerView {
                    name: name.clone(),
                    role: "owner".to_owned(),
                    channels: vec![
                        ChannelView {
                            name: "#general".to_owned(),
                            kind: ChannelKind::Text,
                            retention_status: retention_status(&request.retention),
                        },
                        ChannelView {
                            name: "Voice Lobby".to_owned(),
                            kind: ChannelKind::Voice,
                            retention_status: "Session-state only; media-frame E2E gate required before production voice claims".to_owned(),
                        },
                    ],
                },
            );
        }
        self.state.snapshot.retention.selected = normalize_label(&request.retention, "7 days");
        self.push_activity(format!(
            "Created local-first group '{name}' through persisted AppService command"
        ));
        self.persist()?;
        Ok(self.snapshot())
    }

    /// Persist a joined group shell after admission copy is acknowledged.
    pub fn join_group(
        &mut self,
        request: JoinGroupRequest,
    ) -> Result<AppSnapshot, AppServiceError> {
        let code = normalize_label(&request.invite_code, "invite:local-template");
        let name = if code.contains("enclave") {
            "joined enclave".to_owned()
        } else {
            "joined group".to_owned()
        };
        if !self
            .state
            .snapshot
            .servers
            .iter()
            .any(|server| server.name == name)
        {
            self.state.snapshot.servers.insert(
                0,
                ServerView {
                    name: name.clone(),
                    role: "member".to_owned(),
                    channels: vec![
                        ChannelView {
                            name: "#general".to_owned(),
                            kind: ChannelKind::Text,
                            retention_status: retention_status(&self.state.snapshot.retention.selected),
                        },
                        ChannelView {
                            name: "Voice Lobby".to_owned(),
                            kind: ChannelKind::Voice,
                            retention_status: "Session-state only; media-frame E2E gate required before production voice claims".to_owned(),
                        },
                    ],
                },
            );
        }
        self.push_activity("Joined group shell only after invite/Welcome gate copy was surfaced");
        self.persist()?;
        Ok(self.snapshot())
    }

    /// Create and persist a channel in a server.
    pub fn create_channel(
        &mut self,
        request: CreateChannelRequest,
    ) -> Result<AppSnapshot, AppServiceError> {
        let name = match request.kind {
            ChannelKind::Text => format!("#{}", normalize_channel(&request.name)),
            ChannelKind::Voice => normalize_label(&request.name, "Voice Lobby"),
        };
        let server_name = normalize_label(&request.server_name, "discrypt lab");
        let retention = match request.kind {
            ChannelKind::Text => retention_status(&self.state.snapshot.retention.selected),
            ChannelKind::Voice => {
                "Session-state only; media-frame E2E gate required before production voice claims"
                    .to_owned()
            }
        };
        let server_index = self
            .state
            .snapshot
            .servers
            .iter()
            .position(|server| server.name == server_name)
            .or_else(|| (!self.state.snapshot.servers.is_empty()).then_some(0))
            .ok_or_else(|| {
                AppServiceError::InvalidCommand(
                    "no server available for channel creation".to_owned(),
                )
            })?;
        let server = &mut self.state.snapshot.servers[server_index];
        if !server.channels.iter().any(|channel| channel.name == name) {
            server.channels.push(ChannelView {
                name: name.clone(),
                kind: request.kind,
                retention_status: retention,
            });
        }
        self.push_activity(format!(
            "Persisted channel '{name}' through AppService command"
        ));
        self.persist()?;
        Ok(self.snapshot())
    }

    /// Persist UI preferences.
    pub fn save_preferences(
        &mut self,
        request: SavePreferencesRequest,
    ) -> Result<AppSnapshot, AppServiceError> {
        self.state.snapshot.preferences = PreferencesView {
            theme_id: normalize_label(&request.theme_id, DEFAULT_THEME_ID),
            template_id: normalize_label(&request.template_id, DEFAULT_TEMPLATE_ID),
        };
        self.push_activity("Saved theme/template preferences to the app store");
        self.persist()?;
        Ok(self.snapshot())
    }

    /// Join the command-backed voice session state.
    pub fn join_voice(&mut self) -> Result<AppSnapshot, AppServiceError> {
        self.state.snapshot.voice_session.joined = true;
        self.state.snapshot.voice_session.status_copy =
            "Voice session state joined; audio-frame media path is still gated by E2E tests"
                .to_owned();
        self.set_local_voice_speaking(true);
        self.push_activity("Joined command-backed voice session state");
        self.persist()?;
        Ok(self.snapshot())
    }

    /// Leave the command-backed voice session state.
    pub fn leave_voice(&mut self) -> Result<AppSnapshot, AppServiceError> {
        self.state.snapshot.voice_session.joined = false;
        self.state.snapshot.voice_session.status_copy =
            "Not joined; transport/media unavailable until real adapter gates pass".to_owned();
        self.set_local_voice_speaking(false);
        self.push_activity("Left command-backed voice session state");
        self.persist()?;
        Ok(self.snapshot())
    }

    /// Persist the local mute state.
    pub fn set_self_mute(
        &mut self,
        request: SelfMuteRequest,
    ) -> Result<AppSnapshot, AppServiceError> {
        for participant in &mut self.state.snapshot.voice_session.participants {
            if participant.id == "alice" {
                participant.muted = request.muted;
                participant.speaking = self.state.snapshot.voice_session.joined && !request.muted;
            }
        }
        self.push_activity(if request.muted {
            "Muted local microphone state through voice command"
        } else {
            "Unmuted local microphone state through voice command"
        });
        self.persist()?;
        Ok(self.snapshot())
    }

    /// Persist a speaker volume.
    pub fn set_speaker_volume(
        &mut self,
        request: SpeakerVolumeRequest,
    ) -> Result<AppSnapshot, AppServiceError> {
        let volume = request.volume.min(100);
        let mut updated = false;
        for participant in &mut self.state.snapshot.voice_session.participants {
            if participant.id == request.participant_id {
                participant.volume = volume;
                updated = true;
            }
        }
        if !updated {
            return Err(AppServiceError::InvalidCommand(format!(
                "unknown voice participant '{}'",
                request.participant_id
            )));
        }
        self.push_activity(format!(
            "Persisted speaker volume for '{}' at {volume}%",
            request.participant_id
        ));
        self.persist()?;
        Ok(self.snapshot())
    }

    /// Persist a local-first encrypted text facade message.
    pub fn send_message(
        &mut self,
        request: SendMessageRequest,
    ) -> Result<AppSnapshot, AppServiceError> {
        let body = request.body.trim();
        if body.is_empty() {
            return Err(AppServiceError::InvalidCommand(
                "message body must not be empty".to_owned(),
            ));
        }
        let channel = normalize_label(&request.channel, "#general");
        self.state.next_message_sequence += 1;
        self.state.snapshot.messages.push(MessageView {
            id: format!("local-msg-{}", self.state.next_message_sequence),
            channel: channel.clone(),
            author: "Alice".to_owned(),
            body: body.to_owned(),
            state: "plaintext allowed by current local retention cache; encrypted envelope facade recorded by harness".to_owned(),
        });
        self.push_activity(format!("Persisted local-first text message in {channel}"));
        self.persist()?;
        Ok(self.snapshot())
    }

    fn set_local_voice_speaking(&mut self, speaking: bool) {
        for participant in &mut self.state.snapshot.voice_session.participants {
            if participant.id == "alice" {
                participant.speaking = speaking && !participant.muted;
            }
        }
    }

    fn push_activity(&mut self, item: impl Into<String>) {
        self.state.snapshot.activity_feed.insert(0, item.into());
        self.state.snapshot.activity_feed.truncate(8);
    }

    fn persist(&mut self) -> Result<(), AppServiceError> {
        let bytes = serde_json::to_vec_pretty(&self.state)?;
        self.store.save_app_state(&bytes)?;
        Ok(())
    }
}

/// Create a deterministic DM facade and safety number.
#[must_use]
pub fn create_dm(alice: &Identity, bob: &Identity) -> (GroupState, String) {
    let group = GroupState::new(format!(
        "dm:{}:{}",
        alice.friend_code().as_str(),
        bob.friend_code().as_str()
    ));
    let safety = alice
        .safety_number(&bob.verifying_key())
        .as_str()
        .to_owned();
    (group, safety)
}

/// Summarize group state for UI.
#[must_use]
pub fn summarize(group: &GroupState) -> RoomSummary {
    RoomSummary {
        room_id: group.group_id.clone(),
        epoch: group.epoch,
        members: group.members().len(),
    }
}

/// Build the deterministic command snapshot used by Tauri and the E2E harness.
#[must_use]
pub fn app_snapshot() -> AppSnapshot {
    snapshot_from_state(PROCESS_IDENTITY_STATE.get_or_init(seed_state))
}

fn snapshot_from_state(state: &AppState) -> AppSnapshot {
    state.snapshot.clone()
}

/// Verify an out-of-band safety-number comparison against a deterministic snapshot.
#[must_use]
pub fn verify_safety_number(request: SafetyVerificationRequest) -> SafetyVerificationResult {
    let snapshot = app_snapshot();
    let verified = request.friend_id == snapshot.friend.friend_code
        && request.provided == snapshot.friend.safety_number;
    SafetyVerificationResult {
        verified,
        message: if verified {
            "Safety number verified; MITM risk accepted by explicit user comparison".to_owned()
        } else {
            "Safety number mismatch; do not trust this device or DM".to_owned()
        },
    }
}

/// Verify the Phase C identity, second-device, recovery, and revocation story.
#[must_use]
pub fn identity_recovery_verification_smoke() -> IdentityRecoveryVerification {
    let alice = Identity::generate("Alice");
    let bob = Identity::generate("Bob");
    let alice_code = alice.friend_code();
    let bob_code = bob.friend_code();
    let alice_view = alice.safety_number_from_friend_code(&bob_code);
    let bob_view = bob.safety_number_from_friend_code(&alice_code);
    let two_profiles_verify_safety_numbers = alice_view.is_some() && alice_view == bob_view;

    let mut devices = DeviceSet::new();
    let desktop_key = Identity::generate("Alice desktop device").verifying_key();
    let phone_key = Identity::generate("Alice phone device").verifying_key();
    let desktop = devices.add_authorized_device(&alice, desktop_key, "Desktop", 1);
    let phone = devices.add_authorized_device(&alice, phone_key, "Phone", 2);
    let active_devices = devices.active_devices();
    let second_device_paired = active_devices.len() == 2
        && desktop.identity_key == phone.identity_key
        && desktop.device_key != phone.device_key
        && phone.label == "Phone"
        && phone.added_at_epoch == 2;

    let recovery_without_content_keys = recover_account(RecoveryMaterial::RecoveryCode {
        code_hash: [7u8; 32],
    })
    .map(|recovery| recovery.account_access_restored && !recovery.content_keys_restored)
    .unwrap_or(false);

    let removed = devices.remove_device(phone.device_id, 3);
    let compromised_device_revoked = removed
        && devices.active_devices().len() == 1
        && devices
            .transparency_events()
            .last()
            .is_some_and(|event| event.kind == "device-removed" && event.epoch == 3);

    IdentityRecoveryVerification {
        two_profiles_verify_safety_numbers,
        second_device_paired,
        recovery_without_content_keys,
        compromised_device_revoked,
    }
}

/// Build an in-memory app service seeded with the deterministic fixture.
pub fn in_memory_app_service() -> Result<AppService<MemoryAppStore>, AppServiceError> {
    AppService::load_or_seed(MemoryAppStore::default())
}

fn seed_state() -> AppState {
    let local = Identity::generate("Alice");
    let peer = Identity::generate("New contact");
    let local_friend_code = local.friend_code();
    let peer_friend_code = peer.friend_code();
    let local_device_identity = Identity::generate("Alice Desktop device");
    let peer_device_identity = Identity::generate("New contact primary device");
    let local_device_id = device_id_from_friend_code(local_friend_code.as_str(), "desktop");
    let peer_device_id = device_id_from_friend_code(peer_friend_code.as_str(), "primary");
    let safety_number = local
        .safety_number_from_friend_code(&peer_friend_code)
        .unwrap_or_else(|| local.safety_number(&peer.verifying_key()))
        .as_str()
        .to_owned();
    AppState {
        snapshot: AppSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            friend: FriendView {
                alias: peer.display_name().to_owned(),
                friend_code: peer_friend_code.as_str().to_owned(),
                safety_number,
                verified: false,
            },
            devices: vec![
                DeviceView {
                    device_id: local_device_id,
                    label: "Desktop".to_owned(),
                    leaf_index: 1,
                    identity_key: hex_encode(local.verifying_key().as_bytes()),
                    device_key: hex_encode(local_device_identity.verifying_key().as_bytes()),
                    local: true,
                    authorized: true,
                    revoked: false,
                    added_at_epoch: 1,
                    revoked_at_epoch: None,
                },
                DeviceView {
                    device_id: peer_device_id,
                    label: "Primary".to_owned(),
                    leaf_index: 2,
                    identity_key: hex_encode(peer.verifying_key().as_bytes()),
                    device_key: hex_encode(peer_device_identity.verifying_key().as_bytes()),
                    local: false,
                    authorized: true,
                    revoked: false,
                    added_at_epoch: 1,
                    revoked_at_epoch: None,
                },
            ],
            servers: vec![ServerView {
                name: "discrypt lab".to_owned(),
                role: "owner".to_owned(),
                channels: vec![
                    ChannelView {
                        name: "#general".to_owned(),
                        kind: ChannelKind::Text,
                        retention_status: "7 day default; older messages lock, not vanish".to_owned(),
                    },
                    ChannelView {
                        name: "#ops".to_owned(),
                        kind: ChannelKind::Text,
                        retention_status: "shorten is retroactive; lengthen is future-only".to_owned(),
                    },
                    ChannelView {
                        name: "Voice Lobby".to_owned(),
                        kind: ChannelKind::Voice,
                        retention_status: "Session-state only; media-frame E2E gate required before production voice claims".to_owned(),
                    },
                ],
            }],
            invite: InviteFlowView {
                expires: "Invite expires and can be revoked".to_owned(),
                max_use: "Max-use is enforced before MLS admission".to_owned(),
                password_gate: "Password rooms use OPAQUE/PAKE or an online authorized helper; no offline verifier".to_owned(),
                welcome_required: "Final admission still requires an authorized MLS Welcome/add".to_owned(),
            },
            retention: RetentionSettingsView {
                presets: vec![
                    "1 hour".to_owned(),
                    "24 hours".to_owned(),
                    "7 days".to_owned(),
                    "30 days".to_owned(),
                    "90 days".to_owned(),
                    "custom".to_owned(),
                    "warned unlimited / never-lock".to_owned(),
                ],
                selected: "7 days".to_owned(),
                unlimited_warning: "Unlimited keeps local keys longer and weakens lock behavior; opt in explicitly".to_owned(),
                transition_copy: "Shortening re-locks older messages retroactively; lengthening applies only to future messages".to_owned(),
            },
            voice: VoiceRoomView {
                route: "Local voice controls only; network media route is not connected in this build".to_owned(),
                relay_copy: "No relay is active in the desktop harness until real media/socket E2E gates pass".to_owned(),
                android_path: "Android media routing remains release-gated until platform E2E passes".to_owned(),
            },
            voice_session: VoiceSessionView {
                joined: false,
                participants: vec![VoiceParticipantView {
                    id: "alice".to_owned(),
                    name: "Alice".to_owned(),
                    role: "you".to_owned(),
                    speaking: false,
                    muted: false,
                    volume: 82,
                }],
                status_copy: "Not joined; command-backed local voice controls are idle".to_owned(),
                route_copy: "Local voice controls only; network media route is not connected in this build".to_owned(),
            },
            preferences: PreferencesView {
                theme_id: DEFAULT_THEME_ID.to_owned(),
                template_id: DEFAULT_TEMPLATE_ID.to_owned(),
            },
            messages: vec![
                MessageView {
                    id: "local-msg-1".to_owned(),
                    channel: "#general".to_owned(),
                    author: "Alice".to_owned(),
                    body: "Local-first command-backed timeline is persisted by AppStore.".to_owned(),
                    state: "plaintext allowed by current local retention cache; encrypted envelope facade recorded by harness".to_owned(),
                },
                MessageView {
                    id: "locked-msg-1".to_owned(),
                    channel: "#general".to_owned(),
                    author: peer.display_name().to_owned(),
                    body: "Locked placeholder — author device must be online for a live-key request.".to_owned(),
                    state: "locked".to_owned(),
                },
            ],
            activity_feed: vec![
                "Invite policy checked: expiry + max-use + revoke controls".to_owned(),
                "Android wake path is content-free".to_owned(),
                "Relay route carries ciphertext only in harness gates".to_owned(),
                "Deletion copy includes offline-device caveat".to_owned(),
            ],
            connectivity: ConnectivityView {
                fallback_chain: "Command-backed policy: STUN → relay-overlay → TURN; runtime transport remains release-gated until E2E passes".to_owned(),
                metadata_copy: "Content-private and metadata-minimizing, not metadata-anonymous".to_owned(),
                push_copy: "Android FCM wake is content-free and carries no room, sender, or message body".to_owned(),
            },
            security_copy: SecurityCopyView {
                metadata: "Passive infrastructure can see IPs and timing; discrypt does not claim anonymity".to_owned(),
                deletion: "Deleted on your online devices now; pending on offline devices until they reconnect".to_owned(),
                malicious_member: "Crypto-shred cannot erase screenshots, exports, modified clients, or plaintext already saved by a recipient".to_owned(),
            },
        },
        next_message_sequence: 1,
    }
}

fn device_id_from_friend_code(friend_code: &str, label: &str) -> String {
    let fingerprint = friend_code
        .split("&fp=")
        .nth(1)
        .and_then(|tail| tail.split('&').next())
        .unwrap_or("identity");
    let suffix = fingerprint.chars().take(10).collect::<String>();
    format!("device-{}-{suffix}", slugify(label))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn normalize_label(input: &str, fallback: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn normalize_channel(input: &str) -> String {
    let trimmed = input.trim().trim_start_matches('#');
    let sanitized: String = trimmed
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    normalize_label(&sanitized, "secure-room")
}

fn slugify(label: &str) -> String {
    let slug = label
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
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "local".to_owned()
    } else {
        slug
    }
}

fn retention_status(selected: &str) -> String {
    format!("{selected}; older messages lock, not vanish")
}

#[allow(dead_code)]
fn _invite_boundary(_: &Invite) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_snapshot_covers_required_ui_flows_and_copy() {
        let snapshot = app_snapshot();

        assert_eq!(snapshot.schema_version, SNAPSHOT_SCHEMA_VERSION);
        assert!(!snapshot.friend.verified);
        assert_eq!(snapshot, app_snapshot());
        assert_eq!(snapshot.devices.len(), 2);
        assert!(snapshot.devices.iter().all(|device| {
            !device.label.is_empty()
                && device.identity_key.len() == 64
                && device.device_key.len() == 64
                && !device.revoked
                && device.added_at_epoch == 1
                && device.revoked_at_epoch.is_none()
        }));
        assert!(snapshot
            .servers
            .iter()
            .flat_map(|server| server.channels.iter())
            .any(|channel| channel.kind == ChannelKind::Voice));
        assert!(snapshot
            .invite
            .password_gate
            .contains("OPAQUE/PAKE or an online authorized helper"));
        assert!(snapshot
            .retention
            .presets
            .contains(&"warned unlimited / never-lock".to_owned()));
        assert!(snapshot
            .security_copy
            .deletion
            .contains("pending on offline devices until they reconnect"));
        assert!(snapshot
            .connectivity
            .metadata_copy
            .contains("not metadata-anonymous"));
        assert!(snapshot
            .voice_session
            .status_copy
            .contains("command-backed"));
    }

    #[test]
    fn safety_verification_requires_exact_match() {
        let snapshot = app_snapshot();
        let ok = verify_safety_number(SafetyVerificationRequest {
            friend_id: snapshot.friend.friend_code.clone(),
            provided: snapshot.friend.safety_number.clone(),
        });
        let bad = verify_safety_number(SafetyVerificationRequest {
            friend_id: snapshot.friend.friend_code,
            provided: "9999".to_owned(),
        });

        assert!(ok.verified);
        assert!(!bad.verified);
    }

    #[test]
    fn app_service_persists_mutations_across_restart() -> Result<(), AppServiceError> {
        let store = MemoryAppStore::default();
        let mut service = AppService::load_or_seed(store.clone())?;
        let snapshot = service.create_channel(CreateChannelRequest {
            server_name: "discrypt lab".to_owned(),
            name: "secure-room".to_owned(),
            kind: ChannelKind::Text,
        })?;
        assert!(snapshot.servers[0]
            .channels
            .iter()
            .any(|channel| channel.name == "#secure-room"));
        service.save_preferences(SavePreferencesRequest {
            theme_id: "ocean-contrast".to_owned(),
            template_id: "compact-ops".to_owned(),
        })?;
        service.verify_safety_number(SafetyVerificationRequest {
            friend_id: snapshot.friend.friend_code,
            provided: snapshot.friend.safety_number,
        })?;

        let reloaded = AppService::load_or_seed(store)?;
        let reloaded_snapshot = reloaded.snapshot();
        assert!(reloaded_snapshot.friend.verified);
        assert_eq!(reloaded_snapshot.preferences.theme_id, "ocean-contrast");
        assert!(reloaded_snapshot.servers[0]
            .channels
            .iter()
            .any(|channel| channel.name == "#secure-room"));
        Ok(())
    }

    #[test]
    fn voice_and_message_commands_are_snapshot_backed() -> Result<(), AppServiceError> {
        let mut service = in_memory_app_service()?;
        let muted = service.set_self_mute(SelfMuteRequest { muted: true })?;
        assert!(muted
            .voice_session
            .participants
            .iter()
            .any(|participant| participant.id == "alice" && participant.muted));
        let volume = service.set_speaker_volume(SpeakerVolumeRequest {
            participant_id: "alice".to_owned(),
            volume: 41,
        })?;
        assert!(volume
            .voice_session
            .participants
            .iter()
            .any(|participant| participant.id == "alice" && participant.volume == 41));
        let text = service.send_message(SendMessageRequest {
            channel: "#general".to_owned(),
            body: "hello command-backed timeline".to_owned(),
        })?;
        assert!(text
            .messages
            .iter()
            .any(|message| message.body == "hello command-backed timeline"));
        Ok(())
    }

    #[test]
    fn phase_c_identity_recovery_verification_smoke_passes() {
        let verification = identity_recovery_verification_smoke();
        assert!(verification.two_profiles_verify_safety_numbers);
        assert!(verification.second_device_paired);
        assert!(verification.recovery_without_content_keys);
        assert!(verification.compromised_device_revoked);
    }
}

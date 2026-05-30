//! App-service boundary traits.
//!
//! These traits describe the seams between the command-facing `AppService` and
//! the crypto, persistence, media, overlay, signaling, transport, keychain, and
//! event-bus adapters. They are intentionally implementation-free so production
//! networking, platform keychain, and OpenMLS integrations can be wired without
//! weakening the current local-first command honesty gates.
use crate::{
    AppSnapshot, ChannelKind, SafetyVerificationRequest, SafetyVerificationResult,
    SendMessageRequest,
};
use mls_core::ExportLabel;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Shared result type for service boundary calls.
pub type ServiceResult<T> = Result<T, ServiceBoundaryError>;

/// Stable user identifier at service boundaries.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct UserId(pub String);

/// Stable device identifier at service boundaries.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub String);

/// Stable group/room identifier at service boundaries.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct GroupId(pub String);

/// Stable channel identifier at service boundaries.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct ChannelId(pub String);

/// Stable message identifier at service boundaries.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

/// Stable relay identifier at overlay boundaries.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct RelayId(pub String);

/// Stable signaling/media session identifier.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

/// Keychain slot name for sealed local-device secrets.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct SecretName(pub String);

/// Event-bus topic name.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct EventTopic(pub String);

/// Opaque bytes passed across implementation seams.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpaqueBytes(pub Vec<u8>);

/// Rust-only exporter secret material.
///
/// This type intentionally does not implement `Serialize`/`Deserialize`, and its
/// debug form redacts bytes. Command/Tauri/UI-facing boundaries should exchange
/// ciphertext, protected frames, KIDs, counters, and message ids instead of this
/// raw exporter material.
#[derive(Clone, Eq, PartialEq)]
pub struct RustExporterSecret {
    bytes: Vec<u8>,
}

impl RustExporterSecret {
    /// Wrap raw exporter material inside the Rust-only boundary type.
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }

    /// Borrow bytes for Rust-owned text/media/content-key adapters.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl core::fmt::Debug for RustExporterSecret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RustExporterSecret")
            .field("len", &self.bytes.len())
            .field("raw", &"<redacted>")
            .finish()
    }
}

/// Rust services allowed to receive MLS/OpenMLS exporter material.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RustExporterSecretService {
    /// Text encryption/history delivery service.
    Text,
    /// Media/SFrame service.
    Media,
    /// Message content-key service.
    ContentKey,
}

impl RustExporterSecretService {
    /// Map the service boundary to an approved MLS exporter label.
    #[must_use]
    pub fn export_label(self) -> ExportLabel {
        match self {
            Self::Text => ExportLabel::Text,
            Self::Media => ExportLabel::Media,
            Self::ContentKey => ExportLabel::ContentKey,
        }
    }
}

/// Errors shared by boundary traits.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ServiceBoundaryError {
    /// A requested entity does not exist in the backing service.
    #[error("service entity not found: {0}")]
    NotFound(String),
    /// Caller supplied invalid input.
    #[error("invalid service request: {0}")]
    InvalidRequest(String),
    /// A cryptographic verification or authorization gate failed.
    #[error("service verification failed: {0}")]
    VerificationFailed(String),
    /// The concrete adapter is not configured in this build/profile.
    #[error("service adapter unavailable: {0}")]
    AdapterUnavailable(String),
    /// The underlying durable store failed.
    #[error("service persistence failed: {0}")]
    Persistence(String),
}

/// Local account identity summary exposed by the identity boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IdentitySummary {
    /// Stable local user id.
    pub user_id: UserId,
    /// Current local device id.
    pub device_id: DeviceId,
    /// Friend-code or QR payload owned by the identity layer.
    pub friend_code: String,
    /// Safety number for explicit out-of-band verification UX.
    pub safety_number: String,
}

/// Device enrollment request passed to the identity/group-crypto boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeviceEnrollment {
    /// Device being added or rotated.
    pub device_id: DeviceId,
    /// Human-readable label shown in transparency notices.
    pub label: String,
    /// Opaque device public key or MLS leaf material.
    pub public_key: OpaqueBytes,
}

/// Group creation request for group-crypto services.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupCryptoRequest {
    /// User-visible group name.
    pub name: String,
    /// Initial channel kind for command-facing shell setup.
    pub initial_channel_kind: ChannelKind,
}

/// Group crypto state summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupCryptoState {
    /// Group identifier.
    pub group_id: GroupId,
    /// Accepted MLS-like epoch.
    pub epoch: u64,
    /// Opaque tree hash/confirmation material.
    pub epoch_summary: OpaqueBytes,
}

/// Epoch-bound opaque group commit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupCommit {
    /// Group being changed.
    pub group_id: GroupId,
    /// Commit epoch.
    pub epoch: u64,
    /// Opaque MLS/OpenMLS commit bytes.
    pub commit: OpaqueBytes,
}

/// Group member/device operation request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupMemberOperation {
    /// Group being changed.
    pub group_id: GroupId,
    /// Account or user label.
    pub member: UserId,
    /// Optional device leaf label for multi-device operations.
    pub device: Option<DeviceId>,
}

/// Result of an OpenMLS-backed group member/device commit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupOperationResult {
    /// Group state after the operation is merged locally.
    pub state: GroupCryptoState,
    /// Commit message for existing members.
    pub commit: OpaqueBytes,
    /// Welcome message for added members/devices, if produced.
    pub welcome: Option<OpaqueBytes>,
    /// GroupInfo for joiners/external validation, if produced.
    pub group_info: Option<OpaqueBytes>,
}

/// Governance action envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GovernanceCommand {
    /// Group affected by the action.
    pub group_id: GroupId,
    /// Actor submitting the action.
    pub actor: UserId,
    /// Opaque signed governance payload.
    pub signed_payload: OpaqueBytes,
}

/// Accepted governance event reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GovernanceReceipt {
    /// Group that accepted the event.
    pub group_id: GroupId,
    /// Epoch under which the event was accepted.
    pub epoch: u64,
    /// Canonical ordered event id/hash.
    pub event_ref: String,
}

/// Admission invite creation request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InviteRequest {
    /// Group for the invite.
    pub group_id: GroupId,
    /// Creator/admin identity.
    pub creator: UserId,
    /// Optional password gate label; no offline verifier crosses this boundary.
    pub password_gate: Option<String>,
    /// Maximum uses before final MLS admission.
    pub max_uses: u32,
}

/// Admission result after invite/password/helper checks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdmissionTicket {
    /// Group the user may attempt to join.
    pub group_id: GroupId,
    /// Invite or admission token.
    pub ticket: OpaqueBytes,
    /// Honest copy: final admission still requires an authorized Welcome/add.
    pub welcome_required: bool,
}

/// Text history page returned by the text service.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextHistoryPage {
    /// Channel being read.
    pub channel_id: ChannelId,
    /// Ordered encrypted/plaintext-boundary message ids for the current adapter.
    pub message_ids: Vec<MessageId>,
    /// Cursor for the next page, if any.
    pub next_cursor: Option<String>,
}

/// Media session request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MediaSessionRequest {
    /// Group containing the media room.
    pub group_id: GroupId,
    /// Voice/video channel to join.
    pub channel_id: ChannelId,
    /// Local participant identity.
    pub participant: UserId,
}

/// Current media session state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MediaSessionState {
    /// Session id assigned by the media adapter.
    pub session_id: SessionId,
    /// Whether media is locally joined.
    pub joined: bool,
    /// Honest route/readiness copy for UI surfaces.
    pub route_copy: String,
}

/// Overlay route through content-blind relay nodes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OverlayRoute {
    /// Ordered relay hops.
    pub relays: Vec<RelayId>,
    /// Hop count after route selection.
    pub hop_count: u8,
    /// Whether every hop is ciphertext-only.
    pub ciphertext_only: bool,
}

/// Signaling envelope. Payload remains opaque/content-free at this layer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignalEnvelope {
    /// Signaling session.
    pub session_id: SessionId,
    /// Sender device.
    pub sender: DeviceId,
    /// Opaque rendezvous payload.
    pub payload: OpaqueBytes,
}

/// Connectivity candidate or selected transport leg.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportLeg {
    /// Leg label such as STUN, relay-overlay, or TURN.
    pub label: String,
    /// Endpoint or route descriptor; may be owner-provided.
    pub endpoint: String,
    /// Whether this leg carries ciphertext only.
    pub ciphertext_only: bool,
}

/// App-facing text/control data frame kind carried by the selected transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TextControlFrameKind {
    /// Encrypted user text/message delivery frame.
    Text,
    /// Encrypted control/governance/transport coordination frame.
    Control,
}

/// Opaque text/control frame for the selected data transport.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextControlFrame {
    /// Transport session carrying the frame.
    pub session_id: SessionId,
    /// Frame class visible to local app logic.
    pub kind: TextControlFrameKind,
    /// Already-protected opaque bytes.
    pub payload: OpaqueBytes,
}

/// Store record addressed by the app-state store boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoreRecord {
    /// Namespaced key.
    pub key: String,
    /// Serialized bytes.
    pub value: OpaqueBytes,
}

/// Event emitted across the local app event bus.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppEvent {
    /// Event topic.
    pub topic: EventTopic,
    /// Monotonic sequence assigned by the bus.
    pub sequence: u64,
    /// Opaque event payload.
    pub payload: OpaqueBytes,
}

/// Identity boundary for account/device identity and explicit safety verification.
pub trait IdentityService {
    /// Return the local account/device identity summary.
    fn local_identity(&self) -> ServiceResult<IdentitySummary>;

    /// Verify a safety-number comparison owned by the identity layer.
    fn verify_safety_number(
        &mut self,
        request: SafetyVerificationRequest,
    ) -> ServiceResult<SafetyVerificationResult>;

    /// Enroll or rotate an own device.
    fn enroll_device(&mut self, enrollment: DeviceEnrollment) -> ServiceResult<IdentitySummary>;
}

/// Group crypto boundary for MLS/OpenMLS state, epochs, and commits.
pub trait GroupCryptoService {
    /// Create a group crypto state shell.
    fn create_group_crypto(
        &mut self,
        request: GroupCryptoRequest,
    ) -> ServiceResult<GroupCryptoState>;

    /// Add a member as a new OpenMLS leaf and return commit/Welcome material.
    fn add_group_member(
        &mut self,
        operation: GroupMemberOperation,
    ) -> ServiceResult<GroupOperationResult>;

    /// Add a device as a distinct OpenMLS leaf under an existing account label.
    fn add_group_device(
        &mut self,
        operation: GroupMemberOperation,
    ) -> ServiceResult<GroupOperationResult>;

    /// Remove a member leaf and return the resulting rekey commit.
    fn remove_group_member(
        &mut self,
        operation: GroupMemberOperation,
    ) -> ServiceResult<GroupOperationResult>;

    /// Remove a device leaf and return the resulting rekey commit.
    fn remove_group_device(
        &mut self,
        operation: GroupMemberOperation,
    ) -> ServiceResult<GroupOperationResult>;

    /// Apply an opaque, already-authenticated group commit.
    fn apply_group_commit(&mut self, commit: GroupCommit) -> ServiceResult<GroupCryptoState>;
}

/// Rust-only exporter boundary for encrypted payload services.
///
/// Implement this alongside concrete Rust text/media/content-key adapters. Do
/// not expose it through command handlers, Tauri invoke payloads, governance,
/// admission, relay, signaling, transport, or generic keychain APIs.
pub trait RustExporterSecretProvider {
    /// Export material for one approved Rust payload service under the current epoch.
    fn export_rust_service_secret(
        &self,
        group_id: &GroupId,
        service: RustExporterSecretService,
        context: &[u8],
    ) -> ServiceResult<RustExporterSecret>;
}

/// Real OpenMLS-backed implementation of [`GroupCryptoService`].
///
/// The service keeps raw exporter bytes inside the Rust service boundary and
/// persists group state through `discrypt_mls_core::OpenMlsGroupEngine`, which
/// uses upstream OpenMLS provider/storage APIs.
pub struct OpenMlsGroupCryptoService {
    engine: mls_core::OpenMlsGroupEngine,
}

impl OpenMlsGroupCryptoService {
    /// Open the service using an OpenMLS SQLite storage database.
    pub fn open(path: impl AsRef<std::path::Path>) -> ServiceResult<Self> {
        Ok(Self {
            engine: mls_core::OpenMlsGroupEngine::open(path).map_err(group_crypto_error)?,
        })
    }

    /// Load a persisted group from OpenMLS storage using the caller-held signer handle.
    pub fn load_group(
        &mut self,
        group_id: &GroupId,
        signer_public_key: &[u8],
    ) -> ServiceResult<GroupCryptoState> {
        self.engine
            .load_group(&group_id.0, signer_public_key)
            .map(group_state_from_snapshot)
            .map_err(group_crypto_error)
    }

    /// Return the signer public key handle for a live group.
    pub fn signer_public_key(&self, group_id: &GroupId) -> ServiceResult<Vec<u8>> {
        self.engine
            .signer_public_key(&group_id.0)
            .map_err(group_crypto_error)
    }

    /// Stage a real OpenMLS add-member commit and return its opaque commit bytes.
    pub fn stage_add_member_commit(
        &mut self,
        group_id: &GroupId,
        member_identity: &[u8],
    ) -> ServiceResult<GroupCommit> {
        let current = self
            .engine
            .snapshot(&group_id.0)
            .map_err(group_crypto_error)?
            .epoch;
        let member = self
            .engine
            .generate_member_package(member_identity)
            .map_err(group_crypto_error)?;
        let commit = self
            .engine
            .stage_add_member(&group_id.0, &member)
            .map_err(group_crypto_error)?;
        Ok(GroupCommit {
            group_id: group_id.clone(),
            epoch: current.saturating_add(1),
            commit: OpaqueBytes(commit),
        })
    }
}

impl GroupCryptoService for OpenMlsGroupCryptoService {
    fn create_group_crypto(
        &mut self,
        request: GroupCryptoRequest,
    ) -> ServiceResult<GroupCryptoState> {
        let group_id = normalize_group_crypto_id(&request.name);
        self.engine
            .create_group(&group_id, request.name.as_bytes())
            .map(group_state_from_snapshot)
            .map_err(group_crypto_error)
    }

    fn add_group_member(
        &mut self,
        operation: GroupMemberOperation,
    ) -> ServiceResult<GroupOperationResult> {
        self.engine
            .add_member(&operation.group_id.0, operation.member.0.as_bytes())
            .map(group_operation_from_openmls)
            .map_err(group_crypto_error)
    }

    fn add_group_device(
        &mut self,
        operation: GroupMemberOperation,
    ) -> ServiceResult<GroupOperationResult> {
        let device = operation.device.ok_or_else(|| {
            ServiceBoundaryError::InvalidRequest(
                "add_group_device requires a concrete device id".to_owned(),
            )
        })?;
        self.engine
            .add_device(&operation.group_id.0, &operation.member.0, &device.0)
            .map(group_operation_from_openmls)
            .map_err(group_crypto_error)
    }

    fn remove_group_member(
        &mut self,
        operation: GroupMemberOperation,
    ) -> ServiceResult<GroupOperationResult> {
        self.engine
            .remove_member(&operation.group_id.0, &operation.member.0)
            .map(group_operation_from_openmls)
            .map_err(group_crypto_error)
    }

    fn remove_group_device(
        &mut self,
        operation: GroupMemberOperation,
    ) -> ServiceResult<GroupOperationResult> {
        let device = operation.device.ok_or_else(|| {
            ServiceBoundaryError::InvalidRequest(
                "remove_group_device requires a concrete device id".to_owned(),
            )
        })?;
        self.engine
            .remove_device(&operation.group_id.0, &operation.member.0, &device.0)
            .map(group_operation_from_openmls)
            .map_err(group_crypto_error)
    }

    fn apply_group_commit(&mut self, commit: GroupCommit) -> ServiceResult<GroupCryptoState> {
        self.engine
            .merge_pending_commit(&commit.group_id.0, commit.epoch, &commit.commit.0)
            .map(group_state_from_snapshot)
            .map_err(group_crypto_error)
    }
}

impl RustExporterSecretProvider for OpenMlsGroupCryptoService {
    fn export_rust_service_secret(
        &self,
        group_id: &GroupId,
        service: RustExporterSecretService,
        context: &[u8],
    ) -> ServiceResult<RustExporterSecret> {
        self.engine
            .export_secret(&group_id.0, service.export_label().as_str(), context, 32)
            .map(RustExporterSecret::new)
            .map_err(group_crypto_error)
    }
}

fn normalize_group_crypto_id(name: &str) -> String {
    let normalized = name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    if normalized.is_empty() {
        "group".to_owned()
    } else {
        normalized
    }
}

fn group_state_from_snapshot(snapshot: mls_core::OpenMlsGroupSnapshot) -> GroupCryptoState {
    GroupCryptoState {
        group_id: GroupId(snapshot.group_id),
        epoch: snapshot.epoch,
        epoch_summary: OpaqueBytes(snapshot.confirmation_tag),
    }
}

fn group_operation_from_openmls(
    output: mls_core::OpenMlsGroupOperationResult,
) -> GroupOperationResult {
    GroupOperationResult {
        state: group_state_from_snapshot(output.state),
        commit: OpaqueBytes(output.commit),
        welcome: output.welcome.map(OpaqueBytes),
        group_info: output.group_info.map(OpaqueBytes),
    }
}

fn group_crypto_error(error: mls_core::OpenMlsGroupError) -> ServiceBoundaryError {
    match error {
        mls_core::OpenMlsGroupError::GroupNotFound(group_id) => {
            ServiceBoundaryError::NotFound(group_id)
        }
        mls_core::OpenMlsGroupError::MemberNotFound { .. }
        | mls_core::OpenMlsGroupError::MemberAlreadyExists { .. } => {
            ServiceBoundaryError::InvalidRequest(error.to_string())
        }
        mls_core::OpenMlsGroupError::CommitMismatch(_)
        | mls_core::OpenMlsGroupError::StaleCommitEpoch { .. } => {
            ServiceBoundaryError::VerificationFailed(error.to_string())
        }
        mls_core::OpenMlsGroupError::SignerNotFound { .. } => {
            ServiceBoundaryError::Persistence(error.to_string())
        }
        _ => ServiceBoundaryError::Persistence(error.to_string()),
    }
}

/// Governance boundary for signed epoch-bound room policy events.
pub trait GovernanceService {
    /// Submit a governance command for canonical ordering/authority checks.
    fn submit_governance(&mut self, command: GovernanceCommand)
        -> ServiceResult<GovernanceReceipt>;

    /// Return the caller's current role label in a group.
    fn role_for_user(&self, group_id: &GroupId, user_id: &UserId) -> ServiceResult<String>;
}

/// Admission boundary for invites, password/helper checks, and final Welcome gating.
pub trait AdmissionService {
    /// Create an invite/admission ticket without exposing offline verifier material.
    fn create_invite(&mut self, request: InviteRequest) -> ServiceResult<AdmissionTicket>;

    /// Redeem an admission ticket after invite/password/helper checks.
    fn redeem_invite(&mut self, ticket: AdmissionTicket) -> ServiceResult<AdmissionTicket>;
}

/// Text boundary for encrypted message send and history pagination.
pub trait TextService {
    /// Send a text message through the current encrypted-message adapter.
    fn send_text(&mut self, request: SendMessageRequest) -> ServiceResult<MessageId>;

    /// Load a page of text history for a channel.
    fn load_text_history(
        &self,
        channel_id: &ChannelId,
        cursor: Option<String>,
    ) -> ServiceResult<TextHistoryPage>;
}

/// Media boundary for voice/video session state and encrypted media frames.
pub trait MediaService {
    /// Join a local media session.
    fn join_media(&mut self, request: MediaSessionRequest) -> ServiceResult<MediaSessionState>;

    /// Leave a media session.
    fn leave_media(&mut self, session_id: &SessionId) -> ServiceResult<MediaSessionState>;

    /// Send an opaque encrypted media frame.
    fn send_media_frame(&mut self, session_id: &SessionId, frame: OpaqueBytes)
        -> ServiceResult<()>;
}

/// Relay-overlay boundary for content-blind routing and forwarding.
pub trait OverlayService {
    /// Select a bounded relay route for a group/session.
    fn select_overlay_route(&self, group_id: &GroupId) -> ServiceResult<OverlayRoute>;

    /// Forward ciphertext across the selected route.
    fn forward_ciphertext(
        &mut self,
        route: &OverlayRoute,
        ciphertext: OpaqueBytes,
    ) -> ServiceResult<()>;
}

/// Signaling boundary for content-blind rendezvous.
pub trait SignalingService {
    /// Publish an opaque signaling envelope.
    fn publish_signal(&mut self, envelope: SignalEnvelope) -> ServiceResult<()>;

    /// Poll opaque signaling envelopes for a session.
    fn poll_signals(&mut self, session_id: &SessionId) -> ServiceResult<Vec<SignalEnvelope>>;
}

/// Transport boundary for connectivity planning and ciphertext-only streams.
pub trait TransportService {
    /// Plan ordered connectivity fallback legs.
    fn plan_transport(&self, group_id: &GroupId) -> ServiceResult<Vec<TransportLeg>>;

    /// Open one selected transport leg for a session.
    fn open_transport(&mut self, session_id: &SessionId, leg: TransportLeg) -> ServiceResult<()>;

    /// Return the latest transport session state/route snapshot for UI and command surfaces.
    fn transport_state(
        &self,
        session_id: &SessionId,
    ) -> ServiceResult<transport::TransportSessionSnapshot>;
}

/// Single app-facing data transport seam for encrypted text/control frames.
pub trait TextControlTransportService {
    /// Send one opaque text/control frame through an opened transport session.
    fn send_text_control_frame(&mut self, frame: TextControlFrame) -> ServiceResult<()>;

    /// Poll opaque text/control frames received for a transport session.
    fn poll_text_control_frames(
        &mut self,
        session_id: &SessionId,
    ) -> ServiceResult<Vec<TextControlFrame>>;
}

/// Durable app-state store boundary.
pub trait AppStateStoreService {
    /// Load a namespaced record.
    fn load_record(&self, key: &str) -> ServiceResult<Option<StoreRecord>>;

    /// Save a namespaced record.
    fn save_record(&mut self, record: StoreRecord) -> ServiceResult<()>;
}

/// Platform/local keychain boundary for sealed secrets.
pub trait KeychainService {
    /// Seal a secret into a named local-device keychain slot.
    fn seal_secret(&mut self, name: SecretName, plaintext: OpaqueBytes) -> ServiceResult<()>;

    /// Open a named local-device keychain slot.
    fn open_secret(&self, name: &SecretName) -> ServiceResult<Option<OpaqueBytes>>;

    /// Delete a named local-device keychain slot.
    fn delete_secret(&mut self, name: &SecretName) -> ServiceResult<()>;
}

/// Local event-bus boundary for command, adapter, and UI activity events.
pub trait EventBusService {
    /// Publish an event to the local bus.
    fn publish_event(&mut self, topic: EventTopic, payload: OpaqueBytes)
        -> ServiceResult<AppEvent>;

    /// Drain events from a topic after an optional sequence.
    fn drain_events(
        &mut self,
        topic: &EventTopic,
        after: Option<u64>,
    ) -> ServiceResult<Vec<AppEvent>>;
}

/// Aggregate app-service adapter boundary.
///
/// Production shells can depend on this trait object while individual adapter
/// crates implement the narrower service traits they own. The aggregate remains
/// intentionally behavior-free: it is a compile-time contract, not a concrete
/// network, keychain, or MLS implementation.
pub trait AppServiceBoundary:
    IdentityService
    + GroupCryptoService
    + GovernanceService
    + AdmissionService
    + TextService
    + MediaService
    + OverlayService
    + SignalingService
    + TransportService
    + TextControlTransportService
    + AppStateStoreService
    + KeychainService
    + EventBusService
{
    /// Return the current command-facing snapshot assembled from service state.
    fn command_snapshot(&self) -> ServiceResult<AppSnapshot>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app_snapshot, ChannelKind};
    use std::collections::BTreeMap;

    #[derive(Debug)]
    struct BoundaryHarness {
        snapshot: AppSnapshot,
        records: BTreeMap<String, StoreRecord>,
        secrets: BTreeMap<SecretName, OpaqueBytes>,
        signals: Vec<SignalEnvelope>,
        transport_snapshots: BTreeMap<SessionId, transport::TransportSessionSnapshot>,
        text_control_frames: Vec<TextControlFrame>,
        events: Vec<AppEvent>,
        next_event_sequence: u64,
    }

    impl Default for BoundaryHarness {
        fn default() -> Self {
            Self {
                snapshot: app_snapshot(),
                records: BTreeMap::new(),
                secrets: BTreeMap::new(),
                signals: Vec::new(),
                transport_snapshots: BTreeMap::new(),
                text_control_frames: Vec::new(),
                events: Vec::new(),
                next_event_sequence: 0,
            }
        }
    }

    fn transport_error(error: transport::TransportSessionError) -> ServiceBoundaryError {
        ServiceBoundaryError::AdapterUnavailable(error.to_string())
    }

    fn hex_for_test(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        out
    }

    impl IdentityService for BoundaryHarness {
        fn local_identity(&self) -> ServiceResult<IdentitySummary> {
            Ok(IdentitySummary {
                user_id: UserId("alice".to_owned()),
                device_id: DeviceId(self.snapshot.devices[0].device_id.clone()),
                friend_code: self.snapshot.friend.friend_code.clone(),
                safety_number: self.snapshot.friend.safety_number.clone(),
            })
        }

        fn verify_safety_number(
            &mut self,
            request: SafetyVerificationRequest,
        ) -> ServiceResult<SafetyVerificationResult> {
            let verified = request.friend_id == self.snapshot.friend.friend_code
                && request.provided == self.snapshot.friend.safety_number;
            Ok(SafetyVerificationResult {
                verified,
                message: "boundary-owned verification result".to_owned(),
            })
        }

        fn enroll_device(
            &mut self,
            enrollment: DeviceEnrollment,
        ) -> ServiceResult<IdentitySummary> {
            let mut identity = self.local_identity()?;
            identity.device_id = enrollment.device_id;
            Ok(identity)
        }
    }

    impl GroupCryptoService for BoundaryHarness {
        fn create_group_crypto(
            &mut self,
            request: GroupCryptoRequest,
        ) -> ServiceResult<GroupCryptoState> {
            Ok(GroupCryptoState {
                group_id: GroupId(request.name),
                epoch: 1,
                epoch_summary: OpaqueBytes(vec![request.initial_channel_kind as u8]),
            })
        }

        fn add_group_member(
            &mut self,
            operation: GroupMemberOperation,
        ) -> ServiceResult<GroupOperationResult> {
            Ok(GroupOperationResult {
                state: GroupCryptoState {
                    group_id: operation.group_id,
                    epoch: 2,
                    epoch_summary: OpaqueBytes(operation.member.0.as_bytes().to_vec()),
                },
                commit: OpaqueBytes(b"add-member-commit".to_vec()),
                welcome: Some(OpaqueBytes(b"welcome-member".to_vec())),
                group_info: Some(OpaqueBytes(b"group-info-member".to_vec())),
            })
        }

        fn add_group_device(
            &mut self,
            operation: GroupMemberOperation,
        ) -> ServiceResult<GroupOperationResult> {
            let device = operation.device.ok_or_else(|| {
                ServiceBoundaryError::InvalidRequest(
                    "device operation requires a device id".to_owned(),
                )
            })?;
            Ok(GroupOperationResult {
                state: GroupCryptoState {
                    group_id: operation.group_id,
                    epoch: 2,
                    epoch_summary: OpaqueBytes(device.0.as_bytes().to_vec()),
                },
                commit: OpaqueBytes(b"add-device-commit".to_vec()),
                welcome: Some(OpaqueBytes(b"welcome-device".to_vec())),
                group_info: Some(OpaqueBytes(b"group-info-device".to_vec())),
            })
        }

        fn remove_group_member(
            &mut self,
            operation: GroupMemberOperation,
        ) -> ServiceResult<GroupOperationResult> {
            Ok(GroupOperationResult {
                state: GroupCryptoState {
                    group_id: operation.group_id,
                    epoch: 3,
                    epoch_summary: OpaqueBytes(operation.member.0.as_bytes().to_vec()),
                },
                commit: OpaqueBytes(b"remove-member-commit".to_vec()),
                welcome: None,
                group_info: None,
            })
        }

        fn remove_group_device(
            &mut self,
            operation: GroupMemberOperation,
        ) -> ServiceResult<GroupOperationResult> {
            let device = operation.device.ok_or_else(|| {
                ServiceBoundaryError::InvalidRequest(
                    "device operation requires a device id".to_owned(),
                )
            })?;
            Ok(GroupOperationResult {
                state: GroupCryptoState {
                    group_id: operation.group_id,
                    epoch: 3,
                    epoch_summary: OpaqueBytes(device.0.as_bytes().to_vec()),
                },
                commit: OpaqueBytes(b"remove-device-commit".to_vec()),
                welcome: None,
                group_info: None,
            })
        }

        fn apply_group_commit(&mut self, commit: GroupCommit) -> ServiceResult<GroupCryptoState> {
            Ok(GroupCryptoState {
                group_id: commit.group_id,
                epoch: commit.epoch,
                epoch_summary: commit.commit,
            })
        }
    }

    impl RustExporterSecretProvider for BoundaryHarness {
        fn export_rust_service_secret(
            &self,
            group_id: &GroupId,
            service: RustExporterSecretService,
            context: &[u8],
        ) -> ServiceResult<RustExporterSecret> {
            Ok(RustExporterSecret::new(format!(
                "{}:{:?}:{}:{}",
                group_id.0,
                service.export_label(),
                service as u8,
                hex_for_test(context)
            )))
        }
    }

    impl GovernanceService for BoundaryHarness {
        fn submit_governance(
            &mut self,
            command: GovernanceCommand,
        ) -> ServiceResult<GovernanceReceipt> {
            Ok(GovernanceReceipt {
                group_id: command.group_id,
                epoch: 1,
                event_ref: "governance:event:1".to_owned(),
            })
        }

        fn role_for_user(&self, _group_id: &GroupId, _user_id: &UserId) -> ServiceResult<String> {
            Ok("owner".to_owned())
        }
    }

    impl AdmissionService for BoundaryHarness {
        fn create_invite(&mut self, request: InviteRequest) -> ServiceResult<AdmissionTicket> {
            Ok(AdmissionTicket {
                group_id: request.group_id,
                ticket: OpaqueBytes(vec![request.max_uses as u8]),
                welcome_required: true,
            })
        }

        fn redeem_invite(&mut self, ticket: AdmissionTicket) -> ServiceResult<AdmissionTicket> {
            Ok(ticket)
        }
    }

    impl TextService for BoundaryHarness {
        fn send_text(&mut self, request: SendMessageRequest) -> ServiceResult<MessageId> {
            Ok(MessageId(format!(
                "{}:{}",
                request.channel,
                request.body.len()
            )))
        }

        fn load_text_history(
            &self,
            channel_id: &ChannelId,
            _cursor: Option<String>,
        ) -> ServiceResult<TextHistoryPage> {
            Ok(TextHistoryPage {
                channel_id: channel_id.clone(),
                message_ids: vec![MessageId("m1".to_owned())],
                next_cursor: None,
            })
        }
    }

    impl MediaService for BoundaryHarness {
        fn join_media(&mut self, request: MediaSessionRequest) -> ServiceResult<MediaSessionState> {
            Ok(MediaSessionState {
                session_id: SessionId(format!("media:{}", request.channel_id.0)),
                joined: true,
                route_copy: "test media boundary only".to_owned(),
            })
        }

        fn leave_media(&mut self, session_id: &SessionId) -> ServiceResult<MediaSessionState> {
            Ok(MediaSessionState {
                session_id: session_id.clone(),
                joined: false,
                route_copy: "left".to_owned(),
            })
        }

        fn send_media_frame(
            &mut self,
            _session_id: &SessionId,
            _frame: OpaqueBytes,
        ) -> ServiceResult<()> {
            Ok(())
        }
    }

    impl OverlayService for BoundaryHarness {
        fn select_overlay_route(&self, _group_id: &GroupId) -> ServiceResult<OverlayRoute> {
            Ok(OverlayRoute {
                relays: vec![RelayId("r1".to_owned())],
                hop_count: 1,
                ciphertext_only: true,
            })
        }

        fn forward_ciphertext(
            &mut self,
            route: &OverlayRoute,
            ciphertext: OpaqueBytes,
        ) -> ServiceResult<()> {
            if !route.ciphertext_only || ciphertext.0.is_empty() {
                return Err(ServiceBoundaryError::InvalidRequest(
                    "overlay requires ciphertext-only non-empty payload".to_owned(),
                ));
            }
            Ok(())
        }
    }

    impl SignalingService for BoundaryHarness {
        fn publish_signal(&mut self, envelope: SignalEnvelope) -> ServiceResult<()> {
            self.signals.push(envelope);
            Ok(())
        }

        fn poll_signals(&mut self, session_id: &SessionId) -> ServiceResult<Vec<SignalEnvelope>> {
            Ok(self
                .signals
                .iter()
                .filter(|signal| &signal.session_id == session_id)
                .cloned()
                .collect())
        }
    }

    impl TransportService for BoundaryHarness {
        fn plan_transport(&self, _group_id: &GroupId) -> ServiceResult<Vec<TransportLeg>> {
            Ok(vec![TransportLeg {
                label: "relay-overlay".to_owned(),
                endpoint: "r1".to_owned(),
                ciphertext_only: true,
            }])
        }

        fn open_transport(
            &mut self,
            session_id: &SessionId,
            leg: TransportLeg,
        ) -> ServiceResult<()> {
            if !leg.ciphertext_only {
                return Err(ServiceBoundaryError::VerificationFailed(
                    "transport leg must be ciphertext-only".to_owned(),
                ));
            }
            let mut session = transport::TransportSession::new();
            session.begin_signaling().map_err(transport_error)?;
            session.begin_ice_gathering().map_err(transport_error)?;
            session.begin_checking().map_err(transport_error)?;
            let endpoint = transport::Endpoint::new(leg.endpoint);
            let snapshot = match leg.label.as_str() {
                "direct-ice" | "stun" => session.select_direct(endpoint),
                "turn" | "turn-relay" => session.select_turn_relay(endpoint),
                _ => session.select_overlay_relay(endpoint),
            }
            .map_err(transport_error)?;
            self.transport_snapshots
                .insert(session_id.clone(), snapshot);
            Ok(())
        }

        fn transport_state(
            &self,
            session_id: &SessionId,
        ) -> ServiceResult<transport::TransportSessionSnapshot> {
            Ok(self
                .transport_snapshots
                .get(session_id)
                .cloned()
                .unwrap_or_else(|| transport::TransportSession::new().snapshot()))
        }
    }

    impl TextControlTransportService for BoundaryHarness {
        fn send_text_control_frame(&mut self, frame: TextControlFrame) -> ServiceResult<()> {
            let snapshot = self.transport_state(&frame.session_id)?;
            if !snapshot.connected() {
                return Err(ServiceBoundaryError::AdapterUnavailable(
                    "text/control transport session is not connected".to_owned(),
                ));
            }
            if frame.payload.0.is_empty() {
                return Err(ServiceBoundaryError::InvalidRequest(
                    "text/control frame must be non-empty opaque bytes".to_owned(),
                ));
            }
            self.text_control_frames.push(frame);
            Ok(())
        }

        fn poll_text_control_frames(
            &mut self,
            session_id: &SessionId,
        ) -> ServiceResult<Vec<TextControlFrame>> {
            Ok(self
                .text_control_frames
                .iter()
                .filter(|frame| &frame.session_id == session_id)
                .cloned()
                .collect())
        }
    }

    impl AppStateStoreService for BoundaryHarness {
        fn load_record(&self, key: &str) -> ServiceResult<Option<StoreRecord>> {
            Ok(self.records.get(key).cloned())
        }

        fn save_record(&mut self, record: StoreRecord) -> ServiceResult<()> {
            self.records.insert(record.key.clone(), record);
            Ok(())
        }
    }

    impl KeychainService for BoundaryHarness {
        fn seal_secret(&mut self, name: SecretName, plaintext: OpaqueBytes) -> ServiceResult<()> {
            self.secrets.insert(name, plaintext);
            Ok(())
        }

        fn open_secret(&self, name: &SecretName) -> ServiceResult<Option<OpaqueBytes>> {
            Ok(self.secrets.get(name).cloned())
        }

        fn delete_secret(&mut self, name: &SecretName) -> ServiceResult<()> {
            self.secrets.remove(name);
            Ok(())
        }
    }

    impl EventBusService for BoundaryHarness {
        fn publish_event(
            &mut self,
            topic: EventTopic,
            payload: OpaqueBytes,
        ) -> ServiceResult<AppEvent> {
            self.next_event_sequence += 1;
            let event = AppEvent {
                topic,
                sequence: self.next_event_sequence,
                payload,
            };
            self.events.push(event.clone());
            Ok(event)
        }

        fn drain_events(
            &mut self,
            topic: &EventTopic,
            after: Option<u64>,
        ) -> ServiceResult<Vec<AppEvent>> {
            Ok(self
                .events
                .iter()
                .filter(|event| {
                    &event.topic == topic && after.is_none_or(|sequence| event.sequence > sequence)
                })
                .cloned()
                .collect())
        }
    }

    impl AppServiceBoundary for BoundaryHarness {
        fn command_snapshot(&self) -> ServiceResult<AppSnapshot> {
            Ok(self.snapshot.clone())
        }
    }

    fn temp_openmls_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "discrypt-core-openmls-{name}-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ))
    }

    #[test]
    fn openmls_group_crypto_service_persists_epochs_confirmations_and_exports() -> ServiceResult<()>
    {
        let path = temp_openmls_path("service");
        let mut service = OpenMlsGroupCryptoService::open(&path)?;
        let created = service.create_group_crypto(GroupCryptoRequest {
            name: "Phase D Lab".to_owned(),
            initial_channel_kind: ChannelKind::Text,
        })?;
        assert_eq!(created.group_id, GroupId("phase-d-lab".to_owned()));
        assert_eq!(created.epoch, 0);
        assert!(!created.epoch_summary.0.is_empty());
        let signer_public_key = service.signer_public_key(&created.group_id)?;
        let before = service.export_rust_service_secret(
            &created.group_id,
            RustExporterSecretService::Text,
            b"room",
        )?;

        let added = service.add_group_member(GroupMemberOperation {
            group_id: created.group_id.clone(),
            member: UserId("carol".to_owned()),
            device: None,
        })?;
        assert_eq!(added.state.epoch, 1);
        assert!(added.welcome.is_some());
        assert!(!added.commit.0.is_empty());
        let with_device = service.add_group_device(GroupMemberOperation {
            group_id: created.group_id.clone(),
            member: UserId("carol".to_owned()),
            device: Some(DeviceId("phone".to_owned())),
        })?;
        assert_eq!(with_device.state.epoch, 2);
        let removed_device = service.remove_group_device(GroupMemberOperation {
            group_id: created.group_id.clone(),
            member: UserId("carol".to_owned()),
            device: Some(DeviceId("phone".to_owned())),
        })?;
        assert_eq!(removed_device.state.epoch, 3);
        let removed_member = service.remove_group_member(GroupMemberOperation {
            group_id: created.group_id.clone(),
            member: UserId("carol".to_owned()),
            device: None,
        })?;
        assert_eq!(removed_member.state.epoch, 4);

        let commit = service.stage_add_member_commit(&created.group_id, b"bob")?;
        assert_eq!(commit.epoch, 5);
        assert!(!commit.commit.0.is_empty());
        assert!(matches!(
            service.apply_group_commit(GroupCommit {
                group_id: created.group_id.clone(),
                epoch: 4,
                commit: commit.commit.clone(),
            }),
            Err(ServiceBoundaryError::VerificationFailed(_))
        ));

        let merged = service.apply_group_commit(commit)?;
        assert_eq!(merged.epoch, 5);
        assert_ne!(created.epoch_summary, merged.epoch_summary);
        let after = service.export_rust_service_secret(
            &created.group_id,
            RustExporterSecretService::Text,
            b"room",
        )?;
        assert_ne!(before.as_bytes(), after.as_bytes());
        drop(service);

        let mut reloaded = OpenMlsGroupCryptoService::open(&path)?;
        let restored = reloaded.load_group(&created.group_id, &signer_public_key)?;
        assert_eq!(restored.epoch, merged.epoch);
        assert_eq!(restored.epoch_summary, merged.epoch_summary);
        assert_eq!(
            reloaded
                .export_rust_service_secret(
                    &created.group_id,
                    RustExporterSecretService::Text,
                    b"room",
                )?
                .as_bytes(),
            after.as_bytes()
        );

        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn aggregate_boundary_covers_required_service_seams() -> ServiceResult<()> {
        let mut boundary = BoundaryHarness::default();

        let identity = boundary.local_identity()?;
        assert_eq!(identity.user_id, UserId("alice".to_owned()));
        assert!(
            boundary
                .verify_safety_number(SafetyVerificationRequest {
                    friend_id: identity.friend_code,
                    provided: identity.safety_number,
                })?
                .verified
        );

        let group = boundary.create_group_crypto(GroupCryptoRequest {
            name: "lab".to_owned(),
            initial_channel_kind: ChannelKind::Text,
        })?;
        assert_eq!(group.epoch, 1);
        let add_member = boundary.add_group_member(GroupMemberOperation {
            group_id: group.group_id.clone(),
            member: UserId("bob".to_owned()),
            device: None,
        })?;
        assert_eq!(add_member.state.epoch, 2);
        assert!(add_member.welcome.is_some());
        assert!(!add_member.commit.0.is_empty());
        let add_device = boundary.add_group_device(GroupMemberOperation {
            group_id: group.group_id.clone(),
            member: UserId("alice".to_owned()),
            device: Some(DeviceId("alice-phone".to_owned())),
        })?;
        assert_eq!(add_device.state.epoch, 2);
        assert!(add_device.welcome.is_some());
        let remove_device = boundary.remove_group_device(GroupMemberOperation {
            group_id: group.group_id.clone(),
            member: UserId("alice".to_owned()),
            device: Some(DeviceId("alice-phone".to_owned())),
        })?;
        assert_eq!(remove_device.state.epoch, 3);
        assert!(remove_device.welcome.is_none());
        let remove_member = boundary.remove_group_member(GroupMemberOperation {
            group_id: group.group_id.clone(),
            member: UserId("bob".to_owned()),
            device: None,
        })?;
        assert_eq!(remove_member.state.epoch, 3);
        assert_eq!(
            boundary.role_for_user(&group.group_id, &UserId("alice".to_owned()))?,
            "owner"
        );
        assert!(
            boundary
                .create_invite(InviteRequest {
                    group_id: group.group_id.clone(),
                    creator: UserId("alice".to_owned()),
                    password_gate: Some("online helper".to_owned()),
                    max_uses: 1,
                })?
                .welcome_required
        );
        assert_eq!(
            boundary
                .send_text(SendMessageRequest {
                    channel: "#general".to_owned(),
                    body: "hello".to_owned(),
                })?
                .0,
            "#general:5"
        );

        let route = boundary.select_overlay_route(&group.group_id)?;
        assert!(route.ciphertext_only);
        boundary.forward_ciphertext(&route, OpaqueBytes(vec![1, 2, 3]))?;

        let session_id = SessionId("session-1".to_owned());
        boundary.publish_signal(SignalEnvelope {
            session_id: session_id.clone(),
            sender: DeviceId("alice-laptop".to_owned()),
            payload: OpaqueBytes(vec![7]),
        })?;
        assert_eq!(boundary.poll_signals(&session_id)?.len(), 1);
        let leg = boundary.plan_transport(&group.group_id)?.remove(0);
        boundary.open_transport(&session_id, leg)?;
        assert!(boundary.transport_state(&session_id)?.connected());
        let turn_session_id = SessionId("session-turn".to_owned());
        boundary.open_transport(
            &turn_session_id,
            TransportLeg {
                label: "turn-relay".to_owned(),
                endpoint: "turns:relay.example.invalid:5349".to_owned(),
                ciphertext_only: true,
            },
        )?;
        let turn_state = boundary.transport_state(&turn_session_id)?;
        assert_eq!(
            turn_state.state,
            transport::TransportSessionState::TurnRelay
        );
        assert_eq!(
            turn_state.route.as_ref().map(|route| route.route),
            Some(transport::TransportRoute::TurnRelay)
        );
        assert_eq!(
            turn_state
                .route
                .as_ref()
                .map(|route| route.endpoint.0.as_str()),
            Some("turns:relay.example.invalid:5349")
        );
        boundary.send_text_control_frame(TextControlFrame {
            session_id: turn_session_id.clone(),
            kind: TextControlFrameKind::Text,
            payload: OpaqueBytes(b"ciphertext:text-frame".to_vec()),
        })?;
        boundary.send_text_control_frame(TextControlFrame {
            session_id: turn_session_id.clone(),
            kind: TextControlFrameKind::Control,
            payload: OpaqueBytes(b"ciphertext:control-frame".to_vec()),
        })?;
        assert_eq!(
            boundary
                .poll_text_control_frames(&turn_session_id)?
                .iter()
                .map(|frame| frame.kind)
                .collect::<Vec<_>>(),
            vec![TextControlFrameKind::Text, TextControlFrameKind::Control]
        );
        assert!(matches!(
            boundary.send_text_control_frame(TextControlFrame {
                session_id: SessionId("not-open".to_owned()),
                kind: TextControlFrameKind::Control,
                payload: OpaqueBytes(b"ciphertext:closed".to_vec()),
            }),
            Err(ServiceBoundaryError::AdapterUnavailable(_))
        ));

        let media = boundary.join_media(MediaSessionRequest {
            group_id: group.group_id.clone(),
            channel_id: ChannelId("voice".to_owned()),
            participant: UserId("alice".to_owned()),
        })?;
        assert!(media.joined);
        boundary.send_media_frame(&media.session_id, OpaqueBytes(vec![9]))?;

        boundary.save_record(StoreRecord {
            key: "snapshot".to_owned(),
            value: OpaqueBytes(vec![1]),
        })?;
        assert!(boundary.load_record("snapshot")?.is_some());
        let secret_name = SecretName("identity-key".to_owned());
        boundary.seal_secret(secret_name.clone(), OpaqueBytes(vec![3]))?;
        assert_eq!(
            boundary.open_secret(&secret_name)?,
            Some(OpaqueBytes(vec![3]))
        );

        let topic = EventTopic("commands".to_owned());
        boundary.publish_event(topic.clone(), OpaqueBytes(vec![4]))?;
        assert_eq!(boundary.drain_events(&topic, None)?.len(), 1);
        assert_eq!(boundary.command_snapshot()?.schema_version, 2);
        Ok(())
    }
}

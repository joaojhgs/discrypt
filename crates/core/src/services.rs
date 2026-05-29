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

/// Keychain slot name for sealed local-only secrets.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct SecretName(pub String);

/// Event-bus topic name.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct EventTopic(pub String);

/// Opaque bytes passed across implementation seams.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpaqueBytes(pub Vec<u8>);

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
    /// Ordered encrypted/plaintext-facade message ids for the current adapter.
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

/// Group crypto boundary for MLS/OpenMLS state, epochs, commits, and exporters.
pub trait GroupCryptoService {
    /// Create a group crypto state shell.
    fn create_group_crypto(
        &mut self,
        request: GroupCryptoRequest,
    ) -> ServiceResult<GroupCryptoState>;

    /// Apply an opaque, already-authenticated group commit.
    fn apply_group_commit(&mut self, commit: GroupCommit) -> ServiceResult<GroupCryptoState>;

    /// Export opaque secret material for a named subsystem under the current epoch.
    fn export_group_secret(&self, group_id: &GroupId, label: &str) -> ServiceResult<OpaqueBytes>;
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
    /// Seal a secret into a named local-only keychain slot.
    fn seal_secret(&mut self, name: SecretName, plaintext: OpaqueBytes) -> ServiceResult<()>;

    /// Open a named local-only keychain slot.
    fn open_secret(&self, name: &SecretName) -> ServiceResult<Option<OpaqueBytes>>;

    /// Delete a named local-only keychain slot.
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
    + AppStateStoreService
    + KeychainService
    + EventBusService
{
    /// Return the current command-facing snapshot assembled from service state.
    fn command_snapshot(&self) -> ServiceResult<AppSnapshot>;
}

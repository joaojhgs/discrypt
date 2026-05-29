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

/// Keychain slot name for sealed local-only secrets.
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

/// Group crypto boundary for MLS/OpenMLS state, epochs, and commits.
pub trait GroupCryptoService {
    /// Create a group crypto state shell.
    fn create_group_crypto(
        &mut self,
        request: GroupCryptoRequest,
    ) -> ServiceResult<GroupCryptoState>;

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
                events: Vec::new(),
                next_event_sequence: 0,
            }
        }
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
            _session_id: &SessionId,
            leg: TransportLeg,
        ) -> ServiceResult<()> {
            if leg.ciphertext_only {
                Ok(())
            } else {
                Err(ServiceBoundaryError::VerificationFailed(
                    "transport leg must be ciphertext-only".to_owned(),
                ))
            }
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

        let text_secret = boundary.export_rust_service_secret(
            &group.group_id,
            RustExporterSecretService::Text,
            b"history",
        )?;
        let media_secret = boundary.export_rust_service_secret(
            &group.group_id,
            RustExporterSecretService::Media,
            b"voice",
        )?;
        let content_key_secret = boundary.export_rust_service_secret(
            &group.group_id,
            RustExporterSecretService::ContentKey,
            b"message",
        )?;
        assert_ne!(text_secret.as_bytes(), media_secret.as_bytes());
        assert_ne!(text_secret.as_bytes(), content_key_secret.as_bytes());
        assert!(!format!("{text_secret:?}").contains("lab"));

        let object: &mut dyn AppServiceBoundary = &mut boundary;
        assert_eq!(
            object.role_for_user(&group.group_id, &UserId("alice".to_owned()))?,
            "owner"
        );
        assert!(
            object
                .create_invite(InviteRequest {
                    group_id: group.group_id.clone(),
                    creator: UserId("alice".to_owned()),
                    password_gate: Some("online helper".to_owned()),
                    max_uses: 1,
                })?
                .welcome_required
        );
        assert_eq!(
            object
                .send_text(SendMessageRequest {
                    channel: "#general".to_owned(),
                    body: "hello".to_owned(),
                })?
                .0,
            "#general:5"
        );

        let route = object.select_overlay_route(&group.group_id)?;
        assert!(route.ciphertext_only);
        object.forward_ciphertext(&route, OpaqueBytes(vec![1, 2, 3]))?;

        let session_id = SessionId("session-1".to_owned());
        object.publish_signal(SignalEnvelope {
            session_id: session_id.clone(),
            sender: DeviceId("alice-laptop".to_owned()),
            payload: OpaqueBytes(vec![7]),
        })?;
        assert_eq!(object.poll_signals(&session_id)?.len(), 1);
        let leg = object.plan_transport(&group.group_id)?.remove(0);
        object.open_transport(&session_id, leg)?;

        let media = object.join_media(MediaSessionRequest {
            group_id: group.group_id.clone(),
            channel_id: ChannelId("voice".to_owned()),
            participant: UserId("alice".to_owned()),
        })?;
        assert!(media.joined);
        object.send_media_frame(&media.session_id, OpaqueBytes(vec![9]))?;

        object.save_record(StoreRecord {
            key: "snapshot".to_owned(),
            value: OpaqueBytes(vec![1]),
        })?;
        assert!(object.load_record("snapshot")?.is_some());
        let secret_name = SecretName("identity-key".to_owned());
        object.seal_secret(secret_name.clone(), OpaqueBytes(vec![3]))?;
        assert_eq!(
            object.open_secret(&secret_name)?,
            Some(OpaqueBytes(vec![3]))
        );

        let topic = EventTopic("commands".to_owned());
        object.publish_event(topic.clone(), OpaqueBytes(vec![4]))?;
        assert_eq!(object.drain_events(&topic, None)?.len(), 1);
        assert_eq!(object.command_snapshot()?.schema_version, 2);
        Ok(())
    }
}

//! Content-key retention, live-key, and shred primitives.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use mls_core::{derive_epoch_secret, DeviceStatus, ExportLabel, GovernanceState, GroupState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Retention window presets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RetentionWindow {
    /// One hour.
    Hours1,
    /// Twenty-four hours.
    Hours24,
    /// Seven days, the default.
    Days7,
    /// Thirty days.
    Days30,
    /// Ninety days.
    Days90,
    /// Custom window in seconds.
    CustomSeconds(u64),
    /// Explicit warned never-lock opt-in.
    UnlimitedWarned,
}

impl RetentionWindow {
    /// Window length in seconds, or `None` for warned unlimited.
    #[must_use]
    pub fn seconds(self) -> Option<u64> {
        match self {
            Self::Hours1 => Some(3600),
            Self::Hours24 => Some(86400),
            Self::Days7 => Some(604800),
            Self::Days30 => Some(2592000),
            Self::Days90 => Some(7776000),
            Self::CustomSeconds(s) => Some(s),
            Self::UnlimitedWarned => None,
        }
    }
}

/// Cached, locked, decoy, rate-limited, or shredded message-key state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum KeyState {
    /// Locally cached content key.
    Cached([u8; 32]),
    /// Lock-not-vanish placeholder; key may require an authorized live-key request.
    Locked,
    /// Cooperative shred tombstone exists.
    Shredded,
    /// Deliberate decoy response for unauthorized archival-key requests.
    Decoy([u8; 32]),
    /// Rate limit consumed without revealing author liveness/decryptability.
    RateLimited,
    /// Uniform live-key failure response that does not reveal auth vs reachability.
    Unavailable,
}

/// Deterministic content-key derivation for tests/boundary.
///
/// Content keys are derived through the MLS exporter boundary using the
/// content-key service label. Raw exporter bytes stay inside Rust-owned
/// content-key logic rather than crossing command/UI boundaries.
#[must_use]
pub fn derive_content_key(author: u32, message_id: &str, epoch_secret: &[u8]) -> [u8; 32] {
    let mut context = Vec::with_capacity(12 + message_id.len());
    context.extend_from_slice(&author.to_be_bytes());
    context.extend_from_slice(&(message_id.len() as u64).to_be_bytes());
    context.extend_from_slice(message_id.as_bytes());
    derive_epoch_secret(epoch_secret, ExportLabel::ContentKey, &context)
}

/// Apply retention to message timestamp.
#[must_use]
pub fn key_state(
    now: DateTime<Utc>,
    sent_at: DateTime<Utc>,
    window: RetentionWindow,
    key: [u8; 32],
    tombstoned: bool,
) -> KeyState {
    if tombstoned {
        return KeyState::Shredded;
    }
    match window.seconds() {
        None => KeyState::Cached(key),
        Some(s) if now.signed_duration_since(sent_at) <= Duration::seconds(s as i64) => {
            KeyState::Cached(key)
        }
        Some(_) => KeyState::Locked,
    }
}

/// Retention policy transition semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetentionTransition {
    /// Previous window.
    pub old_window: RetentionWindow,
    /// New window.
    pub new_window: RetentionWindow,
    /// Transition timestamp.
    pub changed_at: DateTime<Utc>,
}

impl RetentionTransition {
    /// Apply shorten-retroactive / lengthen-future semantics for one message.
    #[must_use]
    pub fn state_for_message(
        self,
        now: DateTime<Utc>,
        sent_at: DateTime<Utc>,
        key: [u8; 32],
        tombstoned: bool,
    ) -> KeyState {
        if tombstoned {
            return KeyState::Shredded;
        }
        if is_shorter(self.new_window, self.old_window) {
            return key_state(now, sent_at, self.new_window, key, false);
        }
        if sent_at < self.changed_at {
            key_state(now, sent_at, self.old_window, key, false)
        } else {
            key_state(now, sent_at, self.new_window, key, false)
        }
    }
}

fn is_shorter(new_window: RetentionWindow, old_window: RetentionWindow) -> bool {
    match (new_window.seconds(), old_window.seconds()) {
        (Some(new), Some(old)) => new < old,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// Tombstone set.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Tombstones {
    ids: BTreeSet<String>,
}

impl Tombstones {
    /// Add a shred tombstone.
    pub fn shred(&mut self, id: impl Into<String>) {
        self.ids.insert(id.into());
    }

    /// True when a message has a tombstone.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    /// Ordered tombstone ids.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        self.ids.iter().cloned().collect()
    }
}

/// Per-device shred sync status.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeviceShredStatus {
    /// Device id.
    pub device_id: String,
    /// Whether this own device is currently online/synced.
    pub online: bool,
    /// Tombstones seen by this device.
    pub seen_tombstones: Tombstones,
}

/// Cross-device cooperative shred propagation state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CrossDeviceShredState {
    global_tombstones: Tombstones,
    devices: BTreeMap<String, DeviceShredStatus>,
}

impl CrossDeviceShredState {
    /// Register an own device.
    pub fn register_device(&mut self, device_id: impl Into<String>, online: bool) {
        let device_id = device_id.into();
        self.devices.insert(
            device_id.clone(),
            DeviceShredStatus {
                device_id,
                online,
                seen_tombstones: Tombstones::default(),
            },
        );
    }

    /// Author shreds a message and immediately syncs online own devices.
    pub fn shred(&mut self, message_id: impl Into<String>) {
        let message_id = message_id.into();
        self.global_tombstones.shred(message_id.clone());
        for device in self.devices.values_mut().filter(|device| device.online) {
            device.seen_tombstones.shred(message_id.clone());
        }
    }

    /// Mark a device online/offline; online devices sync current tombstones.
    pub fn set_online(&mut self, device_id: &str, online: bool) {
        if let Some(device) = self.devices.get_mut(device_id) {
            device.online = online;
            if online {
                for id in self.global_tombstones.ids() {
                    device.seen_tombstones.shred(id);
                }
            }
        }
    }

    /// True when this own device is still pending a tombstone sync.
    #[must_use]
    pub fn pending_on_device(&self, device_id: &str, message_id: &str) -> bool {
        self.devices.get(device_id).is_some_and(|device| {
            self.global_tombstones.contains(message_id)
                && !device.seen_tombstones.contains(message_id)
        })
    }

    /// A device may serve only if it has not seen a tombstone for the message.
    #[must_use]
    pub fn device_may_serve(&self, device_id: &str, message_id: &str) -> bool {
        self.devices
            .get(device_id)
            .is_some_and(|device| !device.seen_tombstones.contains(message_id))
    }
}

const LIVE_KEY_MEMBERSHIP_PROOF_DOMAIN: &[u8] = b"discrypt-live-key-membership-proof-v1";

/// Verification errors for signed live-key membership proofs.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum MembershipProofError {
    /// Proof verifier key is not a valid Ed25519 public key.
    #[error("membership proof verifier key is invalid")]
    InvalidVerifierKey,
    /// Proof signature bytes are not a valid Ed25519 signature.
    #[error("membership proof signature bytes are invalid")]
    InvalidSignatureBytes,
    /// Proof signature verification failed.
    #[error("membership proof signature verification failed")]
    InvalidSignature,
    /// Proof was signed for a different epoch group commitment.
    #[error("membership proof group commitment mismatch")]
    GroupCommitmentMismatch,
}

/// Signed membership proof for archival live-key requests.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct MembershipProof {
    /// Requesting leaf.
    pub requester_leaf: u32,
    /// Epoch being proven.
    pub epoch: u64,
    /// Stable local room/group identifier included in the signed transcript.
    pub group_id: String,
    /// Canonical local group-state commitment for the claimed epoch.
    pub group_commitment: [u8; 32],
    /// Ed25519 device public key that signed this proof.
    pub device_public_key: [u8; 32],
    /// Ed25519 signature over the canonical live-key membership transcript.
    pub signature: Vec<u8>,
}

impl MembershipProof {
    /// Sign a live-key membership proof for one leaf at one group epoch.
    #[must_use]
    pub fn sign(
        requester_leaf: u32,
        epoch: u64,
        group_id: impl Into<String>,
        group_commitment: [u8; 32],
        signing_key: &SigningKey,
    ) -> Self {
        let group_id = group_id.into();
        let device_public_key = signing_key.verifying_key().to_bytes();
        let transcript = membership_proof_transcript(
            requester_leaf,
            epoch,
            &group_id,
            &group_commitment,
            &device_public_key,
        );
        let signature = signing_key.sign(&transcript).to_bytes().to_vec();
        Self {
            requester_leaf,
            epoch,
            group_id,
            group_commitment,
            device_public_key,
            signature,
        }
    }

    /// Verify the proof signature and bind it to the expected epoch commitment.
    pub fn verify_signature(
        &self,
        expected_group_commitment: &[u8; 32],
    ) -> Result<(), MembershipProofError> {
        if &self.group_commitment != expected_group_commitment {
            return Err(MembershipProofError::GroupCommitmentMismatch);
        }
        let verifier = VerifyingKey::from_bytes(&self.device_public_key)
            .map_err(|_| MembershipProofError::InvalidVerifierKey)?;
        let signature = Signature::from_slice(&self.signature)
            .map_err(|_| MembershipProofError::InvalidSignatureBytes)?;
        let transcript = membership_proof_transcript(
            self.requester_leaf,
            self.epoch,
            &self.group_id,
            &self.group_commitment,
            &self.device_public_key,
        );
        verifier
            .verify(&transcript, &signature)
            .map_err(|_| MembershipProofError::InvalidSignature)
    }
}

fn membership_proof_transcript(
    requester_leaf: u32,
    epoch: u64,
    group_id: &str,
    group_commitment: &[u8; 32],
    device_public_key: &[u8; 32],
) -> Vec<u8> {
    let mut transcript = Vec::with_capacity(
        LIVE_KEY_MEMBERSHIP_PROOF_DOMAIN.len() + 4 + 8 + 8 + group_id.len() + 32 + 32,
    );
    transcript.extend_from_slice(LIVE_KEY_MEMBERSHIP_PROOF_DOMAIN);
    transcript.extend_from_slice(&requester_leaf.to_be_bytes());
    transcript.extend_from_slice(&epoch.to_be_bytes());
    transcript.extend_from_slice(&(group_id.len() as u64).to_be_bytes());
    transcript.extend_from_slice(group_id.as_bytes());
    transcript.extend_from_slice(group_commitment);
    transcript.extend_from_slice(device_public_key);
    transcript
}

fn derive_membership_group_commitment(epoch: u64, members: &BTreeSet<u32>) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"discrypt-live-key-epoch-group-commitment-v1");
    h.update(epoch.to_be_bytes());
    h.update((members.len() as u64).to_be_bytes());
    for member in members {
        h.update(member.to_be_bytes());
    }
    h.finalize().into()
}

/// Live-key oracle response with explicit authorization flag.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LiveKeyResponse {
    /// Returned state, real key/decoy/rate-limited/unavailable.
    pub state: KeyState,
    /// True only for locally authorized members under the limit.
    pub authorized: bool,
}

/// Failure-shaping policy for live-key requests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum LiveKeyFailureResponseMode {
    /// Unauthorized callers receive a decoy key; rate limits remain explicit.
    #[default]
    DecoyKey,
    /// Unauthorized, invalid, over-limit, and generic transport failures all render
    /// the same unavailable response.
    UniformUnavailable,
}

/// Errors while building live-key authorization from local MLS/governance state.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LocalMembershipStateError {
    /// The MLS group and governance state disagree about the current accepted epoch.
    #[error("local MLS epoch {group_epoch} does not match governance epoch {governance_epoch}")]
    EpochMismatch {
        /// Current MLS group epoch.
        group_epoch: u64,
        /// Current governance epoch.
        governance_epoch: u64,
    },
    /// A local MLS member carries an invalid device verifier key.
    #[error("local member leaf {leaf} has invalid device verifier key")]
    InvalidDeviceKey {
        /// Leaf with invalid verifier bytes.
        leaf: u32,
    },
}

/// Optional dimensions for live-key request rate limiting.
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct LiveKeyRequestScope {
    /// Author/content-key owner being requested, when known.
    pub author_leaf: Option<u32>,
    /// Domain-separated hash of network identity, when available.
    pub network_identity_hash: Option<[u8; 32]>,
}

impl LiveKeyRequestScope {
    /// Scope without author/network dimensions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Scope one request to an author/content-key owner.
    #[must_use]
    pub fn for_author(author_leaf: u32) -> Self {
        Self {
            author_leaf: Some(author_leaf),
            network_identity_hash: None,
        }
    }

    /// Attach a hashed network identity, such as peer socket, relay identity, or
    /// authenticated transport token. Raw identity text is not persisted.
    #[must_use]
    pub fn with_network_identity(mut self, network_identity: impl AsRef<[u8]>) -> Self {
        let mut h = Sha256::new();
        h.update(b"discrypt-live-key-rate-limit-network-identity-v1");
        h.update(network_identity.as_ref());
        self.network_identity_hash = Some(h.finalize().into());
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
struct LiveKeyRateLimitKey {
    requester_leaf: u32,
    epoch: u64,
    author_leaf: Option<u32>,
    network_identity_hash: Option<[u8; 32]>,
}

impl LiveKeyRateLimitKey {
    fn from_proof_scope(proof: &MembershipProof, scope: &LiveKeyRequestScope) -> Self {
        Self {
            requester_leaf: proof.requester_leaf,
            epoch: proof.epoch,
            author_leaf: scope.author_leaf,
            network_identity_hash: scope.network_identity_hash,
        }
    }
}

/// Membership-gated, rate-limited, decoy-capable live-key oracle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LiveKeyOracle {
    members_by_epoch: BTreeMap<u64, BTreeSet<u32>>,
    group_commitments_by_epoch: BTreeMap<u64, [u8; 32]>,
    authorized_device_keys: BTreeMap<(u64, u32), BTreeSet<[u8; 32]>>,
    requests_by_rate_key: BTreeMap<LiveKeyRateLimitKey, usize>,
    max_requests: usize,
    decoy_key: [u8; 32],
    failure_response_mode: LiveKeyFailureResponseMode,
}

impl LiveKeyOracle {
    /// Create an oracle from repaired local MLS group state plus resolved governance state.
    ///
    /// This is intentionally local-device: membership is derived from the current local
    /// OpenMLS/group view intersected with the resolved governance roles and bans. It
    /// registers verifier keys from active MLS device leaves and performs no online
    /// lookup that could leak live presence.
    pub fn from_local_mls_governance_state(
        group: &GroupState,
        governance: &GovernanceState,
        max_requests: usize,
    ) -> Result<Self, LocalMembershipStateError> {
        if group.epoch != governance.epoch {
            return Err(LocalMembershipStateError::EpochMismatch {
                group_epoch: group.epoch,
                governance_epoch: governance.epoch,
            });
        }
        let active_members = group
            .members()
            .iter()
            .filter_map(|(leaf, member)| {
                (member.status == DeviceStatus::Active
                    && governance.role(*leaf).is_some()
                    && !governance.is_banned(*leaf))
                .then_some(*leaf)
            })
            .collect::<BTreeSet<_>>();
        let mut oracle = Self::new(
            BTreeMap::from([(group.epoch, active_members)]),
            max_requests,
        );
        for (leaf, member) in group.members() {
            if !oracle
                .members_by_epoch
                .get(&group.epoch)
                .is_some_and(|members| members.contains(leaf))
            {
                continue;
            }
            let verifier = VerifyingKey::from_bytes(&member.device_key)
                .map_err(|_| LocalMembershipStateError::InvalidDeviceKey { leaf: *leaf })?;
            oracle.authorize_member_device(group.epoch, *leaf, &verifier);
        }
        Ok(oracle)
    }

    /// Create an oracle from epoch membership.
    #[must_use]
    pub fn new(members_by_epoch: BTreeMap<u64, BTreeSet<u32>>, max_requests: usize) -> Self {
        let group_commitments_by_epoch = members_by_epoch
            .iter()
            .map(|(epoch, members)| (*epoch, derive_membership_group_commitment(*epoch, members)))
            .collect();
        Self {
            members_by_epoch,
            group_commitments_by_epoch,
            authorized_device_keys: BTreeMap::new(),
            requests_by_rate_key: BTreeMap::new(),
            max_requests: max_requests.max(1),
            decoy_key: [0xD; 32],
            failure_response_mode: LiveKeyFailureResponseMode::DecoyKey,
        }
    }

    /// Return the canonical local group-state commitment for an epoch.
    #[must_use]
    pub fn epoch_group_commitment(&self, epoch: u64) -> Option<[u8; 32]> {
        self.group_commitments_by_epoch.get(&epoch).copied()
    }

    /// Authorize a device signer for a member leaf at an epoch.
    ///
    /// Returns false if the leaf is not a local member for that epoch.
    pub fn authorize_member_device(
        &mut self,
        epoch: u64,
        requester_leaf: u32,
        verifier: &VerifyingKey,
    ) -> bool {
        let member = self
            .members_by_epoch
            .get(&epoch)
            .is_some_and(|members| members.contains(&requester_leaf));
        if !member {
            return false;
        }
        self.authorized_device_keys
            .entry((epoch, requester_leaf))
            .or_default()
            .insert(verifier.to_bytes());
        true
    }

    /// Configure how unauthorized/failed live-key requests are shaped.
    #[must_use]
    pub fn with_failure_response_mode(mut self, mode: LiveKeyFailureResponseMode) -> Self {
        self.failure_response_mode = mode;
        self
    }

    /// Return the generic failure response for this oracle's failure-shaping mode.
    #[must_use]
    pub fn generic_failure_response(&self) -> LiveKeyResponse {
        self.failure_response(false)
    }

    /// Request an archival key using the default requester+epoch rate scope.
    pub fn request_key(&mut self, proof: &MembershipProof, key: [u8; 32]) -> LiveKeyResponse {
        self.request_key_scoped(proof, &LiveKeyRequestScope::new(), key)
    }

    /// Request an archival key using explicit author/network rate-limit dimensions.
    pub fn request_key_for_author(
        &mut self,
        proof: &MembershipProof,
        author_leaf: u32,
        network_identity: Option<&str>,
        key: [u8; 32],
    ) -> LiveKeyResponse {
        let mut scope = LiveKeyRequestScope::for_author(author_leaf);
        if let Some(network_identity) = network_identity {
            scope = scope.with_network_identity(network_identity);
        }
        self.request_key_scoped(proof, &scope, key)
    }

    /// Request an archival key. Non-members and invalid proofs receive decoys;
    /// authorized members are rate-limited by requester, epoch, author, and
    /// network identity when those dimensions are available.
    pub fn request_key_scoped(
        &mut self,
        proof: &MembershipProof,
        scope: &LiveKeyRequestScope,
        key: [u8; 32],
    ) -> LiveKeyResponse {
        if !self.proof_authorized(proof) {
            return self.failure_response(false);
        }
        let rate_key = LiveKeyRateLimitKey::from_proof_scope(proof, scope);
        let counter = self.requests_by_rate_key.entry(rate_key).or_default();
        *counter = counter.saturating_add(1);
        if *counter > self.max_requests {
            return self.failure_response(true);
        }
        LiveKeyResponse {
            state: KeyState::Cached(key),
            authorized: true,
        }
    }

    fn failure_response(&self, rate_limited: bool) -> LiveKeyResponse {
        match self.failure_response_mode {
            LiveKeyFailureResponseMode::DecoyKey if rate_limited => LiveKeyResponse {
                state: KeyState::RateLimited,
                authorized: false,
            },
            LiveKeyFailureResponseMode::DecoyKey => LiveKeyResponse {
                state: KeyState::Decoy(self.decoy_key),
                authorized: false,
            },
            LiveKeyFailureResponseMode::UniformUnavailable => LiveKeyResponse {
                state: KeyState::Unavailable,
                authorized: false,
            },
        }
    }

    fn proof_authorized(&self, proof: &MembershipProof) -> bool {
        let Some(members) = self.members_by_epoch.get(&proof.epoch) else {
            return false;
        };
        if !members.contains(&proof.requester_leaf) {
            return false;
        }
        let Some(expected_commitment) = self.group_commitments_by_epoch.get(&proof.epoch) else {
            return false;
        };
        if proof.verify_signature(expected_commitment).is_err() {
            return false;
        }
        self.authorized_device_keys
            .get(&(proof.epoch, proof.requester_leaf))
            .is_some_and(|keys| keys.contains(&proof.device_public_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_locks_old_messages_and_lengthen_is_future_only() {
        let now = Utc::now();
        let key = [3; 32];
        assert_eq!(
            key_state(
                now,
                now - Duration::hours(2),
                RetentionWindow::Hours1,
                key,
                false
            ),
            KeyState::Locked
        );
        assert!(matches!(
            key_state(
                now,
                now - Duration::hours(2),
                RetentionWindow::UnlimitedWarned,
                key,
                false
            ),
            KeyState::Cached(_)
        ));
        let transition = RetentionTransition {
            old_window: RetentionWindow::Hours1,
            new_window: RetentionWindow::Days7,
            changed_at: now,
        };
        assert_eq!(
            transition.state_for_message(now, now - Duration::hours(2), key, false),
            KeyState::Locked
        );
        assert!(matches!(
            transition.state_for_message(now, now + Duration::seconds(1), key, false),
            KeyState::Cached(_)
        ));
    }

    #[test]
    fn cross_device_shred_syncs_online_devices_and_blocks_serving_after_reconnect() {
        let mut shred = CrossDeviceShredState::default();
        shred.register_device("laptop", true);
        shred.register_device("phone", false);
        shred.shred("m1");
        assert!(!shred.device_may_serve("laptop", "m1"));
        assert!(shred.pending_on_device("phone", "m1"));
        assert!(shred.device_may_serve("phone", "m1"));
        shred.set_online("phone", true);
        assert!(!shred.pending_on_device("phone", "m1"));
        assert!(!shred.device_may_serve("phone", "m1"));
    }

    #[test]
    fn live_key_oracle_gates_membership_and_rate_limits_with_decoys(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut members = BTreeMap::new();
        members.insert(7, BTreeSet::from([1, 2]));
        let mut oracle = LiveKeyOracle::new(members, 1);
        let signing_key = SigningKey::from_bytes(&[1; 32]);
        assert!(oracle.authorize_member_device(7, 1, &signing_key.verifying_key()));
        let commitment = oracle
            .epoch_group_commitment(7)
            .ok_or_else(|| std::io::Error::other("epoch commitment missing"))?;
        let proof = MembershipProof::sign(1, 7, "room", commitment, &signing_key);
        let key = [9; 32];
        let allowed = oracle.request_key(&proof, key);
        assert_eq!(allowed.state, KeyState::Cached(key));
        assert!(allowed.authorized);
        let limited = oracle.request_key(&proof, key);
        assert_eq!(limited.state, KeyState::RateLimited);
        assert!(!limited.authorized);
        let non_member = SigningKey::from_bytes(&[9; 32]);
        let decoy = oracle.request_key(
            &MembershipProof::sign(9, 7, "room", commitment, &non_member),
            key,
        );
        assert!(matches!(decoy.state, KeyState::Decoy(_)));
        assert!(!decoy.authorized);
        Ok(())
    }

    #[test]
    fn live_key_oracle_requires_signed_epoch_membership_proof(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut members = BTreeMap::new();
        members.insert(11, BTreeSet::from([4]));
        let mut oracle = LiveKeyOracle::new(members, 3);
        let member = SigningKey::from_bytes(&[4; 32]);
        let other_device = SigningKey::from_bytes(&[5; 32]);
        assert!(oracle.authorize_member_device(11, 4, &member.verifying_key()));
        assert!(!oracle.authorize_member_device(11, 99, &other_device.verifying_key()));
        let commitment = oracle
            .epoch_group_commitment(11)
            .ok_or_else(|| std::io::Error::other("epoch commitment missing"))?;
        let proof = MembershipProof::sign(4, 11, "room-alpha", commitment, &member);
        assert!(proof.verify_signature(&commitment).is_ok());
        let key = [8; 32];
        let allowed = oracle.request_key(&proof, key);
        assert_eq!(allowed.state, KeyState::Cached(key));
        assert!(allowed.authorized);

        let mut tampered_group = proof.clone();
        tampered_group.group_commitment = [0xA; 32];
        assert_eq!(
            tampered_group.verify_signature(&commitment),
            Err(MembershipProofError::GroupCommitmentMismatch)
        );
        let rejected = oracle.request_key(&tampered_group, key);
        assert!(matches!(rejected.state, KeyState::Decoy(_)));
        assert!(!rejected.authorized);

        let unregistered = MembershipProof::sign(4, 11, "room-alpha", commitment, &other_device);
        let rejected = oracle.request_key(&unregistered, key);
        assert!(matches!(rejected.state, KeyState::Decoy(_)));
        assert!(!rejected.authorized);

        let mut tampered_signature = proof;
        tampered_signature.signature[0] ^= 0x80;
        assert_eq!(
            tampered_signature.verify_signature(&commitment),
            Err(MembershipProofError::InvalidSignature)
        );
        let rejected = oracle.request_key(&tampered_signature, key);
        assert!(matches!(rejected.state, KeyState::Decoy(_)));
        assert!(!rejected.authorized);
        Ok(())
    }

    #[test]
    fn live_key_oracle_can_shape_failures_as_uniform_unavailable(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut members = BTreeMap::new();
        members.insert(23, BTreeSet::from([1]));
        let signer = SigningKey::from_bytes(&[0x23; 32]);
        let mut oracle = LiveKeyOracle::new(members, 1)
            .with_failure_response_mode(LiveKeyFailureResponseMode::UniformUnavailable);
        assert!(oracle.authorize_member_device(23, 1, &signer.verifying_key()));
        let commitment = oracle
            .epoch_group_commitment(23)
            .ok_or_else(|| std::io::Error::other("epoch commitment missing"))?;
        let key = [0x23; 32];
        let proof = MembershipProof::sign(1, 23, "room-uniform", commitment, &signer);
        let allowed = oracle.request_key(&proof, key);
        assert_eq!(allowed.state, KeyState::Cached(key));
        assert!(allowed.authorized);

        let over_limit = oracle.request_key(&proof, key);
        let non_member = MembershipProof::sign(
            99,
            23,
            "room-uniform",
            commitment,
            &SigningKey::from_bytes(&[0x24; 32]),
        );
        let non_member_response = oracle.request_key(&non_member, key);
        let mut invalid_signature = proof;
        invalid_signature.signature[0] ^= 0x01;
        let invalid_signature_response = oracle.request_key(&invalid_signature, key);
        let generic_failure = oracle.generic_failure_response();
        for response in [
            over_limit,
            non_member_response,
            invalid_signature_response,
            generic_failure,
        ] {
            assert_eq!(response.state, KeyState::Unavailable);
            assert!(!response.authorized);
        }
        Ok(())
    }

    #[test]
    fn malicious_non_member_live_key_probes_are_uniform_and_non_decryptable(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut members = BTreeMap::new();
        members.insert(31, BTreeSet::from([1]));
        let member_signer = SigningKey::from_bytes(&[0x31; 32]);
        let attacker_signer = SigningKey::from_bytes(&[0x32; 32]);
        let mut oracle = LiveKeyOracle::new(members, 1)
            .with_failure_response_mode(LiveKeyFailureResponseMode::UniformUnavailable);
        assert!(oracle.authorize_member_device(31, 1, &member_signer.verifying_key()));
        let commitment = oracle
            .epoch_group_commitment(31)
            .ok_or_else(|| std::io::Error::other("epoch commitment missing"))?;
        let protected_key = [0x91; 32];
        let legitimate = MembershipProof::sign(1, 31, "room-probes", commitment, &member_signer);
        assert!(oracle.request_key(&legitimate, protected_key).authorized);

        let non_member = MembershipProof::sign(99, 31, "room-probes", commitment, &attacker_signer);
        let unregistered_device =
            MembershipProof::sign(1, 31, "room-probes", commitment, &attacker_signer);
        let stale_epoch = MembershipProof::sign(1, 30, "room-probes", commitment, &member_signer);
        let mut invalid_signature = legitimate;
        invalid_signature.signature[0] ^= 0x40;

        let responses = [
            oracle.request_key_for_author(&non_member, 1, Some("attacker-net"), protected_key),
            oracle.request_key_for_author(
                &unregistered_device,
                1,
                Some("attacker-net"),
                protected_key,
            ),
            oracle.request_key_for_author(&stale_epoch, 1, Some("attacker-net"), protected_key),
            oracle.request_key_for_author(
                &invalid_signature,
                1,
                Some("attacker-net"),
                protected_key,
            ),
            oracle.generic_failure_response(),
        ];
        for response in responses {
            assert_eq!(response.state, KeyState::Unavailable);
            assert!(!response.authorized);
            assert_ne!(response.state, KeyState::Cached(protected_key));
        }
        Ok(())
    }

    #[test]
    fn live_key_oracle_rate_limits_by_requester_epoch_author_and_network(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut members = BTreeMap::new();
        members.insert(17, BTreeSet::from([1, 2]));
        let mut oracle = LiveKeyOracle::new(members, 1);
        let requester_one = SigningKey::from_bytes(&[0x71; 32]);
        let requester_two = SigningKey::from_bytes(&[0x72; 32]);
        assert!(oracle.authorize_member_device(17, 1, &requester_one.verifying_key()));
        assert!(oracle.authorize_member_device(17, 2, &requester_two.verifying_key()));
        let commitment = oracle
            .epoch_group_commitment(17)
            .ok_or_else(|| std::io::Error::other("epoch commitment missing"))?;
        let proof_one = MembershipProof::sign(1, 17, "room-rates", commitment, &requester_one);
        let proof_two = MembershipProof::sign(2, 17, "room-rates", commitment, &requester_two);
        let key = [0x88; 32];

        let author_a_network_a =
            LiveKeyRequestScope::for_author(41).with_network_identity("relay-a");
        let first = oracle.request_key_scoped(&proof_one, &author_a_network_a, key);
        assert_eq!(first.state, KeyState::Cached(key));
        assert!(first.authorized);
        let limited = oracle.request_key_scoped(&proof_one, &author_a_network_a, key);
        assert_eq!(limited.state, KeyState::RateLimited);
        assert!(!limited.authorized);

        let author_b_same_network =
            LiveKeyRequestScope::for_author(42).with_network_identity("relay-a");
        let separate_author = oracle.request_key_scoped(&proof_one, &author_b_same_network, key);
        assert_eq!(separate_author.state, KeyState::Cached(key));
        assert!(separate_author.authorized);

        let author_a_network_b =
            LiveKeyRequestScope::for_author(41).with_network_identity("relay-b");
        let separate_network = oracle.request_key_scoped(&proof_one, &author_a_network_b, key);
        assert_eq!(separate_network.state, KeyState::Cached(key));
        assert!(separate_network.authorized);

        let separate_requester = oracle.request_key_scoped(&proof_two, &author_a_network_a, key);
        assert_eq!(separate_requester.state, KeyState::Cached(key));
        assert!(separate_requester.authorized);

        let convenience_limited =
            oracle.request_key_for_author(&proof_one, 41, Some("relay-a"), key);
        assert_eq!(convenience_limited.state, KeyState::RateLimited);
        assert!(!convenience_limited.authorized);
        Ok(())
    }

    #[test]
    fn live_key_oracle_builds_from_local_repaired_mls_governance_state(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use mls_core::{DeviceLeaf, GovernanceAction, GovernanceEvent, Role};
        use uuid::Uuid;

        let owner_signer = SigningKey::from_bytes(&[0x11; 32]);
        let member_signer = SigningKey::from_bytes(&[0x22; 32]);
        let removed_signer = SigningKey::from_bytes(&[0x33; 32]);
        let owner = DeviceLeaf {
            device_id: Uuid::from_u128(1),
            leaf_index: 1,
            identity_key: [0xA1; 32],
            device_key: owner_signer.verifying_key().to_bytes(),
            label: "owner".to_owned(),
            status: DeviceStatus::Active,
            added_at_epoch: 0,
            removed_at_epoch: None,
        };
        let member = DeviceLeaf {
            device_id: Uuid::from_u128(2),
            leaf_index: 2,
            identity_key: [0xB2; 32],
            device_key: member_signer.verifying_key().to_bytes(),
            label: "member".to_owned(),
            status: DeviceStatus::Active,
            added_at_epoch: 0,
            removed_at_epoch: None,
        };
        let removed = DeviceLeaf {
            device_id: Uuid::from_u128(3),
            leaf_index: 3,
            identity_key: [0xC3; 32],
            device_key: removed_signer.verifying_key().to_bytes(),
            label: "removed".to_owned(),
            status: DeviceStatus::Active,
            added_at_epoch: 0,
            removed_at_epoch: None,
        };
        let mut group = GroupState::new("room-local-membership");
        group.add_leaf(owner.clone())?;
        group.add_leaf(member.clone())?;
        group.add_leaf(removed.clone())?;
        group.remove_leaf(removed.leaf_index)?;

        let mut governance = GovernanceState::new(group.epoch, owner.leaf_index);
        governance.apply_event(GovernanceEvent::signed_by(
            group.epoch,
            owner.leaf_index,
            GovernanceAction::SetRole {
                target: member.leaf_index,
                role: Role::Member,
            },
            &owner_signer,
        ))?;
        governance.apply_event(GovernanceEvent::signed_by(
            group.epoch,
            owner.leaf_index,
            GovernanceAction::SetRole {
                target: removed.leaf_index,
                role: Role::Member,
            },
            &owner_signer,
        ))?;

        let mut oracle = LiveKeyOracle::from_local_mls_governance_state(&group, &governance, 2)?;
        let commitment = oracle
            .epoch_group_commitment(group.epoch)
            .ok_or_else(|| std::io::Error::other("local epoch commitment missing"))?;
        let key = [0x44; 32];
        let member_proof = MembershipProof::sign(
            member.leaf_index,
            group.epoch,
            &group.group_id,
            commitment,
            &member_signer,
        );
        let allowed = oracle.request_key(&member_proof, key);
        assert_eq!(allowed.state, KeyState::Cached(key));
        assert!(allowed.authorized);

        let removed_proof = MembershipProof::sign(
            removed.leaf_index,
            group.epoch,
            &group.group_id,
            commitment,
            &removed_signer,
        );
        let rejected = oracle.request_key(&removed_proof, key);
        assert!(matches!(rejected.state, KeyState::Decoy(_)));
        assert!(!rejected.authorized);
        Ok(())
    }

    #[test]
    fn local_membership_state_rejects_unrepaired_epoch_mismatch(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use mls_core::{DeviceLeaf, GovernanceState};
        use uuid::Uuid;

        let signer = SigningKey::from_bytes(&[0x55; 32]);
        let leaf = DeviceLeaf {
            device_id: Uuid::from_u128(55),
            leaf_index: 5,
            identity_key: [0x55; 32],
            device_key: signer.verifying_key().to_bytes(),
            label: "leaf".to_owned(),
            status: DeviceStatus::Active,
            added_at_epoch: 0,
            removed_at_epoch: None,
        };
        let mut group = GroupState::new("room-mismatch");
        group.add_leaf(leaf.clone())?;
        let stale_governance = GovernanceState::new(group.epoch.saturating_sub(1), leaf.leaf_index);
        assert_eq!(
            LiveKeyOracle::from_local_mls_governance_state(&group, &stale_governance, 1),
            Err(LocalMembershipStateError::EpochMismatch {
                group_epoch: group.epoch,
                governance_epoch: stale_governance.epoch,
            })
        );
        Ok(())
    }
}

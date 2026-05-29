//! Content-key retention, live-key, and shred primitives.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use chrono::{DateTime, Duration, Utc};
use mls_core::{derive_epoch_secret, ExportLabel};
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
}

/// Deterministic content-key derivation for tests/facade.
///
/// Content keys are derived through the MLS exporter facade using the
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

/// Local membership proof for archival live-key requests.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct MembershipProof {
    /// Requesting leaf.
    pub requester_leaf: u32,
    /// Epoch being proven.
    pub epoch: u64,
    /// Locally verified group-state credential hash.
    pub credential_hash: [u8; 32],
}

impl MembershipProof {
    /// Build a deterministic proof token.
    #[must_use]
    pub fn new(requester_leaf: u32, epoch: u64, room_id: &str) -> Self {
        let mut h = Sha256::new();
        h.update(b"discrypt-membership-proof");
        h.update(requester_leaf.to_be_bytes());
        h.update(epoch.to_be_bytes());
        h.update(room_id.as_bytes());
        Self {
            requester_leaf,
            epoch,
            credential_hash: h.finalize().into(),
        }
    }
}

/// Live-key oracle response with explicit authorization flag.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LiveKeyResponse {
    /// Returned state, real key/decoy/rate-limited.
    pub state: KeyState,
    /// True only for locally authorized members under the limit.
    pub authorized: bool,
}

/// Membership-gated, rate-limited, decoy-capable live-key oracle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LiveKeyOracle {
    members_by_epoch: BTreeMap<u64, BTreeSet<u32>>,
    requests_by_leaf_epoch: BTreeMap<(u32, u64), usize>,
    max_requests: usize,
    decoy_key: [u8; 32],
}

impl LiveKeyOracle {
    /// Create an oracle from epoch membership.
    #[must_use]
    pub fn new(members_by_epoch: BTreeMap<u64, BTreeSet<u32>>, max_requests: usize) -> Self {
        Self {
            members_by_epoch,
            requests_by_leaf_epoch: BTreeMap::new(),
            max_requests: max_requests.max(1),
            decoy_key: [0xD; 32],
        }
    }

    /// Request an archival key. Non-members and over-limit callers receive decoys.
    pub fn request_key(&mut self, proof: &MembershipProof, key: [u8; 32]) -> LiveKeyResponse {
        let allowed_member = self
            .members_by_epoch
            .get(&proof.epoch)
            .is_some_and(|members| members.contains(&proof.requester_leaf));
        let counter = self
            .requests_by_leaf_epoch
            .entry((proof.requester_leaf, proof.epoch))
            .or_default();
        *counter = counter.saturating_add(1);
        if !allowed_member {
            return LiveKeyResponse {
                state: KeyState::Decoy(self.decoy_key),
                authorized: false,
            };
        }
        if *counter > self.max_requests {
            return LiveKeyResponse {
                state: KeyState::RateLimited,
                authorized: false,
            };
        }
        LiveKeyResponse {
            state: KeyState::Cached(key),
            authorized: true,
        }
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
    fn live_key_oracle_gates_membership_and_rate_limits_with_decoys() {
        let mut members = BTreeMap::new();
        members.insert(7, BTreeSet::from([1, 2]));
        let mut oracle = LiveKeyOracle::new(members, 1);
        let key = [9; 32];
        let allowed = oracle.request_key(&MembershipProof::new(1, 7, "room"), key);
        assert_eq!(allowed.state, KeyState::Cached(key));
        assert!(allowed.authorized);
        let limited = oracle.request_key(&MembershipProof::new(1, 7, "room"), key);
        assert_eq!(limited.state, KeyState::RateLimited);
        assert!(!limited.authorized);
        let decoy = oracle.request_key(&MembershipProof::new(9, 7, "room"), key);
        assert!(matches!(decoy.state, KeyState::Decoy(_)));
        assert!(!decoy.authorized);
    }
}

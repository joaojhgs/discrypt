//! Local-device storage boundaries for author logs, recipient caches, and sealed backup.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod appdb;
pub mod production_status;
#[cfg(all(target_os = "linux", feature = "production-storage"))]
pub use appdb::LinuxOsKeychain;
#[cfg(any(
    test,
    feature = "harness",
    feature = "local-dev",
    not(feature = "production-storage")
))]
pub use appdb::MemoryAppDbKeychain;
pub use appdb::{
    preflight_app_db_keychain, sqlite_wal_path, storage_keychain_decision, AppDbKeychain,
    EncryptedAppDb, StorageKeychainDecision,
};
#[cfg(all(target_os = "linux", feature = "production-storage"))]
pub use appdb::{PassphraseVaultKeychain, ProductionAppDbKeychain};
pub use content_keys::{KeyState, RetentionPolicy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use zeroize::Zeroize;

const SEALED_ACCOUNT_BACKUP_DOMAIN: &[u8] = b"discrypt:v1:sealed-account-backup";
const RECOVERY_CODE_DOMAIN: &[u8] = b"discrypt:v1:account-recovery-code";
const ACCOUNT_BACKUP_EXPORT_VERSION: u16 = 1;

/// App-store persistence errors.
#[derive(Debug, thiserror::Error)]
pub enum AppStoreError {
    /// Underlying filesystem operation failed.
    #[error("app store I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// In-memory store lock was poisoned.
    #[error("app store lock poisoned")]
    LockPoisoned,
    /// Serialized state or encrypted envelope was malformed.
    #[error("app store serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// App DB encryption/decryption failed.
    #[error("app store crypto error: {0}")]
    Crypto(&'static str),
    /// Required keychain wrapping key is unavailable.
    #[error("app store keychain key missing: {0}")]
    KeychainMissing(String),
    /// Platform keychain operation failed.
    #[error("app store keychain error: {0}")]
    Keychain(String),
}

/// Author-log append/merge failures.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AuthorLogError {
    /// Device id is required to keep each own-device branch independently append-only.
    #[error("author log device id cannot be empty")]
    EmptyDeviceId,
    /// Stable message ids are mandatory for merge/dedupe.
    #[error("author log message id cannot be empty")]
    EmptyMessageId,
    /// The same author/device/sequence already contains different ciphertext or metadata.
    #[error("author log fork at author {author_leaf} device {device_id} sequence {sequence}")]
    SequenceFork {
        /// Author MLS leaf.
        author_leaf: u32,
        /// Device branch.
        device_id: String,
        /// Device-local sequence.
        sequence: u64,
    },
    /// One stable message id was reused for another log position.
    #[error("author log message id reused at another position: {message_id}")]
    MessageIdReused {
        /// Reused message id.
        message_id: String,
    },
}

/// Local decrypt failures for cached recipient ciphertext.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LocalDecryptError {
    /// The message is not present in the local recipient cache.
    #[error("cached message not found: {0}")]
    Missing(String),
    /// The message has a cooperative shred tombstone.
    #[error("message content key has been shredded: {0}")]
    Shredded(String),
    /// The message key is retention-locked and requires authorized live-key flow.
    #[error("message content key is retention locked: {0}")]
    Locked(String),
    /// The message has no usable local decrypt key.
    #[error("message content key is unavailable: {0}")]
    Unavailable(String),
}

/// Result of one author-log insert.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorLogAppendOutcome {
    /// A new append-only entry was inserted.
    Inserted,
    /// The entry was already present byte-for-byte.
    Duplicate,
}

/// Aggregate result of an author-log merge.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AuthorLogMergeReport {
    /// Newly inserted entries.
    pub inserted: usize,
    /// Idempotent duplicate entries already present.
    pub duplicates: usize,
}

/// Byte-oriented local app-state store used by the core AppService.
///
/// The core crate owns the typed schema; storage owns durable byte persistence so
/// migrations can be tested without coupling UI state to React state samples.
pub trait AppStore: Clone + Send + Sync + 'static {
    /// Load the serialized app state, if initialized.
    fn load_app_state(&mut self) -> Result<Option<Vec<u8>>, AppStoreError>;

    /// Save the serialized app state durably.
    fn save_app_state(&mut self, bytes: &[u8]) -> Result<(), AppStoreError>;
}

/// Deterministic in-memory app store for tests and harnesses.
#[derive(Clone, Debug, Default)]
pub struct MemoryAppStore {
    bytes: Arc<Mutex<Option<Vec<u8>>>>,
}

impl AppStore for MemoryAppStore {
    fn load_app_state(&mut self) -> Result<Option<Vec<u8>>, AppStoreError> {
        self.bytes
            .lock()
            .map_err(|_| AppStoreError::LockPoisoned)
            .map(|guard| guard.clone())
    }

    fn save_app_state(&mut self, bytes: &[u8]) -> Result<(), AppStoreError> {
        let mut guard = self.bytes.lock().map_err(|_| AppStoreError::LockPoisoned)?;
        *guard = Some(bytes.to_vec());
        Ok(())
    }
}

/// File-backed local app store for native shells.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileAppStore {
    path: PathBuf,
}

impl FileAppStore {
    /// Create a file-backed app store at an explicit path.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Return the backing file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AppStore for FileAppStore {
    fn load_app_state(&mut self) -> Result<Option<Vec<u8>>, AppStoreError> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(AppStoreError::Io(error)),
        }
    }

    fn save_app_state(&mut self, bytes: &[u8]) -> Result<(), AppStoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, bytes)?;
        fs::rename(tmp, &self.path)?;
        Ok(())
    }
}

/// Deterministic key for an author's sent-log entry.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct AuthorLogKey {
    /// Author MLS leaf.
    pub author_leaf: u32,
    /// Device id that authored this entry.
    pub device_id: String,
    /// Device-local sequence under the author.
    pub sequence: u64,
}

/// Authoritative sent-log entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthorLogEntry {
    /// Author MLS leaf.
    pub author_leaf: u32,
    /// Device id that authored this entry.
    pub device_id: String,
    /// Per-author monotonic sequence.
    pub sequence: u64,
    /// MLS epoch used to encrypt/send the message.
    pub epoch: u64,
    /// Stable message id.
    pub message_id: String,
    /// MLS/content encrypted text bytes.
    pub ciphertext: Vec<u8>,
}

impl AuthorLogEntry {
    /// Construct a sent-log entry.
    #[must_use]
    pub fn new(
        author_leaf: u32,
        device_id: impl Into<String>,
        sequence: u64,
        epoch: u64,
        message_id: impl Into<String>,
        ciphertext: Vec<u8>,
    ) -> Self {
        Self {
            author_leaf,
            device_id: device_id.into(),
            sequence,
            epoch,
            message_id: message_id.into(),
            ciphertext,
        }
    }

    /// Construct a sent-log entry with a stable id derived from canonical fields.
    #[must_use]
    pub fn new_stable(
        author_leaf: u32,
        device_id: impl Into<String>,
        sequence: u64,
        epoch: u64,
        ciphertext: Vec<u8>,
    ) -> Self {
        let device_id = device_id.into();
        let message_id =
            Self::stable_message_id(author_leaf, &device_id, sequence, epoch, &ciphertext);
        Self {
            author_leaf,
            device_id,
            sequence,
            epoch,
            message_id,
            ciphertext,
        }
    }

    /// Stable message-id derivation used before transport fanout or history merge.
    #[must_use]
    pub fn stable_message_id(
        author_leaf: u32,
        device_id: &str,
        sequence: u64,
        epoch: u64,
        ciphertext: &[u8],
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"discrypt:v1:author-log-message-id");
        hasher.update(author_leaf.to_be_bytes());
        hasher.update((device_id.len() as u64).to_be_bytes());
        hasher.update(device_id.as_bytes());
        hasher.update(sequence.to_be_bytes());
        hasher.update(epoch.to_be_bytes());
        hasher.update(Sha256::digest(ciphertext));
        format!("msg_{}", hex::encode(hasher.finalize()))
    }

    /// Whether this entry's id matches the canonical stable derivation.
    #[must_use]
    pub fn has_stable_message_id(&self) -> bool {
        self.message_id
            == Self::stable_message_id(
                self.author_leaf,
                &self.device_id,
                self.sequence,
                self.epoch,
                &self.ciphertext,
            )
    }

    /// Key used for deterministic merge/dedupe.
    #[must_use]
    pub fn key(&self) -> AuthorLogKey {
        AuthorLogKey {
            author_leaf: self.author_leaf,
            device_id: self.device_id.clone(),
            sequence: self.sequence,
        }
    }
}

/// Bounded recipient cache entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecipientCacheEntry {
    /// Message id.
    pub message_id: String,
    /// Received ciphertext.
    pub ciphertext: Vec<u8>,
    /// Cached/locked/shredded content-key state.
    pub key_state: KeyState,
    /// Deterministic receive timestamp for cache eviction.
    pub received_at_ms: u64,
}

impl RecipientCacheEntry {
    /// Construct a recipient cache entry.
    #[must_use]
    pub fn new(
        message_id: impl Into<String>,
        ciphertext: Vec<u8>,
        key_state: KeyState,
        received_at_ms: u64,
    ) -> Self {
        Self {
            message_id: message_id.into(),
            ciphertext,
            key_state,
            received_at_ms,
        }
    }
}

/// Bounded recipient cache for received ciphertext plus eligible local keys.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BoundedRecipientCache {
    capacity: usize,
    entries: BTreeMap<String, RecipientCacheEntry>,
}

impl BoundedRecipientCache {
    /// Create a bounded cache. A zero capacity is normalized to one.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: BTreeMap::new(),
        }
    }

    /// Insert an entry and evict the oldest deterministic entries above capacity.
    pub fn insert(&mut self, entry: RecipientCacheEntry) {
        self.entries.insert(entry.message_id.clone(), entry);
        while self.entries.len() > self.capacity {
            let Some(oldest_id) = self
                .entries
                .values()
                .min_by_key(|entry| (entry.received_at_ms, entry.message_id.clone()))
                .map(|entry| entry.message_id.clone())
            else {
                break;
            };
            self.entries.remove(&oldest_id);
        }
    }

    /// Get a cached entry.
    #[must_use]
    pub fn get(&self, message_id: &str) -> Option<&RecipientCacheEntry> {
        self.entries.get(message_id)
    }

    /// Mark one message as shredded and zeroize any cached content-key bytes.
    pub fn shred(&mut self, message_id: &str) -> bool {
        let Some(entry) = self.entries.get_mut(message_id) else {
            return false;
        };
        if let KeyState::Cached(key) | KeyState::Decoy(key) = &mut entry.key_state {
            key.zeroize();
        }
        entry.key_state = KeyState::Shredded;
        true
    }

    /// Harness decrypt boundary: only cached keys may decrypt local ciphertext.
    ///
    /// This intentionally models the storage fail-closed contract rather than
    /// production text crypto. Once a key is locked or shredded, callers cannot
    /// get plaintext through this path.
    pub fn decrypt_for_harness(&self, message_id: &str) -> Result<Vec<u8>, LocalDecryptError> {
        let entry = self
            .entries
            .get(message_id)
            .ok_or_else(|| LocalDecryptError::Missing(message_id.to_owned()))?;
        match &entry.key_state {
            KeyState::Cached(key) => Ok(xor_harness_ciphertext(&entry.ciphertext, key)),
            KeyState::Shredded => Err(LocalDecryptError::Shredded(message_id.to_owned())),
            KeyState::Locked => Err(LocalDecryptError::Locked(message_id.to_owned())),
            KeyState::Decoy(_) | KeyState::RateLimited | KeyState::Unavailable => {
                Err(LocalDecryptError::Unavailable(message_id.to_owned()))
            }
        }
    }

    /// Cache size.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Ordered cache ids.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }
}

fn xor_harness_ciphertext(ciphertext: &[u8], key: &[u8; 32]) -> Vec<u8> {
    ciphertext
        .iter()
        .enumerate()
        .map(|(idx, byte)| byte ^ key[idx % key.len()])
        .collect()
}

impl Default for BoundedRecipientCache {
    fn default() -> Self {
        Self::new(256)
    }
}

/// In-memory local store for deterministic harnesses.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocalStore {
    author_log: BTreeMap<AuthorLogKey, AuthorLogEntry>,
    recipient_cache: BoundedRecipientCache,
    message_tombstones: BTreeSet<String>,
}

impl LocalStore {
    /// Create a store with a bounded recipient cache.
    #[must_use]
    pub fn with_recipient_cache_capacity(capacity: usize) -> Self {
        Self {
            author_log: BTreeMap::new(),
            recipient_cache: BoundedRecipientCache::new(capacity),
            message_tombstones: BTreeSet::new(),
        }
    }

    /// Append an authoritative sent message for its author.
    pub fn append_sent(
        &mut self,
        entry: AuthorLogEntry,
    ) -> Result<AuthorLogAppendOutcome, AuthorLogError> {
        self.insert_author_log_entry(entry)
    }

    /// Merge received author-log entries from another own device or gossip peer.
    pub fn merge_author_logs<I>(
        &mut self,
        entries: I,
    ) -> Result<AuthorLogMergeReport, AuthorLogError>
    where
        I: IntoIterator<Item = AuthorLogEntry>,
    {
        let mut report = AuthorLogMergeReport::default();
        for entry in entries {
            match self.insert_author_log_entry(entry)? {
                AuthorLogAppendOutcome::Inserted => report.inserted += 1,
                AuthorLogAppendOutcome::Duplicate => report.duplicates += 1,
            }
        }
        Ok(report)
    }

    /// Merge received author-log entries atomically.
    ///
    /// If any entry conflicts, the store is left unchanged.
    pub fn merge_author_logs_atomic<I>(
        &mut self,
        entries: I,
    ) -> Result<AuthorLogMergeReport, AuthorLogError>
    where
        I: IntoIterator<Item = AuthorLogEntry>,
    {
        let mut candidate = self.clone();
        let report = candidate.merge_author_logs(entries)?;
        *self = candidate;
        Ok(report)
    }

    /// Ordered author-log snapshot.
    #[must_use]
    pub fn author_log_snapshot(&self) -> Vec<AuthorLogEntry> {
        self.author_log.values().cloned().collect()
    }

    /// Ordered author-log snapshot for one author.
    #[must_use]
    pub fn author_log_for(&self, author_leaf: u32) -> Vec<AuthorLogEntry> {
        self.author_log
            .values()
            .filter(|entry| entry.author_leaf == author_leaf)
            .cloned()
            .collect()
    }

    /// Ordered message ids in the author log.
    #[must_use]
    pub fn author_message_ids(&self) -> BTreeSet<String> {
        self.author_log
            .values()
            .map(|entry| entry.message_id.clone())
            .collect()
    }

    /// Deterministic ordered message ids in the author log.
    #[must_use]
    pub fn ordered_author_message_ids(&self) -> Vec<String> {
        self.author_log
            .values()
            .map(|entry| entry.message_id.clone())
            .collect()
    }

    /// Next append sequence for one author/device branch.
    #[must_use]
    pub fn next_sequence_for_device(&self, author_leaf: u32, device_id: &str) -> u64 {
        self.author_log
            .keys()
            .filter(|key| key.author_leaf == author_leaf && key.device_id == device_id)
            .map(|key| key.sequence)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    /// Cache a received message under the bounded retention cache.
    pub fn cache_received(&mut self, entry: RecipientCacheEntry) {
        if self.message_tombstones.contains(&entry.message_id) {
            let mut entry = entry;
            if let KeyState::Cached(key) | KeyState::Decoy(key) = &mut entry.key_state {
                key.zeroize();
            }
            entry.key_state = KeyState::Shredded;
            self.recipient_cache.insert(entry);
            return;
        }
        self.recipient_cache.insert(entry);
    }

    /// Cooperatively shred a message key and retain a local tombstone.
    pub fn cooperative_shred_message(&mut self, message_id: impl Into<String>) {
        let message_id = message_id.into();
        self.message_tombstones.insert(message_id.clone());
        self.recipient_cache.shred(&message_id);
    }

    /// Merge tombstones received from another own device.
    pub fn merge_message_tombstones<I, S>(&mut self, message_ids: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for message_id in message_ids {
            self.cooperative_shred_message(message_id);
        }
    }

    /// Ordered local tombstone ids.
    #[must_use]
    pub fn message_tombstones(&self) -> Vec<String> {
        self.message_tombstones.iter().cloned().collect()
    }

    /// Harness decrypt boundary for tests and storage contracts.
    pub fn decrypt_cached_message_for_harness(
        &self,
        message_id: &str,
    ) -> Result<Vec<u8>, LocalDecryptError> {
        if self.message_tombstones.contains(message_id) {
            return Err(LocalDecryptError::Shredded(message_id.to_owned()));
        }
        self.recipient_cache.decrypt_for_harness(message_id)
    }

    /// Recipient cache reference.
    #[must_use]
    pub fn recipient_cache(&self) -> &BoundedRecipientCache {
        &self.recipient_cache
    }

    fn insert_author_log_entry(
        &mut self,
        entry: AuthorLogEntry,
    ) -> Result<AuthorLogAppendOutcome, AuthorLogError> {
        if entry.device_id.trim().is_empty() {
            return Err(AuthorLogError::EmptyDeviceId);
        }
        if entry.message_id.trim().is_empty() {
            return Err(AuthorLogError::EmptyMessageId);
        }

        let key = entry.key();
        if let Some(existing) = self.author_log.get(&key) {
            return if existing == &entry {
                Ok(AuthorLogAppendOutcome::Duplicate)
            } else {
                Err(AuthorLogError::SequenceFork {
                    author_leaf: key.author_leaf,
                    device_id: key.device_id,
                    sequence: key.sequence,
                })
            };
        }

        if self.author_log.iter().any(|(existing_key, existing)| {
            existing.message_id == entry.message_id && existing_key != &key
        }) {
            return Err(AuthorLogError::MessageIdReused {
                message_id: entry.message_id,
            });
        }

        self.author_log.insert(key, entry);
        Ok(AuthorLogAppendOutcome::Inserted)
    }
}

/// Persisted application state schema version.
pub const APP_STATE_SCHEMA_VERSION: u32 = 1;

/// Local account identity persisted by the application store.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppIdentityState {
    /// Stable local user id.
    pub user_id: String,
    /// User-visible display name.
    pub display_name: String,
    /// Backend-owned friend code/QR payload.
    pub friend_code: String,
    /// Current local device id.
    pub device_id: String,
    /// Pairwise safety number copy surfaced by the UI.
    pub safety_number: String,
    /// Explicit out-of-band verification flag.
    pub safety_verified: bool,
}

impl AppIdentityState {
    /// Build identity state for persistence-backed command snapshots.
    #[must_use]
    pub fn new(
        user_id: impl Into<String>,
        display_name: impl Into<String>,
        friend_code: impl Into<String>,
        device_id: impl Into<String>,
        safety_number: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            display_name: display_name.into(),
            friend_code: friend_code.into(),
            device_id: device_id.into(),
            safety_number: safety_number.into(),
            safety_verified: false,
        }
    }
}

/// User preference state that must survive restart.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppPreferencesState {
    /// UI theme identifier.
    pub theme: String,
    /// Active UI template identifier.
    pub active_template: String,
    /// Default retention preset for new channels/messages.
    pub retention_preset: String,
    /// Typed backend-owned retention policy that must survive restart.
    #[serde(default)]
    pub retention_policy: RetentionPolicy,
}

impl Default for AppPreferencesState {
    fn default() -> Self {
        Self {
            theme: "system".to_owned(),
            active_template: "command-center".to_owned(),
            retention_preset: "7 days".to_owned(),
            retention_policy: RetentionPolicy::default(),
        }
    }
}

/// Group/server state persisted for command-backed UI navigation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppGroupState {
    /// Stable group id.
    pub group_id: String,
    /// User-visible group/server label.
    pub name: String,
    /// Local user's role label.
    pub role: String,
    /// Current MLS epoch summary.
    pub epoch: u64,
    /// Channels belonging to this group.
    pub channels: Vec<AppChannelState>,
}

impl AppGroupState {
    /// Create an empty group/server.
    #[must_use]
    pub fn new(
        group_id: impl Into<String>,
        name: impl Into<String>,
        role: impl Into<String>,
    ) -> Self {
        Self {
            group_id: group_id.into(),
            name: name.into(),
            role: role.into(),
            epoch: 1,
            channels: Vec::new(),
        }
    }
}

/// Channel state persisted under a group/server.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppChannelState {
    /// Stable channel id.
    pub channel_id: String,
    /// Parent group id.
    pub group_id: String,
    /// User-visible channel name.
    pub name: String,
    /// Channel kind string shared with command DTOs (`text` or `voice`).
    pub kind: String,
    /// Retention status copy derived from preferences/governance.
    pub retention_status: String,
}

impl AppChannelState {
    /// Create channel state.
    #[must_use]
    pub fn new(
        channel_id: impl Into<String>,
        group_id: impl Into<String>,
        name: impl Into<String>,
        kind: impl Into<String>,
        retention_status: impl Into<String>,
    ) -> Self {
        Self {
            channel_id: channel_id.into(),
            group_id: group_id.into(),
            name: name.into(),
            kind: kind.into(),
            retention_status: retention_status.into(),
        }
    }
}

/// Persisted encrypted message record. Plaintext and content keys are never fields here.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppMessageState {
    /// Stable message id.
    pub message_id: String,
    /// Parent group id.
    pub group_id: String,
    /// Parent channel id.
    pub channel_id: String,
    /// Author/device label.
    pub author_id: String,
    /// Monotonic author sequence.
    pub sequence: u64,
    /// MLS epoch used for the ciphertext.
    pub epoch: u64,
    /// Ciphertext bytes only.
    pub ciphertext: Vec<u8>,
    /// Deterministic timestamp used by harnesses and UI ordering.
    pub sent_at_ms: u64,
}

/// Persisted cooperative shred tombstone for a message.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct AppMessageTombstoneState {
    /// Stable message id.
    pub message_id: String,
    /// Backend/device timestamp when the tombstone was recorded.
    pub shredded_at_ms: u64,
    /// Backend-owned reason label, such as `cooperative-shred`.
    pub reason: String,
}

impl AppMessageTombstoneState {
    /// Construct a persisted tombstone.
    #[must_use]
    pub fn new(
        message_id: impl Into<String>,
        shredded_at_ms: u64,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            message_id: message_id.into(),
            shredded_at_ms,
            reason: reason.into(),
        }
    }
}

/// Invite/admission flow state persisted for restart-safe admin UX.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppInviteState {
    /// Stable invite id.
    pub invite_id: String,
    /// Parent group id.
    pub group_id: String,
    /// Expiry copy or timestamp string from the admission layer.
    pub expires: String,
    /// Max-use copy.
    pub max_use: String,
    /// Password-gate posture copy. No offline verifier secret is stored here.
    pub password_gate: String,
    /// Revocation flag.
    pub revoked: bool,
}

/// Voice session state persisted for command-backed voice controls.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppVoiceSessionState {
    /// Stable voice session id.
    pub session_id: String,
    /// Parent group id.
    pub group_id: String,
    /// Parent voice channel id.
    pub channel_id: String,
    /// Current connection route label.
    pub route: String,
    /// Whether local user is joined.
    pub joined: bool,
    /// Whether local microphone is muted.
    pub muted: bool,
}

/// Complete application state persisted by `AppStore`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppState {
    /// Persisted schema version.
    pub schema_version: u32,
    /// Local account identity.
    pub identity: AppIdentityState,
    /// User preferences.
    pub preferences: AppPreferencesState,
    /// Groups with channels.
    pub groups: Vec<AppGroupState>,
    /// Ciphertext-only messages.
    pub messages: Vec<AppMessageState>,
    /// Cooperative shred tombstones. Ciphertext can remain, but content keys cannot.
    #[serde(default)]
    pub message_tombstones: Vec<AppMessageTombstoneState>,
    /// Invite flow state.
    pub invites: Vec<AppInviteState>,
    /// Voice session state.
    pub voice_sessions: Vec<AppVoiceSessionState>,
}

impl AppState {
    /// Construct app state with defaults and no product entities.
    #[must_use]
    pub fn new(identity: AppIdentityState) -> Self {
        Self {
            schema_version: APP_STATE_SCHEMA_VERSION,
            identity,
            preferences: AppPreferencesState::default(),
            groups: Vec::new(),
            messages: Vec::new(),
            message_tombstones: Vec::new(),
            invites: Vec::new(),
            voice_sessions: Vec::new(),
        }
    }

    /// Record a cooperative shred tombstone without deleting ciphertext history.
    pub fn record_message_tombstone(&mut self, tombstone: AppMessageTombstoneState) {
        if let Some(existing) = self
            .message_tombstones
            .iter_mut()
            .find(|existing| existing.message_id == tombstone.message_id)
        {
            if tombstone.shredded_at_ms < existing.shredded_at_ms {
                existing.shredded_at_ms = tombstone.shredded_at_ms;
            }
            if existing.reason.is_empty() {
                existing.reason = tombstone.reason;
            }
            return;
        }
        self.message_tombstones.push(tombstone);
        self.message_tombstones
            .sort_by(|a, b| a.message_id.cmp(&b.message_id));
    }

    /// Merge tombstones from another own device.
    pub fn merge_message_tombstones<I>(&mut self, tombstones: I)
    where
        I: IntoIterator<Item = AppMessageTombstoneState>,
    {
        for tombstone in tombstones {
            self.record_message_tombstone(tombstone);
        }
    }

    /// True when persisted state has a tombstone for this message.
    #[must_use]
    pub fn has_message_tombstone(&self, message_id: &str) -> bool {
        self.message_tombstones
            .iter()
            .any(|tombstone| tombstone.message_id == message_id)
    }

    /// Ordered tombstone ids for sync or assertions.
    #[must_use]
    pub fn message_tombstone_ids(&self) -> Vec<String> {
        self.message_tombstones
            .iter()
            .map(|tombstone| tombstone.message_id.clone())
            .collect()
    }
}

/// Sealed account-continuity backup (no content keys).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountBackup {
    /// Wrapped identity key bytes.
    pub identity_key_ciphertext: Vec<u8>,
    /// Room memberships restored for account continuity.
    pub room_memberships: Vec<String>,
    /// Device count in the backed-up device set.
    pub device_count: usize,
}

/// User-visible account-continuity backup recovery method.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AccountBackupRecoveryMethod {
    /// Another already-authorized own device exported the backup.
    ExistingDevice,
    /// User-held recovery code is required before the backup is restored.
    RecoveryCode,
    /// The backup itself is the sealed account-continuity trust material.
    SealedBackup,
}

/// Persisted backup metadata that survives export/import and restart.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountBackupMetadata {
    /// Backup envelope version.
    pub version: u16,
    /// Creation time recorded by the backend caller.
    pub created_at_ms: u64,
    /// Backend-proven own device that exported the backup.
    pub exported_by_device_id: String,
    /// Recovery method required by this backup.
    pub recovery_method: AccountBackupRecoveryMethod,
    /// Compromised device that must be rotated before account continuity is restored.
    pub compromised_device_id: Option<String>,
    /// True when restore must fail until caller supplies device-rotation evidence.
    pub device_rotation_required: bool,
}

/// Versioned account-continuity backup export.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountBackupExport {
    /// Persisted recovery metadata.
    pub metadata: AccountBackupMetadata,
    /// Sealed account-continuity backup. This never contains archival content keys.
    pub backup: AccountBackup,
}

/// Persistable verifier for a user-held recovery code.
///
/// The raw recovery code is deliberately not stored here. Call
/// [`recovery_code_material`] with the user-supplied code to produce verified
/// recovery material.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryCodeVerifier {
    /// Domain-separated hash of the normalized recovery code.
    pub code_hash: [u8; 32],
}

impl RecoveryCodeVerifier {
    /// Build a recovery-code verifier without retaining the raw code.
    pub fn from_code(code: impl AsRef<str>) -> Result<Self, RecoveryError> {
        Ok(Self {
            code_hash: hash_recovery_code(code.as_ref())?,
        })
    }

    /// Check whether a user-supplied code matches this verifier.
    pub fn verifies(&self, code: impl AsRef<str>) -> bool {
        hash_recovery_code(code.as_ref()).is_ok_and(|candidate| candidate == self.code_hash)
    }
}

/// Seal account-continuity data without content keys.
#[must_use]
pub fn seal_account_backup(
    key: &[u8; 32],
    rooms: Vec<String>,
    device_count: usize,
) -> AccountBackup {
    let mut material = *key;
    let mut hasher = Sha256::new();
    hasher.update(SEALED_ACCOUNT_BACKUP_DOMAIN);
    hasher.update(material);
    let digest = hasher.finalize();
    material.zeroize();
    let (room_memberships, device_count) = normalize_account_continuity(rooms, device_count);
    AccountBackup {
        identity_key_ciphertext: digest.to_vec(),
        room_memberships,
        device_count,
    }
}

/// Create a versioned account-continuity backup export without content keys.
pub fn export_account_backup(
    key: &[u8; 32],
    rooms: Vec<String>,
    device_count: usize,
    exported_by_device_id: impl Into<String>,
    recovery_method: AccountBackupRecoveryMethod,
    created_at_ms: u64,
) -> Result<AccountBackupExport, RecoveryError> {
    let exported_by_device_id = normalize_required_id(exported_by_device_id)?;
    Ok(AccountBackupExport {
        metadata: AccountBackupMetadata {
            version: ACCOUNT_BACKUP_EXPORT_VERSION,
            created_at_ms,
            exported_by_device_id,
            recovery_method,
            compromised_device_id: None,
            device_rotation_required: false,
        },
        backup: seal_account_backup(key, rooms, device_count),
    })
}

/// Create a backup export for a compromised-device recovery path.
///
/// Restoring this envelope through [`restore_account_backup_export`] fails
/// closed until the caller supplies replacement-device rotation evidence through
/// [`restore_account_backup_after_device_rotation`].
pub fn export_device_compromise_backup(
    key: &[u8; 32],
    rooms: Vec<String>,
    device_count: usize,
    exported_by_device_id: impl Into<String>,
    compromised_device_id: impl Into<String>,
    created_at_ms: u64,
) -> Result<AccountBackupExport, RecoveryError> {
    let mut export = export_account_backup(
        key,
        rooms,
        device_count,
        exported_by_device_id,
        AccountBackupRecoveryMethod::ExistingDevice,
        created_at_ms,
    )?;
    export.metadata.compromised_device_id = Some(normalize_required_id(compromised_device_id)?);
    export.metadata.device_rotation_required = true;
    Ok(export)
}

/// Parse and restore a serialized account-continuity backup export.
pub fn restore_account_backup_export_json(bytes: &[u8]) -> Result<AccountRecovery, RecoveryError> {
    let export: AccountBackupExport =
        serde_json::from_slice(bytes).map_err(|error| RecoveryError::MalformedBackup {
            reason: error.to_string(),
        })?;
    restore_account_backup_export(export)
}

/// Restore a backup export that does not require compromised-device rotation or recovery code.
pub fn restore_account_backup_export(
    export: AccountBackupExport,
) -> Result<AccountRecovery, RecoveryError> {
    validate_backup_export(&export)?;
    if export.metadata.device_rotation_required {
        return Err(RecoveryError::DeviceRotationRequired {
            compromised_device_id: export
                .metadata
                .compromised_device_id
                .clone()
                .unwrap_or_else(|| "unknown".to_owned()),
        });
    }
    if export.metadata.recovery_method == AccountBackupRecoveryMethod::RecoveryCode {
        return Err(RecoveryError::RecoveryCodeRequired);
    }
    recover_account(RecoveryMaterial::SealedBackup(export.backup))
}

/// Parse and restore a recovery-code-gated account-continuity backup export.
pub fn restore_recovery_code_backup_export_json(
    bytes: &[u8],
    code: impl AsRef<str>,
    verifier: &RecoveryCodeVerifier,
) -> Result<AccountRecovery, RecoveryError> {
    let export: AccountBackupExport =
        serde_json::from_slice(bytes).map_err(|error| RecoveryError::MalformedBackup {
            reason: error.to_string(),
        })?;
    restore_recovery_code_backup_export(export, code, verifier)
}

/// Restore a recovery-code-gated backup only after verifying the user-held code.
pub fn restore_recovery_code_backup_export(
    export: AccountBackupExport,
    code: impl AsRef<str>,
    verifier: &RecoveryCodeVerifier,
) -> Result<AccountRecovery, RecoveryError> {
    validate_backup_export(&export)?;
    if export.metadata.device_rotation_required {
        return Err(RecoveryError::DeviceRotationRequired {
            compromised_device_id: export
                .metadata
                .compromised_device_id
                .clone()
                .unwrap_or_else(|| "unknown".to_owned()),
        });
    }
    if export.metadata.recovery_method != AccountBackupRecoveryMethod::RecoveryCode {
        return Err(RecoveryError::InvalidBackupMetadata(
            "backup does not require recovery code".to_owned(),
        ));
    }
    let material = recovery_code_material(
        code,
        verifier,
        export.backup.room_memberships,
        export.backup.device_count,
    )?;
    recover_account(material)
}

/// Restore a compromised-device backup after explicit replacement-device evidence.
pub fn restore_account_backup_after_device_rotation(
    export: AccountBackupExport,
    replacement_device_id: impl Into<String>,
) -> Result<AccountRecovery, RecoveryError> {
    validate_backup_export(&export)?;
    if !export.metadata.device_rotation_required {
        return Err(RecoveryError::InvalidBackupMetadata(
            "backup does not require device rotation".to_owned(),
        ));
    }
    let replacement_device_id = normalize_required_id(replacement_device_id)?;
    if export.metadata.compromised_device_id.as_deref() == Some(replacement_device_id.as_str()) {
        return Err(RecoveryError::InvalidBackupMetadata(
            "replacement device must differ from compromised device".to_owned(),
        ));
    }
    recover_account(RecoveryMaterial::SealedBackup(export.backup))
}

/// Recovery material available to a user.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RecoveryMaterial {
    /// Existing authorized device participates.
    ExistingDevice { device_id: String },
    /// User-held recovery code has been verified against a stored verifier.
    RecoveryCode {
        /// Matched recovery-code hash. The raw code is never stored.
        code_hash: [u8; 32],
        /// Room memberships restored for account continuity.
        room_memberships: Vec<String>,
        /// Device count in the recovered device set.
        device_count: usize,
    },
    /// Sealed account-continuity backup.
    SealedBackup(AccountBackup),
    /// No remaining trust material.
    None,
}

/// Recovery errors.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RecoveryError {
    /// No authorized device, recovery code, or backup exists.
    #[error("account recovery requires trust material")]
    NoTrustMaterial,
    /// Empty recovery codes are rejected before hashing.
    #[error("recovery code cannot be empty")]
    EmptyRecoveryCode,
    /// The supplied recovery code does not match the stored verifier.
    #[error("recovery code did not match account verifier")]
    InvalidRecoveryCode,
    /// This backup requires the explicit recovery-code restore path.
    #[error("recovery code is required before restoring this backup")]
    RecoveryCodeRequired,
    /// Serialized backup bytes could not be parsed.
    #[error("account backup is malformed: {reason}")]
    MalformedBackup {
        /// Parse or shape error.
        reason: String,
    },
    /// Backup envelope version is not supported.
    #[error("account backup version {version} is unsupported")]
    UnsupportedBackupVersion {
        /// Unsupported envelope version.
        version: u16,
    },
    /// Backup metadata is missing required backend evidence.
    #[error("account backup metadata is invalid: {0}")]
    InvalidBackupMetadata(String),
    /// The backup came from a compromised-device flow and requires rotation.
    #[error("device rotation is required before recovery: {compromised_device_id}")]
    DeviceRotationRequired {
        /// Device id that must be evicted/rotated before restore.
        compromised_device_id: String,
    },
}

/// Account-continuity recovery result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountRecovery {
    /// Identity/device-set continuity is restored.
    pub account_access_restored: bool,
    /// Room memberships restored.
    pub room_memberships: Vec<String>,
    /// Device count from continuity material.
    pub device_count: usize,
    /// Content keys are deliberately not restored.
    pub content_keys_restored: bool,
}

/// Build verified recovery material from a user-held code and persisted verifier.
///
/// The returned material contains account-continuity metadata only. Archival
/// message/content keys are intentionally not accepted by this API and therefore
/// cannot be restored by [`recover_account`].
pub fn recovery_code_material(
    code: impl AsRef<str>,
    verifier: &RecoveryCodeVerifier,
    rooms: Vec<String>,
    device_count: usize,
) -> Result<RecoveryMaterial, RecoveryError> {
    if !verifier.verifies(code) {
        return Err(RecoveryError::InvalidRecoveryCode);
    }
    let (room_memberships, device_count) = normalize_account_continuity(rooms, device_count);
    Ok(RecoveryMaterial::RecoveryCode {
        code_hash: verifier.code_hash,
        room_memberships,
        device_count,
    })
}

/// Build recovery material for an explicit lost-password flow.
///
/// Without a stored verifier and a user-supplied recovery code, account recovery
/// is non-recoverable and fails closed.
pub fn lost_password_recovery_material(
    code: Option<&str>,
    verifier: Option<&RecoveryCodeVerifier>,
    rooms: Vec<String>,
    device_count: usize,
) -> Result<RecoveryMaterial, RecoveryError> {
    match (code, verifier) {
        (Some(code), Some(verifier)) => recovery_code_material(code, verifier, rooms, device_count),
        _ => Err(RecoveryError::NoTrustMaterial),
    }
}

/// Recover account continuity without restoring archival content keys.
pub fn recover_account(material: RecoveryMaterial) -> Result<AccountRecovery, RecoveryError> {
    match material {
        RecoveryMaterial::None => Err(RecoveryError::NoTrustMaterial),
        RecoveryMaterial::ExistingDevice { .. } => Ok(account_continuity_recovery(Vec::new(), 1)),
        RecoveryMaterial::RecoveryCode {
            room_memberships,
            device_count,
            ..
        } => Ok(account_continuity_recovery(room_memberships, device_count)),
        RecoveryMaterial::SealedBackup(backup) => Ok(account_continuity_recovery(
            backup.room_memberships,
            backup.device_count,
        )),
    }
}

fn hash_recovery_code(code: &str) -> Result<[u8; 32], RecoveryError> {
    let normalized = code.trim();
    if normalized.is_empty() {
        return Err(RecoveryError::EmptyRecoveryCode);
    }
    let mut hasher = Sha256::new();
    hasher.update(RECOVERY_CODE_DOMAIN);
    hasher.update((normalized.len() as u64).to_be_bytes());
    hasher.update(normalized.as_bytes());
    let digest = hasher.finalize();
    let mut hash = [0_u8; 32];
    hash.copy_from_slice(&digest);
    Ok(hash)
}

fn normalize_required_id(id: impl Into<String>) -> Result<String, RecoveryError> {
    let id = id.into().trim().to_owned();
    if id.is_empty() {
        return Err(RecoveryError::InvalidBackupMetadata(
            "device id is required".to_owned(),
        ));
    }
    Ok(id)
}

fn validate_backup_export(export: &AccountBackupExport) -> Result<(), RecoveryError> {
    if export.metadata.version != ACCOUNT_BACKUP_EXPORT_VERSION {
        return Err(RecoveryError::UnsupportedBackupVersion {
            version: export.metadata.version,
        });
    }
    normalize_required_id(export.metadata.exported_by_device_id.clone())?;
    if export.backup.identity_key_ciphertext.len() != 32 {
        return Err(RecoveryError::InvalidBackupMetadata(
            "identity backup material must be 32 bytes".to_owned(),
        ));
    }
    if export.metadata.device_rotation_required
        && export
            .metadata
            .compromised_device_id
            .as_deref()
            .is_none_or(|id| id.trim().is_empty())
    {
        return Err(RecoveryError::InvalidBackupMetadata(
            "compromised device id is required when rotation is required".to_owned(),
        ));
    }
    Ok(())
}

fn normalize_account_continuity(rooms: Vec<String>, device_count: usize) -> (Vec<String>, usize) {
    let room_memberships = rooms
        .into_iter()
        .map(|room| room.trim().to_owned())
        .filter(|room| !room.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    (room_memberships, device_count.max(1))
}

fn account_continuity_recovery(rooms: Vec<String>, device_count: usize) -> AccountRecovery {
    let (room_memberships, device_count) = normalize_account_continuity(rooms, device_count);
    AccountRecovery {
        account_access_restored: true,
        room_memberships,
        device_count,
        content_keys_restored: false,
    }
}

/// In-memory secure-delete simulator for enumerated local stores.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecureDeleteSimulator {
    files: BTreeMap<String, Vec<u8>>,
    deleted_paths: BTreeSet<String>,
}

impl SecureDeleteSimulator {
    /// Add a simulated local file or key-store blob.
    pub fn write(&mut self, path: impl Into<String>, bytes: Vec<u8>) {
        self.files.insert(path.into(), bytes);
    }

    /// Snapshot all simulated files before a two-phase destructive operation.
    #[must_use]
    pub fn snapshot(&self) -> BTreeMap<String, Vec<u8>> {
        self.files.clone()
    }

    /// Restore a previous snapshot, used when shred verification fails.
    pub fn restore(&mut self, snapshot: BTreeMap<String, Vec<u8>>) {
        self.files = snapshot;
        self.deleted_paths.clear();
    }

    /// Zeroize and remove each enumerated path.
    pub fn secure_delete<I, S>(&mut self, paths: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for path in paths {
            let path = path.into();
            if let Some(mut bytes) = self.files.remove(&path) {
                bytes.zeroize();
            }
            self.deleted_paths.insert(path);
        }
    }

    /// True when any retained simulated file still contains the needle.
    #[must_use]
    pub fn contains_material(&self, needle: &[u8]) -> bool {
        !needle.is_empty()
            && self
                .files
                .values()
                .any(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
    }

    /// True when all paths have been enumerated for deletion.
    #[must_use]
    pub fn deleted_all<'a, I>(&self, paths: I) -> bool
    where
        I: IntoIterator<Item = &'a str>,
    {
        paths
            .into_iter()
            .all(|path| self.deleted_paths.contains(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_app_store_round_trips_app_state_bytes() -> Result<(), AppStoreError> {
        let mut store = MemoryAppStore::default();
        assert_eq!(store.load_app_state()?, None);
        store.save_app_state(br#"{"schema_version":2}"#)?;
        assert_eq!(
            store.load_app_state()?,
            Some(br#"{"schema_version":2}"#.to_vec())
        );
        Ok(())
    }

    #[test]
    fn app_state_retention_policy_survives_restart_bytes() -> Result<(), AppStoreError> {
        let mut state = AppState::new(AppIdentityState::new(
            "alice",
            "Alice",
            "friend:alice",
            "device-a",
            "1111",
        ));
        state.preferences.retention_policy = RetentionPolicy::new(
            content_keys::RetentionWindow::CustomSeconds(43_200),
            content_keys::RetentionPolicySource::Governance,
        );
        state.preferences.retention_preset = state.preferences.retention_policy.label();

        let mut store = MemoryAppStore::default();
        store.save_app_state(&serde_json::to_vec(&state)?)?;
        let bytes = store
            .load_app_state()?
            .ok_or(AppStoreError::Crypto("missing app state after save"))?;
        let reloaded: AppState = serde_json::from_slice(&bytes)?;

        assert_eq!(
            reloaded.preferences.retention_policy,
            state.preferences.retention_policy
        );
        assert_eq!(
            reloaded.preferences.retention_policy.seconds(),
            Some(43_200)
        );
        Ok(())
    }

    #[test]
    fn file_app_store_round_trips_app_state_bytes() -> Result<(), AppStoreError> {
        let path = std::env::temp_dir().join(format!(
            "discrypt-app-store-{}-{}.json",
            std::process::id(),
            "roundtrip"
        ));
        let _ = std::fs::remove_file(&path);
        let mut store = FileAppStore::new(&path);
        store.save_app_state(br#"{"schema_version":2}"#)?;
        assert_eq!(
            store.load_app_state()?,
            Some(br#"{"schema_version":2}"#.to_vec())
        );
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn backup_excludes_content_keys() {
        let b = seal_account_backup(&[9; 32], vec![" room ".into(), "room".into()], 0);
        assert_eq!(b.device_count, 1);
        assert_eq!(b.room_memberships, vec!["room"]);
        assert_eq!(b.identity_key_ciphertext.len(), 32);
    }

    #[test]
    fn sealed_account_backup_is_domain_separated_from_raw_content_key_hashes() {
        let content_key = [9; 32];
        let b = seal_account_backup(&content_key, vec!["room".into()], 2);
        assert_ne!(
            b.identity_key_ciphertext,
            Sha256::digest(content_key).to_vec()
        );
        assert_eq!(b.device_count, 2);
        assert_eq!(b.room_memberships, vec!["room"]);
        assert_eq!(b.identity_key_ciphertext.len(), 32);
    }

    #[test]
    fn account_backup_export_restore_persists_metadata_without_content_keys(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let export = export_account_backup(
            &[7; 32],
            vec![" ops ".into(), "ops".into(), "engineering".into()],
            2,
            "alice:laptop",
            AccountBackupRecoveryMethod::RecoveryCode,
            1_782_400_000_000,
        )?;
        let bytes = serde_json::to_vec(&export)?;
        let reloaded: AccountBackupExport = serde_json::from_slice(&bytes)?;

        assert_eq!(reloaded.metadata.version, ACCOUNT_BACKUP_EXPORT_VERSION);
        assert_eq!(reloaded.metadata.exported_by_device_id, "alice:laptop");
        assert_eq!(
            reloaded.metadata.recovery_method,
            AccountBackupRecoveryMethod::RecoveryCode
        );
        assert!(!reloaded.metadata.device_rotation_required);

        assert_eq!(
            restore_account_backup_export_json(&bytes),
            Err(RecoveryError::RecoveryCodeRequired)
        );

        let verifier = RecoveryCodeVerifier::from_code("paper-coral-falcon")?;
        assert_eq!(
            restore_recovery_code_backup_export(export.clone(), "wrong", &verifier),
            Err(RecoveryError::InvalidRecoveryCode)
        );

        let recovered =
            restore_recovery_code_backup_export_json(&bytes, " paper-coral-falcon ", &verifier)?;
        assert_eq!(
            recovered,
            AccountRecovery {
                account_access_restored: true,
                room_memberships: vec!["engineering".to_owned(), "ops".to_owned()],
                device_count: 2,
                content_keys_restored: false,
            }
        );

        let sealed_export = export_account_backup(
            &[3; 32],
            vec!["ops".into()],
            1,
            "alice:laptop",
            AccountBackupRecoveryMethod::SealedBackup,
            1_782_400_000_100,
        )?;
        assert!(restore_account_backup_export(sealed_export)?.account_access_restored);
        Ok(())
    }

    #[test]
    fn malformed_backup_restore_fails_closed_without_overwriting_existing_state(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemoryAppStore::default();
        store.save_app_state(br#"{"schema_version":2,"profile":"existing"}"#)?;

        let restore = restore_account_backup_export_json(br#"{"metadata":{"version":1}}"#);
        assert!(matches!(
            restore,
            Err(RecoveryError::MalformedBackup { .. })
                | Err(RecoveryError::InvalidBackupMetadata(_))
        ));
        assert_eq!(
            store.load_app_state()?,
            Some(br#"{"schema_version":2,"profile":"existing"}"#.to_vec())
        );

        let mut corrupt = export_account_backup(
            &[1; 32],
            vec!["room".into()],
            1,
            "alice:laptop",
            AccountBackupRecoveryMethod::SealedBackup,
            1,
        )?;
        corrupt.backup.identity_key_ciphertext.truncate(8);
        assert_eq!(
            restore_account_backup_export(corrupt),
            Err(RecoveryError::InvalidBackupMetadata(
                "identity backup material must be 32 bytes".to_owned()
            ))
        );
        assert_eq!(
            store.load_app_state()?,
            Some(br#"{"schema_version":2,"profile":"existing"}"#.to_vec())
        );
        Ok(())
    }

    #[test]
    fn author_log_stable_message_id_is_canonical() {
        let entry = AuthorLogEntry::new_stable(7, "laptop", 3, 11, b"ciphertext".to_vec());
        assert!(entry.has_stable_message_id());
        assert_eq!(
            entry.message_id,
            AuthorLogEntry::stable_message_id(7, "laptop", 3, 11, b"ciphertext")
        );

        let same = AuthorLogEntry::new_stable(7, "laptop", 3, 11, b"ciphertext".to_vec());
        let other_device = AuthorLogEntry::new_stable(7, "phone", 3, 11, b"ciphertext".to_vec());
        assert_eq!(entry.message_id, same.message_id);
        assert_ne!(entry.message_id, other_device.message_id);
    }

    #[test]
    fn author_logs_merge_across_devices_deterministically() -> Result<(), AuthorLogError> {
        let mut laptop = LocalStore::default();
        let laptop_entry = AuthorLogEntry::new_stable(1, "laptop", 1, 5, b"ciphertext-a".to_vec());
        let phone_entry = AuthorLogEntry::new_stable(1, "phone", 1, 5, b"ciphertext-b".to_vec());
        assert_eq!(
            laptop.append_sent(laptop_entry.clone())?,
            AuthorLogAppendOutcome::Inserted
        );
        let mut phone = LocalStore::default();
        phone.append_sent(phone_entry.clone())?;
        let report = laptop.merge_author_logs(phone.author_log_snapshot())?;
        assert_eq!(
            report,
            AuthorLogMergeReport {
                inserted: 1,
                duplicates: 0
            }
        );
        assert_eq!(
            laptop.merge_author_logs([phone_entry.clone()])?,
            AuthorLogMergeReport {
                inserted: 0,
                duplicates: 1
            }
        );
        assert_eq!(
            laptop.author_message_ids(),
            BTreeSet::from([
                laptop_entry.message_id.clone(),
                phone_entry.message_id.clone()
            ])
        );
        assert_eq!(
            laptop.ordered_author_message_ids(),
            vec![laptop_entry.message_id, phone_entry.message_id]
        );
        assert_eq!(laptop.next_sequence_for_device(1, "laptop"), 2);
        assert_eq!(laptop.next_sequence_for_device(1, "phone"), 2);
        Ok(())
    }

    #[test]
    fn author_log_rejects_sequence_forks_without_overwrite() -> Result<(), AuthorLogError> {
        let mut store = LocalStore::default();
        let original = AuthorLogEntry::new_stable(1, "laptop", 1, 5, b"ciphertext-a".to_vec());
        store.append_sent(original.clone())?;

        let fork = AuthorLogEntry::new_stable(1, "laptop", 1, 5, b"ciphertext-b".to_vec());
        assert_eq!(
            store.append_sent(fork),
            Err(AuthorLogError::SequenceFork {
                author_leaf: 1,
                device_id: "laptop".to_owned(),
                sequence: 1
            })
        );
        assert_eq!(store.author_log_snapshot(), vec![original]);
        Ok(())
    }

    #[test]
    fn author_log_rejects_message_id_reuse_at_different_position() -> Result<(), AuthorLogError> {
        let mut store = LocalStore::default();
        let original =
            AuthorLogEntry::new(1, "laptop", 1, 5, "stable-id", b"ciphertext-a".to_vec());
        let reused = AuthorLogEntry::new(1, "phone", 1, 5, "stable-id", b"ciphertext-b".to_vec());
        store.append_sent(original.clone())?;

        assert_eq!(
            store.merge_author_logs([reused]),
            Err(AuthorLogError::MessageIdReused {
                message_id: "stable-id".to_owned()
            })
        );
        assert_eq!(store.author_log_snapshot(), vec![original]);
        Ok(())
    }

    #[test]
    fn author_log_atomic_merge_rejects_late_conflict_without_partial_insert(
    ) -> Result<(), AuthorLogError> {
        let mut store = LocalStore::default();
        let original =
            AuthorLogEntry::new_stable(1, "device-1", 2, 5, b"original-ciphertext".to_vec());
        store.append_sent(original.clone())?;

        let earlier = AuthorLogEntry::new_stable(1, "device-1", 1, 5, b"new-ciphertext".to_vec());
        let fork =
            AuthorLogEntry::new_stable(1, "device-1", 2, 5, b"different-ciphertext".to_vec());

        assert_eq!(
            store.merge_author_logs_atomic([earlier, fork]),
            Err(AuthorLogError::SequenceFork {
                author_leaf: 1,
                device_id: "device-1".to_owned(),
                sequence: 2,
            })
        );
        assert_eq!(store.author_log_snapshot(), vec![original]);
        Ok(())
    }

    #[test]
    fn recipient_cache_evicts_oldest_entry() {
        let mut store = LocalStore::with_recipient_cache_capacity(2);
        for idx in 0..3 {
            store.cache_received(RecipientCacheEntry::new(
                format!("m-{idx}"),
                vec![idx as u8],
                KeyState::Cached([idx as u8; 32]),
                idx,
            ));
        }
        assert_eq!(store.recipient_cache().len(), 2);
        assert!(store.recipient_cache().get("m-0").is_none());
        assert!(store.recipient_cache().get("m-2").is_some());
    }

    #[test]
    fn crypto_shred_tombstone_blocks_cached_decrypt_without_deleting_ciphertext() {
        let key = [0xA5; 32];
        let plaintext = b"message plaintext";
        let ciphertext = xor_harness_ciphertext(plaintext, &key);
        let mut store = LocalStore::default();
        store.cache_received(RecipientCacheEntry::new(
            "m-shred",
            ciphertext.clone(),
            KeyState::Cached(key),
            1,
        ));

        assert_eq!(
            store.decrypt_cached_message_for_harness("m-shred"),
            Ok(plaintext.to_vec())
        );
        store.cooperative_shred_message("m-shred");

        assert_eq!(
            store.decrypt_cached_message_for_harness("m-shred"),
            Err(LocalDecryptError::Shredded("m-shred".to_owned()))
        );
        assert_eq!(
            store
                .recipient_cache()
                .get("m-shred")
                .map(|entry| &entry.ciphertext),
            Some(&ciphertext)
        );
        assert_eq!(store.message_tombstones(), vec!["m-shred".to_owned()]);
    }

    #[test]
    fn crypto_shred_tombstones_merge_and_apply_to_late_cache_entries() {
        let mut store = LocalStore::default();
        store.merge_message_tombstones(["m1", "m2", "m1"]);
        store.cache_received(RecipientCacheEntry::new(
            "m1",
            b"ciphertext".to_vec(),
            KeyState::Cached([9; 32]),
            1,
        ));

        assert_eq!(
            store.message_tombstones(),
            vec!["m1".to_owned(), "m2".to_owned()]
        );
        assert_eq!(
            store.decrypt_cached_message_for_harness("m1"),
            Err(LocalDecryptError::Shredded("m1".to_owned()))
        );
        assert_eq!(
            store
                .recipient_cache()
                .get("m1")
                .map(|entry| &entry.key_state),
            Some(&KeyState::Shredded)
        );
    }

    #[test]
    fn crypto_shred_tombstones_survive_app_state_restart_bytes() -> Result<(), AppStoreError> {
        let mut state = AppState::new(AppIdentityState::new(
            "alice",
            "Alice",
            "friend:alice",
            "device-a",
            "1111",
        ));
        state.messages.push(AppMessageState {
            message_id: "m1".to_owned(),
            group_id: "g1".to_owned(),
            channel_id: "c1".to_owned(),
            author_id: "alice-device".to_owned(),
            sequence: 1,
            epoch: 7,
            ciphertext: b"ciphertext-only".to_vec(),
            sent_at_ms: 1000,
        });
        state.record_message_tombstone(AppMessageTombstoneState::new(
            "m1",
            2000,
            "cooperative-shred",
        ));

        let mut store = MemoryAppStore::default();
        store.save_app_state(&serde_json::to_vec(&state)?)?;
        let bytes = store
            .load_app_state()?
            .ok_or(AppStoreError::Crypto("missing app state after save"))?;
        let reloaded: AppState = serde_json::from_slice(&bytes)?;

        assert!(reloaded.has_message_tombstone("m1"));
        assert_eq!(reloaded.message_tombstone_ids(), vec!["m1".to_owned()]);
        assert_eq!(reloaded.messages, state.messages);
        Ok(())
    }

    #[test]
    fn crypto_shred_scan_removes_key_material_from_db_wal_and_key_store() {
        let key_material = b"content-key-material";
        let mut sim = SecureDeleteSimulator::default();
        sim.write("app-state.discrypt", b"ciphertext envelope only".to_vec());
        sim.write("app-state.discrypt-wal", Vec::new());
        sim.write("content-key.store", key_material.to_vec());
        assert!(sim.contains_material(key_material));

        sim.secure_delete(["content-key.store", "app-state.discrypt-wal"]);

        assert!(!sim.contains_material(key_material));
        assert!(sim.deleted_all(["content-key.store", "app-state.discrypt-wal"]));
    }

    #[test]
    fn secure_delete_removes_material_and_snapshot_restores_on_failed_verify() {
        let mut sim = SecureDeleteSimulator::default();
        sim.write("db.sqlite", b"content-key".to_vec());
        sim.write("db.sqlite-wal", b"content-key wal".to_vec());
        sim.write("key.store", b"content-key store".to_vec());
        let snapshot = sim.snapshot();
        sim.secure_delete(["db.sqlite", "db.sqlite-wal"]);
        assert!(sim.contains_material(b"content-key"));
        sim.restore(snapshot);
        assert!(sim.contains_material(b"content-key"));
        sim.secure_delete(["db.sqlite", "db.sqlite-wal", "key.store"]);
        assert!(!sim.contains_material(b"content-key"));
        assert!(sim.deleted_all(["db.sqlite", "db.sqlite-wal", "key.store"]));
    }

    #[test]
    fn recovery_code_requires_verifier_and_never_restores_content_keys() -> Result<(), RecoveryError>
    {
        assert_eq!(
            RecoveryCodeVerifier::from_code("   "),
            Err(RecoveryError::EmptyRecoveryCode)
        );
        let verifier = RecoveryCodeVerifier::from_code(" paper-coral-falcon ")?;
        assert!(verifier.verifies("paper-coral-falcon"));
        assert!(verifier.verifies("  paper-coral-falcon  "));
        assert!(!verifier.verifies("paper-coral-falcon-wrong"));
        assert_eq!(
            recovery_code_material("wrong", &verifier, vec!["room".into()], 2),
            Err(RecoveryError::InvalidRecoveryCode)
        );

        let material = recovery_code_material(
            "paper-coral-falcon",
            &verifier,
            vec!["room".into(), "room".into(), " ".into()],
            3,
        )?;
        let recovered = recover_account(material)?;
        assert_eq!(
            recovered,
            AccountRecovery {
                account_access_restored: true,
                room_memberships: vec!["room".to_owned()],
                device_count: 3,
                content_keys_restored: false,
            }
        );
        Ok(())
    }

    #[test]
    fn lost_password_without_trust_material_is_not_recoverable() -> Result<(), RecoveryError> {
        assert_eq!(
            lost_password_recovery_material(None, None, vec!["room".into()], 1),
            Err(RecoveryError::NoTrustMaterial)
        );

        let verifier = RecoveryCodeVerifier::from_code("paper-coral-falcon")?;
        assert_eq!(
            lost_password_recovery_material(Some("wrong"), Some(&verifier), vec!["room".into()], 1),
            Err(RecoveryError::InvalidRecoveryCode)
        );

        let material = lost_password_recovery_material(
            Some(" paper-coral-falcon "),
            Some(&verifier),
            vec!["room".into()],
            1,
        )?;
        let recovered = recover_account(material)?;
        assert!(recovered.account_access_restored);
        assert!(!recovered.content_keys_restored);
        Ok(())
    }

    #[test]
    fn compromised_device_backup_requires_rotation_before_restore() -> Result<(), RecoveryError> {
        let export = export_device_compromise_backup(
            &[4; 32],
            vec!["room".into()],
            2,
            "alice:laptop",
            "alice:phone",
            1_782_400_001_000,
        )?;

        assert_eq!(
            restore_account_backup_export(export.clone()),
            Err(RecoveryError::DeviceRotationRequired {
                compromised_device_id: "alice:phone".to_owned()
            })
        );
        assert_eq!(
            restore_account_backup_after_device_rotation(export.clone(), "alice:phone"),
            Err(RecoveryError::InvalidBackupMetadata(
                "replacement device must differ from compromised device".to_owned()
            ))
        );

        let recovered = restore_account_backup_after_device_rotation(export, "alice:tablet")?;
        assert_eq!(
            recovered,
            AccountRecovery {
                account_access_restored: true,
                room_memberships: vec!["room".to_owned()],
                device_count: 2,
                content_keys_restored: false,
            }
        );
        Ok(())
    }

    #[test]
    fn recovery_requires_material_and_never_restores_content_keys() {
        assert_eq!(
            recover_account(RecoveryMaterial::None),
            Err(RecoveryError::NoTrustMaterial)
        );
        let backup = seal_account_backup(&[1; 32], vec!["room".into()], 2);
        let recovered = recover_account(RecoveryMaterial::SealedBackup(backup));
        assert!(matches!(
            recovered,
            Ok(AccountRecovery {
                account_access_restored: true,
                room_memberships,
                device_count: 2,
                content_keys_restored: false,
            }) if room_memberships == vec!["room".to_owned()]
        ));
    }
}

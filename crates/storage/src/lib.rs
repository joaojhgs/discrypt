//! Local-only storage facades for author logs, recipient caches, and sealed backup.
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
    sqlite_wal_path, storage_keychain_decision, AppDbKeychain, EncryptedAppDb,
    StorageKeychainDecision,
};
pub use content_keys::KeyState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use zeroize::Zeroize;

const SEALED_ACCOUNT_BACKUP_DOMAIN: &[u8] = b"discrypt:v1:sealed-account-backup";
const RECOVERY_CODE_DOMAIN: &[u8] = b"discrypt:v1:account-recovery-code";

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

/// Byte-oriented local app-state store used by the core AppService.
///
/// The core crate owns the typed schema; storage owns durable byte persistence so
/// migrations can be tested without coupling UI state to React fixtures.
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
    /// Per-author sequence.
    pub sequence: u64,
    /// Stable message id.
    pub message_id: String,
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

    /// Key used for deterministic merge/dedupe.
    #[must_use]
    pub fn key(&self) -> AuthorLogKey {
        AuthorLogKey {
            author_leaf: self.author_leaf,
            sequence: self.sequence,
            message_id: self.message_id.clone(),
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
}

impl LocalStore {
    /// Create a store with a bounded recipient cache.
    #[must_use]
    pub fn with_recipient_cache_capacity(capacity: usize) -> Self {
        Self {
            author_log: BTreeMap::new(),
            recipient_cache: BoundedRecipientCache::new(capacity),
        }
    }

    /// Append an authoritative sent message for its author.
    pub fn append_sent(&mut self, entry: AuthorLogEntry) {
        self.author_log.insert(entry.key(), entry);
    }

    /// Merge received author-log entries from another own device or gossip peer.
    pub fn merge_author_logs<I>(&mut self, entries: I) -> usize
    where
        I: IntoIterator<Item = AuthorLogEntry>,
    {
        let mut inserted = 0;
        for entry in entries {
            let key = entry.key();
            if self.author_log.insert(key, entry).is_none() {
                inserted += 1;
            }
        }
        inserted
    }

    /// Ordered author-log snapshot.
    #[must_use]
    pub fn author_log_snapshot(&self) -> Vec<AuthorLogEntry> {
        self.author_log.values().cloned().collect()
    }

    /// Ordered message ids in the author log.
    #[must_use]
    pub fn author_message_ids(&self) -> BTreeSet<String> {
        self.author_log
            .values()
            .map(|entry| entry.message_id.clone())
            .collect()
    }

    /// Cache a received message under the bounded retention cache.
    pub fn cache_received(&mut self, entry: RecipientCacheEntry) {
        self.recipient_cache.insert(entry);
    }

    /// Recipient cache reference.
    #[must_use]
    pub fn recipient_cache(&self) -> &BoundedRecipientCache {
        &self.recipient_cache
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
}

impl Default for AppPreferencesState {
    fn default() -> Self {
        Self {
            theme: "system".to_owned(),
            active_template: "command-center".to_owned(),
            retention_preset: "7 days".to_owned(),
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
    /// Current MLS epoch facade.
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
            invites: Vec::new(),
            voice_sessions: Vec::new(),
        }
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
    fn author_logs_merge_across_devices_deterministically() {
        let mut laptop = LocalStore::default();
        laptop.append_sent(AuthorLogEntry::new(
            1,
            "laptop",
            1,
            5,
            "a-1",
            b"ciphertext-a".to_vec(),
        ));
        let mut phone = LocalStore::default();
        phone.append_sent(AuthorLogEntry::new(
            1,
            "phone",
            2,
            5,
            "a-2",
            b"ciphertext-b".to_vec(),
        ));
        let inserted = laptop.merge_author_logs(phone.author_log_snapshot());
        assert_eq!(inserted, 1);
        assert_eq!(
            laptop.author_message_ids(),
            BTreeSet::from(["a-1".to_owned(), "a-2".to_owned()])
        );
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

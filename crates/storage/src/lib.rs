//! Local-only storage facades for author logs, recipient caches, and sealed backup.
pub use content_keys::KeyState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use zeroize::Zeroize;

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

/// Seal account-continuity data without content keys.
#[must_use]
pub fn seal_account_backup(
    key: &[u8; 32],
    rooms: Vec<String>,
    device_count: usize,
) -> AccountBackup {
    let mut material = *key;
    let digest = Sha256::digest(material);
    material.zeroize();
    AccountBackup {
        identity_key_ciphertext: digest.to_vec(),
        room_memberships: rooms,
        device_count,
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
    fn backup_excludes_content_keys() {
        let b = seal_account_backup(&[9; 32], vec!["room".into()], 2);
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
}

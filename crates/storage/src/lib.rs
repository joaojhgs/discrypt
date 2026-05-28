//! Local-only storage facades for author logs, recipient caches, and sealed backup.
use content_keys::KeyState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use zeroize::Zeroize;

/// Authoritative sent-log entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthorLogEntry {
    pub author_leaf: u32,
    pub message_id: String,
    pub ciphertext: Vec<u8>,
}

/// Bounded recipient cache entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecipientCacheEntry {
    pub message_id: String,
    pub ciphertext: Vec<u8>,
    pub key_state: KeyState,
}

/// In-memory phase-0 store.
#[derive(Default)]
pub struct LocalStore {
    pub author_log: Vec<AuthorLogEntry>,
    pub recipient_cache: BTreeMap<String, RecipientCacheEntry>,
}
impl LocalStore {
    pub fn append_sent(&mut self, entry: AuthorLogEntry) {
        self.author_log.push(entry);
    }
    pub fn cache_received(&mut self, entry: RecipientCacheEntry) {
        self.recipient_cache.insert(entry.message_id.clone(), entry);
    }
}

/// Sealed account-continuity backup (no content keys).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountBackup {
    pub identity_key_ciphertext: Vec<u8>,
    pub room_memberships: Vec<String>,
    pub device_count: usize,
}
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
}

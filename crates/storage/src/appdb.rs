//! Encrypted local application database facade.
//!
//! The app DB stores an encrypted envelope on disk and keeps the wrapping key
//! behind a keychain trait. The persisted file contains only a wrapped data key,
//! nonces, and ciphertext; callers that need typed state continue to use the
//! byte-oriented [`crate::AppStore`] boundary.

use crate::{AppStore, AppStoreError};
use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use zeroize::Zeroize;

const ENVELOPE_FORMAT: &str = "discrypt.appdb.encrypted.v1";
const DEFAULT_WRAPPING_KEY_ID: &str = "local-appdb-wrapping-key-v1";

/// Local keychain boundary used by the encrypted app DB.
///
/// Implementations should store the wrapping key in an OS/platform keychain or
/// equivalent local-only secret store. The app DB persists only the data key
/// wrapped by this key.
pub trait AppDbKeychain: Clone + Send + Sync + 'static {
    /// Load a wrapping key by stable key id.
    fn load_wrapping_key(&mut self, key_id: &str) -> Result<Option<[u8; 32]>, AppStoreError>;

    /// Store a wrapping key by stable key id.
    fn store_wrapping_key(&mut self, key_id: &str, key: [u8; 32]) -> Result<(), AppStoreError>;

    /// Delete a wrapping key by stable key id.
    fn delete_wrapping_key(&mut self, key_id: &str) -> Result<(), AppStoreError>;
}

/// In-memory keychain adapter for tests and harnesses.
#[derive(Clone, Debug, Default)]
pub struct MemoryAppDbKeychain {
    keys: Arc<Mutex<BTreeMap<String, [u8; 32]>>>,
}

impl MemoryAppDbKeychain {
    /// Insert or replace a wrapping key. Intended for deterministic tests.
    pub fn insert_wrapping_key(
        &mut self,
        key_id: impl Into<String>,
        key: [u8; 32],
    ) -> Result<(), AppStoreError> {
        self.keys
            .lock()
            .map_err(|_| AppStoreError::LockPoisoned)?
            .insert(key_id.into(), key);
        Ok(())
    }

    /// Return a key snapshot for tests that need to inspect keychain contents.
    pub fn snapshot_wrapping_keys(&self) -> Result<BTreeMap<String, [u8; 32]>, AppStoreError> {
        self.keys
            .lock()
            .map_err(|_| AppStoreError::LockPoisoned)
            .map(|guard| guard.clone())
    }
}

impl AppDbKeychain for MemoryAppDbKeychain {
    fn load_wrapping_key(&mut self, key_id: &str) -> Result<Option<[u8; 32]>, AppStoreError> {
        self.keys
            .lock()
            .map_err(|_| AppStoreError::LockPoisoned)
            .map(|guard| guard.get(key_id).copied())
    }

    fn store_wrapping_key(&mut self, key_id: &str, key: [u8; 32]) -> Result<(), AppStoreError> {
        self.keys
            .lock()
            .map_err(|_| AppStoreError::LockPoisoned)?
            .insert(key_id.to_owned(), key);
        Ok(())
    }

    fn delete_wrapping_key(&mut self, key_id: &str) -> Result<(), AppStoreError> {
        self.keys
            .lock()
            .map_err(|_| AppStoreError::LockPoisoned)?
            .remove(key_id);
        Ok(())
    }
}

/// File-backed encrypted application DB.
#[derive(Clone, Debug)]
pub struct EncryptedAppDb<K> {
    path: PathBuf,
    keychain: K,
    key_id: String,
}

impl<K> EncryptedAppDb<K>
where
    K: AppDbKeychain,
{
    /// Create an encrypted app DB using the default wrapping-key id.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, keychain: K) -> Self {
        Self::with_key_id(path, keychain, DEFAULT_WRAPPING_KEY_ID)
    }

    /// Create an encrypted app DB using an explicit wrapping-key id.
    #[must_use]
    pub fn with_key_id(path: impl Into<PathBuf>, keychain: K, key_id: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            keychain,
            key_id: key_id.into(),
        }
    }

    /// Return the encrypted envelope path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the conventional SQLite WAL sidecar path for leakage checks.
    #[must_use]
    pub fn wal_path(&self) -> PathBuf {
        sqlite_wal_path(&self.path)
    }

    fn load_wrapping_key(&mut self, key_id: &str) -> Result<[u8; 32], AppStoreError> {
        self.keychain
            .load_wrapping_key(key_id)?
            .ok_or_else(|| AppStoreError::KeychainMissing(key_id.to_owned()))
    }

    fn load_or_create_wrapping_key(&mut self) -> Result<[u8; 32], AppStoreError> {
        if let Some(key) = self.keychain.load_wrapping_key(&self.key_id)? {
            return Ok(key);
        }
        let key = random_key();
        self.keychain.store_wrapping_key(&self.key_id, key)?;
        Ok(key)
    }
}

impl<K> AppStore for EncryptedAppDb<K>
where
    K: AppDbKeychain,
{
    fn load_app_state(&mut self) -> Result<Option<Vec<u8>>, AppStoreError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(AppStoreError::Io(error)),
        };
        let envelope: EncryptedAppDbEnvelope = serde_json::from_slice(&bytes)?;
        envelope.validate()?;
        let wrapping_key = self.load_wrapping_key(&envelope.key_id)?;
        let mut data_key = decrypt_bytes(
            &wrapping_key,
            &envelope.wrapped_key_nonce,
            &envelope.wrapped_data_key,
        )?;
        let plaintext = decrypt_bytes(&data_key, &envelope.data_nonce, &envelope.ciphertext)?;
        data_key.zeroize();
        Ok(Some(plaintext))
    }

    fn save_app_state(&mut self, bytes: &[u8]) -> Result<(), AppStoreError> {
        let wrapping_key = self.load_or_create_wrapping_key()?;
        let mut data_key = random_key();
        let wrapped_key_nonce = random_nonce();
        let data_nonce = random_nonce();
        let wrapped_data_key = encrypt_bytes(&wrapping_key, &wrapped_key_nonce, &data_key)?;
        let ciphertext = encrypt_bytes(&data_key, &data_nonce, bytes)?;
        data_key.zeroize();

        let envelope = EncryptedAppDbEnvelope {
            format: ENVELOPE_FORMAT.to_owned(),
            key_id: self.key_id.clone(),
            wrapped_key_nonce,
            wrapped_data_key,
            data_nonce,
            ciphertext,
        };
        let serialized = serde_json::to_vec(&envelope)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serialized)?;
        fs::rename(tmp, &self.path)?;
        Ok(())
    }
}

/// Conventional WAL path next to a SQLite-style app DB file.
#[must_use]
pub fn sqlite_wal_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push("-wal");
    path.with_file_name(name)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct EncryptedAppDbEnvelope {
    format: String,
    key_id: String,
    wrapped_key_nonce: [u8; 12],
    wrapped_data_key: Vec<u8>,
    data_nonce: [u8; 12],
    ciphertext: Vec<u8>,
}

impl EncryptedAppDbEnvelope {
    fn validate(&self) -> Result<(), AppStoreError> {
        if self.format != ENVELOPE_FORMAT {
            return Err(AppStoreError::Crypto(
                "unsupported encrypted app db envelope",
            ));
        }
        if self.key_id.is_empty() {
            return Err(AppStoreError::Crypto(
                "encrypted app db envelope has no key id",
            ));
        }
        Ok(())
    }
}

fn random_key() -> [u8; 32] {
    let mut key = [0_u8; 32];
    OsRng.fill_bytes(&mut key);
    key
}

fn random_nonce() -> [u8; 12] {
    let mut nonce = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

fn encrypt_bytes(key: &[u8], nonce: &[u8; 12], plaintext: &[u8]) -> Result<Vec<u8>, AppStoreError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| AppStoreError::Crypto("invalid encryption key"))?;
    cipher
        .encrypt(Nonce::from_slice(nonce), plaintext)
        .map_err(|_| AppStoreError::Crypto("app db encryption failed"))
}

fn decrypt_bytes(
    key: &[u8],
    nonce: &[u8; 12],
    ciphertext: &[u8],
) -> Result<Vec<u8>, AppStoreError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| AppStoreError::Crypto("invalid encryption key"))?;
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| AppStoreError::Crypto("app db decryption failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "discrypt-{name}-{}-{}.sqlite",
            std::process::id(),
            random_nonce()[0]
        ))
    }

    fn path_contains(path: &Path, needle: &[u8]) -> bool {
        !needle.is_empty()
            && fs::read(path)
                .map(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
                .unwrap_or(false)
    }

    #[test]
    fn encrypted_app_db_round_trips_without_plaintext_in_db_or_wal() -> Result<(), AppStoreError> {
        let path = temp_db_path("roundtrip");
        let wal_path = sqlite_wal_path(&path);
        let tmp_path = path.with_extension("json.tmp");
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&wal_path);
        let _ = fs::remove_file(&tmp_path);

        let keychain = MemoryAppDbKeychain::default();
        let mut db = EncryptedAppDb::new(&path, keychain.clone());
        let sensitive = br#"{"friend_code":"alice-friend-code","safety_number":"safety-secret","body":"hello plaintext body","content_key":"content-key-bytes"}"#;
        db.save_app_state(sensitive)?;

        assert_eq!(db.load_app_state()?, Some(sensitive.to_vec()));
        for needle in [
            b"alice-friend-code".as_slice(),
            b"safety-secret".as_slice(),
            b"hello plaintext body".as_slice(),
            b"content-key-bytes".as_slice(),
        ] {
            assert!(!path_contains(&path, needle));
            assert!(!path_contains(&wal_path, needle));
            assert!(!path_contains(&tmp_path, needle));
        }

        let key_snapshot = keychain.snapshot_wrapping_keys()?;
        assert_eq!(key_snapshot.len(), 1);
        assert!(key_snapshot
            .values()
            .all(|key| !sensitive.windows(key.len()).any(|window| window == key)));

        let _ = fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn encrypted_app_db_persists_wrapped_key_separately_from_keychain() -> Result<(), AppStoreError>
    {
        let path = temp_db_path("wrapped-key");
        let _ = fs::remove_file(&path);

        let mut db = EncryptedAppDb::new(&path, MemoryAppDbKeychain::default());
        db.save_app_state(br#"{"display_name":"Alice"}"#)?;

        let envelope_bytes = fs::read(&path)?;
        let envelope: EncryptedAppDbEnvelope = serde_json::from_slice(&envelope_bytes)?;
        assert_eq!(envelope.format, ENVELOPE_FORMAT);
        assert_eq!(envelope.key_id, DEFAULT_WRAPPING_KEY_ID);
        assert!(!envelope.wrapped_data_key.is_empty());
        assert_ne!(envelope.wrapped_data_key, envelope.ciphertext);

        let mut missing_keychain_db = EncryptedAppDb::new(&path, MemoryAppDbKeychain::default());
        assert!(matches!(
            missing_keychain_db.load_app_state(),
            Err(AppStoreError::KeychainMissing(key_id)) if key_id == DEFAULT_WRAPPING_KEY_ID
        ));

        let _ = fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn encrypted_app_db_rejects_wrong_wrapping_key() -> Result<(), AppStoreError> {
        let path = temp_db_path("wrong-key");
        let _ = fs::remove_file(&path);

        let mut db = EncryptedAppDb::new(&path, MemoryAppDbKeychain::default());
        db.save_app_state(br#"{"device_id":"device-secret"}"#)?;

        let mut wrong_keychain = MemoryAppDbKeychain::default();
        wrong_keychain.insert_wrapping_key(DEFAULT_WRAPPING_KEY_ID, [7; 32])?;
        let mut wrong_key_db = EncryptedAppDb::new(&path, wrong_keychain);
        assert!(matches!(
            wrong_key_db.load_app_state(),
            Err(AppStoreError::Crypto("app db decryption failed"))
        ));

        let _ = fs::remove_file(path);
        Ok(())
    }
}

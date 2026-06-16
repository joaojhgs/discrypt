//! Encrypted local application database boundary.
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
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(any(
    test,
    feature = "harness",
    feature = "local-dev",
    not(feature = "production-storage")
))]
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;
#[cfg(all(target_os = "linux", feature = "production-storage"))]
use zeroize::Zeroizing;

const ENVELOPE_FORMAT: &str = "discrypt.appdb.encrypted.v1";
const DEFAULT_WRAPPING_KEY_ID: &str = "local-appdb-wrapping-key-v1";

/// Launch storage/keychain decision locked by ADR-006.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StorageKeychainDecision {
    /// Runtime storage format for app-level state.
    pub app_store_runtime: &'static str,
    /// SQLite compatibility boundary and schema policy.
    pub sqlite_schema_policy: &'static str,
    /// Crates that implement the encrypted store envelope.
    pub encrypted_store_crates: &'static [&'static str],
    /// Crate and provider selected for production keychain integration.
    pub keychain_crate: &'static str,
    /// WAL, SHM, journal, and temp-file handling policy.
    pub wal_journal_policy: &'static str,
    /// Data-key wrapping and envelope policy.
    pub key_wrapping: &'static str,
    /// Versioned schema migration policy.
    pub schema_migrations: &'static str,
    /// Explicit secure-delete limitation statement.
    pub secure_delete_limits: &'static str,
    /// Platform-specific storage differences and launch gates.
    pub platform_differences: &'static [&'static str],
}

impl StorageKeychainDecision {
    /// True when this decision covers every ADR-006 launch axis.
    #[must_use]
    pub fn covers_adr_006(&self) -> bool {
        self.app_store_runtime.contains("EncryptedAppDb")
            && self.app_store_runtime.contains("AES-256-GCM")
            && self
                .app_store_runtime
                .contains("not persist plaintext SQLite pages")
            && self.sqlite_schema_policy.contains("AppDbSchema")
            && self.sqlite_schema_policy.contains("VERSION_1_DDL")
            && self.sqlite_schema_policy.contains("openmls_sqlite_storage")
            && self.encrypted_store_crates.contains(&"aes-gcm")
            && self.encrypted_store_crates.contains(&"serde_json")
            && self.encrypted_store_crates.contains(&"zeroize")
            && self.keychain_crate.contains("keyring 3.6.3")
            && self.keychain_crate.contains("LinuxOsKeychain")
            && self.wal_journal_policy.contains("sqlite_wal_path")
            && self.wal_journal_policy.contains("quarantine_corrupt_store")
            && self.key_wrapping.contains("wrapped_data_key")
            && self.key_wrapping.contains("data key zeroized")
            && self.schema_migrations.contains("AppDbMigrationPlan")
            && self.schema_migrations.contains("validate_observed_schema")
            && self.secure_delete_limits.contains("best-effort")
            && self.secure_delete_limits.contains("SSD")
            && self.secure_delete_limits.contains("cloud snapshot")
            && self
                .platform_differences
                .iter()
                .any(|platform| platform.contains("Linux") && platform.contains("Secret Service"))
            && self.platform_differences.iter().any(|platform| {
                platform.contains("MemoryAppDbKeychain") && platform.contains("non-production")
            })
    }
}

/// Return the ADR-006 storage/keychain launch decision.
#[must_use]
pub const fn storage_keychain_decision() -> StorageKeychainDecision {
    StorageKeychainDecision {
        app_store_runtime: "EncryptedAppDb persists a serde_json envelope encrypted with AES-256-GCM; app DB does not persist plaintext SQLite pages.",
        sqlite_schema_policy: "AppDbSchema and VERSION_1_DDL define the SQLite-compatible durable schema contract; OpenMLS protocol state uses openmls_sqlite_storage separately.",
        encrypted_store_crates: &["aes-gcm", "serde_json", "zeroize", "sha2", "hex"],
        keychain_crate: "keyring 3.6.3 is optional behind production-storage; LinuxOsKeychain uses the default Secret Service sync provider; ProductionAppDbKeychain may use an Argon2id/AES-GCM PassphraseVaultKeychain when the user chooses password-vault storage or an explicit operator passphrase is configured; MemoryAppDbKeychain is restricted to tests/local/non-production builds.",
        wal_journal_policy: "Encrypted envelope writes use temp-file plus rename; sqlite_wal_path, -shm, and -journal sidecars are leakage-checked or moved by quarantine_corrupt_store; no plaintext WAL is expected for EncryptedAppDb.",
        key_wrapping: "A random data key encrypts payload bytes; the AppDbKeychain wrapping key wraps that data key into wrapped_data_key with nonces in the envelope; the data key zeroized after wrapping/decryption.",
        schema_migrations: "AppDbMigrationPlan supports 0<->1 forward/backward/noop transitions, rejects future versions, and validate_observed_schema checks tables and columns before opening state.",
        secure_delete_limits: "Secure delete is best-effort enumeration plus verification for local files/keychain entries and cannot promise erasure from SSD wear-leveling, backups, or cloud snapshot copies.",
        platform_differences: &[
            "Linux production-storage uses keyring default Secret Service through LinuxOsKeychain when keyring mode is selected, or a user-unlocked PassphraseVaultKeychain when password-vault mode is selected.",
            "Tests, harnesses, local-dev, and non-production builds may use MemoryAppDbKeychain only as a non-production boundary.",
            "macOS, Windows, Android, and iOS require platform keychain adapters before any production-storage claim.",
        ],
    }
}

/// Local keychain boundary used by the encrypted app DB.
///
/// Implementations should store the wrapping key in an OS/platform keychain or
/// equivalent local-device secret store. The app DB persists only the data key
/// wrapped by this key.
pub trait AppDbKeychain: Clone + Send + Sync + 'static {
    /// Load a wrapping key by stable key id.
    fn load_wrapping_key(&mut self, key_id: &str) -> Result<Option<[u8; 32]>, AppStoreError>;

    /// Store a wrapping key by stable key id.
    fn store_wrapping_key(&mut self, key_id: &str, key: [u8; 32]) -> Result<(), AppStoreError>;

    /// Delete a wrapping key by stable key id.
    fn delete_wrapping_key(&mut self, key_id: &str) -> Result<(), AppStoreError>;
}

/// Prove that a keychain can persist and reload a Discrypt app DB wrapping key.
///
/// Production setup uses this before claiming OS-keyring storage is usable. The
/// check is intentionally stronger than service discovery: it writes a 32-byte
/// probe key, reloads and byte-compares it, deletes it, then verifies the probe
/// key is gone.
pub fn preflight_app_db_keychain<K: AppDbKeychain>(
    keychain: &mut K,
    key_id: &str,
    test_key: [u8; 32],
) -> Result<(), AppStoreError> {
    keychain.store_wrapping_key(key_id, test_key)?;
    match keychain.load_wrapping_key(key_id) {
        Err(error) => {
            let _ = keychain.delete_wrapping_key(key_id);
            return Err(error);
        }
        Ok(Some(loaded)) if loaded == test_key => {}
        Ok(Some(_)) => {
            let _ = keychain.delete_wrapping_key(key_id);
            return Err(AppStoreError::Crypto(
                "OS keyring preflight returned a different wrapping key",
            ));
        }
        Ok(None) => {
            let _ = keychain.delete_wrapping_key(key_id);
            return Err(AppStoreError::KeychainMissing(key_id.to_owned()));
        }
    }
    keychain.delete_wrapping_key(key_id)?;
    match keychain.load_wrapping_key(key_id)? {
        None => Ok(()),
        Some(_) => Err(AppStoreError::Crypto(
            "OS keyring preflight probe key remained after delete",
        )),
    }
}

/// Local deterministic keychains are excluded from production-storage-only builds.
#[cfg(any(
    test,
    feature = "harness",
    feature = "local-dev",
    not(feature = "production-storage")
))]
/// In-memory keychain adapter for tests and harnesses.
#[derive(Clone, Debug, Default)]
pub struct MemoryAppDbKeychain {
    keys: Arc<Mutex<BTreeMap<String, [u8; 32]>>>,
}

#[cfg(any(
    test,
    feature = "harness",
    feature = "local-dev",
    not(feature = "production-storage")
))]
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

#[cfg(any(
    test,
    feature = "harness",
    feature = "local-dev",
    not(feature = "production-storage")
))]
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

/// Production Linux keychain backed by the desktop Secret Service/keyring provider.
#[cfg(all(target_os = "linux", feature = "production-storage"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinuxOsKeychain {
    service: String,
    legacy_target: String,
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
impl LinuxOsKeychain {
    /// Create a Linux platform keychain adapter for Discrypt app DB wrapping keys.
    #[must_use]
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            legacy_target: "discrypt".to_owned(),
        }
    }

    /// Create the default Discrypt app DB keychain adapter.
    #[must_use]
    pub fn discrypt_app_db() -> Self {
        Self::new("discrypt.appdb")
    }

    /// Service namespace used for OS keychain entries.
    #[must_use]
    pub fn service(&self) -> &str {
        &self.service
    }

    /// Production Linux now uses the desktop default Secret Service collection.
    /// A separate legacy target is retained only to read/delete keys from an
    /// earlier GNOME-specific Discrypt collection build.
    #[must_use]
    pub fn target(&self) -> &str {
        "default"
    }

    /// Legacy custom Secret Service collection target used by earlier Linux builds.
    #[must_use]
    pub fn legacy_target(&self) -> &str {
        &self.legacy_target
    }

    fn entry(&self, key_id: &str) -> Result<keyring::Entry, AppStoreError> {
        keyring::Entry::new(&self.service, key_id).map_err(keyring_error)
    }

    fn legacy_target_entry(&self, key_id: &str) -> Result<keyring::Entry, AppStoreError> {
        keyring::Entry::new_with_target(&self.legacy_target, &self.service, key_id)
            .map_err(keyring_error)
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
impl Default for LinuxOsKeychain {
    fn default() -> Self {
        Self::discrypt_app_db()
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
impl AppDbKeychain for LinuxOsKeychain {
    fn load_wrapping_key(&mut self, key_id: &str) -> Result<Option<[u8; 32]>, AppStoreError> {
        let entry = self.entry(key_id)?;
        match entry.get_secret() {
            Ok(secret) => return secret_to_wrapping_key(secret),
            Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(keyring_error(error)),
        }

        // Backward-compatible read for the temporary build that wrote to a
        // GNOME-specific custom collection. KDE/KWallet implementations may not
        // support that path, so production writes now use the default collection.
        let legacy_entry = match self.legacy_target_entry(key_id) {
            Ok(entry) => entry,
            Err(_) => return Ok(None),
        };
        match legacy_entry.get_secret() {
            Ok(secret) => secret_to_wrapping_key(secret),
            Err(_) => Ok(None),
        }
    }

    fn store_wrapping_key(&mut self, key_id: &str, key: [u8; 32]) -> Result<(), AppStoreError> {
        // Store text instead of arbitrary binary. Some Secret Service/KWallet
        // stacks round-trip byte secrets inconsistently through their prompt
        // bridges; hex keeps the OS-keyring value portable while still storing
        // only key material in the keyring, never in the app-state envelope.
        self.entry(key_id)?
            .set_secret(hex::encode(key).as_bytes())
            .map_err(keyring_error)
    }

    fn delete_wrapping_key(&mut self, key_id: &str) -> Result<(), AppStoreError> {
        let entry = self.entry(key_id)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(keyring_error(error)),
        }

        let Ok(legacy_entry) = self.legacy_target_entry(key_id) else {
            return Ok(());
        };
        legacy_delete_result(legacy_entry.delete_credential())
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn secret_to_wrapping_key(mut secret: Vec<u8>) -> Result<Option<[u8; 32]>, AppStoreError> {
    let mut key = [0_u8; 32];
    if secret.len() == 32 {
        key.copy_from_slice(&secret);
        secret.zeroize();
        return Ok(Some(key));
    }
    if secret.len() == 64 {
        let decoded = std::str::from_utf8(&secret)
            .ok()
            .and_then(|value| hex::decode(value).ok())
            .and_then(|bytes| bytes.try_into().ok());
        secret.zeroize();
        if let Some(decoded) = decoded {
            return Ok(Some(decoded));
        }
        return Err(AppStoreError::Crypto(
            "invalid OS keychain wrapping key encoding",
        ));
    }
    secret.zeroize();
    Err(AppStoreError::Crypto(
        "invalid OS keychain wrapping key length",
    ))
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn keyring_error(error: keyring::Error) -> AppStoreError {
    AppStoreError::Keychain(error.to_string())
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn legacy_delete_result(result: Result<(), keyring::Error>) -> Result<(), AppStoreError> {
    match result {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(keyring_error(error)),
    }
}

/// Production-only encrypted vault fallback for app DB wrapping keys.
///
/// The vault exists only behind `production-storage` and is never used by
/// local-dev/harness stores. It stores wrapping keys encrypted by a key derived
/// from an explicit user/device passphrase; it is an escape hatch when the
/// platform keychain is unavailable, not a plaintext fallback.
#[cfg(all(target_os = "linux", feature = "production-storage"))]
#[derive(Clone, Eq, PartialEq)]
pub struct PassphraseVaultKeychain {
    path: PathBuf,
    passphrase: Zeroizing<String>,
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
impl fmt::Debug for PassphraseVaultKeychain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassphraseVaultKeychain")
            .field("path", &self.path)
            .field("passphrase", &"<redacted>")
            .finish()
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
impl PassphraseVaultKeychain {
    /// Create a production vault keychain using an encrypted vault file and passphrase.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, passphrase: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            passphrase: Zeroizing::new(passphrase.into()),
        }
    }

    /// Return the encrypted vault path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load_entries(&self) -> Result<BTreeMap<String, Vec<u8>>, AppStoreError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new())
            }
            Err(error) => return Err(AppStoreError::Io(error)),
        };
        let envelope: PassphraseVaultEnvelope = serde_json::from_slice(&bytes)?;
        envelope.validate()?;
        let mut key = derive_vault_key(self.passphrase.as_bytes(), &envelope.salt)?;
        let plaintext_result = decrypt_bytes(&key, &envelope.nonce, &envelope.ciphertext);
        key.zeroize();
        let mut plaintext = plaintext_result?;
        let entries = serde_json::from_slice(&plaintext).map_err(AppStoreError::Serde);
        plaintext.zeroize();
        entries
    }

    fn save_entries(&self, entries: &BTreeMap<String, Vec<u8>>) -> Result<(), AppStoreError> {
        if self.passphrase.len() < 12 {
            return Err(AppStoreError::Keychain(
                "Discrypt vault passphrase must be at least 12 characters".to_owned(),
            ));
        }
        let salt = random_vault_salt();
        let nonce = random_nonce();
        let mut plaintext = serde_json::to_vec(entries)?;
        let mut key = derive_vault_key(self.passphrase.as_bytes(), &salt)?;
        let ciphertext = encrypt_bytes(&key, &nonce, &plaintext);
        key.zeroize();
        plaintext.zeroize();
        let ciphertext = ciphertext?;
        let envelope = PassphraseVaultEnvelope {
            format: VAULT_FORMAT.to_owned(),
            kdf: VAULT_KDF.to_owned(),
            salt,
            nonce,
            ciphertext,
        };
        let serialized = serde_json::to_vec(&envelope)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("vault.tmp");
        write_restricted_file(&tmp, &serialized)?;
        fs::rename(tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn write_restricted_file(path: &Path, bytes: &[u8]) -> Result<(), AppStoreError> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let _ = fs::remove_file(path);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::write(path, bytes)?;
        Ok(())
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn zeroize_vault_entries(entries: &mut BTreeMap<String, Vec<u8>>) {
    for secret in entries.values_mut() {
        secret.zeroize();
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
impl AppDbKeychain for PassphraseVaultKeychain {
    fn load_wrapping_key(&mut self, key_id: &str) -> Result<Option<[u8; 32]>, AppStoreError> {
        let mut entries = self.load_entries()?;
        let result = match entries.remove(key_id) {
            Some(secret) => secret_to_wrapping_key(secret),
            None => Ok(None),
        };
        zeroize_vault_entries(&mut entries);
        result
    }

    fn store_wrapping_key(&mut self, key_id: &str, key: [u8; 32]) -> Result<(), AppStoreError> {
        let mut entries = self.load_entries()?;
        entries.insert(key_id.to_owned(), key.to_vec());
        let result = self.save_entries(&entries);
        zeroize_vault_entries(&mut entries);
        result
    }

    fn delete_wrapping_key(&mut self, key_id: &str) -> Result<(), AppStoreError> {
        let mut entries = self.load_entries()?;
        if let Some(mut removed) = entries.remove(key_id) {
            removed.zeroize();
        }
        let result = self.save_entries(&entries);
        zeroize_vault_entries(&mut entries);
        result
    }
}

/// Production keychain strategy for Linux app DB wrapping keys.
#[cfg(all(target_os = "linux", feature = "production-storage"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductionAppDbKeychain {
    os: LinuxOsKeychain,
    vault: Option<PassphraseVaultKeychain>,
    use_os: bool,
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
impl ProductionAppDbKeychain {
    /// Create a production keychain with optional encrypted vault fallback.
    #[must_use]
    pub fn new(os: LinuxOsKeychain, vault: Option<PassphraseVaultKeychain>) -> Self {
        Self {
            os,
            vault,
            use_os: true,
        }
    }

    /// Create the default Linux production keychain.
    #[must_use]
    pub fn discrypt_app_db(vault: Option<PassphraseVaultKeychain>) -> Self {
        Self::new(LinuxOsKeychain::discrypt_app_db(), vault)
    }

    /// Create a production keychain that requires the user-supplied encrypted
    /// vault and does not read or write the OS keychain.
    #[must_use]
    pub fn vault_only(vault: PassphraseVaultKeychain) -> Self {
        Self {
            os: LinuxOsKeychain::discrypt_app_db(),
            vault: Some(vault),
            use_os: false,
        }
    }

    fn vault_mut(&mut self) -> Option<&mut PassphraseVaultKeychain> {
        self.vault.as_mut()
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn production_keychain_fallback_error(
    operation: &str,
    os_error: AppStoreError,
    vault_error: AppStoreError,
) -> AppStoreError {
    AppStoreError::Keychain(format!(
        "OS keychain {operation} failed ({os_error}); configured encrypted vault fallback failed ({vault_error})"
    ))
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn production_keychain_degraded_vault_warning(operation: &str, os_error: &AppStoreError) -> String {
    format!(
        "OS keychain {operation} failed ({os_error}); continuing with explicitly configured encrypted vault fallback"
    )
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn log_production_keychain_degraded_vault(operation: &str, os_error: &AppStoreError) {
    eprintln!(
        "[Discrypt production storage] {}",
        production_keychain_degraded_vault_warning(operation, os_error)
    );
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn production_keychain_delete_result(
    os_result: Result<(), AppStoreError>,
    vault_result: Result<(), AppStoreError>,
) -> Result<(), AppStoreError> {
    match (os_result, vault_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(os_error), Ok(())) => Err(os_error),
        (Ok(()), Err(vault_error)) => Err(vault_error),
        (Err(os_error), Err(vault_error)) => Err(production_keychain_fallback_error(
            "delete",
            os_error,
            vault_error,
        )),
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
impl AppDbKeychain for ProductionAppDbKeychain {
    fn load_wrapping_key(&mut self, key_id: &str) -> Result<Option<[u8; 32]>, AppStoreError> {
        if !self.use_os {
            return self
                .vault_mut()
                .ok_or_else(|| {
                    AppStoreError::KeychainMissing("user password vault is not unlocked".to_owned())
                })?
                .load_wrapping_key(key_id);
        }
        match self.os.load_wrapping_key(key_id) {
            Ok(Some(key)) => Ok(Some(key)),
            Ok(None) => match self.vault_mut() {
                Some(vault) => vault.load_wrapping_key(key_id),
                None => Ok(None),
            },
            Err(os_error) => match self.vault_mut() {
                Some(vault) => match vault.load_wrapping_key(key_id) {
                    Ok(Some(key)) => {
                        log_production_keychain_degraded_vault("load", &os_error);
                        Ok(Some(key))
                    }
                    Ok(None) => Err(production_keychain_fallback_error(
                        "load",
                        os_error,
                        AppStoreError::KeychainMissing(format!(
                            "configured encrypted vault has no wrapping key for {key_id}"
                        )),
                    )),
                    Err(vault_error) => Err(production_keychain_fallback_error(
                        "load",
                        os_error,
                        vault_error,
                    )),
                },
                None => Err(os_error),
            },
        }
    }

    fn store_wrapping_key(&mut self, key_id: &str, key: [u8; 32]) -> Result<(), AppStoreError> {
        if !self.use_os {
            return self
                .vault_mut()
                .ok_or_else(|| {
                    AppStoreError::KeychainMissing("user password vault is not unlocked".to_owned())
                })?
                .store_wrapping_key(key_id, key);
        }
        match self.os.store_wrapping_key(key_id, key) {
            Ok(()) => Ok(()),
            Err(os_error) => match self.vault_mut() {
                Some(vault) => match vault.store_wrapping_key(key_id, key) {
                    Ok(()) => {
                        log_production_keychain_degraded_vault("store", &os_error);
                        Ok(())
                    }
                    Err(vault_error) => Err(production_keychain_fallback_error(
                        "store",
                        os_error,
                        vault_error,
                    )),
                },
                None => Err(os_error),
            },
        }
    }

    fn delete_wrapping_key(&mut self, key_id: &str) -> Result<(), AppStoreError> {
        if !self.use_os {
            return self
                .vault_mut()
                .ok_or_else(|| {
                    AppStoreError::KeychainMissing("user password vault is not unlocked".to_owned())
                })?
                .delete_wrapping_key(key_id);
        }
        let os_result = self.os.delete_wrapping_key(key_id);
        if let Some(vault) = self.vault_mut() {
            let vault_result = vault.delete_wrapping_key(key_id);
            return production_keychain_delete_result(os_result, vault_result);
        }
        os_result
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
const VAULT_FORMAT: &str = "discrypt.appdb.vault.v1";
#[cfg(all(target_os = "linux", feature = "production-storage"))]
const VAULT_KDF: &str = "argon2id-v0.5-default";

#[cfg(all(target_os = "linux", feature = "production-storage"))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct PassphraseVaultEnvelope {
    format: String,
    kdf: String,
    salt: [u8; 16],
    nonce: [u8; 12],
    ciphertext: Vec<u8>,
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
impl PassphraseVaultEnvelope {
    fn validate(&self) -> Result<(), AppStoreError> {
        if self.format != VAULT_FORMAT {
            return Err(AppStoreError::Crypto("unsupported Discrypt vault format"));
        }
        if self.kdf != VAULT_KDF {
            return Err(AppStoreError::Crypto("unsupported Discrypt vault KDF"));
        }
        Ok(())
    }
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn random_vault_salt() -> [u8; 16] {
    let mut salt = [0_u8; 16];
    OsRng.fill_bytes(&mut salt);
    salt
}

#[cfg(all(target_os = "linux", feature = "production-storage"))]
fn derive_vault_key(passphrase: &[u8], salt: &[u8; 16]) -> Result<[u8; 32], AppStoreError> {
    let mut key = [0_u8; 32];
    argon2::Argon2::default()
        .hash_password_into(passphrase, salt, &mut key)
        .map_err(|_| AppStoreError::Crypto("Discrypt vault key derivation failed"))?;
    Ok(key)
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
        match self.keychain.load_wrapping_key(&self.key_id) {
            Ok(Some(key)) => return Ok(key),
            Ok(None) if self.path.exists() => {
                return Err(AppStoreError::KeychainMissing(self.key_id.clone()))
            }
            Ok(None) => {}
            Err(error) if self.path.exists() => return Err(error),
            Err(_) => {}
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
        let mut wrapping_key = self.load_wrapping_key(&envelope.key_id)?;
        let mut data_key = match decrypt_bytes(
            &wrapping_key,
            &envelope.wrapped_key_nonce,
            &envelope.wrapped_data_key,
        ) {
            Ok(key) => key,
            Err(error) => {
                wrapping_key.zeroize();
                return Err(error);
            }
        };
        let plaintext = decrypt_bytes(&data_key, &envelope.data_nonce, &envelope.ciphertext);
        data_key.zeroize();
        wrapping_key.zeroize();
        Ok(Some(plaintext?))
    }

    fn save_app_state(&mut self, bytes: &[u8]) -> Result<(), AppStoreError> {
        let mut wrapping_key = self.load_or_create_wrapping_key()?;
        let mut data_key = random_key();
        let wrapped_key_nonce = random_nonce();
        let data_nonce = random_nonce();
        let wrapped_data_key = match encrypt_bytes(&wrapping_key, &wrapped_key_nonce, &data_key) {
            Ok(bytes) => bytes,
            Err(error) => {
                data_key.zeroize();
                wrapping_key.zeroize();
                return Err(error);
            }
        };
        let ciphertext = match encrypt_bytes(&data_key, &data_nonce, bytes) {
            Ok(bytes) => bytes,
            Err(error) => {
                data_key.zeroize();
                wrapping_key.zeroize();
                return Err(error);
            }
        };
        data_key.zeroize();
        wrapping_key.zeroize();

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

/// Current application database schema version.
pub const APP_DB_SCHEMA_VERSION: u32 = 1;

/// The first schema version supported by this crate.
pub const MIN_SUPPORTED_APP_DB_SCHEMA_VERSION: u32 = 0;

/// Durable database tables required by schema version 1.
pub const REQUIRED_TABLES: &[&str] = &[
    "profiles",
    "devices",
    "groups",
    "channels",
    "invites",
    "governance_events",
    "message_envelopes",
    "retention_state",
    "delivery_queue",
    "voice_preferences",
    "event_cursors",
];

const CREATE_PROFILES: &str = "CREATE TABLE IF NOT EXISTS profiles (profile_id TEXT PRIMARY KEY NOT NULL, user_id TEXT NOT NULL UNIQUE, display_name TEXT NOT NULL, friend_code TEXT NOT NULL, safety_number TEXT NOT NULL, safety_verified INTEGER NOT NULL DEFAULT 0, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL)";
const CREATE_DEVICES: &str = "CREATE TABLE IF NOT EXISTS devices (device_id TEXT PRIMARY KEY NOT NULL, profile_id TEXT NOT NULL REFERENCES profiles(profile_id) ON DELETE CASCADE, mls_leaf INTEGER NOT NULL, credential_hash BLOB NOT NULL, identity_key_ref TEXT NOT NULL, status TEXT NOT NULL, added_at_ms INTEGER NOT NULL, removed_at_ms INTEGER)";
const CREATE_GROUPS: &str = "CREATE TABLE IF NOT EXISTS groups (group_id TEXT PRIMARY KEY NOT NULL, profile_id TEXT NOT NULL REFERENCES profiles(profile_id) ON DELETE CASCADE, name TEXT NOT NULL, role TEXT NOT NULL, mls_epoch INTEGER NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL)";
const CREATE_CHANNELS: &str = "CREATE TABLE IF NOT EXISTS channels (channel_id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, name TEXT NOT NULL, kind TEXT NOT NULL, retention_preset TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL)";
const CREATE_INVITES: &str = "CREATE TABLE IF NOT EXISTS invites (invite_id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, expires_at_ms INTEGER NOT NULL, max_uses INTEGER NOT NULL, password_gate TEXT NOT NULL, revoked INTEGER NOT NULL DEFAULT 0, created_at_ms INTEGER NOT NULL)";
const CREATE_GOVERNANCE_EVENTS: &str = "CREATE TABLE IF NOT EXISTS governance_events (event_id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, sequence INTEGER NOT NULL, event_type TEXT NOT NULL, signed_payload BLOB NOT NULL, author_device_id TEXT NOT NULL, observed_at_ms INTEGER NOT NULL, UNIQUE(group_id, sequence))";
const CREATE_MESSAGE_ENVELOPES: &str = "CREATE TABLE IF NOT EXISTS message_envelopes (message_id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, channel_id TEXT NOT NULL REFERENCES channels(channel_id) ON DELETE CASCADE, author_device_id TEXT NOT NULL, author_sequence INTEGER NOT NULL, mls_epoch INTEGER NOT NULL, ciphertext BLOB NOT NULL, envelope_hash BLOB NOT NULL, sent_at_ms INTEGER NOT NULL, received_at_ms INTEGER, UNIQUE(group_id, author_device_id, author_sequence))";
const CREATE_RETENTION_STATE: &str = "CREATE TABLE IF NOT EXISTS retention_state (retention_id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, channel_id TEXT REFERENCES channels(channel_id) ON DELETE CASCADE, message_id TEXT REFERENCES message_envelopes(message_id) ON DELETE CASCADE, state TEXT NOT NULL, key_ref TEXT, shred_after_ms INTEGER, updated_at_ms INTEGER NOT NULL)";
const CREATE_DELIVERY_QUEUE: &str = "CREATE TABLE IF NOT EXISTS delivery_queue (queue_id TEXT PRIMARY KEY NOT NULL, message_id TEXT NOT NULL REFERENCES message_envelopes(message_id) ON DELETE CASCADE, destination TEXT NOT NULL, status TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, next_attempt_ms INTEGER NOT NULL, last_error TEXT, updated_at_ms INTEGER NOT NULL)";
const CREATE_VOICE_PREFERENCES: &str = "CREATE TABLE IF NOT EXISTS voice_preferences (profile_id TEXT NOT NULL REFERENCES profiles(profile_id) ON DELETE CASCADE, group_id TEXT NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE, channel_id TEXT NOT NULL REFERENCES channels(channel_id) ON DELETE CASCADE, muted INTEGER NOT NULL DEFAULT 0, speaker_volume INTEGER NOT NULL DEFAULT 100, route TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(profile_id, group_id, channel_id))";
const CREATE_EVENT_CURSORS: &str = "CREATE TABLE IF NOT EXISTS event_cursors (cursor_id TEXT PRIMARY KEY NOT NULL, profile_id TEXT NOT NULL REFERENCES profiles(profile_id) ON DELETE CASCADE, source TEXT NOT NULL, position TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, UNIQUE(profile_id, source))";

const CREATE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_devices_profile ON devices(profile_id)",
    "CREATE INDEX IF NOT EXISTS idx_channels_group ON channels(group_id)",
    "CREATE INDEX IF NOT EXISTS idx_messages_channel_time ON message_envelopes(channel_id, sent_at_ms)",
    "CREATE INDEX IF NOT EXISTS idx_delivery_status_attempt ON delivery_queue(status, next_attempt_ms)",
    "CREATE INDEX IF NOT EXISTS idx_retention_message ON retention_state(message_id)",
    "CREATE INDEX IF NOT EXISTS idx_governance_group_sequence ON governance_events(group_id, sequence)",
];

const VERSION_1_DDL: &[&str] = &[
    "PRAGMA foreign_keys = ON",
    CREATE_PROFILES,
    CREATE_DEVICES,
    CREATE_GROUPS,
    CREATE_CHANNELS,
    CREATE_INVITES,
    CREATE_GOVERNANCE_EVENTS,
    CREATE_MESSAGE_ENVELOPES,
    CREATE_RETENTION_STATE,
    CREATE_DELIVERY_QUEUE,
    CREATE_VOICE_PREFERENCES,
    CREATE_EVENT_CURSORS,
    CREATE_INDEXES[0],
    CREATE_INDEXES[1],
    CREATE_INDEXES[2],
    CREATE_INDEXES[3],
    CREATE_INDEXES[4],
    CREATE_INDEXES[5],
    "PRAGMA user_version = 1",
];

const VERSION_1_ROLLBACK: &[&str] = &[
    "DROP TABLE IF EXISTS event_cursors",
    "DROP TABLE IF EXISTS voice_preferences",
    "DROP TABLE IF EXISTS delivery_queue",
    "DROP TABLE IF EXISTS retention_state",
    "DROP TABLE IF EXISTS message_envelopes",
    "DROP TABLE IF EXISTS governance_events",
    "DROP TABLE IF EXISTS invites",
    "DROP TABLE IF EXISTS channels",
    "DROP TABLE IF EXISTS groups",
    "DROP TABLE IF EXISTS devices",
    "DROP TABLE IF EXISTS profiles",
    "PRAGMA user_version = 0",
];

/// A required column in a schema table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppDbColumn {
    /// Column name.
    pub name: &'static str,
    /// Stable SQL type or affinity used by the migration contract.
    pub sql_type: &'static str,
    /// Whether the field may contain secret material and must be keychain-wrapped/encrypted by writers.
    pub sensitive: bool,
}

/// A required table in a schema version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppDbTable {
    /// Table name.
    pub name: &'static str,
    /// Required columns for corruption/migration verification.
    pub columns: &'static [AppDbColumn],
}

/// Durable schema manifest for the current application database.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppDbSchema {
    /// Schema version represented by this manifest.
    pub version: u32,
    /// Required tables.
    pub tables: &'static [AppDbTable],
}

/// Direction for a schema migration plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationDirection {
    /// Upgrade from an older supported schema.
    Forward,
    /// Roll back to an older supported schema for tests/recovery validation.
    Backward,
    /// No schema changes are required.
    Noop,
}

/// SQL migration plan between two supported versions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppDbMigrationPlan {
    /// Source schema version.
    pub from_version: u32,
    /// Target schema version.
    pub to_version: u32,
    /// Migration direction.
    pub direction: MigrationDirection,
    /// Ordered SQL statements to execute transactionally.
    pub statements: Vec<&'static str>,
}

/// Quarantine result for a corrupted database and sidecar files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuarantinedAppDb {
    /// Original database path.
    pub original_path: PathBuf,
    /// Quarantined database path.
    pub quarantine_path: PathBuf,
    /// Quarantined sidecars, such as WAL and SHM files.
    pub sidecars: Vec<(PathBuf, PathBuf)>,
}

/// Application database schema and migration errors.
#[derive(Debug, thiserror::Error)]
pub enum AppDbError {
    /// A requested migration version is newer than this crate understands.
    #[error("unsupported future app DB schema version {version}; current is {current}")]
    UnsupportedFutureVersion { version: u32, current: u32 },
    /// A requested migration version is older than the supported floor.
    #[error("unsupported legacy app DB schema version {version}; minimum is {minimum}")]
    UnsupportedLegacyVersion { version: u32, minimum: u32 },
    /// The observed store is missing a required table.
    #[error("corrupt app DB: missing required table {table}")]
    MissingRequiredTable { table: &'static str },
    /// The observed store is missing a required column.
    #[error("corrupt app DB: missing required column {table}.{column}")]
    MissingRequiredColumn {
        table: &'static str,
        column: &'static str,
    },
    /// Corruption quarantine failed at the filesystem boundary.
    #[error("app DB quarantine I/O error: {0}")]
    QuarantineIo(#[from] std::io::Error),
}

const PROFILE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "profile_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "user_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "display_name",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "friend_code",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "safety_number",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "safety_verified",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "created_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const DEVICE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "device_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "profile_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "mls_leaf",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "credential_hash",
        sql_type: "BLOB",
        sensitive: false,
    },
    AppDbColumn {
        name: "identity_key_ref",
        sql_type: "TEXT",
        sensitive: true,
    },
    AppDbColumn {
        name: "status",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "added_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "removed_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const GROUP_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "profile_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "name",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "role",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "mls_epoch",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "created_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const CHANNEL_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "channel_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "name",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "kind",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "retention_preset",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "created_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const INVITE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "invite_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "expires_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "max_uses",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "password_gate",
        sql_type: "TEXT",
        sensitive: true,
    },
    AppDbColumn {
        name: "revoked",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "created_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const GOVERNANCE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "event_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "sequence",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "event_type",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "signed_payload",
        sql_type: "BLOB",
        sensitive: false,
    },
    AppDbColumn {
        name: "author_device_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "observed_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const MESSAGE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "message_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "channel_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "author_device_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "author_sequence",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "mls_epoch",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "ciphertext",
        sql_type: "BLOB",
        sensitive: true,
    },
    AppDbColumn {
        name: "envelope_hash",
        sql_type: "BLOB",
        sensitive: false,
    },
    AppDbColumn {
        name: "sent_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "received_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const RETENTION_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "retention_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "channel_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "message_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "state",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "key_ref",
        sql_type: "TEXT",
        sensitive: true,
    },
    AppDbColumn {
        name: "shred_after_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const DELIVERY_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "queue_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "message_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "destination",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "status",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "attempts",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "next_attempt_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "last_error",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const VOICE_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "profile_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "group_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "channel_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "muted",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "speaker_volume",
        sql_type: "INTEGER",
        sensitive: false,
    },
    AppDbColumn {
        name: "route",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const CURSOR_COLUMNS: &[AppDbColumn] = &[
    AppDbColumn {
        name: "cursor_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "profile_id",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "source",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "position",
        sql_type: "TEXT",
        sensitive: false,
    },
    AppDbColumn {
        name: "updated_at_ms",
        sql_type: "INTEGER",
        sensitive: false,
    },
];

const APP_DB_TABLES: &[AppDbTable] = &[
    AppDbTable {
        name: "profiles",
        columns: PROFILE_COLUMNS,
    },
    AppDbTable {
        name: "devices",
        columns: DEVICE_COLUMNS,
    },
    AppDbTable {
        name: "groups",
        columns: GROUP_COLUMNS,
    },
    AppDbTable {
        name: "channels",
        columns: CHANNEL_COLUMNS,
    },
    AppDbTable {
        name: "invites",
        columns: INVITE_COLUMNS,
    },
    AppDbTable {
        name: "governance_events",
        columns: GOVERNANCE_COLUMNS,
    },
    AppDbTable {
        name: "message_envelopes",
        columns: MESSAGE_COLUMNS,
    },
    AppDbTable {
        name: "retention_state",
        columns: RETENTION_COLUMNS,
    },
    AppDbTable {
        name: "delivery_queue",
        columns: DELIVERY_COLUMNS,
    },
    AppDbTable {
        name: "voice_preferences",
        columns: VOICE_COLUMNS,
    },
    AppDbTable {
        name: "event_cursors",
        columns: CURSOR_COLUMNS,
    },
];

impl AppDbSchema {
    /// Return the current durable schema manifest.
    #[must_use]
    pub const fn current() -> Self {
        Self {
            version: APP_DB_SCHEMA_VERSION,
            tables: APP_DB_TABLES,
        }
    }

    /// Find a required table by name.
    #[must_use]
    pub fn table(&self, name: &str) -> Option<&'static AppDbTable> {
        self.tables.iter().find(|table| table.name == name)
    }

    /// Iterate all columns that carry sensitive material or key references.
    pub fn sensitive_columns(&self) -> impl Iterator<Item = (&'static str, &'static AppDbColumn)> {
        self.tables.iter().flat_map(|table| {
            table
                .columns
                .iter()
                .filter(|column| column.sensitive)
                .map(move |column| (table.name, column))
        })
    }
}

impl AppDbMigrationPlan {
    /// Build a supported migration plan between schema versions.
    pub fn plan(from_version: u32, to_version: u32) -> Result<Self, AppDbError> {
        validate_version(from_version)?;
        validate_version(to_version)?;

        if from_version == to_version {
            return Ok(Self {
                from_version,
                to_version,
                direction: MigrationDirection::Noop,
                statements: Vec::new(),
            });
        }

        if from_version == 0 && to_version == 1 {
            return Ok(Self {
                from_version,
                to_version,
                direction: MigrationDirection::Forward,
                statements: VERSION_1_DDL.to_vec(),
            });
        }

        if from_version == 1 && to_version == 0 {
            return Ok(Self {
                from_version,
                to_version,
                direction: MigrationDirection::Backward,
                statements: VERSION_1_ROLLBACK.to_vec(),
            });
        }

        // The version validator keeps this arm unreachable for the current two-version graph,
        // but keeping the explicit future error makes added versions fail safely.
        Err(AppDbError::UnsupportedFutureVersion {
            version: to_version,
            current: APP_DB_SCHEMA_VERSION,
        })
    }

    /// True when the plan has statements to execute.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }
}

/// Validate an observed schema inventory against the current manifest.
pub fn validate_observed_schema<'a, I, J>(tables: I) -> Result<(), AppDbError>
where
    I: IntoIterator<Item = (&'a str, J)>,
    J: IntoIterator<Item = &'a str>,
{
    let observed: Vec<(&str, Vec<&str>)> = tables
        .into_iter()
        .map(|(table, columns)| (table, columns.into_iter().collect()))
        .collect();

    for required in AppDbSchema::current().tables {
        let Some((_, columns)) = observed.iter().find(|(table, _)| *table == required.name) else {
            return Err(AppDbError::MissingRequiredTable {
                table: required.name,
            });
        };
        for column in required.columns {
            if !columns.contains(&column.name) {
                return Err(AppDbError::MissingRequiredColumn {
                    table: required.name,
                    column: column.name,
                });
            }
        }
    }
    Ok(())
}

/// Move a corrupt database and its WAL/SHM sidecars aside before opening a fresh store.
pub fn quarantine_corrupt_store(path: impl AsRef<Path>) -> Result<QuarantinedAppDb, AppDbError> {
    let original_path = path.as_ref().to_path_buf();
    let quarantine_path = corruption_path(&original_path, "db");
    fs::rename(&original_path, &quarantine_path)?;

    let mut sidecars = Vec::new();
    for suffix in ["wal", "shm", "journal"] {
        let sidecar = sidecar_path(&original_path, suffix);
        if sidecar.exists() {
            let quarantined = corruption_path(&sidecar, suffix);
            fs::rename(&sidecar, &quarantined)?;
            sidecars.push((sidecar, quarantined));
        }
    }

    Ok(QuarantinedAppDb {
        original_path,
        quarantine_path,
        sidecars,
    })
}

#[allow(clippy::absurd_extreme_comparisons)]
fn validate_version(version: u32) -> Result<(), AppDbError> {
    if version > APP_DB_SCHEMA_VERSION {
        return Err(AppDbError::UnsupportedFutureVersion {
            version,
            current: APP_DB_SCHEMA_VERSION,
        });
    }
    Ok(())
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(format!("-{suffix}"));
    PathBuf::from(raw)
}

fn corruption_path(path: &Path, tag: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    let mut raw = path.as_os_str().to_os_string();
    raw.push(format!(".corrupt-{tag}-{timestamp}"));
    PathBuf::from(raw)
}

impl fmt::Display for AppDbMigrationPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "app DB migration {} -> {} ({:?}, {} statements)",
            self.from_version,
            self.to_version,
            self.direction,
            self.statements.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, io::Write};

    #[test]
    fn encrypted_app_db_exports_versioned_schema_contract() -> Result<(), AppDbError> {
        let plan = AppDbMigrationPlan::plan(0, APP_DB_SCHEMA_VERSION)?;
        assert_eq!(plan.direction, MigrationDirection::Forward);
        for required in REQUIRED_TABLES {
            assert!(AppDbSchema::current().table(required).is_some());
        }
        Ok(())
    }

    #[test]
    fn storage_keychain_decision_covers_adr_006() {
        let decision = storage_keychain_decision();
        assert!(decision.covers_adr_006());
        assert!(decision
            .sqlite_schema_policy
            .contains("SQLite-compatible durable schema contract"));
        assert!(decision
            .platform_differences
            .iter()
            .any(|platform| platform.contains("macOS") && platform.contains("adapters")));
    }

    #[test]
    fn migration_planner_rejects_future_versions_and_noops_current() -> Result<(), AppDbError> {
        let noop = AppDbMigrationPlan::plan(APP_DB_SCHEMA_VERSION, APP_DB_SCHEMA_VERSION)?;
        assert_eq!(noop.direction, MigrationDirection::Noop);
        assert!(noop.is_empty());

        assert!(matches!(
            AppDbMigrationPlan::plan(APP_DB_SCHEMA_VERSION, APP_DB_SCHEMA_VERSION + 1),
            Err(AppDbError::UnsupportedFutureVersion {
                version,
                current: APP_DB_SCHEMA_VERSION,
            }) if version == APP_DB_SCHEMA_VERSION + 1
        ));
        Ok(())
    }

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

    #[derive(Clone, Debug, Default)]
    struct LoadErrorCountingKeychain {
        store_attempts: Arc<Mutex<usize>>,
    }

    impl LoadErrorCountingKeychain {
        fn store_attempts(&self) -> Result<usize, AppStoreError> {
            self.store_attempts
                .lock()
                .map_err(|_| AppStoreError::LockPoisoned)
                .map(|attempts| *attempts)
        }
    }

    impl AppDbKeychain for LoadErrorCountingKeychain {
        fn load_wrapping_key(&mut self, _key_id: &str) -> Result<Option<[u8; 32]>, AppStoreError> {
            Err(AppStoreError::Keychain("native provider locked".to_owned()))
        }

        fn store_wrapping_key(
            &mut self,
            _key_id: &str,
            _key: [u8; 32],
        ) -> Result<(), AppStoreError> {
            *self
                .store_attempts
                .lock()
                .map_err(|_| AppStoreError::LockPoisoned)? += 1;
            Ok(())
        }

        fn delete_wrapping_key(&mut self, _key_id: &str) -> Result<(), AppStoreError> {
            Ok(())
        }
    }

    #[derive(Clone, Debug)]
    struct PreflightTestKeychain {
        state: Arc<Mutex<PreflightTestKeychainState>>,
    }

    #[derive(Clone, Debug)]
    struct PreflightTestKeychainState {
        stored: Option<[u8; 32]>,
        load_override: Option<Option<[u8; 32]>>,
        load_errors: bool,
        delete_removes_key: bool,
    }

    impl PreflightTestKeychain {
        fn new() -> Self {
            Self {
                state: Arc::new(Mutex::new(PreflightTestKeychainState {
                    stored: None,
                    load_override: None,
                    load_errors: false,
                    delete_removes_key: true,
                })),
            }
        }

        fn with_load_override(load_override: Option<[u8; 32]>) -> Self {
            Self {
                state: Arc::new(Mutex::new(PreflightTestKeychainState {
                    stored: None,
                    load_override: Some(load_override),
                    load_errors: false,
                    delete_removes_key: true,
                })),
            }
        }

        fn with_load_error() -> Self {
            Self {
                state: Arc::new(Mutex::new(PreflightTestKeychainState {
                    stored: None,
                    load_override: None,
                    load_errors: true,
                    delete_removes_key: true,
                })),
            }
        }

        fn with_delete_failure() -> Self {
            Self {
                state: Arc::new(Mutex::new(PreflightTestKeychainState {
                    stored: None,
                    load_override: None,
                    load_errors: false,
                    delete_removes_key: false,
                })),
            }
        }

        fn stored_key(&self) -> Result<Option<[u8; 32]>, AppStoreError> {
            self.state
                .lock()
                .map_err(|_| AppStoreError::LockPoisoned)
                .map(|state| state.stored)
        }
    }

    impl Default for PreflightTestKeychain {
        fn default() -> Self {
            Self::new()
        }
    }

    impl AppDbKeychain for PreflightTestKeychain {
        fn load_wrapping_key(&mut self, _key_id: &str) -> Result<Option<[u8; 32]>, AppStoreError> {
            self.state
                .lock()
                .map_err(|_| AppStoreError::LockPoisoned)
                .and_then(|state| {
                    if state.load_errors {
                        Err(AppStoreError::Keychain("reload failed".to_owned()))
                    } else {
                        Ok(state.load_override.unwrap_or(state.stored))
                    }
                })
        }

        fn store_wrapping_key(
            &mut self,
            _key_id: &str,
            key: [u8; 32],
        ) -> Result<(), AppStoreError> {
            self.state
                .lock()
                .map_err(|_| AppStoreError::LockPoisoned)?
                .stored = Some(key);
            Ok(())
        }

        fn delete_wrapping_key(&mut self, _key_id: &str) -> Result<(), AppStoreError> {
            let mut state = self.state.lock().map_err(|_| AppStoreError::LockPoisoned)?;
            if state.delete_removes_key {
                state.stored = None;
            }
            Ok(())
        }
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
    fn encrypted_app_db_does_not_create_new_key_when_existing_store_keychain_load_fails(
    ) -> Result<(), AppStoreError> {
        let path = temp_db_path("existing-keychain-load-failure");
        let _ = fs::remove_file(&path);
        fs::write(&path, b"existing encrypted envelope placeholder")?;
        let keychain = LoadErrorCountingKeychain::default();
        let mut db = EncryptedAppDb::new(&path, keychain.clone());

        let error = match db.save_app_state(br#"{"body":"must not overwrite"}"#) {
            Ok(()) => {
                return Err(AppStoreError::Io(std::io::Error::other(
                    "existing encrypted DB must fail closed when keychain load fails",
                )));
            }
            Err(error) => error,
        };

        assert!(error.to_string().contains("native provider locked"));
        assert_eq!(keychain.store_attempts()?, 0);
        assert_eq!(fs::read(&path)?, b"existing encrypted envelope placeholder");
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

    #[test]
    fn deterministic_memory_keychain_is_available_for_tests_and_local_dev_fallbacks(
    ) -> Result<(), AppStoreError> {
        let mut keychain = MemoryAppDbKeychain::default();
        keychain.store_wrapping_key("local-dev-key", [3; 32])?;
        assert_eq!(keychain.load_wrapping_key("local-dev-key")?, Some([3; 32]));
        keychain.delete_wrapping_key("local-dev-key")?;
        assert_eq!(keychain.load_wrapping_key("local-dev-key")?, None);
        Ok(())
    }

    #[test]
    fn keychain_preflight_round_trips_and_removes_probe_key() -> Result<(), AppStoreError> {
        let keychain = PreflightTestKeychain::new();
        let mut probe = keychain.clone();

        preflight_app_db_keychain(&mut probe, "probe-key", [0x42; 32])?;

        assert_eq!(keychain.stored_key()?, None);
        Ok(())
    }

    #[test]
    fn keychain_preflight_rejects_wrong_wrapping_key_and_cleans_probe() -> Result<(), AppStoreError>
    {
        let keychain = PreflightTestKeychain::with_load_override(Some([0x24; 32]));
        let mut probe = keychain.clone();

        assert!(matches!(
            preflight_app_db_keychain(&mut probe, "probe-key", [0x42; 32]),
            Err(AppStoreError::Crypto(
                "OS keyring preflight returned a different wrapping key"
            ))
        ));
        assert_eq!(keychain.stored_key()?, None);
        Ok(())
    }

    #[test]
    fn keychain_preflight_rejects_missing_loaded_key_and_cleans_probe() -> Result<(), AppStoreError>
    {
        let keychain = PreflightTestKeychain::with_load_override(None);
        let mut probe = keychain.clone();

        assert!(matches!(
            preflight_app_db_keychain(&mut probe, "probe-key", [0x42; 32]),
            Err(AppStoreError::KeychainMissing(ref key_id)) if key_id == "probe-key"
        ));
        assert_eq!(keychain.stored_key()?, None);
        Ok(())
    }

    #[test]
    fn keychain_preflight_cleans_probe_when_reload_errors() -> Result<(), AppStoreError> {
        let keychain = PreflightTestKeychain::with_load_error();
        let mut probe = keychain.clone();

        assert!(matches!(
            preflight_app_db_keychain(&mut probe, "probe-key", [0x42; 32]),
            Err(AppStoreError::Keychain(ref error)) if error == "reload failed"
        ));
        assert_eq!(keychain.stored_key()?, None);
        Ok(())
    }

    #[test]
    fn keychain_preflight_rejects_probe_key_that_survives_delete() -> Result<(), AppStoreError> {
        let keychain = PreflightTestKeychain::with_delete_failure();
        let mut probe = keychain.clone();

        assert!(matches!(
            preflight_app_db_keychain(&mut probe, "probe-key", [0x42; 32]),
            Err(AppStoreError::Crypto(
                "OS keyring preflight probe key remained after delete"
            ))
        ));
        assert_eq!(keychain.stored_key()?, Some([0x42; 32]));
        Ok(())
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn production_storage_exposes_linux_os_keychain_boundary() {
        fn assert_keychain<K: AppDbKeychain>(_keychain: K) {}
        let keychain = LinuxOsKeychain::discrypt_app_db();
        assert_eq!(keychain.service(), "discrypt.appdb");
        assert_eq!(keychain.target(), "default");
        assert_eq!(keychain.legacy_target(), "discrypt");
        assert_keychain(keychain);
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn invalid_os_keychain_wrapping_key_length_is_rejected() {
        let error = secret_to_wrapping_key(vec![0x11; 31])
            .expect_err("short keyring material must fail closed");

        assert!(matches!(
            error,
            AppStoreError::Crypto("invalid OS keychain wrapping key length")
        ));
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn production_passphrase_vault_round_trips_without_plaintext_key() -> Result<(), AppStoreError>
    {
        let path = temp_db_path("discrypt-vault").with_extension("vault");
        let _ = fs::remove_file(&path);
        let key_id = "vault-key";
        let key = [7; 32];
        let mut vault = PassphraseVaultKeychain::new(&path, "correct horse battery staple");

        vault.store_wrapping_key(key_id, key)?;
        assert_eq!(vault.load_wrapping_key(key_id)?, Some(key));
        let bytes = fs::read(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path)?.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "vault file must be owner-read/write only");
        }
        assert!(
            !bytes.windows(32).any(|window| window == key),
            "vault must not contain the raw wrapping key"
        );
        vault.delete_wrapping_key(key_id)?;
        assert_eq!(vault.load_wrapping_key(key_id)?, None);
        Ok(())
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn production_passphrase_vault_app_db_survives_fresh_instance_with_same_password(
    ) -> Result<(), AppStoreError> {
        let state_path = temp_db_path("discrypt-vault-reinstall");
        let vault_path = state_path.with_extension("vault");
        let _ = fs::remove_file(&state_path);
        let _ = fs::remove_file(&vault_path);
        let passphrase = "correct horse battery staple";
        let payload = br#"{"profile_id":"same-profile-after-reinstall"}"#;

        let mut first_install = EncryptedAppDb::new(
            &state_path,
            ProductionAppDbKeychain::vault_only(PassphraseVaultKeychain::new(
                &vault_path,
                passphrase,
            )),
        );
        first_install.save_app_state(payload)?;
        let state_bytes_after_first_save = fs::read(&state_path)?;
        let vault_bytes_after_first_save = fs::read(&vault_path)?;

        let mut reinstalled = EncryptedAppDb::new(
            &state_path,
            ProductionAppDbKeychain::vault_only(PassphraseVaultKeychain::new(
                &vault_path,
                passphrase,
            )),
        );
        assert_eq!(reinstalled.load_app_state()?, Some(payload.to_vec()));

        let mut wrong_password = EncryptedAppDb::new(
            &state_path,
            ProductionAppDbKeychain::vault_only(PassphraseVaultKeychain::new(
                &vault_path,
                "wrong horse battery staple",
            )),
        );
        assert!(matches!(
            wrong_password.load_app_state(),
            Err(AppStoreError::Crypto("app db decryption failed"))
        ));
        assert_eq!(fs::read(&state_path)?, state_bytes_after_first_save);
        assert_eq!(fs::read(&vault_path)?, vault_bytes_after_first_save);
        Ok(())
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn existing_encrypted_app_db_missing_vault_key_fails_closed_without_overwrite(
    ) -> Result<(), AppStoreError> {
        let state_path = temp_db_path("discrypt-vault-missing-key");
        let vault_path = state_path.with_extension("vault");
        let missing_vault_path = state_path.with_extension("missing.vault");
        let _ = fs::remove_file(&state_path);
        let _ = fs::remove_file(&vault_path);
        let _ = fs::remove_file(&missing_vault_path);

        let mut original = EncryptedAppDb::new(
            &state_path,
            ProductionAppDbKeychain::vault_only(PassphraseVaultKeychain::new(
                &vault_path,
                "correct horse battery staple",
            )),
        );
        original.save_app_state(br#"{"profile_id":"must-not-reset"}"#)?;
        let state_bytes_after_first_save = fs::read(&state_path)?;

        let mut missing_vault = EncryptedAppDb::new(
            &state_path,
            ProductionAppDbKeychain::vault_only(PassphraseVaultKeychain::new(
                &missing_vault_path,
                "correct horse battery staple",
            )),
        );
        let error = missing_vault
            .save_app_state(br#"{"profile_id":"replacement-profile"}"#)
            .expect_err("existing encrypted state must not seed a replacement vault key");

        assert!(matches!(
            error,
            AppStoreError::KeychainMissing(ref key_id) if key_id == DEFAULT_WRAPPING_KEY_ID
        ));
        assert_eq!(fs::read(&state_path)?, state_bytes_after_first_save);
        assert!(
            !missing_vault_path.exists(),
            "missing vault path must not be created for existing unreadable state"
        );
        Ok(())
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn production_passphrase_vault_rejects_short_passphrase() {
        let path = temp_db_path("discrypt-short-vault").with_extension("vault");
        let _ = fs::remove_file(&path);
        let mut vault = PassphraseVaultKeychain::new(&path, "too-short");

        let error = vault
            .store_wrapping_key("vault-key", [7; 32])
            .expect_err("short vault passphrase must be rejected");
        assert!(error.to_string().contains("at least 12 characters"));
        assert!(!path.exists());
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn production_passphrase_vault_debug_redacts_passphrase() {
        let path = temp_db_path("discrypt-debug-vault").with_extension("vault");
        let vault =
            PassphraseVaultKeychain::new(&path, "debug-secret-passphrase-that-must-not-leak");
        let debug = format!("{vault:?}");

        assert!(debug.contains("PassphraseVaultKeychain"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("debug-secret-passphrase-that-must-not-leak"));
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn production_keychain_fallback_error_preserves_both_failure_signals() {
        let error = production_keychain_fallback_error(
            "store",
            AppStoreError::Keychain("native provider locked".to_owned()),
            AppStoreError::Keychain(
                "Discrypt vault passphrase must be at least 12 characters".to_owned(),
            ),
        );
        let rendered = error.to_string();

        assert!(rendered.contains("OS keychain store failed"));
        assert!(rendered.contains("native provider locked"));
        assert!(rendered.contains("configured encrypted vault fallback failed"));
        assert!(rendered.contains("at least 12 characters"));
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn production_degraded_vault_warning_preserves_os_failure_signal() {
        let warning = production_keychain_degraded_vault_warning(
            "load",
            &AppStoreError::Keychain("native provider locked".to_owned()),
        );

        assert!(warning.contains("OS keychain load failed"));
        assert!(warning.contains("native provider locked"));
        assert!(warning.contains("explicitly configured encrypted vault fallback"));
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn production_delete_preserves_os_keychain_failure_when_vault_delete_succeeds() {
        let error = production_keychain_delete_result(
            Err(AppStoreError::Keychain(
                "native delete permission denied".to_owned(),
            )),
            Ok(()),
        )
        .expect_err("OS keychain delete failure must not be masked by vault cleanup success");

        let rendered = error.to_string();
        assert!(rendered.contains("native delete permission denied"));
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn linux_legacy_delete_only_ignores_no_entry() {
        assert!(legacy_delete_result(Ok(())).is_ok());
        assert!(legacy_delete_result(Err(keyring::Error::NoEntry)).is_ok());

        let error = legacy_delete_result(Err(keyring::Error::Invalid(
            "legacy-target".to_owned(),
            "delete denied".to_owned(),
        )))
        .expect_err("legacy delete errors other than NoEntry must be reported");
        assert!(error.to_string().contains("delete denied"));
    }

    #[cfg(all(target_os = "linux", feature = "production-storage"))]
    #[test]
    fn linux_secret_service_keychain_live_roundtrip_when_enabled() -> Result<(), AppStoreError> {
        if std::env::var("DISCRYPT_LINUX_SECRET_SERVICE_E2E")
            .ok()
            .as_deref()
            != Some("1")
        {
            eprintln!(
                "skipping live Linux Secret Service keychain roundtrip; set DISCRYPT_LINUX_SECRET_SERVICE_E2E=1"
            );
            return Ok(());
        }

        let service = format!("discrypt.appdb.e2e.{}", std::process::id());
        let key_id = "live-secret-service-key";
        let key = [42; 32];
        let mut keychain = LinuxOsKeychain::new(service);

        let _ = keychain.delete_wrapping_key(key_id);
        preflight_app_db_keychain(&mut keychain, key_id, key)?;
        assert_eq!(keychain.load_wrapping_key(key_id)?, None);
        Ok(())
    }

    #[test]
    fn current_schema_has_all_phase_b_tables() {
        let schema = AppDbSchema::current();
        assert_eq!(schema.version, APP_DB_SCHEMA_VERSION);
        let table_names = schema
            .tables
            .iter()
            .map(|table| table.name)
            .collect::<BTreeSet<_>>();

        for required in REQUIRED_TABLES {
            assert!(table_names.contains(required), "missing {required}");
        }
    }

    #[test]
    fn migration_from_empty_store_creates_required_schema() -> Result<(), AppDbError> {
        let plan = AppDbMigrationPlan::plan(0, APP_DB_SCHEMA_VERSION)?;
        assert_eq!(plan.direction, MigrationDirection::Forward);
        assert!(!plan.is_empty());
        for required in REQUIRED_TABLES {
            let needle = format!("CREATE TABLE IF NOT EXISTS {required}");
            assert!(
                plan.statements
                    .iter()
                    .any(|statement| statement.contains(&needle)),
                "missing migration statement for {required}"
            );
        }
        assert!(plan.statements.contains(&"PRAGMA user_version = 1"));
        Ok(())
    }

    #[test]
    fn backward_migration_drops_required_schema_for_recovery_tests() -> Result<(), AppDbError> {
        let plan = AppDbMigrationPlan::plan(APP_DB_SCHEMA_VERSION, 0)?;
        assert_eq!(plan.direction, MigrationDirection::Backward);
        for required in REQUIRED_TABLES {
            let needle = format!("DROP TABLE IF EXISTS {required}");
            assert!(
                plan.statements
                    .iter()
                    .any(|statement| statement.contains(&needle)),
                "missing rollback statement for {required}"
            );
        }
        Ok(())
    }

    #[test]
    fn schema_validation_reports_missing_table_and_column() {
        let missing_table = validate_observed_schema([(
            "profiles",
            PROFILE_COLUMNS.iter().map(|column| column.name),
        )]);
        assert!(matches!(
            missing_table,
            Err(AppDbError::MissingRequiredTable { table: "devices" })
        ));

        let observed = AppDbSchema::current().tables.iter().map(|table| {
            let columns = table
                .columns
                .iter()
                .filter(|column| !(table.name == "devices" && column.name == "identity_key_ref"))
                .map(|column| column.name);
            (table.name, columns)
        });
        let missing_column = validate_observed_schema(observed);
        assert!(matches!(
            missing_column,
            Err(AppDbError::MissingRequiredColumn {
                table: "devices",
                column: "identity_key_ref",
            })
        ));
    }

    #[test]
    fn sensitive_fields_are_key_references_or_ciphertext_only() {
        let sensitive = AppDbSchema::current()
            .sensitive_columns()
            .map(|(table, column)| format!("{table}.{}", column.name))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            sensitive,
            BTreeSet::from([
                "devices.identity_key_ref".to_owned(),
                "invites.password_gate".to_owned(),
                "message_envelopes.ciphertext".to_owned(),
                "retention_state.key_ref".to_owned(),
            ])
        );
    }

    #[test]
    fn corrupt_store_quarantine_moves_db_and_sidecars() -> Result<(), Box<dyn std::error::Error>> {
        let base = std::env::temp_dir().join(format!(
            "discrypt-app-db-corrupt-{}-{}.sqlite",
            std::process::id(),
            unique_test_suffix()
        ));
        let wal = sidecar_path(&base, "wal");
        let shm = sidecar_path(&base, "shm");

        write_file(&base, b"not sqlite")?;
        write_file(&wal, b"wal bytes")?;
        write_file(&shm, b"shm bytes")?;

        let quarantined = quarantine_corrupt_store(&base)?;
        assert!(!base.exists());
        assert!(!wal.exists());
        assert!(!shm.exists());
        assert!(quarantined.quarantine_path.exists());
        assert_eq!(quarantined.sidecars.len(), 2);
        for (_, quarantined_sidecar) in &quarantined.sidecars {
            assert!(quarantined_sidecar.exists());
            let _ = fs::remove_file(quarantined_sidecar);
        }
        let _ = fs::remove_file(quarantined.quarantine_path);
        Ok(())
    }

    fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let mut file = fs::File::create(path)?;
        file.write_all(bytes)
    }

    fn unique_test_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    }
}

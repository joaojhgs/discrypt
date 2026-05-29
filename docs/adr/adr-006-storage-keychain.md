# ADR-006: Storage and keychain boundary

## Status

Accepted for the production E2E P2P overlay mesh launch gate.

## Context

Discrypt needs durable local state for profiles, devices, groups, channels,
invites, governance events, text message envelopes, retention state, delivery
queues, voice preferences, and sync cursors. The original production plan asks
for SQLite/encrypted store/keychain crate decisions, WAL/journal policy, key
wrapping, schema migrations, secure-delete limits, and platform differences.

The current implementation has two storage domains:

- App-level state is owned by `EncryptedAppDb` in `crates/storage/src/appdb.rs`.
  It persists a `serde_json` envelope encrypted with AES-256-GCM and does **not
  persist plaintext SQLite pages**.
- OpenMLS protocol storage is a separate provider decision covered by ADR-002 and
  uses `openmls_sqlite_storage` for MLS protocol internals.

## Decision

For the launch gate, app state uses the existing encrypted-envelope store rather
than claiming a plaintext SQLite runtime:

- `EncryptedAppDb` stores only `format`, `key_id`, `wrapped_key_nonce`,
  `wrapped_data_key`, `data_nonce`, and encrypted `ciphertext` bytes.
- The encrypted envelope is implemented with `aes-gcm`, `serde_json`, `zeroize`,
  `sha2`, and `hex`.
- `AppDbSchema` and `VERSION_1_DDL` define the SQLite-compatible durable schema
  contract for migration/recovery tests and future SQL-backed app storage.
- OpenMLS state remains separate and uses `openmls_sqlite_storage` under the
  provider/storage policy in ADR-002.
- `storage_keychain_decision()` is the code-level launch decision and
  `covers_adr_006()` is the executable coverage assertion for this ADR.

This is intentionally honest: app state has a SQLite-compatible schema manifest
and migration contract, but the currently shipped app-state persistence file is
an encrypted JSON envelope, not raw SQLite table pages.

## Keychain and key wrapping

The keychain boundary is `AppDbKeychain`:

1. On first save, `EncryptedAppDb` creates a random 32-byte wrapping key if the
   configured keychain has no `local-appdb-wrapping-key-v1` entry.
2. Each save creates a fresh random data key.
3. The wrapping key encrypts the data key into `wrapped_data_key` with
   `wrapped_key_nonce`.
4. The data key encrypts the serialized app-state payload with `data_nonce`.
5. The data key is zeroized after wrapping and after decrypting the envelope.
6. The persisted file never contains the wrapping key or plaintext app state.

The production keychain crate decision is `keyring 3.6.3` behind the
`production-storage` feature. On Linux, `LinuxOsKeychain` uses the Secret Service
sync provider. `MemoryAppDbKeychain` exists only for tests, harnesses,
local-development, and non-production builds.

## WAL, journal, and corruption policy

`EncryptedAppDb` writes an encrypted temp file and then renames it into place.
Because the persisted app-state file is not a live SQLite database, it should not
create plaintext SQLite WAL pages. The policy still treats conventional sidecars
as sensitive because future SQL-backed stores and OpenMLS stores may have them:

- `sqlite_wal_path(path)` returns the conventional `-wal` sidecar path used by
  leakage checks.
- `quarantine_corrupt_store(path)` moves the primary DB and any `-wal`, `-shm`,
  and `-journal` sidecars into timestamped quarantine files before reopening a
  fresh store.
- Tests assert encrypted app-state plaintext does not appear in the envelope,
  WAL path, or temp file.

## Schema migrations

`APP_DB_SCHEMA_VERSION` is `1`; `MIN_SUPPORTED_APP_DB_SCHEMA_VERSION` is `0`.
`AppDbMigrationPlan` supports:

- `0 -> 1` forward creation of all required tables and indexes in `VERSION_1_DDL`.
- `1 -> 0` rollback for recovery tests with `VERSION_1_ROLLBACK`.
- `1 -> 1` no-op.
- rejection of future versions before opening state.

`validate_observed_schema` checks required tables and columns from `AppDbSchema`
so corrupt or partial stores fail closed. Sensitive columns are explicit key
references or ciphertext-only fields.

## Secure-delete limits

Secure delete is a best-effort local operation. `SecureDeleteSimulator` models
three properties required by the production checklist: enumerate local DB files,
WAL/journal sidecars, and key-store entries; take a snapshot before destructive
work; and restore if verification fails. The product must not promise impossible
delete semantics: SSD wear-leveling, OS backups, external sync, and cloud snapshot copies can retain older bytes outside Discrypt's control.

Release copy and UX must describe shredding as local best-effort verified
enumeration, not guaranteed physical erasure.

## Platform differences

| Platform/build | Launch policy |
| --- | --- |
| Linux + `production-storage` | Uses `keyring 3.6.3` Secret Service through `LinuxOsKeychain`. |
| Tests/harness/local-dev/non-production | May use `MemoryAppDbKeychain`; never a production claim. |
| macOS | Requires a Keychain Services adapter before a production-storage claim. |
| Windows | Requires a Credential Manager/DPAPI adapter before a production-storage claim. |
| Android/iOS | Requires mobile keystore/keychain adapters and Tauri mobile integration before a production-storage claim. |

## Verification

Required gates for this decision:

1. `cargo test -p discrypt-storage storage_keychain_decision_covers_adr_006 --quiet`
   proves the code-level decision covers the ADR axes.
2. `cargo test -p discrypt-storage encrypted_app_db_round_trips_without_plaintext_in_db_or_wal --quiet`
   proves plaintext does not appear in the encrypted file, WAL path, or temp path.
3. `cargo test -p discrypt-storage encrypted_app_db_persists_wrapped_key_separately_from_keychain --quiet`
   proves the persisted envelope contains only a wrapped data key and requires
   the keychain wrapping key.
4. `cargo test -p discrypt-storage migration_from_empty_store_creates_required_schema --quiet`
   proves the forward schema contract.
5. `cargo test -p discrypt-storage backward_migration_drops_required_schema_for_recovery_tests --quiet`
   proves rollback coverage for recovery tests.
6. `cargo test -p discrypt-storage corrupt_store_quarantine_moves_db_and_sidecars --quiet`
   proves corrupt DB/WAL/SHM quarantine.
7. `cargo test -p discrypt-storage secure_delete_removes_material_and_snapshot_restores_on_failed_verify --quiet`
   proves best-effort secure-delete enumeration and rollback behavior.

## Consequences

- Launch storage claims are precise: encrypted app envelope plus SQLite-compatible
  schema contract, not a hidden plaintext SQLite app DB.
- Linux production storage has a concrete OS keychain implementation.
- Other desktop/mobile platforms remain gated until their OS keychain adapters are
  implemented and tested.
- Future SQL-backed app storage can reuse the schema/migration manifest, but must
  add its own WAL encryption/leakage proof before replacing `EncryptedAppDb`.

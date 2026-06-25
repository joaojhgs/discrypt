# PER-79 Crypto-Shred Tombstones Evidence

## Scope

PER-79 / P9-T05 adds local backend/storage evidence for cooperative crypto-shred tombstones. This is local Rust model evidence only; it is not production split-machine multi-device identity, backup/recovery, UI, packaging, or release-readiness evidence.

## Implemented Contract

- `crates/content-keys/src/lib.rs` syncs global tombstones to online own devices when tombstones are created, merged, or when a new online own device registers.
- `crates/storage/src/lib.rs` records persisted message tombstones, changes cached content-key state to `Shredded`, and makes harness decrypt fail with `LocalDecryptError::Shredded` while preserving ciphertext/history records.
- Tombstone state survives app-state serialization/restart and merges idempotently across own-device snapshots.

## SQLite/WAL Caveat

Current app state uses `EncryptedAppDb`, an encrypted envelope with a SQLite-compatible schema contract, not plaintext SQLite pages. PER-79 scan evidence is therefore limited to local model checks that key material is removed from enumerated key-store/WAL simulator paths and is not required in ciphertext-only app-state bytes. Secure delete remains best-effort and cannot promise erasure from SSD wear-leveling, backups, cloud snapshots, crash dumps, screenshots, exports, or modified clients.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-content-keys shred -- --test-threads=1` — passed, 3 targeted shred/retention tests.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage crypto_shred -- --test-threads=1` — passed, 4 targeted crypto-shred tests.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage secure_delete_removes_material_and_snapshot_restores_on_failed_verify -- --test-threads=1` — passed.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-content-keys -- --test-threads=1` — passed, 14 tests.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage -- --test-threads=1` — passed, 39 tests.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check` — passed.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-content-keys --lib -- -D warnings` — passed.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-storage --lib -- -D warnings` — passed.
- `git diff --check` — passed.

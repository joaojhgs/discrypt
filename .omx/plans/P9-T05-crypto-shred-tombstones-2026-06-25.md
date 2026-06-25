# P9-T05 Crypto-Shred Tombstones Plan

Source: PER-79 / P9-T05 from Phase 9 text/history, retention, crypto-shred, and multi-device. The named master plan file is absent in this checkout; current scope is taken from PER-79 issue metadata, `.omc/plans/discrypt-plan.md` D8/AC11, `docs/phase-4-retention-shred-recovery.md`, `docs/adr/adr-006-storage-keychain.md`, and the PER-78 retention implementation.

## Scope

- Extend `crates/content-keys/src/lib.rs` tombstone sync primitives so newly online or newly registered own devices converge on the global cooperative shred set.
- Extend `crates/storage/src/lib.rs` local storage models so a cooperative shred records a persisted tombstone, removes cached content-key material, and makes later decrypt attempts fail closed while keeping ciphertext/history records.
- Preserve PER-78 retention semantics: locked/shredded states must never be resurrected by a later retention window change.
- Add focused unit evidence for negative decrypt, tombstone merge/sync, restart persistence, and SQLite/WAL/key-store material scan caveats.

## Acceptance Criteria

- Shredding a message with a cached key changes the local key state to `Shredded`, removes decrypt authority, and a harness decrypt returns a typed `Shredded` error.
- Tombstones merge idempotently across own devices and survive app-state serialization/restart.
- A newly online or newly registered online own device receives existing tombstones and cannot serve the shredded message.
- Existing retention transition tests still prove no resurrection of locked or shredded keys.
- WAL/SQLite evidence is honest: current app state is an encrypted envelope, not plaintext SQLite pages; local scan tests verify no key bytes remain in the primary app DB, conventional WAL sidecar, or key-store simulator after shred, while docs retain SSD/backup/snapshot caveats.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-content-keys shred -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage crypto_shred -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage secure_delete_removes_material_and_snapshot_restores_on_failed_verify -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-content-keys --lib -- -D warnings`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-storage --lib -- -D warnings`
- `git diff --check`

## Risk Boundaries

- This is local Rust storage/content-key model evidence, not production split-machine multi-device identity, backup/recovery, UI, packaging, or release-matrix evidence.
- Cooperative shred is best-effort across own devices: online devices converge immediately; offline devices remain pending until reconnect.
- Secure delete remains best-effort local enumeration. It does not promise erasure from SSD wear-leveling, OS backups, crash dumps, cloud snapshots, screenshots, exports, or modified recipients.

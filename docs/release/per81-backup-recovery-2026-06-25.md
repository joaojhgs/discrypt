# PER-81 Backup/Recovery Evidence

## Scope

PER-81 / P9-T07 adds local backend/storage evidence for explicit account-continuity backup and recovery flows. This is local Rust storage model evidence only; it is not production split-machine recovery, native keyring/vault restore UX, UI recovery screens, packaging, or release-readiness evidence.

## Implemented Contract

- `crates/storage/src/lib.rs` now exposes versioned `AccountBackupExport` metadata for backend-created account-continuity backups.
- Backup metadata persists across JSON serialization/restart and records version, created time, exporting own device, recovery method, and compromised-device rotation requirements.
- Restore validates the backup envelope before returning account continuity and never restores archival content keys.
- Malformed/corrupt backup restore fails closed with typed `RecoveryError` variants and does not overwrite existing app-state bytes.
- Lost-password recovery is explicit: without both a stored verifier and user-held recovery code it returns `NoTrustMaterial`; wrong codes return `InvalidRecoveryCode`.
- Device-compromise recovery is explicit: compromised-device backups return `DeviceRotationRequired` until the caller supplies distinct replacement-device rotation evidence.

## Deletion-Control Boundary

Backups remain account-continuity only. They include room membership/device-count continuity and sealed identity continuity metadata, not per-message archival content keys. Restore therefore cannot decrypt messages whose content keys were shredded or retention-locked. User-made external backups, screenshots, exports, modified clients, OS snapshots, and cloud copies remain outside Discrypt's deletion control.

## Verification

Local verification from repository root on 2026-06-25:

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage account_backup -- --test-threads=1` - passed, 2 tests.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage lost_password -- --test-threads=1` - passed, 1 test.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage compromised_device_backup -- --test-threads=1` - passed, 1 test.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage malformed_backup -- --test-threads=1` - passed, 1 test.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage -- --test-threads=1` - passed, 43 tests plus doc tests.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check` - passed.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-storage --lib -- -D warnings` - passed.
- `git diff --check` - passed.

Re-run commands:


```sh
RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage account_backup -- --test-threads=1
RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage lost_password -- --test-threads=1
RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage compromised_device_backup -- --test-threads=1
RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage malformed_backup -- --test-threads=1
RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage -- --test-threads=1
RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check
RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-storage --lib -- -D warnings
git diff --check
```

## Artifacts

- Plan: `.omx/plans/P9-T07-backup-recovery-2026-06-25.md`
- Release evidence: `docs/release/per81-backup-recovery-2026-06-25.md`
- Durable goal state: `.omx/ultragoal/goals.json` goal `G028-per-81-p9-t07-backup-recovery`

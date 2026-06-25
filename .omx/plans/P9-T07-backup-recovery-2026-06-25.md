# P9-T07 Backup/Recovery Plan

Source: PER-81 / P9-T07 from Phase 9 text/history, retention, crypto-shred, and multi-device. The issue metadata names `.omx/plans/production-release-master-plan-2026-06-10.md`, but this checkout does not contain that master plan file. Scope is taken from PER-81 metadata, `.omc/plans/discrypt-plan.md` AC-RECOVERY/R22, existing P9-T04/P9-T05/P9-T06 artifacts, and the current `crates/storage` recovery primitives.

## Scope

- Add explicit account-continuity backup export/restore behavior in `crates/storage/src/lib.rs`.
- Keep backups account-continuity only: identity continuity metadata, device count, and room membership; no archival content-key vault and no ability to resurrect shredded/locked content.
- Model lost-password recovery as non-recoverable without a stored verifier plus user-held recovery code.
- Model compromised-device recovery as fail-closed until replacement-device rotation evidence is supplied.
- Prove persisted recovery metadata survives serialization/restart and malformed/corrupt backups do not overwrite existing app state.
- Add release evidence under `docs/release/` with exact commands, artifacts, and production-readiness caveat.

## Acceptance Criteria

- Backup export is versioned and carries persisted metadata: creation time, exporting device id, recovery method, and compromised-device rotation requirement.
- Restore accepts only valid versioned account-continuity backups and returns `content_keys_restored=false`.
- Malformed/corrupt backup restore returns a typed recovery error and leaves existing app state bytes unchanged.
- Lost-password recovery without verifier/code returns `NoTrustMaterial`; wrong code returns `InvalidRecoveryCode`.
- Compromised-device backups return `DeviceRotationRequired` until a distinct replacement device id is supplied.

## Implementation Steps

1. Extend `crates/storage/src/lib.rs` around `AccountBackup`/`RecoveryMaterial` with versioned export metadata, JSON restore validation, lost-password helper, and compromised-device rotation-gated restore.
2. Add focused storage tests for export/restore persistence, malformed backup fail-closed behavior, lost-password trust material, and device-compromise rotation gating.
3. Add release evidence at `docs/release/per81-backup-recovery-2026-06-25.md`.
4. Verify with targeted storage tests, full storage crate tests, `cargo fmt --check`, storage clippy, and `git diff --check`.

## Risks and Mitigations

- Recovery could weaken PER-79 deletion control if it carries content keys. Mitigation: backup export only wraps the existing `AccountBackup` account-continuity model and tests assert `content_keys_restored=false`.
- A corrupt restore could reset an unreadable profile. Mitigation: restore helpers validate before returning recovery material and tests prove existing app-state bytes remain unchanged on restore failure.
- Device compromise could be treated as ordinary restore. Mitigation: compromised-device exports require distinct replacement-device evidence before restore.
- This does not implement UI flows, full native keyring/vault restore UX, split-machine recovery, or release readiness. Evidence must label the result as local Rust storage model evidence.

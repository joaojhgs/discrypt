# P1-T01 Storage Schema Quarantine Plan

## Requirements Summary

Source issue: PER-7 / P1-T01, Phase 1 storage foundation. The production-release master plan named by the issue is not present in this checkout, so this plan uses the issue text, `docs/release/handoff-2026-06-10-current-state.md`, and `.omc/plans/discrypt-plan.md` as available context.

Relevant paths:
- `crates/storage/src/appdb.rs`: encrypted app DB envelope, schema manifest, migration planner, corrupt-store quarantine helper.
- `apps/desktop/src-tauri/src/lib.rs`: typed app-state schema, load/persist behavior, storage security UI state.

Invariant: existing unreadable storage must not be overwritten or silently reset to first-run. Invite/OpenMLS/transport state remains out of scope except that persisted app state must not be lost.

## Acceptance Criteria

- Missing `schema_version` is treated as an explicit legacy app-state migration to schema 1 and persists the migrated version only after successful decode.
- Old supported app-state schema versions migrate by explicit rule.
- Future/unsupported schema versions fail closed with a typed persistence error and recovery hint.
- Corrupt/unreadable state fails closed with a typed persistence error; the existing bytes remain preserved.
- Regression tests cover missing schema version, old store, corrupt store preservation, and future-version rejection.

## Implementation Steps

1. Add a small schema-version classifier around raw persisted JSON in `apps/desktop/src-tauri/src/lib.rs` before deserializing with serde defaults.
2. Replace the broad schema mismatch branch with explicit current, legacy-migrate, unsupported-old, and future-version outcomes.
3. Persist migrated legacy state through the existing `AppStore` only after successful decode and normalization.
4. Add targeted desktop unit tests for missing schema, old schema, corrupt bytes preservation, and future schema rejection.
5. Run focused Rust tests for the desktop persistence path and storage app DB tests; run format check if toolchain availability allows.

## Failure Modes And Safety

- Corrupt JSON: do not write, delete, or rename the store in the desktop loader; surface `state_decode_failed`.
- Future schema: do not downgrade or overwrite; surface `state_schema_future`.
- Legacy schema decode failure: do not overwrite; surface `state_decode_failed`.
- Legacy migration save failure: load the migrated state but surface the save error so the UI does not claim persistence.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml persisted_state_without_schema_version_migrates_to_current_schema`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml persisted_old_schema_version_migrates_to_current_schema`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml corrupt_persisted_state_surfaces_recovery_error_instead_of_silent_first_run`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage appdb`

## Stop Condition

Stop at local/harness evidence and PR handoff. Do not claim production-ready storage until QA and architect review confirm the PR and any required CI evidence.

# P11-T05 - MLS/Admission Diagnostic Report

Issue: PER-94 / P11-T05.

## Requirements Summary

Source context:
- PER-94 requires separate stable redacted diagnostic codes for missing OpenMLS handle, pending admission, revoked member, missing Welcome, and fork mismatch.
- `docs/release/handoff-2026-06-10-current-state.md` keeps invite/admission behavior in the current not-production-ready evidence boundary.
- `.omc/plans/discrypt-plan.md` requires invite parsing to remain insufficient for membership; final group admission requires an authorized MLS add/commit or Welcome and persisted OpenMLS state.
- Existing Phase 11 plans `P11-T01` and `P11-T04` establish the backend support-bundle pattern: stable schema, timestamp, recovery hint, redacted context, and no raw secrets.
- The named production master plan file is absent in this checkout; issue body/metadata, current release docs, original OMC context, and adjacent committed plans are the active constraints.

## Acceptance Criteria

- App state and diagnostics export include a `discrypt.mls_admission_diagnostic.v1` report derived from backend state.
- The report distinguishes `mls_missing_openmls_handle`, `mls_admission_pending`, `mls_member_revoked`, `mls_welcome_missing`, and `mls_fork_mismatch` with actionable recovery hints.
- Invite parsing alone never reports admitted/joined membership; pending groups without OpenMLS handles remain pending in diagnostics.
- Report context uses redacted refs/hashes only and excludes group secrets, Welcome payload bytes, epoch/exporter secrets, private keys, raw member ids, provider credentials, and raw store dumps.
- Support bundle export includes the latest report alongside existing storage/transport diagnostics.

## Implementation Steps

1. Add serializable MLS/admission diagnostic report types in `apps/desktop/src-tauri/src/lib.rs` near existing diagnostic views.
2. Implement a classifier over `PersistedAppState` plus relevant `CommandErrorView` evidence, using persisted OpenMLS handles, group role/member status, pending admission requests, queued Welcome frames, and last MLS/admission command failures.
3. Include the report in `AppStateView::to_view()` and `export_diagnostics_log()`.
4. Add focused backend tests for all five required diagnostic codes, invite-is-not-membership behavior, support-bundle presence, and redaction.
5. Add release evidence under `docs/release/` and run targeted Tauri tests, `cargo fmt --check`, and `git diff --check`.

## Failure Modes And Safety

- Missing OpenMLS handle: report fail-closed; do not create a replacement handle or claim membership from invite metadata.
- Pending admission: report waiting state until an authorized Welcome/add is applied and persisted.
- Revoked member: report denied state and require future owner/staff restoration flow; do not allow protected text/voice.
- Welcome missing: report owner/staff or transport admission delivery gap without exposing Welcome payloads.
- Fork mismatch: classify confirmation-tag/tree mismatch as a fork/downgrade/replay risk and require deterministic recovery/rejoin; never silently accept.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib mls_admission_diagnostic_report -- --test-threads=1 --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: deterministic local Rust/Tauri backend diagnostics evidence. It is support-bundle compatible and redaction-focused, not real installed-app production recovery evidence.

# PER-94 MLS/admission diagnostic report evidence

Date: 2026-06-25

## Scope

Implemented local backend diagnostics for P11-T05. The report is exposed as
`mls_admission_diagnostic_report` in app state and the diagnostics support
bundle.

## Behavior Covered

- Stable schema: `discrypt.mls_admission_diagnostic.v1`.
- Stable fail-closed codes:
  - `mls_missing_openmls_handle`
  - `mls_admission_pending`
  - `mls_member_revoked`
  - `mls_welcome_missing`
  - `mls_fork_mismatch`
- Invite parsing alone remains pending and does not report membership without a
  persisted OpenMLS handle.
- Context uses redacted refs and low-cardinality counts rather than raw group
  ids, raw member ids, Welcome payloads, key packages, exporter/epoch secrets,
  provider credentials, or private keys.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib mls_admission_diagnostic_report -- --test-threads=1 --nocapture`

Result: passed locally, 2 tests.

Artifact:

- `target/per94-mls-admission-diagnostic-report/diagnostics-log.json`

## Evidence Boundary

This is deterministic local Rust/Tauri backend diagnostics evidence. It is
support-bundle compatible, but it is not installed-app production recovery,
split-machine MLS fork recovery, or live public-provider evidence.

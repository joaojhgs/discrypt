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
- Recovery hints remain actionable static guidance in app state and diagnostics
  export; conceptual references to an OpenMLS Welcome are not treated as secret
  payload material.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib mls_admission_diagnostic_report -- --test-threads=1 --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib g010_native_command_e2e_setup_group_invite_text_voice_is_honest -- --test-threads=1 --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib g009_observable_copy_redacts_sensitive_classes -- --test-threads=1 --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `npm --prefix apps/ui run typecheck`
- `git diff --check`

Result: passed locally. The regenerated diagnostics artifact reports
`mls_welcome_missing`, `fail_closed=true`, and keeps the aggregate and per-group
recovery hint as `Refresh owner/staff admission delivery and require an
authorized OpenMLS Welcome before marking the joiner admitted.`

Artifact:

- `target/per94-mls-admission-diagnostic-report/diagnostics-log.json`

## Evidence Boundary

This is deterministic local Rust/Tauri backend diagnostics evidence. It is
support-bundle compatible, but it is not installed-app production recovery,
split-machine MLS fork recovery, or live public-provider evidence.

# P11-T01 - Redacted Structured Logs

Issue: PER-90 / P11-T01

## Requirements Summary

Source context: The issue points at `.omx/plans/production-release-master-plan-2026-06-10.md`, but that file is absent from this checkout. The active constraints are the PER-90 issue body/metadata, `docs/release/handoff-2026-06-10-current-state.md`, `.omc/plans/discrypt-plan.md`, and the existing diagnostics/export work in `.omx/plans/P7-T05-route-diagnostics-export-2026-06-21.md`.

Acceptance:
- Backend/Tauri command errors include a stable command name, stable error code, actionable recovery hint, timestamp, and redacted context.
- Structured log output serializes those fields without raw secrets, tokens, vault/key material, provider credentials, SDP, ICE credentials/candidates, plaintext, media, or key material.
- The diagnostics support bundle exposes structured log evidence through the same redaction boundary.
- This task does not add a log viewer/export UI, crash reporting, package/release matrix work, or broader storage/MLS/ICE diagnostic reports.

## Implementation Steps

1. Extend `CommandErrorView` in `apps/desktop/src-tauri/src/lib.rs` with defaulted `timestamp` and `redacted_context` fields for backward-compatible persisted state decoding.
2. Centralize command-error construction so normal command failures, persistence failures, and storage-security failures redact message, recovery hint, and context consistently.
3. Emit structured JSON command-error log lines from `push_command_error` and include a structured command-error entry in `export_diagnostics_log()`.
4. Add targeted backend tests for typed command error timestamp/context, structured log serialization, support-bundle inclusion, and redaction of representative forbidden strings.
5. Run targeted desktop tests, format check, diff check, and a secret scan over the generated diagnostics artifact.

## Failure Modes And Safety

- Legacy persisted errors may not have the new fields; serde defaults must keep old state loadable.
- Redaction must fail closed for known sensitive observable classes by replacing the whole sensitive string with class labels, not partial masking.
- Diagnostics remain evidence-only and must not imply joined, connected, delivered, admitted, or production-ready state.
- Providers remain signaling/rendezvous only; this task adds no application relay path.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib redacted_structured_command_logs -- --test-threads=1 --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib typed_command_errors_surface_actionable_codes -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`
- Secret scan over the generated diagnostics artifact under `target/per90-redacted-structured-logs/`.

Evidence classification: local Rust/Tauri backend diagnostics evidence. This is not production support workflow, crash-report, installed-app, or release-matrix evidence.

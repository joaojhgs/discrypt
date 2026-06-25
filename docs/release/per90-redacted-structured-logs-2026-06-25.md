# PER-90 Redacted Structured Logs Evidence - 2026-06-25

## Scope

PER-90 / P11-T01 adds a backend diagnostics foundation for redacted structured command-error logs. This is local Tauri backend evidence only. It does not implement a log viewer, crash reporting, storage/MLS/ICE diagnostic reports, packaging, release-matrix promotion, or production support workflow.

## Implemented Behavior

- `CommandErrorView` now includes a stable command, stable code, RFC3339 timestamp, redacted message, redacted recovery hint, and redacted structured context.
- Command-error stderr output now emits `discrypt.command_error.v1` JSON entries from the same redacted command-error object.
- `export_diagnostics_log()` includes `structured_logs.last_command_error` so support bundles expose the structured log entry without raw secrets.
- Redaction classes now cover provider credentials, bearer/API/access/refresh tokens, vault passwords/passphrases, private/secret keys, SDP/ICE/TURN material, plaintext, SFrame/content/MLS key material, and invite/room seed patterns.

## Evidence

Artifact:
- `target/per90-redacted-structured-logs/diagnostics-log.json`

Commands run:
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib redacted_structured_command_logs -- --test-threads=1 --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib typed_command_errors_surface_actionable_codes -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`
- Secret scan over `target/per90-redacted-structured-logs/diagnostics-log.json` for seeded PER-90 token, API key, vault passphrase, private key, provider credential, raw SDP/ICE candidate, TURN credential, plaintext, SFrame key, content key, and MLS exporter strings returned no matches.

## Evidence Boundary

This is local Rust/Tauri backend diagnostics evidence. It proves typed command-error structure and redaction for representative sensitive log/support-bundle content. It is not installed-app production support evidence and does not promote Discrypt to production-ready.

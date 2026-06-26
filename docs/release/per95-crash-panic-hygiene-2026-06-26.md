# PER-95 Crash/Panic Hygiene Evidence - 2026-06-26

## Scope

PER-95 / P11-T06 adds local Tauri backend panic hygiene. It does not add an external crash reporter, upload service, packaging gate, UI log viewer, or production support workflow.

## Implemented Behavior

- Tauri startup installs a panic hook before runtime setup work starts.
- Panic output is emitted as a structured `discrypt.panic.v1` JSON entry.
- Panic payloads, recovery hints, and context use the existing Phase 11 redaction boundary.
- The panic entry omits raw backtraces and raw source paths; source location is represented by a redacted file reference plus line/column.
- `export_diagnostics_log()` includes `structured_logs.last_panic` so support bundles can carry redacted crash evidence.

## Evidence

Artifact:
- `target/per95-crash-panic-hygiene/panic-diagnostics-log.json`

Verification commands:
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib crash_panic_hygiene -- --test-threads=1 --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib g009_observable_copy_redacts_sensitive_classes -- --test-threads=1 --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --lib -- -D warnings`
- `git diff --check`
- `rg -n "per95-provider|per95-token|per95-api|per95-vault|per95-password|per95-private|per95-ufrag|per95-ice|per95-candidate|per95-turn|per95-body|per95-audio|per95-media|per95-content|per95-exporter|per95-epoch|per95-welcome|per95-openmls-welcome|per95-key-package|per95-member|per95-profile|per95-store-dump|app-state\\.discrypt-store" target/per95-crash-panic-hygiene/panic-diagnostics-log.json` returned no matches.

Secret scan expectation:
- The forced-panic test seeds provider credential, bearer token, API key, vault passphrase/password, private key, raw SDP, ICE credentials/candidates, TURN credential, plaintext message, audio plaintext, SFrame key, content key, MLS exporter/epoch secret, Welcome payload/bytes, serialized OpenMLS Welcome, serialized key package, raw member id, profile name, and store dump strings.
- The structured panic line and diagnostics artifact must contain none of those raw values.

## Evidence Boundary

This is deterministic local Rust/Tauri backend diagnostics evidence. It proves the in-process panic hook and support-bundle serialization redact representative sensitive crash material. It is not production installed-app crash reporter evidence and does not promote Discrypt to production-ready.

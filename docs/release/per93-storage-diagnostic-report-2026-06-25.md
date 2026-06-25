# PER-93 Storage Diagnostic Report Evidence - 2026-06-25

## Scope

PER-93 / P11-T04 adds a redacted storage diagnostic report for keyring, vault,
schema, and preserve/quarantine failures. This is local Rust/Tauri backend
diagnostics evidence. It does not implement a storage recovery wizard, live
KDE/KWallet prompt automation, package reinstall proof, or production release
promotion.

## Implemented Behavior

- `AppStateView.storage_diagnostic_report` and support-bundle
  `storage_diagnostic_report` now expose schema `discrypt.storage_diagnostic.v1`.
- Reports include timestamp, storage status/mode, stable diagnostic code,
  failure class, fail-closed flag, production-storage boundary, recovery hint,
  and redacted context.
- Deterministic classification covers keyring missing/denied/unavailable/corrupt
  entry, password-vault wrong-password/corrupt-vault/decode, schema/decode, and
  preserve/quarantine failures.
- Context uses redacted observable refs instead of raw keyring details or paths.
  The diagnostic path does not create replacement keyring/vault material.

## Evidence

Artifact:
- `target/per93-storage-diagnostic-report/diagnostics-log.json`

Commands run:
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib storage_diagnostic_report -- --test-threads=1 --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage appdb -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `npm --prefix apps/ui ci`
- `npm --prefix apps/ui run typecheck`
- `git diff --check`

## Evidence Boundary

This is deterministic local backend diagnostics evidence. Live KDE/KWallet or
GNOME Keyring prompt behavior still needs a display/keyring-capable installed-app
smoke before this can be cited as real platform evidence.

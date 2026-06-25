# P11-T04 - Storage Diagnostic Report

Issue: PER-93 / P11-T04.

## Requirements Summary

Source context:
- PER-93 requires actionable storage diagnostics for keyring/vault failures without leaking secrets.
- `docs/release/handoff-2026-06-10-current-state.md` keeps Discrypt not production-ready and names storage vault reinstall as a current regression area.
- `docs/release/storage-security-roadmap.md` requires preserving unreadable storage and surfacing whether the failure is missing keyring material, wrong password, moved vault/profile files, unsupported schema, or corrupt bytes.
- Adjacent plans `P1-T01`, `P1-T02`, and `P11-T01` already define fail-closed storage decode/schema behavior, password-vault durability, and redacted structured command-error logs.
- The named production master plan path is absent in this checkout; issue body, metadata, current release docs, original OMC context, and adjacent plans are the active constraints.

## Acceptance Criteria

- Storage diagnostics expose a stable schema with RFC3339 timestamp, storage mode, stable error code, failure class, recovery hint, and redacted context suitable for support bundles.
- Keyring unavailable/denied/missing/corrupt-entry, password-vault wrong-password/corrupt-vault/decode, unsupported/missing schema, and quarantine/preserve failures are classified into actionable stable codes.
- Support bundle export includes the latest storage diagnostic report and does not expose vault material, wrapping keys, passwords, profile names, provider credentials, raw store bytes, raw paths, or plaintext.
- Existing unreadable app-state and vault/keyring material remain preserved; no diagnostic path creates replacement storage or claims restore.
- Production and development storage modes remain distinct in diagnostics.

## Implementation Steps

1. Add serializable storage diagnostic report types and a classifier around existing `CommandErrorView`/storage-security state in `apps/desktop/src-tauri/src/lib.rs`.
2. Reuse PER-90 redaction for messages, recovery hints, and context; expose low-cardinality mode/path/hash evidence only.
3. Include the report in `AppStateView` and `export_diagnostics_log()` so support bundles carry storage evidence.
4. Add deterministic unit tests for keyring missing/denied/unavailable/corrupt-entry, password-vault wrong-password/corrupt-vault/decode, schema/quarantine classes, dev-vs-production mode, timestamp validity, and redaction.
5. Add release evidence under `docs/release/` and run targeted desktop/storage tests, formatting, and diff checks.

## Failure Modes And Safety

- Missing keyring/vault material: report `fail_closed=true`, recommend retrying the same OS session/password/original profile directory, and do not generate new wrapping keys over existing encrypted state.
- Wrong password or corrupt vault: report password-vault failure without echoing password, vault contents, or raw path.
- Unsupported/future schema or decode failure: report preservation/quarantine guidance without writing replacement state.
- Live KDE/KWallet evidence may be unavailable in this container; deterministic injected-failure tests are acceptable local evidence, with real-platform evidence left as an explicit follow-up.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib storage_diagnostic_report -- --test-threads=1 --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage appdb -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: deterministic local Rust/Tauri backend diagnostics evidence. It is support-bundle compatible but not real KDE/KWallet installed-app production evidence unless a live platform smoke is run separately.

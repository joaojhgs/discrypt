# P1-T02 Password Vault Reinstall Durability Plan

## Requirements Summary

Source issue: PER-8 / P1-T02, Phase 1 storage foundation. The production-release master plan named by the issue is not present in this checkout, so this plan uses the issue text, `docs/release/handoff-2026-06-10-current-state.md`, `docs/release/current-regressions.md`, `docs/release/release-gap-matrix-2026-06-15.md`, `docs/release/storage-security-roadmap.md`, and `.omc/plans/discrypt-plan.md` as available context.

Relevant paths:
- `crates/storage/src/appdb.rs`: encrypted app-state envelope and production passphrase-vault wrapping-key store.
- `apps/desktop/src-tauri/src/lib.rs`: production Linux storage gate and password-vault unlock flow.
- `scripts/smoke-linux-packages.mjs`: Linux package install smoke coverage.

Invariant: existing unreadable encrypted app state must not be overwritten or silently reset. Password-vault mode must require the same password and vault material across restart/reinstall before claiming storage is ready.

## Acceptance Criteria

- Same passphrase and same persisted vault file unlock the same encrypted app-state envelope across a fresh process/app instance.
- If an encrypted app-state envelope exists but the selected keychain/vault has no wrapping key, saving fails closed instead of creating a replacement wrapping key.
- Wrong passphrase or missing vault material preserves the existing encrypted app-state bytes.
- Regression coverage includes an isolated file/vault durability test and package-smoke planning for `.deb` reinstall.

## Implementation Steps

1. Tighten `EncryptedAppDb::load_or_create_wrapping_key` so existing encrypted app-state files treat `Ok(None)` from the keychain as `KeychainMissing` rather than generating a new wrapping key.
2. Add production-storage tests proving passphrase-vault encrypted app-state survives a fresh keychain instance with the same password and rejects wrong password/missing vault without modifying files.
3. Extend Linux package smoke script to exercise `.deb` install, launch, reinstall, and relaunch with the same HOME/XDG data path so package reinstall cannot clear local state silently.
4. Run targeted storage tests and package-smoke dry-run/checks; run real `.deb` smoke if release bundles and container runtime are available.

## Failure Modes And Safety

- Missing vault/keyring entry with existing encrypted app-state: return `KeychainMissing`; do not generate a new wrapping key.
- Wrong password: AES-GCM decrypt failure propagates; do not write the app-state file or vault file.
- Fresh first run: app-state file is absent, so creating the initial wrapping key remains allowed.
- Package reinstall: app data under HOME/XDG must survive package remove/install and relaunch.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage --features production-storage production_passphrase_vault_app_db_survives_fresh_instance_with_same_password -- --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage --features production-storage existing_encrypted_app_db_missing_vault_key_fails_closed_without_overwrite -- --nocapture`
- `node scripts/check-linux-package-smoke.mjs`
- Real `.deb` reinstall smoke via `node scripts/smoke-linux-packages.mjs` after release bundles are available.

## Stop Condition

Stop at PR + QA handoff with exact test evidence. Do not claim production-ready release evidence if the real `.deb` reinstall smoke cannot run in this environment.

## 2026-06-16 Package Linux Unblock Addendum

Observed blocker: manual GitHub package run `27595324488` built Linux package artifacts, then `scripts/generate-sbom-g124.mjs --require-packaged-artifacts` failed because `cargo sbom` was not available in the `package Linux` job. Because `release:linux` exited there, `smoke:linux-packages` never ran and the required real `.deb` reinstall evidence remained missing.

Focused unblock plan:
- Add explicit release cargo-tool provisioning to `.github/workflows/package-desktop.yml` before `npm --prefix apps/ui run release:linux`; `release:linux` currently needs `cargo-sbom` for G124 SBOM generation and `cargo-audit`/`cargo-deny` for G126 reproducibility evidence.
- Run the focused `discrypt-storage --features production-storage` password-vault durability tests in the same Linux package workflow before packaging, because this task's exact regression tests are feature-gated and not covered by default `cargo test --workspace`.
- Keep the package workflow validator in `scripts/check-desktop-package-ci.mjs` aware of that prerequisite.
- Rerun the package workflow on `multica/P1-T02-password-vault-reinstall-durability` with Linux packaging enabled and treat the workflow artifact/log as the real `.deb` reinstall evidence only if `smoke:linux-packages` completes after package build.

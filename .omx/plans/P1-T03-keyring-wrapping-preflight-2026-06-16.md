# P1-T03 keyring wrapping preflight plan

Source issue: PER-9 / P1-T03, Phase 1 storage foundation. The named production-release master plan `.omx/plans/production-release-master-plan-2026-06-10.md` is not present in this checkout, so this plan uses the issue body, `docs/release/handoff-2026-06-10-current-state.md`, `docs/release/storage-security-roadmap.md`, `docs/release/linux-runtime-dependencies.md`, `docs/adr/adr-006-storage-keychain.md`, and the adjacent P1-T02 storage plan.

## Requirements summary

- Keyring preflight must prove a real wrapping-key write/read/delete round trip, not just service discovery.
- The wrapping key must be exactly 32 bytes after loading; invalid native keyring/vault material must fail closed.
- Existing unreadable encrypted app state must remain untouched when keyring/vault preflight or unlock fails.
- Linux setup UI/backend must still offer the password-vault path when OS keyring preflight fails.
- Evidence must distinguish automated local/harness proof from live KDE/KWallet or GNOME Secret Service proof.

## Relevant code paths

- `crates/storage/src/appdb.rs`: `AppDbKeychain`, `LinuxOsKeychain`, wrapping-key decoding, `EncryptedAppDb::load_or_create_wrapping_key`, production vault fallback tests.
- `apps/desktop/src-tauri/src/lib.rs`: storage-security view, `keyring_preflight_status`, setup/unlock commands.
- `apps/ui/src/main.tsx`: first-run storage mode panel showing keyring preflight result and password-vault option.

## Acceptance criteria

- A reusable storage-layer preflight writes a 32-byte probe key, reloads and byte-compares it, deletes it, and verifies deletion.
- Corrupt keyring material with non-32-byte/non-valid-hex encoding returns a typed storage error instead of being accepted.
- Automated tests cover successful preflight, mismatched load, deletion verification, invalid wrapping key length, and no-overwrite behavior for existing encrypted state.
- Live Secret Service/KWallet proof is gated behind an environment variable and skipped honestly when unavailable in this container.

## Failure modes and safety behavior

- OS keyring unavailable: preflight reports unavailable and setup continues to expose password-vault choice.
- OS keyring returns wrong or malformed key: preflight fails, state is not marked ready from keyring evidence.
- Existing encrypted app-state file with missing keychain/vault entry: save/open fails closed and does not create replacement key material.
- Probe cleanup failure: preflight reports failure because persistent probe material would undermine the evidence.

## Implementation steps

1. Add a small storage-layer keychain preflight helper in `crates/storage/src/appdb.rs` that exercises `AppDbKeychain` write/read/delete semantics using a caller-provided probe id and 32-byte key.
2. Replace the Tauri production Linux `keyring_preflight_status` copy with the storage helper while preserving current user-facing recovery copy.
3. Add focused storage tests for successful preflight, wrong loaded key, delete verification, invalid key length, and existing-state no-overwrite behavior.
4. Run format and targeted storage/desktop tests. Attempt live Secret Service E2E only if the current runtime exposes a usable provider; otherwise record it as skipped with reason.
5. Commit, open/update PR, and hand off to QA with exact commands, artifacts, and skipped live-provider evidence.

## Verification strategy

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage keychain_preflight --features production-storage -- --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage invalid_os_keychain_wrapping_key_length_is_rejected --features production-storage -- --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage existing_encrypted_app_db_missing_vault_key_fails_closed_without_overwrite --features production-storage -- --nocapture`
- `DISCRYPT_LINUX_SECRET_SERVICE_E2E=1 RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage linux_secret_service_keychain_live_roundtrip_when_enabled --features production-storage -- --nocapture` only when Secret Service/KWallet/GNOME keyring is available.

## Scope boundary

This task does not implement storage migration/recovery UX, macOS/Windows keychain adapters, or claim production readiness. It only hardens Linux production-storage preflight and automated regression evidence.

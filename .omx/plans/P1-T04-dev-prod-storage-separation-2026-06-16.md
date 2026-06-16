# P1-T04 dev/prod storage separation plan

Source issue: PER-10 / P1-T04, Phase 1 storage foundation. The named production-release master plan `.omx/plans/production-release-master-plan-2026-06-10.md` is not present in this checkout, so this plan uses the issue body, `docs/release/handoff-2026-06-10-current-state.md`, `docs/release/storage-security-roadmap.md`, `docs/adr/adr-006-storage-keychain.md`, adjacent P1 storage plans, and `.omc/plans/discrypt-plan.md`.

## Requirements summary

- `cargo tauri dev` must not implicitly open the production host app-state store.
- Local-dev and harness builds may still use `DISCRYPT_APP_STATE_PATH` as an explicit profile override for two-profile and launch evidence.
- Production/package builds must keep using the host production profile path unless explicitly configured through the existing production runtime choices.
- Existing unreadable production storage must remain untouched; this task changes default path selection, not vault/keyring recovery semantics.
- No invite, admission, OpenMLS, transport, or production-readiness claim is in scope.

## Relevant code paths

- `apps/desktop/src-tauri/src/lib.rs`: `app_store_path`, explicit app-state domain helpers, OpenMLS sidecar path derivation, and storage path unit tests.
- `apps/desktop/src-tauri/tauri.conf.json`: normal dev config with `local-dev` and without `production-storage`.
- `apps/desktop/src-tauri/tauri.release.conf.json`: release config with `production-storage` and without `local-dev`.
- `scripts/g010-tauri-two-profile-launch.mjs`: concurrent dev profile launcher using explicit `DISCRYPT_APP_STATE_PATH`.
- New focused static check: prove the config/path/test contract without launching GUI processes.

## Acceptance criteria

- Default local-dev/harness path resolves under a non-production directory such as `discrypt-local-dev`, not the production `discrypt` directory.
- `DISCRYPT_APP_STATE_PATH` remains honored only for test/harness/local-dev explicit profile isolation.
- Calling the production path helper ignores a local-dev override and resolves to the production host profile directory.
- If `production-storage` is ever combined manually with `local-dev` or `harness`, the production storage domain wins outside tests rather than honoring a dev/harness override.
- The no-HOME/no-XDG fallback still includes the selected app domain directory, so fallback paths do not collapse dev and production into the same relative file.
- OpenMLS sidecar paths are derived from the selected app-state path so dev/prod separation applies to protocol sidecars too.
- Static verification ties `tauri.conf.json` to local-dev, `tauri.release.conf.json` to production-storage, and the two-profile harness to explicit per-profile overrides.

## Failure modes and safety

- Accidental `cargo tauri dev` on a host with an existing production profile: the dev build lands in the local-dev domain and does not touch the production app-state file.
- Intentional two-profile dev test: each launched process still receives a distinct `DISCRYPT_APP_STATE_PATH`, preserving existing harness evidence; the launcher rejects `production-storage` because that mode intentionally disables profile overrides.
- Production/package run: the release config does not include local-dev/harness, so it keeps the production app-state path and existing fail-closed storage behavior.
- Accidental mixed production/local-dev/harness features: non-test `production-storage` disables dev/harness env overrides and selects the production domain explicitly.
- Minimal runtime environment without `HOME` or `XDG_DATA_HOME`: relative fallback remains domain-qualified (`discrypt-local-dev/app-state.discrypt-store` or `discrypt/app-state.discrypt-store`).
- Operator explicitly sets `DISCRYPT_APP_STATE_PATH` in local-dev/harness: this remains an explicit override; the handoff must call out that such an override can point anywhere by design.

## Implementation steps

1. Add a small app-state path domain helper in `apps/desktop/src-tauri/src/lib.rs` and route implicit local-dev/harness defaults to `discrypt-local-dev`.
2. Keep explicit `DISCRYPT_APP_STATE_PATH` override behavior unchanged for `cfg(test)`, `harness`, and `local-dev`.
3. Add regression tests for default local-dev path separation, explicit override behavior, production host path preservation, and OpenMLS sidecar derivation.
4. Add a focused Node check and package script that statically validates the dev/release config and two-profile launcher contract.
5. Run format, focused Rust tests, the new static check, and any existing release/no-fallback checks that cover local-dev versus production-storage features.

## Verification strategy

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml dev_app_store_path_uses_local_dev_domain_by_default`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml explicit_env_override_can_select_profile_path_in_local_dev`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml production_app_store_path_stays_on_host_profile_domain`
- `npm --prefix apps/ui run test:p1-t04-dev-prod-storage`
- `npm --prefix apps/ui run test:release-no-fallback-g129`
- `npm --prefix apps/ui run test:g010-tauri-launch-dry-run`

## Stop condition

Stop at PR plus QA handoff with exact commands and artifacts. The result is storage-path separation evidence, not a broad production-ready storage claim.

# PER-78 Retention Windows Evidence - 2026-06-25

## Scope

Implemented local backend/core retention-window evidence for PER-78 / P9-T04:

- Preset, custom seconds, and warned-unlimited retention policy parsing/display in `discrypt-content-keys`.
- Shorten-retroactive and lengthen-future-only key-state transitions.
- No resurrection of already locked or shredded key state after policy lengthening.
- Typed persisted retention policy in storage preferences.
- Backend-computed retention metadata in the core/UI command snapshot (`selected_window_seconds`, `policy_source`).

This is local Rust/backend and UI contract evidence. It is not production split-machine text/history evidence and does not implement broader crypto-shred/tombstone expansion, multi-device identity, backup/recovery, adapter registry, release-gate matrix, or autopilot retrigger.

## Verification

Passed locally:

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-content-keys retention -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-content-keys -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage retention_policy -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-core retention_policy_surface -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-content-keys --lib -- -D warnings`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-storage --lib -- -D warnings`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-core --lib -- -D warnings`
- `npm --prefix apps/ui ci`
- `npm --prefix apps/ui run typecheck`
- `git diff --check`

## Notes

- The initial UI typecheck failed because `tsc` was not installed in the checkout; `npm --prefix apps/ui ci` installed dependencies cleanly and the rerun passed.
- Local branch creation as `multica/P9-T04-retention-windows` failed because the shared repo cache refs directory is read-only. Work was performed on the Multica-generated worktree branch and should be pushed as `HEAD:multica/P9-T04-retention-windows`.

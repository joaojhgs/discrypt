# P9-T04 Retention Windows Plan

Source: PER-78 / P9-T04 from Phase 9 text/history, retention, crypto-shred, and multi-device. The named master plan path is absent in this checkout; current scope is taken from the PER-78 issue metadata, `.omc/plans/discrypt-plan.md` D8/AC10/AC10b, and `docs/phase-4-retention-shred-recovery.md`.

## Scope

- Implement typed retention windows for presets, custom seconds, and warned unlimited in `crates/content-keys/src/lib.rs`.
- Prove shorten-retroactive and lengthen-future-only semantics, including no resurrection of locked or shredded key states.
- Persist backend-owned retention policy state through storage/core restart paths.
- Surface UI/Tauri retention state only from backend snapshot metadata; no frontend-only success claims.

## Acceptance Criteria

- Presets include 1h, 24h, 7d, 30d, and 90d; custom and warned unlimited parse/serialize.
- Shortening a window re-locks existing messages that exceed the new window.
- Lengthening a window applies to messages created after the change and does not unlock already expired/locked or shredded keys.
- Retention policy survives serialization/restart in storage/core tests.
- UI command types include backend-derived policy source and finite window seconds/unlimited state.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-content-keys retention -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage retention_policy -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-core retention_policy_surface -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-content-keys --lib -- -D warnings`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-storage --lib -- -D warnings`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-core --lib -- -D warnings`

## Risk Boundaries

- This is local Rust/backend contract evidence, not full production split-machine text/history evidence.
- This does not implement crypto-shred/tombstone expansion, multi-device identity, backup/recovery, adapter registry, release-gate matrix, or autopilot retrigger.
- Lock-not-vanish is preserved: ciphertext/history records can remain while plaintext/content keys become unavailable.

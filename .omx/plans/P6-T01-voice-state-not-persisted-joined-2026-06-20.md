# P6-T01 - Voice State Not Persisted As Joined

## Requirements Summary

Source: Multica PER-52 / Phase 6 two-person voice chat production path.

Acceptance criterion: app restart must not restore a joined voice channel from persisted UI/state. Joined state is valid only while an active backend media session exists; stale `voice_session` and `voice_channel` active context must fail closed to idle on load.

Relevant paths:
- `apps/desktop/src-tauri/src/lib.rs`: Tauri state persistence, `join_voice`, `leave_voice`, `load_state_from_store`, and `clear_non_persistent_voice_runtime`.
- `apps/ui/src/main.tsx`: startup handling for any backend-returned joined voice session.
- `apps/ui/src/commands.ts`: web fallback voice state, not production truth.

## Implementation Steps

1. Verify the existing backend load/persist boundary strips runtime-only voice session state before it reaches `AppStateView`.
2. Add a focused Tauri/backend regression that injects a stale persisted joined `voice_session` and `voice_channel` active context, reloads through the real store loader, and asserts the resulting view is idle.
3. Run targeted desktop backend tests plus formatting checks. Record local/harness evidence only; do not claim production media readiness.

## Risks And Mitigations

- Risk: persisting a voice session could make the UI claim joined after restart without capture/media route evidence.
  Mitigation: keep `clear_non_persistent_voice_runtime` on both decode and persist paths and cover stale current-schema state directly.
- Risk: clearing active context could disrupt non-voice restart focus.
  Mitigation: clear only `active_context.kind == "voice_channel"`.
- Risk: frontend fallback could fake joined state.
  Mitigation: this task relies on backend/Tauri restart evidence; fallback remains non-production and is not used as proof.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per52 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml stale_persisted_joined_voice_session_is_cleared_on_restart --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per52 cargo fmt --check`

Evidence classification: local backend/Tauri harness evidence for restart-state behavior, not production two-machine voice/media proof.

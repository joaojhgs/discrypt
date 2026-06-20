# P6-T06 - Mute, VAD, And Speaking State

## Requirements Summary

Source: Multica PER-57 / Phase 6 two-person voice chat production path.

Acceptance criteria:
- Mute toggles backend self-mute state only for the active joined voice session.
- Local speaking state reflects media level/VAD evidence from the active capture stream.
- Remote speaking state reflects backend remote-media evidence or decoded/native media proof, not UI optimism.

Relevant context:
- `.omx/plans/P6-T03-voice-channel-ux-2026-06-20.md`: voice UI must render from `AppState.voice_session` and fail closed on permission/device denial.
- `.omx/plans/P6-T04-native-rust-media-session-boundary-2026-06-20.md`: native media proof requires backend voice join/session state and backend-derived peers.
- `.omx/plans/P6-T05-two-person-media-signaling-bridge-2026-06-20.md`: provider-signaled text/control runtime carries sealed voice signals; providers are not media or app-message relays.
- `docs/release/handoff-2026-06-10-current-state.md`: WebRTC/media state needs route evidence, and UI must not claim connected/joined/active without backend evidence.
- `.omc/plans/discrypt-plan.md`: voice is WebRTC with Rust-owned SFrame/OpenMLS media boundaries.

Note: the issue references `.omx/plans/production-release-master-plan-2026-06-10.md`, but that exact file is not present in this checkout. The adjacent committed Phase 6 plans, current-state handoff, and original OMC product plan are the available release context.

## Implementation Steps

1. Keep the existing media crate VAD/mute primitives as the media-level source of truth:
   - `crates/media/src/capture.rs::VoiceActivityDetector`
   - `crates/media/src/capture.rs::VoiceCaptureSFramePipeline::set_muted`
2. Tighten the Tauri command boundary in `apps/desktop/src-tauri/src/lib.rs` so `set_self_mute` rejects missing, stale, or unjoined sessions instead of toggling state on a permission-denied/left session.
3. Preserve `update_voice_activity` as the local speaking-state bridge from real microphone RMS/peak evidence, with self-mute suppressing speaking.
4. Ensure React surfaces voice participants from backend command state only:
   - `apps/ui/src/main.tsx` must not synthesize a remote participant with `speaking: true` merely because a WebRTC stream object exists before backend `attachVoiceRemoteMedia` state lands.
5. Add focused regression coverage:
   - backend unit/Tauri command test for self-mute rejection on permission-denied/unjoined session and speaking suppression while muted.
   - UI/Playwright assertion that mute flips visible participant state from speaking to muted, using the media analyser harness.

## Failure Modes And Safety Behavior

- Permission denied or missing input device: voice session remains unjoined, mute and activity updates reject or stay idle, and no participant/speaking UI is shown.
- Stale/left session: self-mute rejects with a typed `voice_not_joined` error rather than reviving voice controls.
- Muted local capture: speaking is forced false even when RMS/peak evidence crosses threshold.
- Remote stream object without backend media-route evidence: UI may retain the `MediaStream` for later attachment, but no remote participant/speaking claim is rendered until backend state admits the evidence.
- Provider adapters remain signaling-only; no application payload or media fallback is introduced.

## Acceptance Criteria

- `set_self_mute` accepts only the matching active joined session and updates `voice_session.self_muted` plus the local participant `muted` flag.
- Muting clears local participant speaking state; later high microphone levels remain non-speaking while muted.
- Local speaking uses RMS/peak evidence from the media activity sampler or Rust VAD output, not button state.
- Remote participants/speaking indicators render from backend participants/remote media evidence, not temporary UI-only streams.
- UI mute control remains disabled/hidden for non-joined sessions and exposes no debug clutter.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per57 cargo test -p discrypt-media mute_control -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per57 cargo test -p discrypt-media voice_activity_detector -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per57 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml voice_join_mute_volume_leave_flow_does_not_clear_state --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per57 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml voice_join_requires_microphone_permission_and_input_device --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per57 cargo fmt --check`
- `npm --prefix apps/ui run typecheck`
- `npm --prefix apps/ui exec playwright test apps/ui/tests/e2e/stateful-ui.spec.ts -g "voice channel"`

Evidence classification: local media/Tauri/UI harness evidence for mute and speaking truth boundaries. It is not a production two-machine voice proof by itself.

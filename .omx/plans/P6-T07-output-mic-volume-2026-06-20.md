# P6-T07 - Output And Mic Volume

## Requirements Summary

Source: Multica PER-58 / Phase 6 two-person voice chat production path.

Acceptance criteria:
- Persisted microphone gain and app output volume survive app reload/profile reload.
- Native/Rust media uses microphone gain before Opus/SFrame proof generation.
- App voice/sound output volume applies globally to app-owned playback without claiming hardware-level system volume control.
- Deterministic media/audio harness coverage proves gain/output behavior.

Relevant context:
- `.omx/plans/P6-T02-audio-device-selection-2026-06-20.md`: voice preferences already persist selected input/output device ids.
- `.omx/plans/P6-T04-native-rust-media-session-boundary-2026-06-20.md`: native media proof must require backend voice session state and OpenMLS/SFrame media boundaries.
- `.omx/plans/P6-T06-mute-vad-speaking-2026-06-20.md`: mute/VAD/speaking state comes from media evidence, not UI optimism.
- `docs/release/handoff-2026-06-10-current-state.md`: UI must not claim media route readiness without backend evidence.
- `.omc/plans/discrypt-plan.md`: voice media remains WebRTC/OpenMLS/SFrame-bound; providers are signaling only.

Note: the issue references `.omx/plans/production-release-master-plan-2026-06-10.md`, but that exact file is not present in this checkout. The adjacent Phase 6 plans, current-state handoff, original OMC plan, and issue metadata are the available release context.

## Implementation Steps

1. Extend persisted preferences in `apps/desktop/src-tauri/src/lib.rs` and `apps/ui/src/commands.ts` with bounded `mic_gain_percent` and `app_output_volume_percent`.
2. Normalize preference writes so older clients preserve existing values, missing legacy state defaults to 100%, mic gain is bounded to 0-200%, and output volume is bounded to 0-100%.
3. Add deterministic media gain support in `crates/media/src/capture.rs` and apply it in the native Rust voice media proof path before Opus/SFrame encoding.
4. Wire React sliders in `apps/ui/src/main.tsx` to persisted preferences and keep app output volume applied to app-owned remote audio and harness sound paths.
5. Add focused Rust/Tauri/UI contract tests for persisted preferences, native media gain effect, and deterministic mixer/output behavior.

## Failure Modes And Safety Behavior

- Invalid or absent preference values: normalize to bounded defaults and preserve existing stored values when older clients omit fields.
- Mic gain at 0%: local generated/native proof becomes effectively silent and VAD should not claim speaking from that frame.
- Output volume at 0%: app-owned playback is muted locally, but backend route state is unchanged and no hardware/system volume claim is made.
- Provider adapters remain signaling-only; no media or application payload fallback is introduced.
- Voice session state remains runtime-only; persisted volume preferences must not restore a joined voice session after reload.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per58 cargo test -p discrypt-media app_audio_gain -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per58 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml preferences_use_app_config_ids_and_persist_across_reload --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per58 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml native_voice_media_uses_persisted_microphone_gain --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per58 cargo fmt --check`
- `npm --prefix apps/ui run typecheck`

Evidence classification: local media/Tauri/UI contract evidence for persisted app volume and mic gain. It is not a production two-machine voice proof by itself.

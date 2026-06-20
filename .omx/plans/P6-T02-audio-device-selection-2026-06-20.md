# P6-T02 - Audio Device Selection

## Requirements Summary

Source: Multica PER-53 / Phase 6 two-person voice chat production path.

Acceptance criterion: input/output selectors enumerate devices where the runtime permits, and selected microphone/output device ids persist as user preferences. Voice session truth must remain backend-derived: selectors/preferences do not imply joined, connected, voice-active, or delivered state.

Relevant paths:
- `apps/desktop/src-tauri/src/lib.rs`: persisted app preferences, `save_preferences`, voice join request/device selection, and backend regression tests.
- `apps/ui/src/commands.ts`: command-client preference types and local-dev fallback persistence.
- `apps/ui/src/main.tsx`: browser/native media-device enumeration, settings selectors, and selected-device handoff into `join_voice`.
- `apps/ui/tests/e2e/stateful-ui.spec.ts`: Playwright selector enumeration/persistence smoke.
- `crates/media/src/transport.rs`: existing redacted `VoiceDeviceDescriptor` and `VoiceDeviceSelection` join gate.

## Implementation Steps

1. Extend preferences with `voice_input_device_id` and `voice_output_device_id`, defaulting to `default` for old saved state.
2. Preserve existing audio preferences when older/theme-only `save_preferences` calls omit audio fields.
3. Enumerate both input and output devices in the UI when `navigator.mediaDevices.enumerateDevices` is available; keep unavailable devices as empty lists and default selectors.
4. Persist selector changes through the Tauri command surface while keeping `join_voice` as the backend truth source for actual voice session state.
5. Add focused backend and Playwright coverage for preference persistence and selector enumeration/hydration.

## Risks And Mitigations

- Risk: theme-only preference saves reset selected devices.
  Mitigation: optional request fields preserve current backend/fallback values unless explicitly provided.
- Risk: device preferences could be mistaken for an active media route.
  Mitigation: preferences are separate from `voice_session`; join/capture state still requires `join_voice` plus microphone permission and input descriptor evidence.
- Risk: platform permission/device APIs may be unavailable.
  Mitigation: UI leaves device lists empty, keeps `default`, and does not claim capture/route readiness from enumeration alone.
- Risk: output routing support varies by runtime.
  Mitigation: existing playback `setSinkId` path remains best-effort; this task persists the preference and proves selector state, not production loopback playback.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per53 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml preferences_use_app_config_ids_and_persist_across_reload --lib -- --test-threads=1`
- `npm --prefix apps/ui run typecheck`
- `npm --prefix apps/ui run test:e2e -- stateful-ui.spec.ts -g "audio device selectors enumerate and persist preferences"`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`

Evidence classification: backend/Tauri and browser-harness evidence for selector enumeration and preference persistence. It is not production two-machine voice media transport proof.

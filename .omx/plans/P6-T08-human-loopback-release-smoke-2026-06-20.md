# P6-T08 - Human Or Loopback Release Smoke

## Requirements Summary

Source: Multica PER-59 / P6-T08 human or loopback release smoke in the Phase 6
two-person voice chat production path.

Acceptance criteria:
- Produce a pushed evidence branch and PR for `multica/P6-T08-human-loopback-release-smoke`.
- Retain a native Tauri/WebDriver or display/audio-capable loopback artifact proving voice join, backend self-mute, speaking/VAD media evidence, mic gain/app output volume behavior, remote participant volume behavior, authenticated native media loopback, and leave cleanup.
- Do not count browser local-media shims, synthetic WebView peer connections, or raw Pulse monitor capture as production voice evidence.

Relevant context:
- `docs/release/handoff-2026-06-10-current-state.md`: release evidence must be fresh and must not claim joined/connected/media-active without backend proof.
- `docs/release/release-verification-matrix.md`: voice proof must name the exact layer proven and cannot infer transported audio from backend state alone.
- `.omx/plans/P6-T05-two-person-media-signaling-bridge-2026-06-20.md`: voice signaling must traverse provider-signaled backend text/control; manual command bridge is fallback evidence only.
- `.omx/plans/P6-T06-mute-vad-speaking-2026-06-20.md`: speaking indicators are media/VAD evidence, not UI optimism.
- `.omx/plans/P6-T07-output-mic-volume-2026-06-20.md`: persisted mic gain and app output volume are backend/native media inputs that the release smoke must record.
- The issue description references `.omx/plans/production-release-master-plan-2026-06-10.md`, but that exact file is not present in this checkout; the adjacent Phase 6 plans and release handoff above are the available current context.

## Implementation Steps

1. Keep the canonical native release smoke on `scripts/g012-tauri-webdriver-integrated.mjs` because it already drives two real Tauri WebViews and the Rust-native media proof path.
2. Extend the retained summary artifact with a `per59_release_smoke` matrix that explicitly reports join, mute, speaking/VAD, mic gain/app output volume, remote participant volume, native loopback, leave cleanup, and production-claim eligibility.
3. Make the harness configure non-default backend audio preferences before the voice flow, carry those values into `backend_native_proofs`, reread them after reload, and reject production eligibility if the values are not preserved and consumed by native media.
4. Change remote participant volume only after backend-admitted remote media exists; keep local/self volume rejection behavior untouched.
5. Add a static guard and release note documenting the exact display/audio-capable command and the current local-run blocker.

## Failure Modes And Safety Behavior

- Headless runner without `DISPLAY`/`WAYLAND_DISPLAY`, WebKitGTK/JSC 4.1, `WebKitWebDriver`, `tauri-driver`, or built Tauri binary: the script must fail preflight and retain a dry-run/failed-preflight manifest rather than claiming production evidence.
- Synthetic WebView media fallback: not production evidence, and `production_claim_allowed` remains false.
- Raw Pulse/ALSA monitor capture without Discrypt app/backend path: not production evidence.
- Provider adapters remain signaling-only; the harness may move sealed backend voice signals through provider runtime or mark manual command bridge as fallback evidence only.
- Physical microphone/speaker proof remains stronger than the automated generated-audio/native Rust loopback; summaries must keep that boundary explicit.

## Verification

- `node scripts/check-per59-release-smoke-proof.mjs`
- `node scripts/g012-tauri-webdriver-integrated.mjs --artifact-dir target/per59-release-smoke-dry-run`
- `TMPDIR=/tmp/discrypt-per59-test RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=target/per59 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml voice_join_mute_volume_leave_flow_does_not_clear_state --lib -- --test-threads=1`
- Display/audio-capable evidence command, to be run on the proper runner:

```sh
DISCRYPT_G012_REQUIRE_NATIVE_VOICE=1 \
  RUSTUP_TOOLCHAIN=1.89.0 \
  node scripts/g012-tauri-webdriver-integrated.mjs \
    --run \
    --require-native-voice \
    --artifact-dir target/per59-release-smoke/native-tauri-webdriver
```

Evidence classification: static/dry-run/local backend checks are PR readiness evidence only. PER-59 production-ready release evidence requires the display/audio-capable command above, or the Docker/Xvfb/Pulse wrapper in `scripts/g012-docker-tauri-preflight.sh`, to complete with `per59_release_smoke.production_claim_allowed: true`.

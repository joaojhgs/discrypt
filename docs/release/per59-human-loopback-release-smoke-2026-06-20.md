# PER-59 Human Or Loopback Release Smoke

PER-59 / P6-T08 is not satisfied by backend-only tests, browser local-media shims,
or raw Pulse monitor capture. The retained artifact must come from the native
Discrypt Tauri/backend path and must include the summary field
`per59_release_smoke.production_claim_allowed: true`.

## Required Artifact

Run on a display/audio-capable Linux runner with WebKitGTK/JSC 4.1,
`WebKitWebDriver`, `tauri-driver`, Pulse/PipeWire or ALSA loopback, and a built
Discrypt Tauri binary:

```sh
DISCRYPT_G012_REQUIRE_NATIVE_VOICE=1 \
  RUSTUP_TOOLCHAIN=1.89.0 \
  node scripts/g012-tauri-webdriver-integrated.mjs \
    --run \
    --require-native-voice \
    --artifact-dir target/per59-release-smoke/native-tauri-webdriver
```

The Docker/Xvfb/Pulse helper remains available for a disposable display/audio
runner:

```sh
DISCRYPT_G012_ARTIFACT_DIR=target/per59-release-smoke/native-docker-gui-audio \
  scripts/g012-docker-tauri-preflight.sh
```

## PASS Conditions

The retained `tauri-webdriver-integrated-summary.json` must show:

- `per59_release_smoke.join_proved: true`
- `per59_release_smoke.mute_proved: true`
- `per59_release_smoke.speaking_vad_proved: true`
- `per59_release_smoke.mic_gain_and_output_volume_proved: true`
- `per59_release_smoke.per_peer_volume_surface_proved: true`
- `per59_release_smoke.native_loopback_proved: true`
- `per59_release_smoke.leave_cleanup_proved: true`
- `per59_release_smoke.production_claim_allowed: true`

The summary also records the configured/reloaded audio preferences and the
native media payload fields that consumed them. Synthetic WebView media fallback
or raw Pulse capture without Discrypt's Tauri/backend path must leave production
eligibility false.

## Current Local Runner Boundary

This branch can produce a dry-run/failed-preflight manifest in the current
headless runner, but the local environment lacks the display/native WebDriver
stack and native sound devices needed to complete the release smoke. That
preflight artifact is blocker evidence only, not production-ready PER-59
evidence.

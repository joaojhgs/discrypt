# Android build and emulator voice gate

Discrypt's Android gate is split between host validation and a runner-backed
APK launch path. The host validation keeps pull requests cheap. The emulator job
is explicit `workflow_dispatch` work because it needs Android SDK/NDK packages,
an x86_64 system image, and an Android emulator runner.

## Workflow

`.github/workflows/android.yml` provides:

- `validate-android-gate` on `ubuntu-latest` for static workflow checks, media
  unit tests, honesty checks, command coverage, and the UI build.
- The main `ci.yml` `android-check` job installs the Android NDK before
  `cargo check --workspace --target aarch64-linux-android`, so the cross-target
  compiler is present on GitHub-hosted Ubuntu runners.
- `android-emulator-voice-path`, gated by `workflow_dispatch` input
  `run_android_emulator`, which installs Android SDK/NDK packages, initializes the generated Android
  project with the Tauri Android CLI `android init --ci`, builds an unsigned
  x86_64 APK for the `x86_64-linux-android` Rust target with `android build`,
  installs it on an Android emulator, grants/checks `RECORD_AUDIO`, starts the
  Tauri activity, verifies the app process, and uploads the APK plus emulator
  logs.

The Android job uses the native media contingency already exercised by
`cargo test -p discrypt-media android --quiet`: Android WebViews without encoded
transform support select `NativeWebRtcRsContingency`; that path requires a
microphone grant, an input device, STUN/TURN ICE endpoints, native capture,
native playback, and Rust SFrame before network transit.

## Local validation

Run:

```sh
npm --prefix apps/ui run test:android-gate
cargo test -p discrypt-media android --quiet
```

A local Linux shell can validate the workflow contract and Android media path
logic. It does not prove an APK launch unless the Android SDK, NDK, emulator, and
system image are installed and the runner-backed job is executed.

## Release boundary

Android package and voice-path readiness are not claimed until the runner-backed job passes and its uploaded APK plus emulator logs are retained for the release
candidate. Store distribution, signing credentials, app-store metadata, and
public release governance remain separate release tasks.

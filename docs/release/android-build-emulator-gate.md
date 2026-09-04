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
  project from the desktop wrapper package root with the lockfile-pinned Tauri Android CLI
  `npm run tauri -- android init --ci` (which discovers `src-tauri`), builds an x86_64 APK
  for the `x86_64-linux-android` Rust target, signs it with an ephemeral emulator-only key,
  verifies that signature with `apksigner`, installs it on an Android emulator, and grants/checks the committed Android manifest's
  `RECORD_AUDIO` and Android 12+ `BLUETOOTH_CONNECT` runtime permissions plus the install-time
  `MODIFY_AUDIO_SETTINGS` audio-routing permission, starts the
  Tauri activity, verifies the app process, and uploads the APK plus emulator
  logs.

The committed Android activity exposes a narrow WebView bridge that requests
`BLUETOOTH_CONNECT` only when the user joins voice. Denying nearby-device access
does not block built-in microphone and speaker use; it limits paired Bluetooth
headset routing. Android 11 and older use the manifest's legacy `BLUETOOTH`
permission, capped at API 30.

The Android job uses the native media contingency already exercised by
`cargo test -p discrypt-media android --quiet`: Android WebViews without encoded
transform support select `NativeWebRtcRsContingency`; that path requires a
microphone grant, an input device, STUN/TURN ICE endpoints, native capture,
native playback, and Rust SFrame before network transit.

The Tauri bundle requires Android API 26 or newer. The native CPAL capture and
playback backend uses Android's AAudio library, which is available beginning at
API 26; keeping `bundle.android.minSdkVersion` aligned with that runtime
dependency also makes the Tauri-generated Rust linker target the correct API.

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

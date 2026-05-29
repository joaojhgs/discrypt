#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const workflow = readFileSync(resolve(repoRoot, ".github/workflows/android.yml"), "utf8");
const mainCi = readFileSync(resolve(repoRoot, ".github/workflows/ci.yml"), "utf8");
const mediaTransport = readFileSync(resolve(repoRoot, "crates/media/src/transport.rs"), "utf8");
const docs = readFileSync(resolve(repoRoot, "docs/release/android-build-emulator-gate.md"), "utf8");

const failures = [];
const workflowTokens = [
  "workflow_dispatch:",
  "run_android_emulator:",
  "validate-android-gate:",
  "android-emulator-voice-path:",
  "github.event_name == 'workflow_dispatch' && inputs.run_android_emulator",
  "android-actions/setup-android@v3",
  "reactivecircus/android-emulator-runner@v2",
  "targets: aarch64-linux-android,armv7-linux-androideabi,i686-linux-android,x86_64-linux-android",
  "ANDROID_NDK_HOME",
  "cargo test -p discrypt-media android --quiet",
  "cargo check -p discrypt-media --target x86_64-linux-android --quiet",
  "@tauri-apps/cli@2.11.2 android init",
  "--skip-targets-install",
  "@tauri-apps/cli@2.11.2 android build",
  "--ci",
  "--apk",
  "--target x86_64",
  "--config apps/desktop/src-tauri/tauri.conf.json",
  "--features tauri-runtime,production-network,production-media",
  "android.permission.RECORD_AUDIO",
  "RECORD_AUDIO allow",
  "adb logcat -d",
  "actions/upload-artifact@v4",
  "target/**/*.apk",
  "android-logcat.txt",
];
for (const token of workflowTokens) {
  if (!workflow.includes(token)) failures.push(`Android workflow missing token: ${token}`);
}
for (const token of [
  "android-check:",
  "android-actions/setup-android@v3",
  "ANDROID_NDK_HOME",
  "cargo check --workspace --target aarch64-linux-android",
]) {
  if (!mainCi.includes(token)) failures.push(`Main CI Android target check missing token: ${token}`);
}

const mediaTokens = [
  "AndroidVoiceContingency",
  "NativeWebRtcRsContingency",
  "MediaTransportPath::NativeWebRtcRsContingency",
  "rust_sframe_required: true",
  "native_capture_required: true",
  "native_playback_required: true",
  "requires at least one STUN/TURN ICE endpoint",
  "android_without_encoded_transform_selects_native_contingency",
];
for (const token of mediaTokens) {
  if (!mediaTransport.includes(token)) failures.push(`Android media path missing token: ${token}`);
}

const docsTokens = [
  "# Android build and emulator voice gate",
  "workflow_dispatch",
  "run_android_emulator",
  "x86_64-linux-android",
  "RECORD_AUDIO",
  "NativeWebRtcRsContingency",
  "not claimed until the runner-backed job passes",
  "Tauri Android CLI",
  "emulator logs",
];
for (const token of docsTokens) {
  if (!docs.includes(token)) failures.push(`Android gate docs missing token: ${token}`);
}

for (const forbidden of [/Google Play release/i, /signed release/i, /store-ready/i, /certified/i]) {
  if (forbidden.test(workflow) || forbidden.test(docs)) {
    failures.push(`Android gate must not make unproven release claim matching ${forbidden}`);
  }
}

if (failures.length > 0) {
  console.error("Android gate check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("Android gate check passed");

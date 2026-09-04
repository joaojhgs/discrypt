#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const workflow = readFileSync(
  resolve(repoRoot, ".github/workflows/android.yml"),
  "utf8",
);
const mainCi = readFileSync(
  resolve(repoRoot, ".github/workflows/ci.yml"),
  "utf8",
);
const desktopCargo = readFileSync(
  resolve(repoRoot, "apps/desktop/src-tauri/Cargo.toml"),
  "utf8",
);
const tauriConfig = JSON.parse(
  readFileSync(
    resolve(repoRoot, "apps/desktop/src-tauri/tauri.conf.json"),
    "utf8",
  ),
);
const desktopPackage = JSON.parse(
  readFileSync(resolve(repoRoot, "apps/desktop/package.json"), "utf8"),
);
const desktopPackageLock = JSON.parse(
  readFileSync(resolve(repoRoot, "apps/desktop/package-lock.json"), "utf8"),
);
const androidManifest = readFileSync(
  resolve(
    repoRoot,
    "apps/desktop/src-tauri/gen/android/app/src/main/AndroidManifest.xml",
  ),
  "utf8",
);
const androidMainActivity = readFileSync(
  resolve(
    repoRoot,
    "apps/desktop/src-tauri/gen/android/app/src/main/java/chat/discrypt/desktop/MainActivity.kt",
  ),
  "utf8",
);
const mediaTransport = readFileSync(
  resolve(repoRoot, "crates/media/src/transport.rs"),
  "utf8",
);
const docs = readFileSync(
  resolve(repoRoot, "docs/release/android-build-emulator-gate.md"),
  "utf8",
);

const failures = [];
const workflowTokens = [
  "workflow_dispatch:",
  "run_android_emulator:",
  "validate-android-gate:",
  "android-emulator-voice-path:",
  '"apps/desktop/package*.json"',
  "github.event_name == 'workflow_dispatch' && inputs.run_android_emulator",
  "android-actions/setup-android@v3",
  "reactivecircus/android-emulator-runner@v2",
  "targets: aarch64-linux-android,armv7-linux-androideabi,i686-linux-android,x86_64-linux-android",
  "ANDROID_NDK_HOME",
  "CC_x86_64_linux_android",
  "AR_x86_64_linux_android",
  "CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER",
  "x86_64-linux-android${ANDROID_API_LEVEL}-clang",
  "cargo test -p discrypt-media android --quiet",
  "cargo check -p discrypt-media --target x86_64-linux-android --quiet",
  "npm run tauri -- android init",
  "--skip-targets-install",
  "npm run tauri -- android build",
  "working-directory: apps/desktop",
  "npm --prefix apps/desktop ci --audit=false",
  "--ci",
  "--apk",
  "--target x86_64",
  "--features tauri-runtime,production-network,production-media",
  "Build Android APK for virtual-device validation",
  "Sign APK for test installation",
  "discrypt-ci-signing.p12",
  "*-ci-signed.apk",
  'apksigner" verify --verbose',
  "android.permission.BLUETOOTH_CONNECT",
  "android.permission.MODIFY_AUDIO_SETTINGS",
  "android.permission.RECORD_AUDIO",
  "RECORD_AUDIO allow",
  "android.permission.BLUETOOTH_CONNECT: granted=true",
  "android.permission.RECORD_AUDIO: granted=true",
  "adb logcat -d",
  "actions/upload-artifact@v4",
  "apps/desktop/src-tauri/gen/android/app/build/outputs/apk",
  "apps/desktop/src-tauri/gen/android/app/build/outputs/apk/**/*.apk",
  "android-logcat.txt",
];
for (const token of workflowTokens) {
  if (!workflow.includes(token))
    failures.push(`Android workflow missing token: ${token}`);
}
for (const token of [
  "android-check:",
  "android-actions/setup-android@v3",
  "ANDROID_NDK_HOME",
  "cargo check --workspace --target aarch64-linux-android",
]) {
  if (!mainCi.includes(token))
    failures.push(`Main CI Android target check missing token: ${token}`);
}

if (tauriConfig.bundle?.android?.minSdkVersion !== 26) {
  failures.push(
    "Tauri Android minSdkVersion must be 26 because the native CPAL backend links AAudio",
  );
}
if (
  !androidManifest.includes(
    '<uses-permission android:name="android.permission.RECORD_AUDIO" />',
  )
) {
  failures.push(
    "Android manifest must request RECORD_AUDIO for native voice capture",
  );
}
if (
  !androidManifest.includes(
    '<uses-permission android:name="android.permission.BLUETOOTH" android:maxSdkVersion="30" />',
  )
) {
  failures.push(
    "Android manifest must request legacy BLUETOOTH access through API 30 for headset routing",
  );
}
if (
  !androidManifest.includes(
    '<uses-permission android:name="android.permission.BLUETOOTH_CONNECT" />',
  )
) {
  failures.push(
    "Android manifest must request BLUETOOTH_CONNECT for Android 12+ headset routing",
  );
}
for (const token of [
  'addJavascriptInterface(AndroidVoicePermissions(), "DiscryptAndroidVoice")',
  "ActivityResultContracts.RequestPermission()",
  "Manifest.permission.BLUETOOTH_CONNECT",
  "discrypt:android-bluetooth-audio-permission",
]) {
  if (!androidMainActivity.includes(token)) {
    failures.push(`Android voice permission bridge missing token: ${token}`);
  }
}
if (
  !androidManifest.includes(
    '<uses-permission android:name="android.permission.MODIFY_AUDIO_SETTINGS" />',
  )
) {
  failures.push(
    "Android manifest must request MODIFY_AUDIO_SETTINGS for microphone and speaker routing",
  );
}
if (desktopPackage.scripts?.tauri !== "tauri") {
  failures.push(
    "Desktop wrapper package must expose the Tauri CLI for generated Android builds",
  );
}
if (desktopPackage.devDependencies?.["@tauri-apps/cli"] !== "2.11.2") {
  failures.push(
    "Desktop wrapper package must lock @tauri-apps/cli to the release-tooling version 2.11.2",
  );
}
if (
  desktopPackageLock.packages?.[""]?.devDependencies?.["@tauri-apps/cli"] !==
  "2.11.2"
) {
  failures.push("Desktop wrapper lockfile must pin @tauri-apps/cli to 2.11.2");
}
if (
  desktopPackageLock.packages?.["node_modules/@tauri-apps/cli"]?.version !==
  "2.11.2"
) {
  failures.push("Desktop wrapper lockfile must resolve @tauri-apps/cli 2.11.2");
}

const desktopLibSection = desktopCargo.match(
  /(?:^|\n)\[lib\]\s*\n(?<body>[\s\S]*?)(?=\n\[|$)/,
);
if (!desktopLibSection) {
  failures.push(
    "Desktop Tauri Cargo manifest missing [lib] section for Android native library build",
  );
} else {
  const libNameMatch = desktopLibSection.groups.body.match(
    /^\s*name\s*=\s*"(?<name>[^"]+)"/m,
  );
  if (libNameMatch && libNameMatch.groups.name !== "discrypt_desktop") {
    failures.push(
      "Desktop Tauri Cargo manifest [lib] name must produce libdiscrypt_desktop.so",
    );
  }
  const crateTypeMatch = desktopLibSection.groups.body.match(
    /^\s*crate-type\s*=\s*\[(?<types>[^\]]*)\]/m,
  );
  if (!crateTypeMatch) {
    failures.push(
      "Desktop Tauri Cargo manifest [lib] section missing crate-type",
    );
  } else {
    const crateTypes = [
      ...crateTypeMatch.groups.types.matchAll(/"([^"]+)"/g),
    ].map((match) => match[1]);
    for (const crateType of ["staticlib", "cdylib", "rlib"]) {
      if (!crateTypes.includes(crateType)) {
        failures.push(
          `Desktop Tauri Cargo manifest crate-type missing ${crateType}`,
        );
      }
    }
  }
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
  if (!mediaTransport.includes(token))
    failures.push(`Android media path missing token: ${token}`);
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
  if (!docs.includes(token))
    failures.push(`Android gate docs missing token: ${token}`);
}

for (const forbidden of [
  /Google Play release/i,
  /signed release/i,
  /store-ready/i,
  /certified/i,
]) {
  if (forbidden.test(workflow) || forbidden.test(docs)) {
    failures.push(
      `Android gate must not make unproven release claim matching ${forbidden}`,
    );
  }
}

if (failures.length > 0) {
  console.error("Android gate check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("Android gate check passed");

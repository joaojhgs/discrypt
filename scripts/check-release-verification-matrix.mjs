#!/usr/bin/env node
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const docs = read("docs/release/release-verification-matrix.md");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const failures = [];

function resolveSiblingRepoRoot(repoName) {
  const envValue = process.env[`${repoName.toUpperCase().replace(/-/g, "_")}_REPO_ROOT`];
  if (envValue && existsSync(envValue)) return envValue;

  return null;
}

for (const token of [
  "# Release verification matrix",
  "npm --prefix apps/ui run release:linux",
  "npm --prefix apps/ui run release:two-profile-harness-g010",
  "npm --prefix apps/ui run release:two-profile-harness-g010:dry-run",
  "target/release/g010-two-profile-harness/report.json",
  "DISCRYPT_G010_PUBLIC_MATRIX=1",
  "skipped_missing_external_credentials",
  "npm --prefix apps/ui run smoke:linux-packages",
  "npm --prefix apps/ui run test:desktop-package-ci",
  "npm --prefix apps/ui run test:android-gate",
  "npm --prefix apps/ui run test:release-verification-matrix",
  "npm --prefix apps/ui run test:release-governance",
  "External signaling service smoke is opt-in",
  "DISCRYPT_EXTERNAL_SIGNALING_SMOKE=1",
  "G008 STUN/TURN/fallback hardening",
  "npm --prefix apps/ui run test:g008-stun-turn-fallback",
  "Credentialed TURN remains opt-in",
  "cargo test -q -p discrypt-desktop text_control_frame_roundtrip_persists_across_two_profile_state_files -- --nocapture",
  "cargo test -q -p discrypt-desktop text_control_session_pump_uses_data_transport_trait_and_persists_receipt -- --nocapture",
  "DISCRYPT_DESKTOP_PUBLIC_MQTT_RECEIPT_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 cargo test -q -p discrypt-desktop --features mqtt-adapter public_mqtt_two_profile_receipt_crosses_provider_webrtc_when_enabled -- --nocapture",
  "DISCRYPT_DESKTOP_PUBLIC_NOSTR_RECEIPT_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol cargo test -q -p discrypt-desktop --features nostr-adapter public_nostr_two_profile_receipt_crosses_provider_webrtc_when_enabled -- --nocapture",
  "Sensitive data exclusion",
  "signaling admin audit tokens",
  "TURN static auth secrets",
  "crash collector upload",
]) {
  if (!docs.includes(token)) failures.push(`release verification matrix missing token: ${token}`);
}
for (const scriptName of [
  "test:release-linux",
  "release:two-profile-harness-g010",
  "release:two-profile-harness-g010:dry-run",
  "test:release-two-profile-harness-g010",
  "test:linux-package-smoke",
  "test:desktop-package-ci",
  "test:android-gate",
  "test:release-governance",
  "test:release-verification-matrix",
  "test:g008-stun-turn-fallback",
  "test:stun-turn-provider-privacy-g132",
]) {
  if (!packageJson.scripts?.[scriptName]) failures.push(`package script missing ${scriptName}`);
}

const forbiddenValues = [
  "plaintext-message",
  "alice",
  "bob",
  "group-secret",
  "sframe-key",
  "mls-epoch-secret",
  "room-secret",
  "CRASH_REPORT_UPLOAD_TOKEN",
  "TAURI_PRIVATE_KEY",
];

if (failures.length === 0) {
  const runExternalSmoke = process.env.DISCRYPT_EXTERNAL_SIGNALING_SMOKE === "1";
  const signalingRepoRoot = resolveSiblingRepoRoot("discrypt-signaling");
  if (runExternalSmoke && !signalingRepoRoot) {
    failures.push(
      "external signaling smoke requested, but DISCRYPT_SIGNALING_REPO_ROOT does not point at a checkout",
    );
  } else if (runExternalSmoke) {
    const run = spawnSync("cargo", [
      "test",
      "--manifest-path", "Cargo.toml",
      "-p", "discrypt-signaling",
      "config_parses_cli_values",
      "--quiet",
    ], { cwd: signalingRepoRoot, encoding: "utf8" });
    if (run.status !== 0) {
      failures.push(`external signaling config smoke failed:
${run.stdout}
${run.stderr}`);
    }
  } else {
    console.info(
      "external signaling smoke skipped: set DISCRYPT_EXTERNAL_SIGNALING_SMOKE=1 and DISCRYPT_SIGNALING_REPO_ROOT=<path> to run the isolated service proof",
    );
  }
}

if (failures.length > 0) {
  console.error("release verification matrix check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("release verification matrix check passed");

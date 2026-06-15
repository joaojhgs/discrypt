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
  "## Frozen release evidence definitions",
  "| Production-ready | A release candidate built from a named commit has passed every required package, platform runner, governance, privacy, provider, OpenMLS/admission, text delivery, and voice/media gate in this matrix with retained artifacts and no unresolved critical release blockers. |",
  "| E2E-tested | The named user journey was exercised through the real layer under discussion, with both endpoints, state persistence, cryptographic/admission evidence, transport route evidence, and retained logs/artifacts. |",
  "| Split-machine | Two peers ran on distinct machines or network hosts with isolated profile/state paths and exchanged the claimed payload over the stated route while retaining local and remote artifacts. |",
  "| Voice proof | The named voice claim is backed by native/media evidence appropriate to that claim: generated or captured audio frames, SFrame/MLS keying boundary, WebRTC route state, remote receive evidence, and retained artifacts. |",
  "| Overlay relay | Application text/media was forwarded by a peer-assisted encrypted overlay leg with explicit route evidence, relay authority, E2EE/ciphertext-only validation, and no provider application relay. |",
  "unqualified \"E2E-tested\" is forbidden",
  "MQTT, Nostr, IPFS PubSub, and Discrypt QUIC rendezvous are signaling-only and are never overlay relay evidence.",
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

const forbiddenOverclaimPatterns = [
  {
    pattern: /\bDiscrypt is production-ready\b(?!\s+(?:yet|only within|within))/i,
    message: "absolute Discrypt production-ready claim without release-matrix qualifier",
  },
  {
    pattern: /\bproduction-ready(?:\s+(?:app|release|build|product|candidate|status|proof))\b/i,
    message: "production-ready phrase attached to an app/release/build/product without matrix evidence qualifier",
  },
  {
    pattern: /\bfully E2E-tested\b/i,
    message: "unqualified fully E2E-tested claim",
  },
  {
    pattern: /\b(?:complete|full|final)\s+split-machine\s+(?:proof|evidence|validation)\b/i,
    message: "absolute split-machine proof claim",
  },
  {
    pattern: /\b(?:complete|full|final|real)\s+voice proof\b/i,
    message: "absolute voice proof claim",
  },
  {
    pattern: /\b(?:MQTT|Nostr|IPFS PubSub|Discrypt QUIC)\s+(?:is|as|provides|proves)\s+(?:an?\s+)?overlay relay\b/i,
    message: "signaling provider described as overlay relay evidence",
  },
];

for (const { pattern, message } of forbiddenOverclaimPatterns) {
  if (pattern.test(docs)) failures.push(`release verification matrix contains forbidden overclaim: ${message}`);
}

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

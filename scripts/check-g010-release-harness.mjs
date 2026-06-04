#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const harness = read("scripts/g010-release-harness.mjs");
const launchHarness = read("scripts/g010-tauri-two-profile-launch.mjs");
// Adapter matrix implementation is owned by the release-matrix lane; this contract
// only checks that the G010 wrapper can call the existing matrix command.
const docs = read("docs/release/g010-release-harness.md");
// CI/release-matrix reads intentionally omitted: worker-2 owns launch/profile scripts only.

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const script of [
  "test:g010-release-harness",
  "test:g010-release-harness:public",
  "test:g010-release-contract",
  "test:g010-tauri-launch-dry-run",
]) {
  if (!packageJson.scripts?.[script]) failures.push(`apps/ui/package.json missing ${script}`);
}

for (const token of [
  "discrypt.g010.release_harness.v1",
  "scripts/g010-tauri-two-profile-launch.mjs",
  "target/g010-release-harness",
  "alice_state_path",
  "bob_state_path",
  "tests/e2e/two-profile-flow.spec.ts",
  "tests/e2e/voice-media-session.spec.ts",
  "g004_two_profile_state_survives_reload_with_invites_receipts_voice_and_preferences",
  "text_control_frame_roundtrip_persists_across_two_profile_state_files",
  "test:signaling-e2e-matrix-g132",
  "test:security-privacy-g009",
]) {
  requireText("g010 harness", harness, token);
}


for (const token of [
  "discrypt.g010.tauri_two_profile_launch.v1",
  "DISCRYPT_APP_STATE_PATH",
  "alice/app-state.discrypt-store",
  "bob/app-state.discrypt-store",
  "tauri-launch-manifest.json",
  "cargo",
  "tauri",
  "dev",
  "--app-mode=",
  "DISCRYPT_G010_TAURI_FEATURES",
  "tauri-runtime,local-dev,production-media,mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter",
  "must include local-dev or harness",
  "profile_isolation_env",
  "manual_pairing_required",
  "production_claim",
]) {
  requireText("g010 launch harness", launchHarness, token);
}

for (const token of [
  "# G010 release harness and automation",
  "npm --prefix apps/ui run test:g010-release-harness",
  "npm --prefix apps/ui run test:g010-release-harness:public",
  "target/g010-release-harness/<run-id>/manifest.json",
  "target/g010-release-harness/<run-id>/tauri-launch-manifest.json",
  "npm --prefix apps/ui run test:g010-tauri-launch-dry-run",
  "No fake production claims",
  "DISCRYPT_PUBLIC_MQTT_E2E",
  "DISCRYPT_PUBLIC_NOSTR_E2E",
  "DISCRYPT_PUBLIC_IPFS_E2E",
  "DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E",
  "DISCRYPT_PUBLIC_TURN_E2E",
  "DISCRYPT_G010_TAURI_FEATURES",
  "tauri-runtime,local-dev,production-media,mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter",
]) {
  requireText("g010 docs", docs, token);
}

// CI and release-matrix wiring are owned by the adapter/release lane; this
// contract stays scoped to the reusable launch/profile artifact scripts.


const dryRun = spawnSync("node", ["scripts/g010-tauri-two-profile-launch.mjs"], {
  cwd: repoRoot,
  encoding: "utf8",
  env: {
    ...process.env,
    DISCRYPT_G010_RUN_ID: "contract-dry-run",
    DISCRYPT_G010_ARTIFACT_DIR: "target/g010-release-harness/contract-dry-run",
  },
});
if (dryRun.status !== 0) {
  failures.push(`g010 tauri launch dry-run failed:\n${dryRun.stdout}\n${dryRun.stderr}`.trim());
} else {
  const manifest = JSON.parse(read("target/g010-release-harness/contract-dry-run/tauri-launch-manifest.json"));
  if (manifest.schema_version !== "discrypt.g010.tauri_two_profile_launch.v1") failures.push("launch manifest schema mismatch");
  if (manifest.mode !== "dry-run" || manifest.status !== "dry-run") failures.push("launch manifest must record dry-run status");
  if (!manifest.tauri_features?.includes("tauri-runtime") || !manifest.tauri_features?.some((feature) => ["local-dev", "harness"].includes(feature))) {
    failures.push("launch manifest must include tauri-runtime plus local-dev/harness features");
  }
  if (manifest.production_claim !== "none; this wrapper is a harness/local-dev launch aid, not release packaging evidence") failures.push("launch manifest must avoid production claims");
  if (manifest.manual_pairing_required !== false) failures.push("launch manifest must not require manual pairing");
  const alice = manifest.profiles?.alice?.state_path;
  const bob = manifest.profiles?.bob?.state_path;
  if (!alice || !bob || alice === bob || !alice.includes("alice/app-state.discrypt-store") || !bob.includes("bob/app-state.discrypt-store")) {
    failures.push("launch manifest must include distinct Alice/Bob state paths");
  }
  for (const profile of ["alice", "bob"]) {
    const command = manifest.commands?.find((entry) => entry.profile === profile);
    if (!command?.env?.DISCRYPT_APP_STATE_PATH || !command.args?.includes("--features") || !command.args?.includes(manifest.tauri_features.join(","))) {
      failures.push(`launch manifest missing isolated env/features for ${profile}`);
    }
  }
}

if (failures.length > 0) {
  console.error("G010 release harness contract failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G010 release harness contract passed.");

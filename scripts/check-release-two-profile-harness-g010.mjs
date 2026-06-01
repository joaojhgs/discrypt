#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const harness = read("scripts/release-two-profile-harness-g010.mjs");
const matrix = read("docs/release/release-verification-matrix.md");
const g131 = read("docs/release/g131-final-e2e-verification.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}
function requireScript(name) {
  if (!packageJson.scripts?.[name]) failures.push(`package.json missing script: ${name}`);
}

for (const script of [
  "release:two-profile-harness-g010",
  "release:two-profile-harness-g010:dry-run",
  "test:release-two-profile-harness-g010",
]) requireScript(script);

for (const token of [
  "discrypt.g010.two_profile_release_harness.v1",
  "DISCRYPT_APP_STATE_PATH",
  "browser-ui-build",
  "browser-two-profile-ui",
  "text_control_frame_roundtrip_persists_across_two_profile_state_files",
  "text_control_session_pump_uses_data_transport_trait_and_persists_receipt",
  "g004_two_profile_restart_matrix_persists_invites_connectivity_receipts_voice_and_preferences",
  "public-mqtt-two-profile-receipt",
  "public-nostr-two-profile-receipt",
  "skipped_missing_external_credentials",
  "DISCRYPT_PUBLIC_TURN_ENDPOINT",
  "DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS",
  "DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT",
  "version metadata must match",
  "target/release/g010-two-profile-harness",
]) requireText("G010 harness", harness, token);

for (const token of [
  "G010 two-profile release harness",
  "npm --prefix apps/ui run release:two-profile-harness-g010",
  "npm --prefix apps/ui run release:two-profile-harness-g010:dry-run",
  "target/release/g010-two-profile-harness/report.json",
  "DISCRYPT_G010_PUBLIC_MATRIX=1",
  "skipped_missing_external_credentials",
]) requireText("release verification matrix", matrix, token);

for (const token of [
  "npm --prefix apps/ui run test:release-two-profile-harness-g010",
  "npm --prefix apps/ui run release:two-profile-harness-g010:dry-run",
]) requireText("G131 final verification doc", g131, token);

const dryRun = spawnSync(process.execPath, ["scripts/release-two-profile-harness-g010.mjs", "--dry-run"], {
  cwd: repoRoot,
  encoding: "utf8",
  env: { ...process.env, DISCRYPT_G010_DRY_RUN: "1" },
  maxBuffer: 1024 * 1024 * 8,
});
if (dryRun.status !== 0) {
  failures.push(`G010 harness dry-run failed:\n${dryRun.stdout}\n${dryRun.stderr}`.trim());
} else {
  const plan = JSON.parse(dryRun.stdout);
  if (plan.schema_version !== "discrypt.g010.two_profile_release_harness.v1") failures.push("dry-run schema mismatch");
  if (plan.product.version !== plan.product.versionTargets.tauriConfig || plan.product.version !== plan.product.versionTargets.desktopCargo) failures.push("dry-run version sync mismatch");
  if (!plan.profiles?.alice?.statePath?.includes("alice") || !plan.profiles?.bob?.statePath?.includes("bob")) failures.push("dry-run missing isolated profile state paths");
  const localIds = (plan.localAdapterMatrix ?? []).map((entry) => entry.id).join("\n");
  for (const id of ["browser-ui-build", "browser-two-profile-ui", "desktop-two-profile-state-roundtrip", "desktop-text-control-transport-pump", "desktop-two-profile-restart-matrix"]) {
    if (!localIds.includes(id)) failures.push(`dry-run missing local matrix row ${id}`);
  }
  const publicStatuses = (plan.publicAdapterMatrix ?? []).map((entry) => `${entry.id}:${(entry.missingEnv ?? []).join(",")}`).join("\n");
  for (const id of ["public-mqtt-two-profile-receipt", "public-nostr-two-profile-receipt", "public-turn-relay-only", "public-ipfs-topic-peer", "public-quic-rendezvous"]) {
    if (!publicStatuses.includes(id)) failures.push(`dry-run missing public matrix row ${id}`);
  }
}

if (failures.length > 0) {
  console.error("G010 release two-profile harness check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G010 release two-profile harness check passed");

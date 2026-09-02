#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const harness = read("scripts/g012-tauri-two-profile-e2e.mjs");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}
function requireScript(name) {
  if (!packageJson.scripts?.[name]) failures.push(`package.json missing script: ${name}`);
}

for (const script of ["g012:tauri-two-profile-e2e", "test:g012-tauri-two-profile-e2e"]) requireScript(script);
for (const token of [
  "discrypt.g012.tauri_two_profile_e2e_harness.v1",
  "target/g012-e2e",
  "DISCRYPT_APP_STATE_PATH",
  "tauri-runtime,local-dev,production-media,mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter",
  "cargo",
  "tauri",
  "dev",
  "shared vite dev server",
  "profile_state_files",
  "screenshot_capability",
  "launch_ready_timeout_ms",
  "tauri-build-preflight.log",
  "launch_readiness",
  "G012 is not complete until two launched Tauri profiles complete text plus voice UX proof",
  "DISCRYPT_G012_DEV_SERVER_PORT",
  "devServerPort",
  "launch-smoke-passed",
  "integrated_e2e_status",
  "remaining_integrated_e2e_requirements",
  "discrypt.g012.launcher_evidence_boundary.v1",
  "production_claim_allowed",
  "action_driven_evidence",
  "g012_checkpoint_eligible",
  "--delegate-webdriver",
  "delegated WebDriver integrated E2E",
  "failed-preflight",
  "proven_by_delegated_webdriver_artifact",
  "node scripts/g012-tauri-webdriver-integrated.mjs --run --require-native-voice",
]) requireText("G012 harness", harness, token);

const dryRun = spawnSync(process.execPath, ["scripts/g012-tauri-two-profile-e2e.mjs"], {
  cwd: repoRoot,
  encoding: "utf8",
  env: { ...process.env, DISCRYPT_G012_RUN_ID: "contract-dry-run" },
  maxBuffer: 1024 * 1024 * 8,
});
if (![0, 3].includes(dryRun.status ?? 1)) {
  failures.push(`G012 harness dry-run exited unexpectedly with ${dryRun.status}:\n${dryRun.stdout}\n${dryRun.stderr}`.trim());
}
const manifestPath = resolve(repoRoot, "target/g012-e2e/contract-dry-run/tauri-two-profile-launch-manifest.json");
try {
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (manifest.schema_version !== "discrypt.g012.tauri_two_profile_e2e_harness.v1") failures.push("dry-run schema mismatch");
  if (manifest.mode !== "dry-run") failures.push("dry-run mode mismatch");
  if (!manifest.artifact_root.includes("target/g012-e2e/contract-dry-run")) failures.push("dry-run artifact root must be under target/g012-e2e");
  if (!manifest.profiles?.alice?.state_path?.includes("alice") || !manifest.profiles?.bob?.state_path?.includes("bob")) failures.push("dry-run missing isolated Alice/Bob state paths");
  if (!manifest.planned_commands?.some((entry) => entry.label === "tauri alice" && entry.rendered.includes("cargo tauri dev"))) failures.push("dry-run missing planned tauri alice command");
  if (!manifest.planned_commands?.some((entry) => entry.label === "tauri bob" && entry.env?.DISCRYPT_APP_STATE_PATH?.includes("bob"))) failures.push("dry-run missing planned tauri bob isolated env");
  if (!manifest.planned_commands?.some((entry) => entry.label === "shared vite dev server" && entry.rendered.includes("--port 1420"))) failures.push("dry-run missing managed Vite port in planned command");
  if (manifest.evidence_boundary?.evidence_mode !== "dry-run-contract") failures.push("dry-run manifest missing dry-run evidence boundary");
  if (manifest.evidence_boundary?.production_claim_allowed !== false) failures.push("dry-run manifest must forbid production claims");
  if (manifest.evidence_boundary?.action_driven_evidence !== false) failures.push("dry-run manifest must not claim action-driven evidence");
  if (manifest.evidence_boundary?.g012_checkpoint_eligible !== false) failures.push("dry-run manifest must not be checkpoint eligible");
  const summary = JSON.parse(readFileSync(resolve(repoRoot, "target/g012-e2e/contract-dry-run/launch-summary.json"), "utf8"));
  if (summary.status !== "dry-run" || summary.evidence_mode !== "dry-run-contract") failures.push("dry-run summary missing dry-run boundary");
  if (summary.production_claim_allowed !== false || summary.action_driven_evidence !== false || summary.g012_checkpoint_eligible !== false) {
    failures.push("dry-run summary must not claim production/action/checkpoint evidence");
  }
} catch (error) {
  failures.push(`could not read dry-run manifest ${manifestPath}: ${error instanceof Error ? error.message : String(error)}`);
}

const delegatedDryRun = spawnSync(process.execPath, ["scripts/g012-tauri-two-profile-e2e.mjs", "--delegate-webdriver", "--artifact-dir", "target/g012-e2e/contract-delegated-dry-run"], {
  cwd: repoRoot,
  encoding: "utf8",
  env: { ...process.env, DISCRYPT_G012_RUN_ID: "contract-delegated-dry-run" },
  maxBuffer: 1024 * 1024 * 8,
});
if (delegatedDryRun.status !== 0) {
  failures.push(`G012 delegated dry-run exited unexpectedly with ${delegatedDryRun.status}:\n${delegatedDryRun.stdout}\n${delegatedDryRun.stderr}`.trim());
}
try {
  const delegatedManifest = JSON.parse(readFileSync(resolve(repoRoot, "target/g012-e2e/contract-delegated-dry-run/tauri-two-profile-launch-manifest.json"), "utf8"));
  if (delegatedManifest.runner_mode !== "delegate-webdriver") failures.push("delegated dry-run manifest missing delegate runner mode");
  if (delegatedManifest.evidence_boundary?.evidence_mode !== "delegated-webdriver") failures.push("delegated dry-run manifest missing delegated evidence mode");
  if (!delegatedManifest.planned_commands?.some((entry) => entry.label === "delegated WebDriver integrated E2E" && entry.rendered.includes("g012-tauri-webdriver-integrated.mjs --run --require-native-voice"))) {
    failures.push("delegated dry-run missing integrated WebDriver planned command");
  }
  if (delegatedManifest.evidence_boundary?.production_claim_allowed !== false || delegatedManifest.evidence_boundary?.action_driven_evidence !== false) {
    failures.push("delegated dry-run manifest must delegate without claiming action evidence");
  }
  const delegatedSummary = JSON.parse(readFileSync(resolve(repoRoot, "target/g012-e2e/contract-delegated-dry-run/launch-summary.json"), "utf8"));
  if (delegatedSummary.delegated_webdriver !== true || delegatedSummary.action_driven_evidence !== false) {
    failures.push("delegated dry-run summary must distinguish delegation from completed action evidence");
  }
} catch (error) {
  failures.push(`could not read delegated dry-run artifacts: ${error instanceof Error ? error.message : String(error)}`);
}

for (const forbidden of [
  "production_claim_allowed: true",
  "Production-ready: true",
  "launch-smoke production evidence",
]) {
  if (harness.includes(forbidden)) failures.push(`G012 harness contains forbidden token: ${forbidden}`);
}

if (failures.length > 0) {
  console.error("G012 Tauri two-profile E2E harness check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G012 Tauri two-profile E2E harness check passed");

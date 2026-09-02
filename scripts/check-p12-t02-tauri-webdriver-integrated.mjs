#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const harness = read("scripts/g012-tauri-webdriver-integrated.mjs");
const releaseMatrix = read("docs/release/release-verification-matrix.md");
const evidenceDoc = read("docs/release/per97-tauri-webdriver-integrated-2026-06-26.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

function rejectText(name, text, token) {
  if (text.includes(token)) failures.push(`${name} contains forbidden token: ${token}`);
}

if (!packageJson.scripts?.["test:p12-t02-tauri-webdriver-integrated"]) {
  failures.push("package script missing test:p12-t02-tauri-webdriver-integrated");
}

for (const token of [
  "per97WorkflowSteps",
  "per97ArtifactContract",
  "setup",
  "invite",
  "approval",
  "text",
  "voice",
  "persistence",
  "degraded_unavailable",
  "Invite parsing is not membership",
  "manual command bridge fallback is labeled non-provider-runtime evidence",
  "Synthetic WebView peer-connection fallback is diagnostic only",
  "per97_acceptance",
  "owner_staff_approval_applied",
  "openmls_admission_persisted",
  "text_plaintext_observed_both_ways",
  "voice_native_or_capability_evidence_recorded",
  "persistence_reloaded_after_admission_text_and_voice",
  "degraded_unavailable_states_recorded_by_preflight",
  "failed-preflight",
  "summaryPath",
  "screenshotDir",
  "profile_state_files",
  "openmls_admission_owner_approval",
  "hasOpenMlsAdmission",
  "waitForAdmissionUnlockedUi",
  "voice_proof",
  "native_voice_capability",
  "g012_checkpoint_eligible: remotePlaintextObserved && nativeVoiceLoopbackObserved",
]) {
  requireText("G012 integrated WebDriver harness", harness, token);
}

for (const token of [
  "P12-T02 Tauri WebDriver integrated two-profile",
  "npm --prefix apps/ui run test:p12-t02-tauri-webdriver-integrated",
  "node scripts/g012-tauri-webdriver-integrated.mjs --run --require-native-voice",
  "target/g012-e2e/<run-id>/tauri-webdriver-integrated-summary.json",
  "setup, invite, owner/staff approval, text, voice, persistence, and degraded/unavailable-state evidence",
  "Dry-run is contract/preflight evidence only",
]) {
  requireText("PER-97 evidence doc", evidenceDoc, token);
}

for (const token of [
  "P12-T02 Tauri WebDriver integrated two-profile",
  "test:p12-t02-tauri-webdriver-integrated",
  "target/g012-e2e/<run-id>/tauri-webdriver-integrated-summary.json",
]) {
  requireText("release verification matrix", releaseMatrix, token);
}

for (const token of [
  "production_claim_allowed: true",
  "Production-ready: true",
  "provider application relay fallback",
]) {
  rejectText("G012 integrated WebDriver harness", harness, token);
}

const dryRunArtifactDir = resolve(repoRoot, "target/p12-t02-tauri-webdriver-integrated-contract");
const dryRun = spawnSync(process.execPath, [
  "scripts/g012-tauri-webdriver-integrated.mjs",
  "--artifact-dir",
  dryRunArtifactDir,
], {
  cwd: repoRoot,
  encoding: "utf8",
  env: {
    ...process.env,
    DISCRYPT_G012_WEBDRIVER_RUN_ID: "p12-t02-contract-dry-run",
  },
});
if (dryRun.status !== 0) {
  failures.push(`dry-run harness contract exited ${dryRun.status}: ${dryRun.stdout}\n${dryRun.stderr}`);
}

const manifestPath = resolve(dryRunArtifactDir, "tauri-webdriver-integrated-manifest.json");
if (!existsSync(manifestPath)) {
  failures.push(`dry-run manifest missing at ${manifestPath}`);
} else {
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (manifest.mode !== "dry-run") failures.push(`dry-run manifest mode was ${manifest.mode}`);
  if (!Array.isArray(manifest.per97_workflow_steps) || manifest.per97_workflow_steps.length < 7) {
    failures.push("dry-run manifest missing PER-97 workflow steps");
  }
  if (manifest.per97_artifact_contract?.dry_run_boundary !== "Dry-run writes the manifest/preflight contract only; it is not setup, invite, approval, text, voice, persistence, or production evidence.") {
    failures.push("dry-run manifest missing dry-run boundary");
  }
  if (!manifest.preflight_result?.checks) failures.push("dry-run manifest missing preflight checks");
}

if (failures.length > 0) {
  console.error("P12-T02 Tauri WebDriver integrated contract check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("P12-T02 Tauri WebDriver integrated contract check passed");

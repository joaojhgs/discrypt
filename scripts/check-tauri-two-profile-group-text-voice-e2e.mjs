#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const harness = read("scripts/tauri-two-profile-group-text-voice-e2e.mjs");
const uiMain = read("apps/ui/src/main.tsx");
const uiCommands = read("apps/ui/src/commands.ts");
const releaseMatrix = read("docs/release/release-verification-matrix.md");
const evidenceDoc = read("docs/release/tauri-two-profile-group-text-voice-e2e.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

function rejectText(name, text, token) {
  if (text.includes(token)) failures.push(`${name} contains forbidden token: ${token}`);
}

if (!packageJson.scripts?.["test:tauri-two-profile-group-text-voice-e2e-contract"]) {
  failures.push("package script missing test:tauri-two-profile-group-text-voice-e2e-contract");
}

for (const token of [
  "workflowSteps",
  "artifactContract",
  "setup",
  "invite",
  "approval",
  "text",
  "voice",
  "persistence",
  "degraded_unavailable",
  "Invite parsing is not membership",
  "rejects manual command-bridge fallback",
  "Synthetic WebView peer-connection fallback is diagnostic only",
  "acceptance_criteria",
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
  "startLocalMqttBroker",
  "DISCRYPT_DEFAULT_MQTT_ENDPOINT",
  "VITE_DISCRYPT_DEFAULT_MQTT_ENDPOINT",
  "approvePendingAdmissionThroughUi",
  "admission-approve-${pending.request_id}",
  "startProviderControlLanePair",
  "attach_broker_control_lane_runtime",
  "drain_text_control_inbound_frames",
  "admissionRequestProviderPump = await pumpProviderControlLaneBidirectional",
  "admissionDecisionProviderPump = await pumpProviderControlLaneBidirectional",
  '"group-text",\n    8,\n    groupTextProviderBaseline',
  '"voice-signaling-provider-runtime"',
  "manual_command_bridge_used: false",
  "strict_e2e_eligible: remotePlaintextObserved && nativeVoiceLoopbackObserved && strictProviderRuntimeObserved",
]) {
  requireText("Tauri two-profile group text and voice E2E harness", harness, token);
}

for (const token of [
  "requiresAdmissionControlLane",
  "attachBrokerControlLaneRuntime",
  "drainTextControlInboundFrames",
  "startControlLaneSessionManager",
  'activeGroup.role === "pending"',
  'member.status !== "pending"',
]) {
  requireText("native UI admission control-lane selection", uiMain, token);
}

for (const token of [
  '"attach_broker_control_lane_runtime"',
  '"drain_text_control_inbound_frames"',
  '"start_control_lane_session_manager"',
]) {
  requireText("UI native command wrappers", uiCommands, token);
}

for (const token of [
  "Tauri Two-Profile Group Text and Voice E2E",
  "npm --prefix apps/ui run test:tauri-two-profile-group-text-voice-e2e-contract",
  "node scripts/tauri-two-profile-group-text-voice-e2e.mjs --run --require-native-voice",
  "target/tauri-two-profile-group-text-voice-e2e/<run-id>/tauri-two-profile-group-text-voice-e2e-summary.json",
  "setup, invite, owner/staff approval, text, voice, persistence, and degraded/unavailable-state evidence",
  "Dry-run is contract/preflight evidence only",
]) {
  requireText("two-profile E2E evidence doc", evidenceDoc, token);
}

for (const token of [
  "Tauri two-profile group text and voice E2E",
  "test:tauri-two-profile-group-text-voice-e2e-contract",
  "target/tauri-two-profile-group-text-voice-e2e/<run-id>/tauri-two-profile-group-text-voice-e2e-summary.json",
]) {
  requireText("release verification matrix", releaseMatrix, token);
}

for (const token of [
  "production_claim_allowed: true",
  "Production-ready: true",
  "provider application relay fallback",
  "bridgeTextControlFramesOnce",
  "bridgeTextControlFramesBidirectional",
  'invokeTauriCommand(profile, "approve_group_admission_request"',
]) {
  rejectText("Tauri two-profile group text and voice E2E harness", harness, token);
}

const dryRunArtifactDir = resolve(repoRoot, "target/tauri-two-profile-group-text-voice-e2e-contract");
const dryRun = spawnSync(process.execPath, [
  "scripts/tauri-two-profile-group-text-voice-e2e.mjs",
  "--artifact-dir",
  dryRunArtifactDir,
], {
  cwd: repoRoot,
  encoding: "utf8",
  env: {
    ...process.env,
    DISCRYPT_TAURI_TWO_PROFILE_E2E_RUN_ID: "contract-dry-run",
  },
});
if (dryRun.status !== 0) {
  failures.push(`dry-run harness contract exited ${dryRun.status}: ${dryRun.stdout}\n${dryRun.stderr}`);
}

const manifestPath = resolve(dryRunArtifactDir, "tauri-two-profile-group-text-voice-e2e-manifest.json");
if (!existsSync(manifestPath)) {
  failures.push(`dry-run manifest missing at ${manifestPath}`);
} else {
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (manifest.mode !== "dry-run") failures.push(`dry-run manifest mode was ${manifest.mode}`);
  if (!Array.isArray(manifest.workflow_steps) || manifest.workflow_steps.length < 7) {
    failures.push("dry-run manifest missing workflow steps");
  }
  if (manifest.artifact_contract?.dry_run_boundary !== "Dry-run writes the manifest/preflight contract only; it is not setup, invite, approval, text, voice, persistence, or production evidence.") {
    failures.push("dry-run manifest missing dry-run boundary");
  }
  if (!manifest.preflight_result?.checks) failures.push("dry-run manifest missing preflight checks");
}

if (failures.length > 0) {
  console.error("Tauri two-profile group text and voice E2E contract check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("Tauri two-profile group text and voice E2E contract check passed");

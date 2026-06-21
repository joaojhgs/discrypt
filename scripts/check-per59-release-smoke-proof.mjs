#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const harness = read("scripts/g012-tauri-webdriver-integrated.mjs");
const ui = read("apps/ui/src/main.tsx");
const desktop = read("apps/desktop/src-tauri/src/lib.rs");
const releaseNote = read("docs/release/per59-human-loopback-release-smoke-2026-06-20.md");
const plan = read(".omx/plans/P6-T08-human-loopback-release-smoke-2026-06-20.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "configureReleaseSmokeAudioPreferences",
  "readReleaseSmokeAudioPreferences",
  "per59_release_smoke",
  "mic_gain_and_output_volume_proved",
  "per_peer_volume_surface_proved",
  "production_claim_allowed",
  "browser_shim_or_raw_pulse_capture_counts_as_production: false",
  "nativeMedia.mic_gain_percent",
  "nativeMedia.app_output_volume_percent",
  "adjustRemoteParticipantVolumes",
  "backendRuntimePeerIdFromCommitment",
  "backend-derived-signed-group-bootstrap",
  "voice-session-signaling",
  "media_runtime?.local_capture_active",
  "profile ready or trust setup screen",
  "local profile ready|start a private space",
  "contextClickText",
  "Open Two Profile WebDriver Lab group",
  "await click(profile, \"Create invite\");",
  "Create invite for Two Profile WebDriver Lab",
  "Join with invite",
  "Local label",
  "approvePendingAdmission",
  "approve_group_admission_request",
  "openmls-admission-request",
  "openmls_admission_owner_approval",
  "waitForAdmissionUnlockedUi",
  "post-admission unlocked composer",
  "Waiting for owner\\/staff approval before protected messages can be sent",
  "messageEditable",
  "Send a message",
  "Local profile ready|Start a private space|Two Profile WebDriver Lab",
  "assertNoAdmissionDecisionApplyFailure",
  "admission_decision_apply_failed",
  "await click(profile, \"Send message\");",
  "native voice media failed after join",
  "already joined native voice media",
  "alreadyJoinedVoiceUiPredicate",
  "waitForAlreadyJoinedNativeVoice",
  "already_joined_native_voice_last",
  "voice.native_media_started",
  "voice.native_media_received",
]) {
  requireText("G012 WebDriver harness", harness, token);
}

for (const token of [
  "localGovernedGroupRole",
  "voiceSignaling?.local_peer_id",
  "groupRuntimePeers.find",
  "peer.role === localGroupPeerRole",
  "startNativeRustVoiceMediaSession",
]) {
  requireText("Discrypt UI native voice peer selection", ui, token);
}

for (const token of [
  "voice_runtime_peer_boundary_missing",
  "Voice signaling is ready with backend-derived runtime peer ids before SDP/ICE exchange",
  "joined_session.signaling.local_peer_id",
]) {
  requireText("Discrypt backend native voice peer seeding", desktop, token);
}

for (const token of [
  "per59_release_smoke.production_claim_allowed: true",
  "Synthetic WebView media fallback",
  "raw Pulse capture",
  "DISCRYPT_G012_REQUIRE_NATIVE_VOICE=1",
  "scripts/g012-docker-tauri-preflight.sh",
]) {
  requireText("PER-59 release note", releaseNote, token);
}

for (const token of [
  "PER-59 / P6-T08",
  "voice join, backend self-mute, speaking/VAD media evidence",
  "static/dry-run/local backend checks are PR readiness evidence only",
  "production-ready release evidence requires the display/audio-capable command",
]) {
  requireText("PER-59 OMX plan", plan, token);
}

if (failures.length > 0) {
  console.error("PER-59 release smoke proof guard failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("PER-59 release smoke proof guard passed");

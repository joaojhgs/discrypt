#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const harness = read("scripts/tauri-two-profile-group-text-voice-e2e.mjs");
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
  "runtimePeersFromAppState",
  "persisted-runtime-peers",
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
  "approvePendingAdmissionThroughUi",
  "admission-approve-${pending.request_id}",
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
  "after_voice_leave_reload",
  "await click(profiles.alice, \"^Mute$\");",
  "await click(profiles.alice, \"^Unmute$\");",
  "await click(profile, \"Leave voice call\");",
  "waitForLeftVoice",
  "readJsonIfExists(profile.state_path)",
  "source: \"persisted_app_state\"",
  "timeoutMs = 120_000",
  "voice_session_cleared",
  "voice_left_event",
  "backendLeaveCleanupObserved",
  "leave_cleanup",
  "aliceRetainedNativeVoiceEvidence",
  "voice?.before_leave?.alice?.evidence",
  "bobRetainedNativeVoiceEvidence",
  "voice?.before_leave?.bob?.evidence",
  "nativeRustWebDriverEvidenceObserved",
  "mode === \"native_rust_webrtc_datachannel\"",
  "remoteTrackEvents > 0",
  "nativeRustVoiceRuntimeAvailable",
  "syntheticFallback === false",
  "native_rust_evidence_source",
  "voice.before_leave.*.evidence",
  "native Rust Opus/SFrame media proof or generated-audio loopback",
  "voice.native_media_started",
  "voice.native_media_received",
]) {
  requireText("two-profile E2E harness", harness, token);
}

for (const token of [
  "hasCurrentAdvertisedRemoteGroupRuntimePeer",
  "voiceSignaling?.local_peer_id",
  "groupRuntimePeers.find",
  "peer.source !== \"sealed_provider_peer_advertisement_v1\"",
  "startNativeRustVoiceMediaSession",
]) {
  requireText("Discrypt UI native voice peer selection", ui, token);
}

for (const token of [
  "native_voice_runtime_peer_attachment",
  "Voice signaling is ready with provider-advertised runtime peer ids before SDP/ICE exchange",
  "joined_session.signaling.local_peer_id",
  "voice_join_without_remote_peer_waits_without_error",
]) {
  requireText("Discrypt backend native voice peer seeding", desktop, token);
}

if (desktop.includes("voice_runtime_peer_boundary_missing")) {
  failures.push(
    "Discrypt backend retains obsolete voice join failure for a temporarily absent remote peer",
  );
}

for (const token of [
  "backendRuntimePeerIdFromCommitment",
  "backend-derived-signed-group-bootstrap",
  "group-owner-runtime-peer",
  "group-member-runtime-peer",
]) {
  if (harness.includes(token)) {
    failures.push(`two-profile E2E harness retains obsolete runtime-peer derivation: ${token}`);
  }
}

for (const token of [
  "per59_release_smoke.production_claim_allowed: true",
  "Synthetic WebView media fallback",
  "raw Pulse capture",
  "DISCRYPT_TAURI_TWO_PROFILE_E2E_REQUIRE_NATIVE_VOICE=1",
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

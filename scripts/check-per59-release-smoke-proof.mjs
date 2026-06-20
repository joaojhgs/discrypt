#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const harness = read("scripts/g012-tauri-webdriver-integrated.mjs");
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
  "profile ready or trust setup screen",
  "local profile ready|start a private space",
  "contextClickText",
  "Open Two Profile WebDriver Lab group",
  "await click(profile, \"Create invite\");",
  "Create invite for Two Profile WebDriver Lab",
  "Join with invite",
  "Local label",
]) {
  requireText("G012 WebDriver harness", harness, token);
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

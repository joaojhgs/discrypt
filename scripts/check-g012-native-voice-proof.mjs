#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const harness = read("scripts/g012-tauri-webdriver-integrated.mjs");
const handoff = read("docs/release/handoff-2026-06-01.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

function rejectText(name, text, token) {
  if (text.includes(token)) failures.push(`${name} still contains forbidden token: ${token}`);
}

for (const token of [
  "nativeRTCPeerConnectionAvailable",
  "nativeGeneratedAudioTrackAvailable",
  "syntheticFallback",
  "fallbackReason",
  "remote_plaintext_text_and_native_voice_loopback_observed",
  "synthetic_peerconnection_fallback_loopback",
  "g012_checkpoint_eligible: remotePlaintextObserved && nativeVoiceLoopbackObserved",
  "production_claim_allowed: nativeVoiceLoopbackObserved",
  "this artifact is not eligible to checkpoint G012 as production voice",
]) {
  requireText("G012 WebDriver harness", harness, token);
}

rejectText(
  "G012 WebDriver harness",
  harness,
  "remotePlaintextObserved && voiceLoopbackObserved ? \"remote_plaintext_text_and_voice_loopback_observed\"",
);
rejectText(
  "G012 WebDriver harness",
  harness,
  "voiceLoopbackObserved ? \"browser_media_harness_loopback\"",
);

for (const token of [
  "2026-06-01 21:20 UTC — G012 native voice proof audit",
  "native `RTCPeerConnection` generated-audio loopback",
  "`g012_checkpoint_eligible` is `true` only when remote plaintext text and native generated-audio voice both pass",
  "Current worker-3 environment cannot rerun the full WebDriver proof",
  "G012 cannot be checkpointed from the synthetic fallback artifact",
]) {
  requireText("G012 handoff", handoff, token);
}

if (failures.length > 0) {
  console.error("G012 native voice proof guard failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G012 native voice proof guard passed");

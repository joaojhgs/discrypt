#!/usr/bin/env node
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const failures = [];

function requireText(path, token) {
  const text = read(path);
  if (!text.includes(token)) failures.push(`${path}: missing token ${token}`);
}

function forbid(path, pattern, reason) {
  const text = read(path);
  const match = typeof pattern === "string" ? text.includes(pattern) : pattern.test(text);
  if (match) failures.push(`${path}: ${reason}`);
}

for (const token of [
  "P3-T09",
  "WebRtcIceTransportPolicy::RelayOnly",
  "DISCRYPT_PUBLIC_TURN_E2E",
  "DISCRYPT_PUBLIC_TURN_ENDPOINT",
  "DISCRYPT_PUBLIC_TURN_USERNAME",
  "DISCRYPT_PUBLIC_TURN_CREDENTIAL",
  "write_public_turn_proof_artifact",
  "provider_application_relay_used",
  "raw_turn_credential_logged",
  "offerer_turn_fallback_ready",
  "answerer_turn_fallback_ready",
  "offerer_local_relay_candidates_gathered",
  "answerer_local_relay_candidates_gathered",
]) {
  requireText("crates/transport/tests/public_webrtc_datachannel_e2e.rs", token);
}

for (const token of [
  "P3-T09 Configured TURN Proof",
  "provider_application_relay_used=false",
  "target/e2e/per-30-configured-turn-proof/public-turn-relay-only.json",
  "DISCRYPT_PUBLIC_TURN_ARTIFACT_PATH",
  "username, credential, SDP, ICE candidate lines",
  "not production-ready installed-app evidence",
]) {
  requireText("docs/release/p3-t09-configured-turn-proof-2026-06-19.md", token);
}

for (const token of [
  "P3-T09 configured TURN proof",
  "crates/transport/tests/public_webrtc_datachannel_e2e.rs",
  "scripts/check-configured-turn-proof-p3-t09.mjs",
  "RelayOnly",
  "provider signaling-only policy",
]) {
  requireText(".omx/plans/P3-T09-configured-turn-proof-2026-06-19.md", token);
}

requireText("apps/ui/package.json", "test:p3-t09-configured-turn-proof");

for (const token of [
  "RTCPeerConnection",
  "iceTransportPolicy: \"relay\"",
  "offerer_relay_candidates",
  "answerer_relay_candidates",
  "selected_candidate_pairs",
  "not Rust webrtc dependency TURN-gathering support",
]) {
  requireText("scripts/per30-browser-turn-proof.mjs", token);
}

for (const token of [
  "per30-configured-turn-proof",
  "PER-30 configured TURN proof",
  "turnserver -c",
  "playwright install --with-deps chromium",
  "scripts/per30-browser-turn-proof.mjs",
  "DISCRYPT_PUBLIC_TURN_E2E: \"1\"",
  "DISCRYPT_PUBLIC_TURN_ARTIFACT_PATH",
  "actions/upload-artifact@v4",
]) {
  requireText(".github/workflows/ci.yml", token);
}

forbid(
  "docs/release/p3-t09-configured-turn-proof-2026-06-19.md",
  /DISCRYPT_PUBLIC_TURN_(?:USERNAME|CREDENTIAL)=([^<\s][^\s]*)/,
  "release doc must not include raw TURN username or credential values"
);

const defaultArtifact = resolve(
  repoRoot,
  "target/e2e/per-30-configured-turn-proof/public-turn-relay-only.json"
);
const artifactPath = process.env.DISCRYPT_PUBLIC_TURN_ARTIFACT_PATH
  ? resolve(repoRoot, process.env.DISCRYPT_PUBLIC_TURN_ARTIFACT_PATH)
  : defaultArtifact;

if (existsSync(artifactPath)) {
  const raw = readFileSync(artifactPath, "utf8");
  const artifact = JSON.parse(raw);
  const requiredPairs = [
    ["schema_version", "discrypt.p3_t09.configured_turn_proof.v1"],
    ["issue", "PER-30 / P3-T09"],
    ["status", "passed"],
    ["provider_application_relay_used", false],
  ];
  for (const [key, expected] of requiredPairs) {
    if (artifact[key] !== expected) {
      failures.push(`${artifactPath}: expected ${key}=${expected}, got ${artifact[key]}`);
    }
  }
  if (artifact.route_policy?.ice_transport_policy !== "relay_only") {
    failures.push(`${artifactPath}: route_policy.ice_transport_policy must be relay_only`);
  }
  if (artifact.route_policy?.direct_candidates_allowed !== false) {
    failures.push(`${artifactPath}: direct_candidates_allowed must be false`);
  }
  if (artifact.route_evidence?.offerer_relay_candidates < 1) {
    failures.push(`${artifactPath}: offerer relay candidate count must be positive`);
  }
  if (artifact.route_evidence?.answerer_relay_candidates < 1) {
    failures.push(`${artifactPath}: answerer relay candidate count must be positive`);
  }
  if (artifact.route_evidence?.text_control_frame_roundtrip !== true) {
    failures.push(`${artifactPath}: text/control frame roundtrip must be true`);
  }
  if (artifact.route_evidence?.receipt_frame_roundtrip !== true) {
    failures.push(`${artifactPath}: receipt frame roundtrip must be true`);
  }
  if (artifact.turn_credentials?.username_redacted !== true) {
    failures.push(`${artifactPath}: TURN username must be marked redacted`);
  }
  if (artifact.turn_credentials?.credential_redacted !== true) {
    failures.push(`${artifactPath}: TURN credential must be marked redacted`);
  }
  for (const field of [
    "raw_turn_endpoint_logged",
    "raw_turn_username_logged",
    "raw_turn_credential_logged",
    "raw_sdp_logged",
    "raw_ice_candidate_logged",
    "raw_text_control_payload_logged",
  ]) {
    if (artifact.redaction?.[field] !== false) {
      failures.push(`${artifactPath}: redaction.${field} must be false`);
    }
  }
  for (const secretName of [
    "DISCRYPT_PUBLIC_TURN_USERNAME",
    "DISCRYPT_PUBLIC_TURN_CREDENTIAL",
  ]) {
    const secret = process.env[secretName];
    if (secret && raw.includes(secret)) {
      failures.push(`${artifactPath}: artifact contains raw ${secretName}`);
    }
  }
}

if (failures.length > 0) {
  console.error("P3-T09 configured TURN proof gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("P3-T09 configured TURN proof gate passed.");

#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const report = read("docs/release/per100-public-mqtt-split-machine-matrix-2026-06-26.md");
const matrix = read("docs/release/release-verification-matrix.md");
const plan = read(".omx/plans/P12-T05-public-mqtt-split-machine-matrix-2026-06-26.md");
const g009 = read("apps/desktop/src-tauri/examples/g009_split_machine_app_flow.rs");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

function rejectPattern(name, text, pattern, label) {
  if (pattern.test(text)) failures.push(`${name} contains forbidden overclaim: ${label}`);
}

if (!packageJson.scripts?.["test:p12-t05-public-mqtt-split-machine-matrix"]) {
  failures.push("apps/ui/package.json missing test:p12-t05-public-mqtt-split-machine-matrix");
}

for (const token of [
  "PER-100 / P12-T05",
  "Phase 12 full E2E harness expansion",
  "MQTT remains signaling/rendezvous only",
  "Invite parsing is not membership",
  "direct WebRTC DataChannel or configured TURN-backed WebRTC route evidence",
  "Presence claims require backend route-gated TTL evidence",
  "RUSTUP_TOOLCHAIN=1.89.0 cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --features harness --example g009_split_machine_app_flow",
  "--adapter mqtt",
  "SSH",
]) {
  requireText("PER-100 plan", plan, token);
}

for (const token of [
  "# PER-100 Public MQTT Split-Machine Matrix",
  "Evidence level: local MQTT-labeled app-flow harness evidence",
  "This is not production-ready evidence.",
  "Local MQTT app-flow substitute",
  "Local host + SSH remote public MQTT promotion",
  "DISCRYPT_G009_SSH_TARGET",
  "mqtts://broker.emqx.io:8883",
  "target/per100-public-mqtt-split-machine-matrix/local-pair.json",
  "provider_application_relay_used: false",
  "authorized OpenMLS admission before protected text",
  "direct or configured TURN-backed WebRTC route evidence",
  "route-gated presence TTL",
  "MQTT is not an application relay",
]) {
  requireText("PER-100 report", report, token);
}

for (const token of [
  "P12-T05 public MQTT split-machine matrix",
  "npm --prefix apps/ui run test:p12-t05-public-mqtt-split-machine-matrix",
  "target/per100-public-mqtt-split-machine-matrix/local-pair.json",
  "local+SSH public MQTT promotion",
  "Local substitute artifacts are not split-machine proof",
]) {
  requireText("release verification matrix", matrix, token);
}

for (const token of [
  "\"provider_application_relay_used\": false",
  "message_relay_fallback",
  "\"disabled\".to_owned()",
  "\"backend_route_gated_ttl\"",
  "Local pair uses harness-only isolated app-state files",
  "provider_relay_boundary",
  "voice_evidence",
]) {
  requireText("G009 app-flow example", g009, token);
}

for (const [name, text] of [
  ["PER-100 report", report],
  ["PER-100 plan", plan],
]) {
  rejectPattern(name, text, /\bDiscrypt is production-ready\./i, "absolute production-ready claim");
  rejectPattern(name, text, /\bcomplete split-machine proof\b/i, "complete split-machine claim");
  rejectPattern(name, text, /\bMQTT (?:is|as|provides|proves) (?:an? )?application relay\b/i, "MQTT application relay claim");
}

if (failures.length > 0) {
  console.error("P12-T05 public MQTT split-machine matrix check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("P12-T05 public MQTT split-machine matrix check passed");

#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const ci = read(".github/workflows/ci.yml");
const releaseMatrix = read("docs/release/release-verification-matrix.md");
const g010Doc = read("docs/release/g010-adapter-public-matrix.md");
const g132Script = read("scripts/check-signaling-e2e-matrix-g132.mjs");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "# G010 adapter/public matrix",
  "Local deterministic gates",
  "Public adapter matrix",
  "DISCRYPT_PUBLIC_SIGNALING_E2E",
  "DISCRYPT_PUBLIC_NOSTR_E2E",
  "DISCRYPT_PUBLIC_IPFS_E2E",
  "DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E",
  "DISCRYPT_PUBLIC_TURN_E2E",
  "G011/G012 are not claimed",
]) {
  requireText("g010-adapter-doc", g010Doc, token);
}

for (const token of [
  "Public MQTT smoke (opt-in real provider)",
  "Public Nostr smoke (opt-in real provider)",
  "Public IPFS explicit-topic-peer smoke (opt-in real provider)",
  "Deployed Discrypt rendezvous smoke (opt-in real provider)",
  "Public MQTT relay-only TURN WebRTC DataChannel (opt-in real provider)",
  "DISCRYPT_PUBLIC_TURN_E2E",
]) {
  requireText("g132-signaling-matrix-script", g132Script, token);
}

if (packageJson.scripts?.["test:g010-adapter-public-matrix"] !== "node ../../scripts/check-g010-adapter-public-matrix.mjs") {
  failures.push("package.json missing test:g010-adapter-public-matrix script");
}

for (const token of [
  "npm run test:g010-adapter-public-matrix",
  "G010 adapter/public matrix",
  "npm --prefix apps/ui run test:g010-adapter-public-matrix",
]) {
  requireText(token.includes("npm run") ? "ci" : "release-verification-matrix", token.includes("npm run") ? ci : releaseMatrix, token);
}

if (failures.length === 0) {
  const env = { ...process.env };
  for (const key of [
    "DISCRYPT_PUBLIC_SIGNALING_E2E",
    "DISCRYPT_PUBLIC_MQTT_E2E",
    "DISCRYPT_PUBLIC_NOSTR_E2E",
    "DISCRYPT_PUBLIC_IPFS_E2E",
    "DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E",
    "DISCRYPT_PUBLIC_TURN_E2E",
  ]) {
    delete env[key];
  }

  const result = spawnSync("node", ["scripts/check-signaling-e2e-matrix-g132.mjs"], {
    cwd: repoRoot,
    encoding: "utf8",
    env,
  });
  const output = `${result.stdout}\n${result.stderr}`;
  if (result.status !== 0) {
    failures.push(`underlying signaling matrix failed:\n${output}`.trim());
  }
  for (const token of [
    "Public MQTT smoke (opt-in real provider) skipped",
    "Public Nostr smoke (opt-in real provider) skipped",
    "Public IPFS explicit-topic-peer smoke (opt-in real provider) skipped",
    "Deployed Discrypt rendezvous smoke (opt-in real provider) skipped",
    "Public MQTT relay-only TURN WebRTC DataChannel (opt-in real provider) skipped",
  ]) {
    if (!output.includes(token)) failures.push(`signaling matrix output missing explicit skip: ${token}`);
  }
  if (!output.includes("G132 real E2E matrix check passed with missing optional entries reported as skips.")) {
    failures.push("signaling matrix did not report pass-with-skips boundary");
  }
}

if (failures.length > 0) {
  console.error("G010 adapter/public matrix check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G010 adapter/public matrix check passed with public MQTT/Nostr/IPFS/QUIC/TURN gates env-skipped unless explicitly enabled.");

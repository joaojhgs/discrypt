#!/usr/bin/env node
import { readFileSync, readdirSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const failures = [];

function repoPath(path) {
  return relative(repoRoot, path).split(sep).join("/");
}

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

function walk(root, predicate, files = []) {
  for (const entry of readdirSync(root)) {
    if (["node_modules", "target", ".git", ".omx", ".omc"].includes(entry)) continue;
    const full = resolve(root, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) walk(full, predicate, files);
    else if (predicate(full)) files.push(full);
  }
  return files;
}

const docs = read("docs/security/g009-security-privacy-no-shim-gates.md");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const signaling = read("crates/transport/src/signaling.rs");
const providerAdapters = read("crates/transport/src/provider_adapters.rs");
const webrtc = read("crates/transport/src/webrtc_negotiation.rs");
const ice = read("crates/transport/src/ice.rs");
const desktop = read("apps/desktop/src-tauri/src/lib.rs");
const storage = read("crates/storage/src/appdb.rs");
const commands = read("apps/ui/src/commands.ts");
const voiceMedia = read("apps/ui/src/voice-media.ts");
const commandErrorLog = read("apps/ui/src/command-error-log.ts");
const ci = read(".github/workflows/ci.yml");
const releaseMatrix = read("docs/release/release-verification-matrix.md");
const handoff = read("docs/release/handoff-2026-06-01.md");
const finalE2eGate = read("scripts/check-final-e2e-g131.mjs");

for (const [name, text] of Object.entries({ handoff, finalE2eGate })) {
  for (const forbidden of [
    "Ship production UI",
    "G131-full-production-e2e-verification-acr",
  ]) {
    if (text.includes(forbidden)) failures.push(`${name} contains pre-G012 production overclaim: ${forbidden}`);
  }
}


for (const token of [
  "# G009 security, privacy, and no-shim gate",
  "raw SDP offers/answers or ICE candidates/credentials",
  "TURN usernames, credentials, or credential-bearing URLs",
  "plaintext text messages, audio frames, MLS/SFrame/content keys",
  "EncryptedAppDb",
  "test:security-privacy-g009",
  "G011 and G012",
]) requireText("g009-doc", docs, token);

if (!packageJson.scripts?.["test:security-privacy-g009"]) {
  failures.push("apps/ui/package.json missing test:security-privacy-g009");
}
requireText("ci", ci, "npm run test:security-privacy-g009");
requireText("release matrix", releaseMatrix, "G009 security/privacy/no-shim gate");
requireText("release matrix", releaseMatrix, "test:security-privacy-g009");

for (const token of [
  "Adapters exchange pre-derived rendezvous capabilities and already-sealed",
  "They do not receive invite secrets",
  "raw SDP",
  "raw ICE credentials",
  "TURN secrets",
  "message/audio plaintext",
  "impl fmt::Debug for OpaqueSignalingPayload",
]) requireText("signaling contract", signaling, token);

for (const token of [
  "impl fmt::Debug for WebRtcSessionDescription",
  "impl fmt::Debug for WebRtcIceCandidate",
  "impl fmt::Debug for SealedWebRtcNegotiationPayload",
]) requireText("webrtc debug redaction", webrtc, token);

for (const token of [
  "impl std::fmt::Debug for TurnServerConfig",
  "credential",
  "<redacted>",
]) requireText("turn debug redaction", ice, token);

for (const token of [
  "EncryptedAppDb persists a serde_json envelope encrypted with AES-256-GCM",
  "MemoryAppDbKeychain is restricted to tests/local/non-production builds",
  "encrypted_app_db_round_trips_without_plaintext_in_db_or_wal",
]) requireText("encrypted app db", storage, token);

for (const token of [
  "LOCAL_DEV_VOICE_SIGNAL_FALLBACK_ENABLED",
  "tauriVoiceSignalingAvailable",
  "createLocalDevVoiceSignalBroadcast",
  "postLocalDevVoiceSignal",
  "Backend sealed voice signaling failed closed",
]) requireText("voice media sealed signaling fallback", voiceMedia, token);
if (/catch\(\([^)]*\)\s*=>\s*\{[\s\S]{0,240}broadcast\?\.postMessage\(signal\)/.test(voiceMedia)) {
  failures.push("voice-media must not recover from backend sealed signaling failure by posting raw BroadcastChannel signals");
}
if (/if\s*\(\s*tauriVoiceSignalingAvailable\(\)\s*\)\s*\{[\s\S]{0,900}broadcast\?\.postMessage\(signal\)/.test(voiceMedia)) {
  failures.push("voice-media must not post raw BroadcastChannel signals while Tauri IPC is available");
}

for (const token of [
  "fn redacted_observable_ref",
  "fn redacted_message_ref",
  "redacted_observable_label(\"topic\"",
  "mqtt event error redacted",
]) requireText("redacted runtime observability", `${desktop}\n${providerAdapters}`, token);

for (const token of [
  "COMMAND_ERROR_LOG_MARKER",
  "\"[discrypt:command-error] command_error_reported\"",
  "export function logSanitizedCommandError(): void",
  "console.error(COMMAND_ERROR_LOG_MARKER);",
]) requireText("sanitized UI command-error console logger", commandErrorLog, token);
const commandErrorConsoleMatches =
  commandErrorLog.match(/console\.(?:log|debug|info|warn|error)\(/g) ?? [];
if (
  commandErrorConsoleMatches.length !== 1 ||
  commandErrorConsoleMatches[0] !== "console.error("
) {
  failures.push(
    "apps/ui/src/command-error-log.ts may contain only the approved marker-only console.error path",
  );
}
if (
  /title|message|stack|profile|channel|identity|provider|transport|backend/.test(
    commandErrorLog,
  )
) {
  failures.push(
    "apps/ui/src/command-error-log.ts must not include runtime payload fields in the console log path",
  );
}

const forbiddenSourcePatterns = [
  [/eprintln!\([^\n]*(?:\{topic\}|\{payload\}|\{err\}|room_secret|credential|message_id)/, "source eprintln may leak raw topic/payload/error/secret/message id"],
  [/console\.(?:log|debug|info|warn|error)\(/, "UI source must not log potentially sensitive runtime state"],
  [/(?:globalThis|window)\s*\[\s*["']console["']\s*\]|\[\s*["']console["']\s*\]\s*\?\./, "UI source must not bypass console logging policy with bracket access"],
  [/localStorage\.setItem\((?!\s*FALLBACK_STORAGE_KEY)/, "localStorage writes must use the explicit local-dev fallback key"],
];

for (const file of [
  ...walk(resolve(repoRoot, "apps/ui/src"), (path) => /\.(ts|tsx)$/.test(path)),
  resolve(repoRoot, "apps/desktop/src-tauri/src/lib.rs"),
  resolve(repoRoot, "crates/transport/src/provider_adapters.rs"),
]) {
  const text = readFileSync(file, "utf8");
  for (const [pattern, message] of forbiddenSourcePatterns) {
    if (
      message === "UI source must not log potentially sensitive runtime state" &&
      repoPath(file) === "apps/ui/src/command-error-log.ts"
    ) {
      continue;
    }
    if (pattern.test(text)) failures.push(`${repoPath(file)}: ${message}`);
  }
}

if (!/const LOCAL_DEV_FALLBACK_ENABLED\s*=/.test(commands) || !commands.includes("VITE_DISCRYPT_LOCAL_DEV_FALLBACK")) {
  failures.push("commands.ts must gate browser fallback persistence behind local-dev/test configuration");
}
if (!commands.includes("Tauri IPC unavailable") || !commands.includes("local-dev/test harness")) {
  failures.push("commands.ts fallback error must identify local-dev/test harness only");
}

function run(label, command, args) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8" });
  if (result.status !== 0) {
    failures.push(`${label} failed:\n${result.stdout}\n${result.stderr}`.trim());
  }
}

run("Provider plaintext rejection", "cargo", [
  "test",
  "-q",
  "-p",
  "discrypt-transport",
  "local_conformance_adapter_rejects_plaintext_sdp_and_ice_markers",
  "--",
  "--nocapture",
]);
run("Encrypted app db no-plaintext persistence", "cargo", [
  "test",
  "-q",
  "-p",
  "discrypt-storage",
  "encrypted_app_db_round_trips_without_plaintext_in_db_or_wal",
  "--",
  "--nocapture",
]);

if (failures.length > 0) {
  console.error("G009 security/privacy/no-shim gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G009 security/privacy/no-shim gate passed.");

#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const failures = [];

function forbid(path, pattern, reason) {
  const text = read(path);
  const match = typeof pattern === "string" ? text.includes(pattern) : pattern.test(text);
  if (match) failures.push(`${path}: ${reason}`);
}

function requireText(path, token) {
  const text = read(path);
  if (!text.includes(token)) failures.push(`${path}: missing token ${token}`);
}

function extractBraceBlock(text, startIndex) {
  const firstBrace = text.indexOf("{", startIndex);
  if (firstBrace === -1) return "";
  let depth = 0;
  for (let index = firstBrace; index < text.length; index += 1) {
    const char = text[index];
    if (char === "{") depth += 1;
    if (char === "}") {
      depth -= 1;
      if (depth === 0) return text.slice(firstBrace, index + 1);
    }
  }
  return "";
}

function requireMethodFailsClosed(text, roomName, methodName) {
  const implToken = `impl RendezvousRoom for ${roomName}`;
  const implStart = text.indexOf(implToken);
  if (implStart === -1) {
    failures.push(`crates/transport/src/provider_adapters.rs: missing ${implToken}`);
    return;
  }
  const implBlock = extractBraceBlock(text, implStart);
  const methodStart = implBlock.indexOf(`async fn ${methodName}`);
  if (methodStart === -1) {
    failures.push(
      `crates/transport/src/provider_adapters.rs: ${roomName}.${methodName} is missing`
    );
    return;
  }
  const methodBlock = extractBraceBlock(implBlock, methodStart);
  if (!methodBlock.includes("Err(provider_app_payload_relay_disabled_error())")) {
    failures.push(
      `crates/transport/src/provider_adapters.rs: ${roomName}.${methodName} must fail closed with provider_app_payload_relay_disabled_error()`
    );
  }
}

forbid(
  "apps/desktop/src-tauri/examples/g009_split_machine_app_flow.rs",
  "continuing with provider relay fallback",
  "g009 must fail instead of continuing through provider app-payload relay fallback"
);
requireText(
  "apps/desktop/src-tauri/examples/g009_split_machine_app_flow.rs",
  "provider signaling is not a message relay"
);

forbid(
  "crates/transport/src/provider_adapters.rs",
  /\bcontrol_roundtrip\b/,
  "provider adapter probes must not expose provider control/app-payload roundtrip evidence"
);
requireText(
  "crates/transport/src/provider_adapters.rs",
  "provider application-payload relay is disabled; providers carry presence and sealed WebRTC negotiation only"
);
forbid(
  "crates/transport/src/provider_adapters.rs",
  /sealed control delivery|control delivery without reaching|control bytes|one sealed\s+control payload over the configured provider profile/,
  "provider adapter source docs must not advertise provider control/app-payload relay"
);

for (const path of [
  "crates/transport/tests/public_signaling_e2e.rs",
  "scripts/check-stun-turn-provider-privacy-g132.mjs",
  "scripts/check-signaling-e2e-matrix-g132.mjs",
  "docs/adapters/ipfs-pubsub-adapter-readiness.md",
  "docs/security/g132-stun-turn-provider-privacy-gate.md",
  "docs/release/g010-adapter-public-matrix.md",
  "docs/release/public-signaling-production-status.md",
]) {
  forbid(
    path,
    "presence_signal_and_control_roundtrip",
    "provider public/local smoke names must not claim control payload relay"
  );
}

for (const path of [
  "docs/adapters/nostr-adapter-readiness.md",
  "docs/security/g132-stun-turn-provider-privacy-gate.md",
  "docs/release/public-signaling-production-status.md",
]) {
  forbid(
    path,
    /sealed (?:WebRTC-negotiation payload, and sealed control broadcast|presence\/signal\/control)/,
    "release copy must describe providers as presence plus sealed WebRTC negotiation only"
  );
}

for (const path of [
  "docs/adapters/nostr-adapter-readiness.md",
  "docs/release/public-signaling-production-status.md",
]) {
  forbid(
    path,
    /opaque room control bytes|control messages via the healthy relay set|discrypt\/v1\/rendezvous\/\{hashed-topic\}\/control|Bob broadcasts sealed control/,
    "provider-facing docs must not advertise provider control/app-payload relay"
  );
}

forbid(
  "apps/ui/src/commands.ts",
  "control_roundtrip",
  "UI DTO must not surface provider control relay as route evidence"
);
forbid(
  "apps/ui/src/commands.ts",
  '"broadcast_control"',
  "UI default signaling profile must not advertise provider control/app-payload relay"
);
forbid(
  "apps/desktop/src-tauri/src/lib.rs",
  "control_roundtrip",
  "desktop DTO must not surface provider control relay as route evidence"
);
forbid(
  "apps/desktop/src-tauri/src/lib.rs",
  '"broadcast_control"',
  "desktop default signaling profile must not advertise provider control/app-payload relay"
);
forbid(
  "crates/transport/src/policy.rs",
  "broadcast_control",
  "transport provider capabilities must not require provider control/app-payload relay"
);

const providerAdapters = read("crates/transport/src/provider_adapters.rs");
for (const roomName of [
  "IpfsPubsubProviderRoom",
  "NostrProviderRoom",
  "DiscryptQuicRendezvousProviderRoom",
  "MqttProviderRoom",
  "LocalConformanceProviderRoom",
]) {
  requireMethodFailsClosed(providerAdapters, roomName, "broadcast_control");
  requireMethodFailsClosed(providerAdapters, roomName, "take_control_payloads");
}

if (failures.length > 0) {
  console.error("P3-T05 provider no-app-relay gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("P3-T05 provider no-app-relay gate passed.");

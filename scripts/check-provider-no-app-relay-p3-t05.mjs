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

function methodBlock(text, roomName, methodName) {
  const implToken = `impl RendezvousRoom for ${roomName}`;
  const implStart = text.indexOf(implToken);
  if (implStart === -1) {
    failures.push(`crates/transport/src/provider_adapters.rs: missing ${implToken}`);
    return "";
  }
  const implBlock = extractBraceBlock(text, implStart);
  const methodStart = implBlock.indexOf(`async fn ${methodName}`);
  if (methodStart === -1) {
    failures.push(
      `crates/transport/src/provider_adapters.rs: ${roomName}.${methodName} is missing`
    );
    return "";
  }
  return extractBraceBlock(implBlock, methodStart);
}

function requireMethodContains(text, roomName, methodName, token, reason) {
  const block = methodBlock(text, roomName, methodName);
  if (block && !block.includes(token)) {
    failures.push(
      `crates/transport/src/provider_adapters.rs: ${roomName}.${methodName} ${reason}`
    );
  }
}

function forbidMethod(text, roomName, methodName, pattern, reason) {
  const block = methodBlock(text, roomName, methodName);
  const match = typeof pattern === "string" ? block.includes(pattern) : pattern.test(block);
  if (block && match) {
    failures.push(`crates/transport/src/provider_adapters.rs: ${roomName}.${methodName} ${reason}`);
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

requireText(
  "crates/transport/src/provider_adapters.rs",
  "DataChannel route evidence, not provider app-payload relay"
);
forbid(
  "crates/transport/src/provider_adapters.rs",
  /provider_application_relay_used:\s*true/,
  "provider route evidence must not report provider application relay use"
);
forbid(
  "apps/ui/src/commands.ts",
  /provider_application_relay_used:\s*true/,
  "UI DTO/defaults must not report provider application relay use"
);
forbid(
  "apps/desktop/src-tauri/src/lib.rs",
  /provider_application_relay_used:\s*true/,
  "desktop DTO/defaults must not report provider application relay use"
);

const providerAdapters = read("crates/transport/src/provider_adapters.rs");
for (const roomName of [
  "IpfsPubsubProviderRoom",
  "NostrProviderRoom",
  "DiscryptQuicRendezvousProviderRoom",
  "MqttProviderRoom",
  "LocalConformanceProviderRoom",
]) {
  requireMethodContains(
    providerAdapters,
    roomName,
    "broadcast_control",
    "reject_forbidden_plaintext",
    "must reject plaintext before relaying sealed provider control frames"
  );
  methodBlock(providerAdapters, roomName, "take_control_payloads");
  for (const methodName of ["broadcast_control", "take_control_payloads"]) {
    forbidMethod(
      providerAdapters,
      roomName,
      methodName,
      /TextMessageEnvelope|TextDeliveryReceipt|receive_text_delivery_envelope|handle_text_control_frame|ProviderApplicationRelay/,
      "must not relay text envelopes/receipts or application payloads through the provider"
    );
  }
}

if (failures.length > 0) {
  console.error("P3-T05 provider no-app-relay gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("P3-T05 provider no-app-relay gate passed.");

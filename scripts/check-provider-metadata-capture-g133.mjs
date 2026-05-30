#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const docs = read("docs/security/g133-provider-visible-metadata-capture.md");
const status = read("docs/release/public-signaling-production-status.md");
const providerAdapters = read("crates/transport/src/provider_adapters.rs");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "# G133 provider-visible metadata capture gate",
  "MQTT (`mqtt`)",
  "Nostr (`nostr`)",
  "IPFS/libp2p PubSub (`ipfs_pubsub`)",
  "Discrypt Rust QUIC rendezvous boundary (`discrypt_quic_rendezvous`)",
  "external host packet-capture artifact",
]) requireText("g133-doc", docs, token);

for (const token of [
  "local_conformance_adapters_deliver_opaque_dm_payloads_without_plaintext_leaks",
  "local_conformance_adapter_rejects_plaintext_sdp_and_ice_markers",
  "SignalingAdapterKind::Mqtt",
  "SignalingAdapterKind::Nostr",
  "SignalingAdapterKind::IpfsPubsub",
  "SignalingAdapterKind::DiscryptQuicRendezvous",
  "assert_no_forbidden_plaintext",
  "assert_no_forbidden_text",
  "TransportError::PlaintextLeak",
]) requireText("provider-adapters", providerAdapters, token);

for (const token of [
  "Provider-visible metadata capture/PCAP tests for MQTT, Nostr, IPFS, and QUIC",
  "test:provider-metadata-capture-g133",
]) requireText("release-status", status, token);

if (!packageJson.scripts?.["test:provider-metadata-capture-g133"]) {
  failures.push("apps/ui/package.json missing test:provider-metadata-capture-g133");
}

if (/external host packet captures pass|libpcap capture passed|tcpdump capture passed/i.test(docs)) {
  failures.push("G133 docs overclaim external packet-capture evidence");
}

function run(label, args) {
  const result = spawnSync("cargo", args, { cwd: repoRoot, encoding: "utf8" });
  if (result.status !== 0) {
    failures.push(`${label} failed:\n${result.stdout}\n${result.stderr}`.trim());
  }
}

run("Provider-visible conformance capture", [
  "test",
  "-q",
  "-p",
  "discrypt-transport",
  "local_conformance_adapters_deliver_opaque_dm_payloads_without_plaintext_leaks",
  "--",
  "--nocapture",
]);
run("Provider plaintext rejection", [
  "test",
  "-q",
  "-p",
  "discrypt-transport",
  "local_conformance_adapter_rejects_plaintext_sdp_and_ice_markers",
  "--",
  "--nocapture",
]);

if (failures.length > 0) {
  console.error("G133 provider metadata capture check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G133 provider metadata capture check passed.");

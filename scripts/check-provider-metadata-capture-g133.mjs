#!/usr/bin/env node
import { existsSync, readFileSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const artifactRel = "target/provider-visible-captures/per88-g133-provider-visible-capture.json";
const artifactPath = resolve(repoRoot, artifactRel);
const docs = read("docs/security/g133-provider-visible-metadata-capture.md");
const status = read("docs/release/public-signaling-production-status.md");
const evidence = read("docs/release/per88-provider-visible-privacy-captures-2026-06-25.md");
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
  "# PER-88 Provider-Visible Privacy Captures Evidence - 2026-06-25",
  artifactRel,
  "repository-local deterministic provider-visible capture",
  "external host packet capture remains required",
]) requireText("per88-evidence", evidence, token);

for (const token of [
  "local_conformance_adapters_deliver_opaque_dm_payloads_without_plaintext_leaks",
  "local_conformance_adapter_rejects_plaintext_sdp_and_ice_markers",
  "SignalingAdapterKind::Mqtt",
  "SignalingAdapterKind::Nostr",
  "SignalingAdapterKind::IpfsPubsub",
  "SignalingAdapterKind::DiscryptQuicRendezvous",
  "assert_no_forbidden_plaintext",
  "assert_no_forbidden_text",
  "DISCRYPT_PROVIDER_METADATA_CAPTURE_OUT",
  "raw_payloads_retained: false",
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
  const env = { ...process.env };
  if (label === "Provider-visible conformance capture") {
    rmSync(artifactPath, { force: true });
    env.DISCRYPT_PROVIDER_METADATA_CAPTURE_OUT = artifactPath;
  }
  const result = spawnSync("cargo", args, { cwd: repoRoot, encoding: "utf8", env });
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

if (!existsSync(artifactPath)) {
  failures.push(`Provider-visible capture artifact was not written: ${artifactRel}`);
} else {
  const artifact = JSON.parse(read(artifactRel));
  const kinds = new Set(artifact.captures?.map((capture) => capture.adapter_kind));
  for (const kind of ["mqtt", "nostr", "ipfs_pubsub", "discrypt_quic_rendezvous"]) {
    if (!kinds.has(kind)) failures.push(`Provider-visible capture missing adapter: ${kind}`);
  }
  for (const capture of artifact.captures ?? []) {
    if (capture.entries <= 0) failures.push(`Capture for ${capture.adapter_kind} has no provider-visible rows`);
    if (capture.total_provider_visible_bytes <= 0) failures.push(`Capture for ${capture.adapter_kind} has no bytes`);
    if (capture.raw_payloads_retained !== false) failures.push(`Capture for ${capture.adapter_kind} retained raw payloads`);
    if (capture.forbidden_field_scan !== "passed") failures.push(`Capture for ${capture.adapter_kind} scan did not pass`);
  }
  for (const forbidden of [
    "Alice Display",
    "Bob Display",
    "v=0",
    "a=ice-ufrag",
    "a=ice-pwd",
    "candidate:",
    "turn credential",
    "plaintext message",
    "raw audio",
    "sframe key",
    "content key",
    "private key",
  ]) {
    if (JSON.stringify(artifact).toLowerCase().includes(forbidden.toLowerCase())) {
      failures.push(`Provider-visible capture artifact contains forbidden token: ${forbidden}`);
    }
  }
}

if (failures.length > 0) {
  console.error("G133 provider metadata capture check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`G133 provider metadata capture check passed. Artifact: ${artifactRel}`);

#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const docs = read("docs/security/g096-pcap-acceptance-suite.md");
const harness = read("harness/multinode/src/lib.rs");
const signalingPaths = read("external/signaling-repository/tests/process_webrtc_transport_paths.rs");
const signalingExchange = read("external/signaling-repository/tests/process_signal_exchange.rs");
const push = read("crates/push/src/lib.rs");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "# G096 pcap acceptance suite",
  "AC1 — identity/DM + verify",
  "AC8 — relays cannot decrypt",
  "AC15 — Android wake",
  "AC18 — signaling zero metadata at rest",
  "AC-METADATA",
  "Forbidden-byte sentinel classes",
  "does not claim external libpcap/tcpdump capture",
]) requireText("docs", docs, token);

for (const token of [
  "pub struct PcapAcceptanceMatrixSmoke",
  "pcap_acceptance_matrix_smoke",
  "ac1_identity_dm_safety_pcap_clean",
  "ac8_relay_media_ciphertext_only",
  "ac15_android_wake_content_free",
  "ac18_signaling_zero_linkage_at_rest",
  "ac_metadata_matrix_validated",
  "forbidden_scanner_covers_release_tokens",
  "pcap_acceptance_matrix_covers_ac1_ac8_ac15_ac18_and_metadata",
  "MetadataMatrix::approved_v1",
  "verify_safety_number",
  "no_directory_or_account_component",
]) requireText("harness", harness, token);

for (const token of [
  "two_process_webrtc_paths_pass_with_ciphertext_only_pcap_audit",
  "AuditFixture",
  "no_forbidden_content_egress",
  "matches_matrix",
  "PeerRelay",
  "Turn",
]) requireText("process_webrtc_transport_paths", signalingPaths, token);

for (const token of [
  "separate_process_clients_exchange_generated_offer_answer_and_candidate",
  "forbidden_tokens_scanned",
  "zero_linkage",
  "assert_no_forbidden_plaintext",
]) requireText("process_signal_exchange", signalingExchange, token);

for (const token of [
  "AndroidWakeService",
  "provider_visible_bytes",
  "android_wake_envelope_is_content_free",
  "contains_forbidden_token",
]) requireText("push", push, token);

if (/TODO|FIXME|unimplemented!|todo!/i.test(docs)) {
  failures.push("pcap suite docs contain unfinished-work marker");
}
if (/external host packet captures pass|libpcap capture passed|tcpdump capture passed/i.test(docs)) {
  failures.push("pcap suite docs overclaim external capture evidence");
}

const commands = [
  ["cargo", ["test", "-p", "discrypt-multinode-harness", "pcap_acceptance_matrix_covers_ac1_ac8_ac15_ac18_and_metadata", "--quiet"]],
  ["cargo", ["test", "-p", "external-signaling", "--test", "process_webrtc_transport_paths", "--quiet"]],
  ["cargo", ["test", "-p", "external-signaling", "--test", "process_signal_exchange", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-push", "android_wake_envelope_is_content_free", "--quiet"]],
];
for (const [cmd, args] of commands) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) {
    failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
  }
}

if (failures.length > 0) {
  console.error("G096 pcap suite check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G096 pcap suite check passed");

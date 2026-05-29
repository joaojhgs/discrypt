#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const docs = read("docs/security/g097-malicious-relay-adversary-suite.md");
const harness = read("harness/multinode/src/lib.rs");
const relayIntegrity = read("crates/relay-overlay/src/integrity.rs");
const relayRedelivery = read("crates/relay-overlay/src/redelivery.rs");
const relayManager = read("crates/relay-overlay/src/manager.rs");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "# G097 malicious relay adversary suite",
  "Passive read",
  "Tamper",
  "Replay",
  "Drop",
  "Reorder",
  "Endpoint churn",
  "does not replace the later full production E2E gate",
]) requireText("docs", docs, token);

for (const token of [
  "pub struct MaliciousRelayAdversarySmoke",
  "malicious_relay_adversary_smoke",
  "passive_read_blocked",
  "tamper_rejected",
  "replay_rejected",
  "drop_requests_bounded_redelivery",
  "reorder_window_enforced",
  "endpoint_churn_damped_and_failover_recovered",
  "malicious_relay_adversary_smoke_covers_passive_active_and_churn_cases",
]) requireText("harness", harness, token);

for (const token of ["RelayProtectedEnvelope", "visible_bytes", "contains_plaintext", "tamper"]) requireText("relay integrity", relayIntegrity, token);
for (const token of ["RedeliveryTracker", "request_redelivery", "FanoutExhausted", "Replay"]) requireText("relay redelivery", relayRedelivery, token);
for (const token of ["ChurnDampingPolicy", "TopologyChangeReason", "ChurnDamped", "HardFailure"]) requireText("relay manager", relayManager, token);

if (/TODO|FIXME|unimplemented!|todo!/i.test(docs)) failures.push("malicious relay docs contain unfinished-work marker");

const commands = [
  ["cargo", ["test", "-p", "discrypt-multinode-harness", "malicious_relay_adversary_smoke_covers_passive_active_and_churn_cases", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-relay-overlay", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-media", "sframe", "--quiet"]],
];
for (const [cmd, args] of commands) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G097 malicious relay check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G097 malicious relay check passed");

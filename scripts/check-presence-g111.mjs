#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const content = read("crates/content-keys/src/lib.rs");
const mlsCore = read("crates/mls-core/src/lib.rs");
const docs = read("docs/phase-4-retention-shred-recovery.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "LocalMembershipStateError",
  "from_local_mls_governance_state",
  "group.epoch != governance.epoch",
  "governance.role(*leaf).is_some()",
  "!governance.is_banned(*leaf)",
  "DeviceStatus::Active",
  "VerifyingKey::from_bytes(&member.device_key)",
  "live_key_oracle_builds_from_local_repaired_mls_governance_state",
  "local_membership_state_rejects_unrepaired_epoch_mismatch",
]) requireText("content-keys", content, token);

for (const token of ["GovernanceState", "GovernanceError"]) requireText("mls-core exports", mlsCore, token);
for (const token of [
  "repaired local MLS group state plus resolved governance state",
  "does not perform an online lookup",
  "unrepaired epoch mismatches are rejected",
]) requireText("docs", docs, token);

const localBuilder = content.slice(content.indexOf("from_local_mls_governance_state"), content.indexOf("/// Create an oracle from epoch membership."));
if (/https?:|wss?:|reqwest|hyper|axum|SignalingClient|publish|take/.test(localBuilder)) {
  failures.push("local membership builder contains network/signaling lookup token");
}

const commands = [
  ["cargo", ["test", "-p", "discrypt-content-keys", "local_membership", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-content-keys", "live_key_oracle_builds_from_local_repaired_mls_governance_state", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-content-keys", "local_membership_state_rejects_unrepaired_epoch_mismatch", "--quiet"]],
];
for (const [cmd, args] of commands) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G111 local membership state check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G111 local membership state check passed");

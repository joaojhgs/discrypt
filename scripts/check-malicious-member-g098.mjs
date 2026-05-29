#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const docs = read("docs/security/g098-malicious-member-adversary-suite.md");
const harness = read("harness/multinode/src/lib.rs");
const media = read("crates/media/src/sframe.rs");
const delivery = read("crates/mls-delivery/src/lib.rs");
const governance = read("crates/mls-core/src/governance.rs");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "# G098 malicious member/device adversary suite",
  "Media impersonation",
  "Evicted member text send",
  "Evicted device media send",
  "Forked MLS commit",
  "Out-of-epoch governance",
  "Unauthorized governance",
  "Removed admin race",
  "does not replace the later full multi-device release E2E gate",
]) requireText("docs", docs, token);

for (const token of [
  "pub struct MaliciousMemberAdversarySmoke",
  "malicious_member_adversary_smoke",
  "media_impersonation_rejected",
  "evicted_member_text_rejected",
  "evicted_device_media_rejected",
  "forked_mls_commit_rejected",
  "out_of_epoch_governance_rejected",
  "unauthorized_governance_rejected",
  "removed_admin_race_rejected",
  "malicious_member_adversary_smoke_covers_impersonation_eviction_divergence_and_admin_cases",
]) requireText("harness", harness, token);

for (const token of ["SenderBinding", "verify_derived_kid", "KidBindingMismatch", "UnknownSender"]) requireText("media", media, token);
for (const token of ["TextReceiveUnauthorizedSender", "DivergentTree", "DeliveryState", "CommitEnvelope"]) requireText("delivery", delivery, token);
for (const token of ["OutOfEpoch", "Unauthorized", "EvictedCommitter", "resolve_epoch_events"]) requireText("governance", governance, token);

if (/TODO|FIXME|unimplemented!|todo!/i.test(docs)) failures.push("malicious member docs contain unfinished-work marker");

const commands = [
  ["cargo", ["test", "-p", "discrypt-multinode-harness", "malicious_member_adversary_smoke_covers_impersonation_eviction_divergence_and_admin_cases", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-media", "sframe", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-mls-core", "governance", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-mls-delivery", "--quiet"]],
];
for (const [cmd, args] of commands) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G098 malicious member check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G098 malicious member check passed");

#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const content = read("crates/content-keys/src/lib.rs");
const harness = read("harness/multinode/src/lib.rs");
const docs = read("docs/phase-4-retention-shred-recovery.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "discrypt-live-key-membership-proof-v1",
  "MembershipProofError",
  "pub fn sign(",
  "verify_signature",
  "group_commitments_by_epoch",
  "authorized_device_keys",
  "authorize_member_device",
  "epoch_group_commitment",
  "proof.verify_signature(expected_commitment)",
  "live_key_oracle_requires_signed_epoch_membership_proof",
]) requireText("content-keys", content, token);

for (const token of [
  "SigningKey::from_bytes",
  "MembershipProof::sign",
  "authorize_member_device",
  "epoch_group_commitment",
]) requireText("harness", harness, token);

for (const token of [
  "signed Ed25519 device proof",
  "group-state commitment",
  "registered signer key",
  "rate-limits authorized signed members",
]) requireText("docs", docs, token);

if (content.includes("MembershipProof::new")) {
  failures.push("content-keys still exposes or tests unsigned MembershipProof::new");
}
if (/production membership proofs\s+must bind/i.test(docs)) {
  failures.push("docs still describe signed live-key membership proofs as future work");
}

const commands = [
  ["cargo", ["test", "-p", "discrypt-content-keys", "live_key_oracle", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-multinode-harness", "retention_shred", "--quiet"]],
];
for (const [cmd, args] of commands) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G110 signed live-key membership proof check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G110 signed live-key membership proof check passed");

#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const content = read("crates/content-keys/src/lib.rs");
const docs = read("docs/phase-4-retention-shred-recovery.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "LiveKeyRequestScope",
  "LiveKeyRateLimitKey",
  "author_leaf: Option<u32>",
  "network_identity_hash: Option<[u8; 32]>",
  "discrypt-live-key-rate-limit-network-identity-v1",
  "request_key_for_author",
  "request_key_scoped",
  "requests_by_rate_key",
  "live_key_oracle_rate_limits_by_requester_epoch_author_and_network",
]) requireText("content-keys", content, token);

for (const token of [
  "requester, epoch, author, and hashed network identity",
  "Raw network identity strings are not stored",
]) requireText("docs", docs, token);

if (content.includes("requests_by_leaf_epoch")) failures.push("old leaf/epoch-only rate map remains");

const commands = [
  ["cargo", ["test", "-p", "discrypt-content-keys", "live_key_oracle_rate_limits_by_requester_epoch_author_and_network", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-content-keys", "live_key_oracle_gates_membership_and_rate_limits_with_decoys", "--quiet"]],
];
for (const [cmd, args] of commands) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G112 live-key rate-limit scope check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G112 live-key rate-limit scope check passed");

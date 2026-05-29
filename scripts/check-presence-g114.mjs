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
  "malicious_non_member_live_key_probes_are_uniform_and_non_decryptable",
  "non_member",
  "unregistered_device",
  "stale_epoch",
  "invalid_signature",
  "generic_failure_response",
  "assert_ne!(response.state, KeyState::Cached(protected_key))",
]) requireText("content-keys", content, token);

for (const token of [
  "malicious non-member probes",
  "invalid proof, stale epoch, unregistered device, non-member, and generic reachability",
  "without returning a decryptable key",
]) requireText("docs", docs, token);

const run = spawnSync("cargo", ["test", "-p", "discrypt-content-keys", "malicious_non_member_live_key_probes_are_uniform_and_non_decryptable", "--quiet"], { cwd: repoRoot, encoding: "utf8" });
if (run.status !== 0) failures.push(`cargo test malicious_non_member failed:\n${run.stdout}\n${run.stderr}`);

if (failures.length > 0) {
  console.error("G114 malicious live-key probe check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G114 malicious live-key probe check passed");

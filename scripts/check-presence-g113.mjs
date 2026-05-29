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
  "LiveKeyFailureResponseMode",
  "UniformUnavailable",
  "generic_failure_response",
  "with_failure_response_mode",
  "failure_response(false)",
  "failure_response(true)",
  "KeyState::Unavailable",
  "live_key_oracle_can_shape_failures_as_uniform_unavailable",
]) requireText("content-keys", content, token);

for (const token of [
  "uniform unavailable mode",
  "non-members cannot distinguish authorization failure from generic reachability",
  "Decoy mode remains the default",
]) requireText("docs", docs, token);

const commands = [
  ["cargo", ["test", "-p", "discrypt-content-keys", "live_key_oracle_can_shape_failures_as_uniform_unavailable", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-content-keys", "live_key_oracle_gates_membership_and_rate_limits_with_decoys", "--quiet"]],
];
for (const [cmd, args] of commands) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G113 live-key uniform failure check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G113 live-key uniform failure check passed");

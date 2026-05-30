#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const harness = read("harness/multinode/src/lib.rs");
const docs = read("docs/phase-5-governance-admission-recovery-abuse.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "AbuseE2eSmoke",
  "abuse_e2e_smoke",
  "invite_flood_rate_limited",
  "spam_burst_rate_limited",
  "admission_helper_bruteforce_rejected",
  "signaling_blob_flood_rate_limited",
  "relay_freeloading_downranked",
  "service_request_size_exhaustion_rejected",
  "HTTP/1.1 429 Too Many Requests",
  "HTTP/1.1 413 Payload Too Large",
  "!flood_three.contains(\"opaque-payload\")",
  "abuse_e2e_smoke_covers_g119_gate",
]) requireText("multinode abuse E2E harness", harness, token);

for (const token of [
  "G119 abuse E2E",
  "invite flood",
  "spam burst",
  "online admission-helper brute force",
  "signaling opaque blob flood",
  "relay freeloading route downranking",
  "service-level request size exhaustion",
  "request_too_large",
  "do not echo opaque payload bytes",
]) requireText("phase 5 abuse docs", docs, token);

const checks = [
  ["cargo", ["test", "-p", "discrypt-multinode-harness", "abuse_e2e_smoke_covers_g119_gate", "--quiet"]],
  ["cargo", ["test", "--manifest-path", "../discrypt-signaling/Cargo.toml", "-p", "discrypt-signaling", "oversized_body_is_rejected_before_json_parsing", "--quiet"]],
  ["cargo", ["test", "--manifest-path", "../discrypt-signaling/Cargo.toml", "-p", "discrypt-signaling", "guarded_requests_reject_replay_and_rate_limit_with_structured_errors", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-desktop", "abuse_rate_limits_invite_consume_helper_and_text_send_commands", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-relay-overlay", "relay_contribution_accounting_penalizes_freeloaders_in_route_ranking", "--quiet"]],
];

for (const [cmd, args] of checks) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G119 abuse E2E gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G119 abuse E2E gate passed");

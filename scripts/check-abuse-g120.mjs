#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const abuse = read("crates/abuse/src/lib.rs");
const docs = read("docs/phase-5-governance-admission-recovery-abuse.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "AbuseMetricsSnapshot",
  "invite_allowed_total",
  "invite_rate_limited_total",
  "message_allowed_total",
  "message_rate_limited_total",
  "relay_peers_tracked",
  "relay_total_relayed_for_others",
  "relay_total_consumed_from_others",
  "relay_freeload_penalty_total",
  "metrics_snapshot",
  "content_free_and_safe_to_export",
  "without actor, peer, group",
  "metrics_snapshot_is_content_free_and_safe_to_export",
]) requireText("abuse metrics implementation", abuse, token);

for (const token of [
  "G120 operational metrics",
  "AbuseMetricsSnapshot",
  "aggregate",
  "without actor ids",
  "peer ids",
  "group ids",
  "invite secrets",
  "endpoint payloads",
  "key material",
  "must not grow labels",
]) requireText("phase 5 abuse docs", docs, token);

const checks = [
  ["cargo", ["test", "-p", "discrypt-abuse", "metrics_snapshot_is_content_free_and_safe_to_export", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-abuse", "rate_limits_invites_and_spam_and_scores_freeloading", "--quiet"]],
];

for (const [cmd, args] of checks) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G120 content-free abuse metrics gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G120 content-free abuse metrics gate passed");

#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const manager = read("crates/relay-overlay/src/manager.rs");
const relayLib = read("crates/relay-overlay/src/lib.rs");
const phase2 = read("docs/phase-2-relay-overlay-review.md");
const phase5 = read("docs/phase-5-governance-admission-recovery-abuse.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "use discrypt_abuse::RelayContribution",
  "to_ranking_metrics_with_contribution",
  "RelayContributionAccountingSnapshot",
  "record_relay_contribution",
  "relay_contribution_snapshots",
  "freeload_penalty",
  "relay_contribution_accounting_penalizes_freeloaders_in_route_ranking",
]) requireText("relay-overlay manager", manager, token);

requireText("relay-overlay lib", relayLib, "RelayContributionAccountingSnapshot");

for (const token of [
  "record_relay_contribution",
  "content-free",
  "RelayContributionAccountingSnapshot",
  "freeload_penalty",
  "route ranking",
]) requireText("phase 2 relay docs", phase2, token);

for (const token of [
  "record_relay_contribution",
  "content-free",
  "RelayContributionAccountingSnapshot",
  "RelayMetrics",
  "overlay route ranking",
]) requireText("phase 5 abuse docs", phase5, token);

const checks = [
  ["cargo", ["test", "-p", "discrypt-relay-overlay", "relay_contribution_accounting_penalizes_freeloaders_in_route_ranking", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-relay-overlay", "ranks_low_latency_stable_peer_and_penalizes_freeloading", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-abuse", "rate_limits_invites_and_spam_and_scores_freeloading", "--quiet"]],
];

for (const [cmd, args] of checks) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G117 relay freeload penalty gate check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G117 relay freeload penalty gate check passed");

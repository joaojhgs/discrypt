#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const ui = read("apps/ui/src/main.tsx");
const commands = read("apps/ui/src/commands.ts");
const core = read("crates/core/src/lib.rs");
const desktop = read("apps/desktop/src-tauri/src/lib.rs");
const docs = read("docs/phase-4-retention-shred-recovery.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const [name, text] of [
  ["commands", commands],
  ["core", core],
  ["docs", docs],
]) {
  requireText(name, text, "Authorized members can still infer some liveness from archival live-key behavior");
  requireText(name, text, "not metadata anonymity");
}

for (const token of [
  "Residual presence risk",
  "snapshot.security_copy.malicious_member",
]) requireText("ui", ui, token);

for (const token of [
  "fallbackState.security_copy.malicious_member.includes(\"not metadata anonymity\")",
]) requireText("commands", commands, token);

for (const token of [
  "malicious_member",
  "contains(\"not metadata anonymity\")",
]) requireText("desktop", desktop, token);

for (const token of [
  "contains(\"not metadata anonymity\")",
  "contains(\"infer some liveness\")",
]) requireText("core", core, token);

const checks = [
  ["cargo", ["test", "-p", "discrypt-core", "command_snapshot_covers_required_ui_flows_and_copy", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-desktop", "command_health_covers_full_user_flow", "--quiet"]],
  ["npm", ["--prefix", "apps/ui", "run", "typecheck"]],
];
for (const [cmd, args] of checks) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G115 residual presence-risk UX copy check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G115 residual presence-risk UX copy check passed");

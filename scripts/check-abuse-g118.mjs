#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const core = read("crates/core/src/lib.rs");
const desktop = read("apps/desktop/src-tauri/src/lib.rs");
const uiCommands = read("apps/ui/src/commands.ts");
const ui = read("apps/ui/src/main.tsx");
const docs = read("docs/phase-5-governance-admission-recovery-abuse.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "sybil_resistance",
  "do not solve Sybil attacks without a central identity",
  "invite creation",
  "admission-helper attempts",
  "signaling publish/take",
  "text bursts",
  "relay freeloading",
]) requireText("core security copy", core, token);

for (const token of [
  "sybil_resistance",
  "do not solve Sybil attacks without a central identity",
]) {
  requireText("desktop honest copy gate", desktop, token);
  requireText("UI fallback commands", uiCommands, token);
}

for (const token of [
  "Sybil-resistance posture",
  "snapshot.security_copy.sybil_resistance",
]) requireText("UI surface", ui, token);

for (const token of [
  "invite creation",
  "invite consumption",
  "admission-helper attempts",
  "signaling publish/take requests",
  "text sends",
  "live-key probing",
  "relay freeloading",
  "Sybil resistance is intentionally not claimed",
  "Without a central identity",
  "not a cryptographic guarantee",
]) requireText("phase 5 abuse docs", docs, token);

const checks = [
  ["cargo", ["test", "-p", "discrypt-core", "app_snapshot_has_expected_command_contract", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-desktop", "command_health_reports_real_state_and_identity_checks", "--quiet"]],
  ["npm", ["--prefix", "apps/ui", "run", "test:honesty"]],
];

for (const [cmd, args] of checks) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G118 Sybil posture documentation gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G118 Sybil posture documentation gate passed");

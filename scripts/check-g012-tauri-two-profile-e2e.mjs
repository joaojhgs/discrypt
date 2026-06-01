#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const harness = read("scripts/g012-tauri-two-profile-e2e.mjs");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}
function requireScript(name) {
  if (!packageJson.scripts?.[name]) failures.push(`package.json missing script: ${name}`);
}

for (const script of ["g012:tauri-two-profile-e2e", "test:g012-tauri-two-profile-e2e"]) requireScript(script);
for (const token of [
  "discrypt.g012.tauri_two_profile_e2e_harness.v1",
  "target/g012-e2e",
  "DISCRYPT_APP_STATE_PATH",
  "tauri-runtime,local-dev",
  "cargo",
  "tauri",
  "dev",
  "shared vite dev server",
  "profile_state_files",
  "screenshot_capability",
  "G012 is not complete until two launched Tauri profiles complete text plus voice UX proof",
]) requireText("G012 harness", harness, token);

const dryRun = spawnSync(process.execPath, ["scripts/g012-tauri-two-profile-e2e.mjs"], {
  cwd: repoRoot,
  encoding: "utf8",
  env: { ...process.env, DISCRYPT_G012_RUN_ID: "contract-dry-run" },
  maxBuffer: 1024 * 1024 * 8,
});
if (![0, 3].includes(dryRun.status ?? 1)) {
  failures.push(`G012 harness dry-run exited unexpectedly with ${dryRun.status}:\n${dryRun.stdout}\n${dryRun.stderr}`.trim());
}
const manifestPath = resolve(repoRoot, "target/g012-e2e/contract-dry-run/tauri-two-profile-launch-manifest.json");
try {
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (manifest.schema_version !== "discrypt.g012.tauri_two_profile_e2e_harness.v1") failures.push("dry-run schema mismatch");
  if (manifest.mode !== "dry-run") failures.push("dry-run mode mismatch");
  if (!manifest.artifact_root.includes("target/g012-e2e/contract-dry-run")) failures.push("dry-run artifact root must be under target/g012-e2e");
  if (!manifest.profiles?.alice?.state_path?.includes("alice") || !manifest.profiles?.bob?.state_path?.includes("bob")) failures.push("dry-run missing isolated Alice/Bob state paths");
  if (!manifest.planned_commands?.some((entry) => entry.label === "tauri alice" && entry.rendered.includes("cargo tauri dev"))) failures.push("dry-run missing planned tauri alice command");
  if (!manifest.planned_commands?.some((entry) => entry.label === "tauri bob" && entry.env?.DISCRYPT_APP_STATE_PATH?.includes("bob"))) failures.push("dry-run missing planned tauri bob isolated env");
} catch (error) {
  failures.push(`could not read dry-run manifest ${manifestPath}: ${error instanceof Error ? error.message : String(error)}`);
}

if (failures.length > 0) {
  console.error("G012 Tauri two-profile E2E harness check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G012 Tauri two-profile E2E harness check passed");

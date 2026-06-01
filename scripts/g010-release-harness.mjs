#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = new Set(process.argv.slice(2));
const modeArg = process.argv.find((arg) => arg.startsWith("--mode="));
const mode = modeArg ? modeArg.split("=", 2)[1] : "local";
const allowedModes = new Set(["local", "public", "all"]);
if (!allowedModes.has(mode)) {
  console.error(`Unsupported --mode=${mode}. Use local, public, or all.`);
  process.exit(2);
}
const runId =
  process.env.DISCRYPT_G010_RUN_ID ||
  new Date().toISOString().replace(/[:.]/g, "-");
const artifactRoot = resolve(
  repoRoot,
  process.env.DISCRYPT_G010_ARTIFACT_DIR || `target/g010-release-harness/${runId}`,
);
const logDir = resolve(artifactRoot, "logs");
const profileDir = resolve(artifactRoot, "profiles");
const playwrightDir = resolve(artifactRoot, "playwright");
const screenshotDir = resolve(artifactRoot, "screenshots");
for (const dir of [artifactRoot, logDir, profileDir, playwrightDir, screenshotDir]) {
  mkdirSync(dir, { recursive: true });
}

const manifest = {
  schema_version: "discrypt.g010.release_harness.v1",
  generated_at: new Date().toISOString(),
  mode,
  run_id: runId,
  artifact_root: artifactRoot,
  profiles: {
    alice_state_path: resolve(profileDir, "alice/app-state.discrypt-store"),
    bob_state_path: resolve(profileDir, "bob/app-state.discrypt-store"),
  },
  commands: [],
  skips: [],
};
for (const dir of [dirname(manifest.profiles.alice_state_path), dirname(manifest.profiles.bob_state_path)]) {
  mkdirSync(dir, { recursive: true });
}

function logName(label) {
  return `${String(manifest.commands.length + 1).padStart(2, "0")}-${label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")}.log`;
}

function runStep(label, command, commandArgs, options = {}) {
  const startedAt = new Date().toISOString();
  const logPath = resolve(logDir, logName(label));
  const result = spawnSync(command, commandArgs, {
    cwd: options.cwd || repoRoot,
    encoding: "utf8",
    env: { ...process.env, ...(options.env || {}) },
    maxBuffer: 1024 * 1024 * 64,
  });
  const endedAt = new Date().toISOString();
  const stdout = result.stdout || "";
  const stderr = result.stderr || "";
  writeFileSync(
    logPath,
    [
      `$ ${command} ${commandArgs.join(" ")}`,
      `cwd=${options.cwd || repoRoot}`,
      `started_at=${startedAt}`,
      `ended_at=${endedAt}`,
      `status=${result.status ?? "signal:" + result.signal}`,
      "--- stdout ---",
      stdout,
      "--- stderr ---",
      stderr,
    ].join("\n"),
  );
  const entry = {
    label,
    command: [command, ...commandArgs].join(" "),
    cwd: options.cwd || repoRoot,
    started_at: startedAt,
    ended_at: endedAt,
    status: result.status,
    signal: result.signal,
    log_path: logPath,
  };
  manifest.commands.push(entry);
  if (result.status !== 0) {
    writeManifest("failed");
    console.error(`G010 harness step failed: ${label}. See ${logPath}`);
    process.exit(result.status ?? 1);
  }
  console.log(`PASS ${label} -> ${logPath}`);
}

function skip(label, reason) {
  const entry = { label, reason };
  manifest.skips.push(entry);
  console.log(`SKIP ${label}: ${reason}`);
}

function writeManifest(status = "running") {
  manifest.status = status;
  manifest.updated_at = new Date().toISOString();
  writeFileSync(
    resolve(artifactRoot, "manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
}

function shouldRunLocal() {
  return mode === "local" || mode === "all";
}

function shouldRunPublic() {
  return mode === "public" || mode === "all";
}

writeManifest();

if (shouldRunLocal()) {
  runStep("ui typecheck", "npm", ["--prefix", "apps/ui", "run", "typecheck"]);
  runStep("ui local fallback build", "npm", ["--prefix", "apps/ui", "run", "build"], {
    env: { VITE_DISCRYPT_LOCAL_DEV_FALLBACK: "1" },
  });
  runStep("tauri two profile launch dry-run", "node", [
    "scripts/g010-tauri-two-profile-launch.mjs",
  ], {
    env: {
      DISCRYPT_G010_ARTIFACT_DIR: artifactRoot,
      DISCRYPT_G010_RUN_ID: runId,
    },
  });
  runStep(
    "two profile playwright flows",
    "npx",
    [
      "playwright",
      "test",
      "tests/e2e/two-profile-flow.spec.ts",
      "tests/e2e/voice-media-session.spec.ts",
      "--project=chromium",
      "--workers=1",
      "--reporter=line",
      `--output=${playwrightDir}`,
    ],
    {
      cwd: resolve(repoRoot, "apps/ui"),
      env: { VITE_DISCRYPT_LOCAL_DEV_FALLBACK: "1", CI: "1" },
    },
  );
  runStep("desktop isolated profiles", "cargo", [
    "test",
    "-q",
    "-p",
    "discrypt-desktop",
    "test_harness_can_run_two_isolated_app_profiles",
    "--",
    "--nocapture",
  ]);
  runStep("desktop setup invite text voice restart matrix", "cargo", [
    "test",
    "-q",
    "-p",
    "discrypt-desktop",
    "g004_two_profile_state_survives_reload_with_invites_receipts_voice_and_preferences",
    "--",
    "--nocapture",
  ]);
  runStep("desktop text receipt roundtrip", "cargo", [
    "test",
    "-q",
    "-p",
    "discrypt-desktop",
    "text_control_frame_roundtrip_persists_across_two_profile_state_files",
    "--",
    "--nocapture",
  ]);
  runStep("core voice restart state", "cargo", [
    "test",
    "-q",
    "-p",
    "discrypt-core",
    "voice_session_state_persists_across_restart",
    "--",
    "--nocapture",
  ]);
  runStep("local adapter matrix", "npm", [
    "--prefix",
    "apps/ui",
    "run",
    "test:signaling-e2e-matrix-g132",
  ]);
  runStep("g009 security privacy gate", "npm", [
    "--prefix",
    "apps/ui",
    "run",
    "test:security-privacy-g009",
  ]);
}

if (shouldRunPublic()) {
  runStep("public adapter matrix", "npm", [
    "--prefix",
    "apps/ui",
    "run",
    "test:signaling-e2e-matrix-g132",
  ]);
  runStep("turn fallback matrix", "npm", [
    "--prefix",
    "apps/ui",
    "run",
    "test:g008-stun-turn-fallback",
  ]);
}

if (args.has("--tauri-launch-smoke") || process.env.DISCRYPT_G010_TAURI_LAUNCH_SMOKE === "1") {
  skip(
    "tauri two-window launch smoke",
    "not run by this deterministic command: launch automation requires a graphical display and tauri-driver/WebDriver in the runner; profile isolation paths are recorded for the launch wrapper contract",
  );
} else {
  skip(
    "tauri two-window launch smoke",
    "set DISCRYPT_G010_TAURI_LAUNCH_SMOKE=1 or pass --tauri-launch-smoke on a GUI/WebDriver runner; local command-layer and Playwright profile coverage still ran",
  );
}

writeManifest("passed");
console.log(`G010 release harness passed. Artifact manifest: ${resolve(artifactRoot, "manifest.json")}`);

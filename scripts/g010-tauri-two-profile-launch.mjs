#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync, appendFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { once } from "node:events";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const argv = process.argv.slice(2);
const run = argv.includes("--run");
const appModeArg = argv.find((arg) => arg.startsWith("--app-mode="));
const appMode = appModeArg ? appModeArg.split("=", 2)[1] : "dev";
if (!["dev", "build"].includes(appMode)) {
  console.error("Unsupported --app-mode. Use dev or build.");
  process.exit(2);
}
const runId =
  process.env.DISCRYPT_G010_RUN_ID ||
  `tauri-launch-${new Date().toISOString().replace(/[:.]/g, "-")}`;
const devServerPort = Number(process.env.DISCRYPT_G010_DEV_SERVER_PORT || 1420);
if (!Number.isInteger(devServerPort) || devServerPort <= 0 || devServerPort > 65_535) {
  console.error("DISCRYPT_G010_DEV_SERVER_PORT must be an integer TCP port.");
  process.exit(2);
}
const devServerUrl = `http://127.0.0.1:${devServerPort}`;
const tauriFeatures = (process.env.DISCRYPT_G010_TAURI_FEATURES || "tauri-runtime,local-dev,production-media,mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter")
  .split(",")
  .map((feature) => feature.trim())
  .filter(Boolean);
if (!tauriFeatures.includes("tauri-runtime")) {
  console.error("DISCRYPT_G010_TAURI_FEATURES must include tauri-runtime for GUI launch.");
  process.exit(2);
}
if (!tauriFeatures.some((feature) => ["local-dev", "harness"].includes(feature))) {
  console.error(
    "DISCRYPT_G010_TAURI_FEATURES must include local-dev or harness so DISCRYPT_APP_STATE_PATH profile isolation is honored; this script is not a production release claim.",
  );
  process.exit(2);
}
if (tauriFeatures.includes("production-storage")) {
  console.error(
    "DISCRYPT_G010_TAURI_FEATURES must not include production-storage because production-storage builds do not honor DISCRYPT_APP_STATE_PATH profile isolation.",
  );
  process.exit(2);
}
const tauriFeatureArg = tauriFeatures.join(",");
const artifactRoot = resolve(
  repoRoot,
  process.env.DISCRYPT_G010_ARTIFACT_DIR || `target/g010-release-harness/${runId}`,
);
const logDir = resolve(artifactRoot, "logs");
const profileDir = resolve(artifactRoot, "profiles");
const screenshotDir = resolve(artifactRoot, "screenshots");
for (const dir of [artifactRoot, logDir, profileDir, screenshotDir]) mkdirSync(dir, { recursive: true });

const profiles = {
  alice: {
    name: "alice",
    state_path: resolve(profileDir, "alice/app-state.discrypt-store"),
    log_path: resolve(logDir, "tauri-alice.log"),
  },
  bob: {
    name: "bob",
    state_path: resolve(profileDir, "bob/app-state.discrypt-store"),
    log_path: resolve(logDir, "tauri-bob.log"),
  },
};
for (const profile of Object.values(profiles)) mkdirSync(dirname(profile.state_path), { recursive: true });

const manifest = {
  schema_version: "discrypt.g010.tauri_two_profile_launch.v1",
  generated_at: new Date().toISOString(),
  mode: run ? "run" : "dry-run",
  app_mode: appMode,
  run_id: runId,
  artifact_root: artifactRoot,
  tauri_features: tauriFeatures,
  profile_isolation_env: "DISCRYPT_APP_STATE_PATH",
  production_claim: "none; this wrapper is a harness/local-dev launch aid, not release packaging evidence",
  manual_pairing_required: false,
  profiles,
  shared_frontend: {
    url: devServerUrl,
    log_path: resolve(logDir, "vite-shared.log"),
  },
  commands: [],
  screenshots_dir: screenshotDir,
  notes: [
    "Each Tauri instance receives a distinct DISCRYPT_APP_STATE_PATH.",
    "The shared Vite server avoids racing two beforeDevCommand processes on the same port.",
    "This launch smoke captures process logs and profile paths only; WebDriver-driven UI assertions remain a separate runner concern.",
    "The launch features include local-dev or harness so DISCRYPT_APP_STATE_PATH is honored; do not cite this as a production build claim.",
  ],
};

function writeManifest(status) {
  manifest.status = status;
  manifest.updated_at = new Date().toISOString();
  writeFileSync(resolve(artifactRoot, "tauri-launch-manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
}

function tauriCommandFor(profile) {
  const env = { DISCRYPT_APP_STATE_PATH: profile.state_path };
  if (appMode === "dev") {
    return {
      command: "cargo",
      args: [
        "tauri",
        "dev",
        "--no-watch",
        "--no-dev-server-wait",
        "--features",
        tauriFeatureArg,
        "--config",
        JSON.stringify({ build: { beforeDevCommand: "", devUrl: manifest.shared_frontend.url } }),
      ],
      cwd: resolve(repoRoot, "apps/desktop/src-tauri"),
      env,
    };
  }
  return {
    command: process.env.DISCRYPT_G010_TAURI_BUILD_BINARY || resolve(repoRoot, "target/debug/discrypt-desktop"),
    args: [],
    cwd: repoRoot,
    env,
    preflight: {
      command: "cargo",
      args: ["build", "-p", "discrypt-desktop", "--features", tauriFeatureArg],
      cwd: repoRoot,
    },
  };
}

for (const profile of Object.values(profiles)) {
  const command = tauriCommandFor(profile);
  manifest.commands.push({ profile: profile.name, ...command, env: command.env });
}

writeManifest(run ? "running" : "dry-run");
if (!run) {
  console.log(`G010 Tauri two-profile launch dry-run manifest: ${resolve(artifactRoot, "tauri-launch-manifest.json")}`);
  process.exit(0);
}

const hasLinuxDisplay = Boolean(process.env.DISPLAY || process.env.WAYLAND_DISPLAY);
if (process.platform === "linux" && !hasLinuxDisplay) {
  writeManifest("skipped-no-display");
  console.error("No DISPLAY/WAYLAND_DISPLAY found; run under xvfb-run/dbus-run-session or a desktop session.");
  process.exit(3);
}

function spawnLogged(label, command, args, options) {
  appendFileSync(
    options.logPath,
    `$ ${command} ${args.join(" ")}\ncwd=${options.cwd}\nprofile_state_path=${options.env?.DISCRYPT_APP_STATE_PATH || "shared"}\nstarted_at=${new Date().toISOString()}\n`,
  );
  const child = spawn(command, args, {
    cwd: options.cwd,
    env: { ...process.env, ...options.env },
    stdio: ["ignore", "pipe", "pipe"],
    detached: process.platform !== "win32",
  });
  child.stdout.on("data", (chunk) => appendFileSync(options.logPath, chunk));
  child.stderr.on("data", (chunk) => appendFileSync(options.logPath, chunk));
  const entry = { label, pid: child.pid, command, args, cwd: options.cwd, log_path: options.logPath };
  manifest.commands.push(entry);
  child.on("exit", (code, signal) => {
    const exitedAt = new Date().toISOString();
    entry.exit_code = code;
    entry.exit_signal = signal;
    entry.exited_at = exitedAt;
    appendFileSync(options.logPath, `\nexited_at=${exitedAt} code=${code} signal=${signal}\n`);
  });
  return child;
}

function syncChildExitEntry(child) {
  const entry = manifest.commands.find((command) => command.pid === child.pid);
  if (!entry) return;
  entry.exit_code = child.exitCode;
  entry.exit_signal = child.signalCode;
  if (!entry.exited_at && !childIsRunning(child)) entry.exited_at = new Date().toISOString();
}

function childIsRunning(child) {
  return child.exitCode === null && child.signalCode === null;
}

async function sleep(ms) {
  await new Promise((resolveWait) => setTimeout(resolveWait, ms));
}

async function waitForExit(child, timeoutMs) {
  if (!childIsRunning(child)) return true;
  const timeout = sleep(timeoutMs).then(() => false);
  return Promise.race([once(child, "exit").then(() => true), timeout]);
}

async function stopChildren(children) {
  for (const child of children.slice().reverse()) {
    if (childIsRunning(child)) terminateChild(child, "SIGTERM");
  }
  await Promise.all(children.map((child) => waitForExit(child, 5_000)));
  for (const child of children.slice().reverse()) {
    if (childIsRunning(child)) terminateChild(child, "SIGKILL");
  }
  await Promise.all(children.map((child) => waitForExit(child, 2_000)));
  for (const child of children) syncChildExitEntry(child);
}

function terminateChild(child, signal) {
  if (!childIsRunning(child)) return;
  try {
    if (process.platform !== "win32") {
      process.kill(-child.pid, signal);
    } else {
      child.kill(signal);
    }
  } catch {
    child.kill(signal);
  }
}

if (appMode === "build") {
  const preflight = tauriCommandFor(profiles.alice).preflight;
  const result = spawnSync(preflight.command, preflight.args, {
    cwd: preflight.cwd,
    encoding: "utf8",
    env: process.env,
    maxBuffer: 1024 * 1024 * 64,
  });
  const preflightLog = resolve(logDir, "tauri-build-preflight.log");
  writeFileSync(preflightLog, `${result.stdout || ""}\n${result.stderr || ""}`);
  manifest.commands.push({ label: "build preflight", ...preflight, status: result.status, log_path: preflightLog });
  if (result.status !== 0) {
    writeManifest("failed");
    console.error(`Tauri build preflight failed: ${preflightLog}`);
    process.exit(result.status ?? 1);
  }
}

const vite = spawnLogged(
  "shared vite dev server",
  "npm",
  ["run", "dev", "--", "--host", "127.0.0.1", "--port", String(devServerPort), "--strictPort"],
  { cwd: resolve(repoRoot, "apps/ui"), logPath: manifest.shared_frontend.log_path, env: {} },
);

await sleep(Number(process.env.DISCRYPT_G010_VITE_WAIT_MS || 5_000));
const children = [vite];
for (const profile of Object.values(profiles)) {
  const command = tauriCommandFor(profile);
  children.push(
    spawnLogged(`tauri ${profile.name}`, command.command, command.args, {
      cwd: command.cwd,
      env: command.env,
      logPath: profile.log_path,
    }),
  );
}

const durationMs = Number(process.env.DISCRYPT_G010_LAUNCH_DURATION_MS || 20_000);
await sleep(durationMs);
const earlyExit = children.find((child) => !childIsRunning(child));
await stopChildren(children);
if (earlyExit) {
  writeManifest("failed");
  console.error(
    `G010 Tauri two-profile launch failed: ${earlyExit.spawnargs?.join(" ") || earlyExit.pid} exited before the ${durationMs}ms runtime window elapsed with code=${earlyExit.exitCode} signal=${earlyExit.signalCode}`,
  );
  process.exit(earlyExit.exitCode ?? 1);
}
writeManifest("passed");
console.log(`G010 Tauri two-profile launch artifact: ${resolve(artifactRoot, "tauri-launch-manifest.json")}`);

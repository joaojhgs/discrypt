#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync, appendFileSync } from "node:fs";
import http from "node:http";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const argv = process.argv.slice(2);
const run = argv.includes("--run");
const noVite = argv.includes("--no-vite");
const runId = valueAfter("--run-id") ?? process.env.DISCRYPT_G012_RUN_ID ?? `g012-tauri-${new Date().toISOString().replace(/[:.]/g, "-")}`;
const appMode = valueAfter("--app-mode") ?? valueFromPrefix("--app-mode=") ?? "dev";
if (!["dev", "build"].includes(appMode)) failCli("Unsupported --app-mode. Use dev or build.", 2);

const artifactRoot = resolve(repoRoot, valueAfter("--artifact-dir") ?? process.env.DISCRYPT_G012_ARTIFACT_DIR ?? `target/g012-e2e/${runId}`);
const logDir = resolve(artifactRoot, "logs");
const profileDir = resolve(artifactRoot, "profiles");
const screenshotDir = resolve(artifactRoot, "screenshots");
for (const dir of [artifactRoot, logDir, profileDir, screenshotDir]) mkdirSync(dir, { recursive: true });

const durationMs = Number(valueAfter("--duration-ms") ?? process.env.DISCRYPT_G012_LAUNCH_DURATION_MS ?? 20_000);
const launchReadyTimeoutMs = Number(valueAfter("--launch-ready-timeout-ms") ?? process.env.DISCRYPT_G012_LAUNCH_READY_TIMEOUT_MS ?? 120_000);
const skipBuildPreflight = argv.includes("--skip-build-preflight") || process.env.DISCRYPT_G012_SKIP_BUILD_PREFLIGHT === "1";
const viteUrl = process.env.DISCRYPT_G012_VITE_URL ?? "http://127.0.0.1:1420";
const tauriFeatures = (process.env.DISCRYPT_G012_TAURI_FEATURES || "tauri-runtime,local-dev")
  .split(",")
  .map((feature) => feature.trim())
  .filter(Boolean);
if (!tauriFeatures.includes("tauri-runtime")) failCli("DISCRYPT_G012_TAURI_FEATURES must include tauri-runtime for real Tauri launch.", 2);
if (!tauriFeatures.some((feature) => ["local-dev", "harness"].includes(feature))) {
  failCli("DISCRYPT_G012_TAURI_FEATURES must include local-dev or harness so DISCRYPT_APP_STATE_PATH isolates the two real profiles.", 2);
}
const tauriFeatureArg = tauriFeatures.join(",");

const profiles = Object.fromEntries(
  ["alice", "bob"].map((name) => [
    name,
    {
      name,
      state_path: resolve(profileDir, name, "app-state.discrypt-store"),
      log_path: resolve(logDir, `tauri-${name}.log`),
      profile_env: `DISCRYPT_G012_PROFILE=${name}`,
    },
  ]),
);
for (const profile of Object.values(profiles)) mkdirSync(dirname(profile.state_path), { recursive: true });

const manifestPath = resolve(artifactRoot, "tauri-two-profile-launch-manifest.json");
const summaryPath = resolve(artifactRoot, "launch-summary.json");
const manifest = {
  schema_version: "discrypt.g012.tauri_two_profile_e2e_harness.v1",
  generated_at: new Date().toISOString(),
  mode: run ? "run" : "dry-run",
  app_mode: appMode,
  run_id: runId,
  artifact_root: rel(artifactRoot),
  profile_root: rel(profileDir),
  log_dir: rel(logDir),
  screenshots_dir: rel(screenshotDir),
  tauri_features: tauriFeatures,
  profile_isolation_env: "DISCRYPT_APP_STATE_PATH",
  g012_boundary: "Launch harness only: G012 is not complete until two launched Tauri profiles complete text plus voice UX proof and artifacts cite this run.",
  production_ux_constraint: "The harness launches real Tauri WebViews with backend IPC; local-dev/harness features are used only to permit per-profile state-path isolation.",
  profiles,
  shared_frontend: noVite
    ? { url: viteUrl, managed_by_harness: false, log_path: null }
    : { url: viteUrl, managed_by_harness: true, log_path: resolve(logDir, "vite-shared.log") },
  planned_commands: [],
  runtime_processes: [],
  preflight: {},
  artifact_policy: {
    required_root: "target/g012-e2e",
    logs: true,
    isolated_profile_state_files: true,
    screenshots: "captured only when a screenshot utility is available; otherwise screenshot_capability records the missing local tool",
    no_secret_env_dump: true,
  },
};

function valueAfter(flag) {
  const index = argv.indexOf(flag);
  return index >= 0 ? argv[index + 1] : undefined;
}

function valueFromPrefix(prefix) {
  return argv.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
}

function failCli(message, code = 1) {
  console.error(`g012-tauri-two-profile-e2e: ${message}`);
  process.exit(code);
}

function rel(path) {
  return path && path.startsWith(repoRoot) ? path.slice(repoRoot.length + 1) : path;
}

function render(command) {
  return [command.command, ...command.args].join(" ");
}

function stripAnsi(text) {
  return text.replace(/\x1b\[[0-9;]*m/g, "").replace(/\r/g, "");
}

function sha256IfExists(path) {
  return existsSync(path) ? createHash("sha256").update(readFileSync(path)).digest("hex") : null;
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function writeManifest(status, details = {}) {
  manifest.status = status;
  manifest.updated_at = new Date().toISOString();
  Object.assign(manifest, details);
  writeJson(manifestPath, manifest);
}

function commandExists(command) {
  const result = spawnSync("sh", ["-lc", `command -v ${JSON.stringify(command)} >/dev/null 2>&1`], { encoding: "utf8" });
  return result.status === 0;
}

function tauriCommandFor(profile) {
  const env = {
    DISCRYPT_APP_STATE_PATH: profile.state_path,
    DISCRYPT_G012_PROFILE: profile.name,
    WEBKIT_DISABLE_COMPOSITING_MODE: "1",
  };
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
        JSON.stringify({ build: { beforeDevCommand: "", devUrl: viteUrl } }),
      ],
      cwd: resolve(repoRoot, "apps/desktop/src-tauri"),
      env,
    };
  }
  return {
    command: process.env.DISCRYPT_G012_TAURI_BUILD_BINARY || resolve(repoRoot, "target/debug/discrypt-desktop"),
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

function planCommands() {
  manifest.planned_commands = [];
  if (!noVite) {
    manifest.planned_commands.push({
      label: "shared vite dev server",
      command: "npm",
      args: ["run", "dev", "--", "--host", "127.0.0.1", "--port", "1420", "--strictPort"],
      cwd: resolve(repoRoot, "apps/ui"),
      log_path: manifest.shared_frontend.log_path,
      env_keys: [],
      rendered: "npm run dev -- --host 127.0.0.1 --port 1420 --strictPort",
    });
  }
  for (const profile of Object.values(profiles)) {
    const command = tauriCommandFor(profile);
    manifest.planned_commands.push({
      label: `tauri ${profile.name}`,
      command: command.command,
      args: command.args,
      cwd: command.cwd,
      log_path: profile.log_path,
      env: { DISCRYPT_APP_STATE_PATH: profile.state_path, DISCRYPT_G012_PROFILE: profile.name, WEBKIT_DISABLE_COMPOSITING_MODE: "1" },
      rendered: render(command),
    });
  }
}

function preflightChecks() {
  const hasDisplay = process.platform !== "linux" || Boolean(process.env.DISPLAY || process.env.WAYLAND_DISPLAY);
  const checks = {
    platform: process.platform,
    display: { DISPLAY: process.env.DISPLAY || null, WAYLAND_DISPLAY: process.env.WAYLAND_DISPLAY || null, ok: hasDisplay },
    cargo_tauri_cli: commandExists("cargo") ? spawnSync("cargo", ["tauri", "--version"], { cwd: resolve(repoRoot, "apps/desktop/src-tauri"), encoding: "utf8" }) : { status: 127, stdout: "", stderr: "cargo missing" },
    node_modules_present: existsSync(resolve(repoRoot, "apps/ui/node_modules")),
    screenshot_capability: ["gnome-screenshot", "import", "scrot", "xwd"].find(commandExists) ?? null,
    build_preflight_enabled: !skipBuildPreflight,
    launch_ready_timeout_ms: launchReadyTimeoutMs,
  };
  checks.cargo_tauri_cli = {
    status: checks.cargo_tauri_cli.status,
    stdout: String(checks.cargo_tauri_cli.stdout || "").trim(),
    stderr: String(checks.cargo_tauri_cli.stderr || "").trim(),
  };
  manifest.preflight = checks;
  if (!hasDisplay) return { ok: false, reason: "No DISPLAY/WAYLAND_DISPLAY found; run under a desktop session or xvfb/dbus session." };
  if (checks.cargo_tauri_cli.status !== 0) return { ok: false, reason: `cargo tauri CLI unavailable: ${checks.cargo_tauri_cli.stderr || checks.cargo_tauri_cli.stdout}` };
  if (!noVite && !checks.node_modules_present) return { ok: false, reason: "apps/ui/node_modules missing; run npm --prefix apps/ui install before live launch." };
  return { ok: true };
}

function spawnLogged(label, command, args, options) {
  appendFileSync(
    options.logPath,
    `$ ${command} ${args.join(" ")}\ncwd=${options.cwd}\nstarted_at=${new Date().toISOString()}\nprofile_state_path=${options.env?.DISCRYPT_APP_STATE_PATH || "shared"}\n`,
  );
  const child = spawn(command, args, {
    cwd: options.cwd,
    env: { ...process.env, ...options.env },
    stdio: ["ignore", "pipe", "pipe"],
    detached: process.platform !== "win32",
  });
  child.stdout.on("data", (chunk) => appendFileSync(options.logPath, chunk));
  child.stderr.on("data", (chunk) => appendFileSync(options.logPath, chunk));
  child.on("exit", (code, signal) => appendFileSync(options.logPath, `\nexited_at=${new Date().toISOString()} code=${code} signal=${signal}\n`));
  manifest.runtime_processes.push({ label, pid: child.pid, command, args, cwd: options.cwd, log_path: rel(options.logPath) });
  writeManifest("running");
  return child;
}

async function waitForHttp(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let lastError = "not attempted";
  while (Date.now() < deadline) {
    try {
      await new Promise((resolveWait, rejectWait) => {
        const request = http.get(url, (response) => {
          response.resume();
          if ((response.statusCode ?? 500) < 500) resolveWait();
          else rejectWait(new Error(`HTTP ${response.statusCode}`));
        });
        request.setTimeout(1000, () => request.destroy(new Error("timeout")));
        request.on("error", rejectWait);
      });
      return;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
      await new Promise((resolveWait) => setTimeout(resolveWait, 500));
    }
  }
  throw new Error(`Timed out waiting for ${url}: ${lastError}`);
}

function captureScreenshot(label) {
  const tool = manifest.preflight.screenshot_capability;
  if (!tool) return { label, status: "skipped", reason: "no screenshot utility found", path: null };
  const output = resolve(screenshotDir, `${label}.png`);
  let result;
  if (tool === "gnome-screenshot") result = spawnSync(tool, ["-f", output], { encoding: "utf8" });
  else if (tool === "import") result = spawnSync(tool, ["-window", "root", output], { encoding: "utf8" });
  else if (tool === "scrot") result = spawnSync(tool, [output], { encoding: "utf8" });
  else return { label, status: "skipped", reason: "xwd output is not png; install gnome-screenshot/import/scrot for PNG capture", path: null };
  return {
    label,
    status: result.status === 0 && existsSync(output) ? "captured" : "failed",
    path: rel(output),
    sha256: sha256IfExists(output),
    stderr: String(result.stderr || "").trim(),
  };
}

async function terminateProcess(entry, signal) {
  if (entry.child.exitCode !== null || entry.child.signalCode !== null) return;
  try {
    if (process.platform === "win32") entry.child.kill(signal);
    else process.kill(-entry.child.pid, signal);
  } catch {
    try {
      entry.child.kill(signal);
    } catch {
      // Process may already have exited.
    }
  }
}

async function terminateChildren(children) {
  for (const entry of [...children].reverse()) await terminateProcess(entry, "SIGTERM");
  await new Promise((resolveWait) => setTimeout(resolveWait, 1_500));
  for (const entry of [...children].reverse()) await terminateProcess(entry, "SIGKILL");
  await new Promise((resolveWait) => setTimeout(resolveWait, 300));
}

async function runBuildPreflight() {
  if (skipBuildPreflight) return { status: "skipped", reason: "--skip-build-preflight or DISCRYPT_G012_SKIP_BUILD_PREFLIGHT=1" };
  const build = {
    command: "cargo",
    args: ["build", "-p", "discrypt-desktop", "--features", tauriFeatureArg],
    cwd: repoRoot,
  };
  const buildLog = resolve(logDir, "tauri-build-preflight.log");
  const result = spawnSync(build.command, build.args, { cwd: build.cwd, encoding: "utf8", env: process.env, maxBuffer: 1024 * 1024 * 128 });
  writeFileSync(buildLog, `${result.stdout || ""}\n${result.stderr || ""}`);
  const status = { ...build, status: result.status === 0 ? "passed" : "failed", exit_status: result.status, log_path: rel(buildLog), sha256: sha256IfExists(buildLog) };
  manifest.preflight.build = status;
  writeManifest(result.status === 0 ? "build-preflight-passed" : "failed", { preflight: manifest.preflight });
  if (result.status !== 0) throw new Error(`Tauri build preflight failed: ${rel(buildLog)}`);
  return status;
}

async function waitForLogPattern(path, pattern, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  let latest = "";
  while (Date.now() < deadline) {
    if (existsSync(path)) {
      latest = readFileSync(path, "utf8");
      const normalized = stripAnsi(latest);
      if (pattern.test(normalized)) return { label, status: "ready", pattern: String(pattern), log_path: rel(path) };
      if (/exited_at=.*code=(?!null)|error while running discrypt Tauri application|panicked at/i.test(normalized)) {
        throw new Error(`${label} exited or failed before launch readiness; see ${rel(path)}`);
      }
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 500));
  }
  const tail = latest.split(/\r?\n/).slice(-20).join("\n");
  throw new Error(`${label} did not reach Tauri binary launch readiness within ${timeoutMs}ms; see ${rel(path)}; tail=${stripAnsi(tail)}`);
}

function summarize(children, screenshots, launchReadiness) {
  const logs = Object.fromEntries(
    Object.entries(profiles).map(([name, profile]) => [name, { path: rel(profile.log_path), sha256: sha256IfExists(profile.log_path) }]),
  );
  if (!noVite) logs.vite = { path: rel(manifest.shared_frontend.log_path), sha256: sha256IfExists(manifest.shared_frontend.log_path) };
  const summary = {
    schema_version: "discrypt.g012.tauri_two_profile_launch_summary.v1",
    generated_at: new Date().toISOString(),
    status: "passed",
    run_id: runId,
    artifact_root: rel(artifactRoot),
    manifest: rel(manifestPath),
    profile_state_files: Object.fromEntries(Object.entries(profiles).map(([name, profile]) => [name, { path: rel(profile.state_path), exists: existsSync(profile.state_path), sha256: sha256IfExists(profile.state_path) }])),
    logs,
    screenshots,
    launch_readiness: launchReadiness,
    processes: children.map((entry) => ({ label: entry.label, pid: entry.child.pid, exitCode: entry.child.exitCode, signalCode: entry.child.signalCode })),
    next_g012_steps: [
      "Drive setup/recovery UX inside both launched Tauri windows.",
      "Capture invite acceptance, bidirectional encrypted text, persistence/reload, and voice media evidence into this artifact root.",
    ],
  };
  writeJson(summaryPath, summary);
  return summary;
}

planCommands();
const preflight = preflightChecks();
writeManifest(run ? "planned" : "dry-run", { preflight_result: preflight });
if (!run) {
  console.log(`G012 Tauri two-profile E2E dry-run manifest: ${manifestPath}`);
  process.exit(0);
}
if (!preflight.ok) {
  writeManifest("failed-preflight", { preflight_result: preflight });
  console.error(preflight.reason);
  process.exit(3);
}

const children = [];
try {
  await runBuildPreflight();

  if (!noVite) {
    children.push({ label: "shared vite dev server", child: spawnLogged("shared vite dev server", "npm", ["run", "dev", "--", "--host", "127.0.0.1", "--port", "1420", "--strictPort"], { cwd: resolve(repoRoot, "apps/ui"), env: {}, logPath: manifest.shared_frontend.log_path }) });
    await waitForHttp(viteUrl, Number(process.env.DISCRYPT_G012_VITE_WAIT_MS || 30_000));
  }

  for (const profile of Object.values(profiles)) {
    const command = tauriCommandFor(profile);
    children.push({ label: `tauri ${profile.name}`, child: spawnLogged(`tauri ${profile.name}`, command.command, command.args, { cwd: command.cwd, env: command.env, logPath: profile.log_path }) });
  }
  const launchReadiness = [];
  for (const profile of Object.values(profiles)) {
    launchReadiness.push(await waitForLogPattern(profile.log_path, /Running .*discrypt-desktop|Finished .*dev.*profile/i, launchReadyTimeoutMs, `tauri ${profile.name}`));
  }
  writeManifest("launch-ready", { launch_readiness: launchReadiness });
  await new Promise((resolveWait) => setTimeout(resolveWait, Math.max(2_000, Math.floor(durationMs / 2))));
  const screenshots = [captureScreenshot("two-profile-launch-midrun")];
  await new Promise((resolveWait) => setTimeout(resolveWait, Math.max(0, durationMs - Math.max(2_000, Math.floor(durationMs / 2)))));
  screenshots.push(captureScreenshot("two-profile-launch-before-stop"));

  await terminateChildren(children);
  const summary = summarize(children, screenshots, launchReadiness);
  writeManifest("passed", { summary: rel(summaryPath) });
  console.log(`G012 Tauri two-profile launch artifact: ${summary.artifact_root}`);
} catch (error) {
  await terminateChildren(children);
  const message = error instanceof Error ? error.message : String(error);
  writeManifest("failed", { error: message });
  console.error(`g012-tauri-two-profile-e2e: ${message}`);
  process.exit(1);
}

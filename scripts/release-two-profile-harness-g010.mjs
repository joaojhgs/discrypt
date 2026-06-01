#!/usr/bin/env node
import { createWriteStream, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { once } from "node:events";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run") || process.env.DISCRYPT_G010_DRY_RUN === "1";
const includePublic = args.includes("--include-public") || process.env.DISCRYPT_G010_PUBLIC_MATRIX === "1";
const launchBuilt = args.includes("--launch-built") || process.env.DISCRYPT_G010_LAUNCH_BUILT === "1";
const runBrowserFlow = !args.includes("--skip-browser-flow") && process.env.DISCRYPT_G010_SKIP_BROWSER_FLOW !== "1";
const runRustMatrix = !args.includes("--skip-rust-matrix") && process.env.DISCRYPT_G010_SKIP_RUST_MATRIX !== "1";
const artifactRoot = resolve(repoRoot, valueAfter("--artifact-dir") ?? "target/release/g010-two-profile-harness");
const playwrightOutputDir = resolve(artifactRoot, "playwright-output");
const profileRoot = resolve(artifactRoot, "profiles");
const profileState = {
  alice: resolve(profileRoot, "alice", "app-state.discrypt-store"),
  bob: resolve(profileRoot, "bob", "app-state.discrypt-store"),
};
const packageJson = JSON.parse(readFileSync(resolve(repoRoot, "apps/ui/package.json"), "utf8"));
const tauriConfig = JSON.parse(readFileSync(resolve(repoRoot, "apps/desktop/src-tauri/tauri.conf.json"), "utf8"));
const desktopCargo = readFileSync(resolve(repoRoot, "apps/desktop/src-tauri/Cargo.toml"), "utf8");
const desktopCrate = desktopCargo.match(/^name\s*=\s*"([^"]+)"/m)?.[1] ?? "discrypt-desktop";
const versionTargets = {
  uiPackage: packageJson.version,
  tauriConfig: tauriConfig.version,
  desktopCargo: desktopCargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? null,
};
const localAdapterMatrix = [
  {
    id: "browser-ui-build",
    kind: "local-browser",
    command: "npm",
    args: ["run", "build"],
    cwd: "apps/ui",
    env: { VITE_DISCRYPT_LOCAL_DEV_FALLBACK: "1" },
    stdout: "browser-ui-build.stdout.log",
    stderr: "browser-ui-build.stderr.log",
    required: true,
    enabled: runBrowserFlow,
    disabledReason: "disabled by --skip-browser-flow or DISCRYPT_G010_SKIP_BROWSER_FLOW=1",
  },
  {
    id: "browser-two-profile-ui",
    kind: "local-browser",
    command: "npx",
    args: ["playwright", "test", "tests/e2e/two-profile-flow.spec.ts", "--workers=1", "--reporter=json", "--output", playwrightOutputDir],
    cwd: "apps/ui",
    env: { VITE_DISCRYPT_LOCAL_DEV_FALLBACK: "1", PLAYWRIGHT_HTML_OPEN: "never" },
    stdout: "browser-two-profile-flow.json",
    stderr: "browser-two-profile-flow.stderr.log",
    required: true,
    enabled: runBrowserFlow,
    disabledReason: "disabled by --skip-browser-flow or DISCRYPT_G010_SKIP_BROWSER_FLOW=1",
  },
  {
    id: "desktop-two-profile-state-roundtrip",
    kind: "local-rust",
    command: "cargo",
    args: ["test", "-q", "-p", "discrypt-desktop", "text_control_frame_roundtrip_persists_across_two_profile_state_files", "--", "--nocapture"],
    stdout: "desktop-two-profile-state-roundtrip.stdout.log",
    stderr: "desktop-two-profile-state-roundtrip.stderr.log",
    required: true,
    enabled: runRustMatrix,
    disabledReason: "disabled by --skip-rust-matrix or DISCRYPT_G010_SKIP_RUST_MATRIX=1",
  },
  {
    id: "desktop-text-control-transport-pump",
    kind: "local-rust",
    command: "cargo",
    args: ["test", "-q", "-p", "discrypt-desktop", "text_control_session_pump_uses_data_transport_trait_and_persists_receipt", "--", "--nocapture"],
    stdout: "desktop-text-control-transport-pump.stdout.log",
    stderr: "desktop-text-control-transport-pump.stderr.log",
    required: true,
    enabled: runRustMatrix,
    disabledReason: "disabled by --skip-rust-matrix or DISCRYPT_G010_SKIP_RUST_MATRIX=1",
  },
  {
    id: "desktop-two-profile-restart-matrix",
    kind: "local-rust",
    command: "cargo",
    args: ["test", "-q", "-p", "discrypt-desktop", "g004_two_profile_restart_matrix_persists_invites_connectivity_receipts_voice_and_preferences", "--", "--nocapture"],
    stdout: "desktop-two-profile-restart-matrix.stdout.log",
    stderr: "desktop-two-profile-restart-matrix.stderr.log",
    required: true,
    enabled: runRustMatrix,
    disabledReason: "disabled by --skip-rust-matrix or DISCRYPT_G010_SKIP_RUST_MATRIX=1",
  },
];
const publicAdapterMatrix = [
  {
    id: "public-mqtt-two-profile-receipt",
    adapter: "mqtt",
    command: "cargo",
    args: ["test", "-q", "-p", "discrypt-desktop", "--features", "mqtt-adapter", "public_mqtt_two_profile_receipt_crosses_provider_webrtc_when_enabled", "--", "--nocapture"],
    env: {
      DISCRYPT_DESKTOP_PUBLIC_MQTT_RECEIPT_E2E: "1",
      DISCRYPT_PUBLIC_MQTT_ENDPOINT: process.env.DISCRYPT_PUBLIC_MQTT_ENDPOINT || "mqtts://broker.emqx.io:8883",
    },
    stdout: "public-mqtt-two-profile-receipt.stdout.log",
    stderr: "public-mqtt-two-profile-receipt.stderr.log",
    requiredEnv: [],
  },
  {
    id: "public-nostr-two-profile-receipt",
    adapter: "nostr",
    command: "cargo",
    args: ["test", "-q", "-p", "discrypt-desktop", "--features", "nostr-adapter", "public_nostr_two_profile_receipt_crosses_provider_webrtc_when_enabled", "--", "--nocapture"],
    env: {
      DISCRYPT_DESKTOP_PUBLIC_NOSTR_RECEIPT_E2E: "1",
      DISCRYPT_PUBLIC_NOSTR_ENDPOINT: process.env.DISCRYPT_PUBLIC_NOSTR_ENDPOINT || "wss://nos.lol",
    },
    stdout: "public-nostr-two-profile-receipt.stdout.log",
    stderr: "public-nostr-two-profile-receipt.stderr.log",
    requiredEnv: [],
  },
  {
    id: "public-turn-relay-only",
    adapter: "turn",
    command: "cargo",
    args: ["test", "-q", "-p", "discrypt-transport", "--features", "mqtt-adapter", "--test", "public_webrtc_datachannel_e2e", "public_mqtt_relay_only_turn_fallback_roundtrip_when_configured", "--", "--nocapture"],
    env: { DISCRYPT_PUBLIC_TURN_E2E: "1" },
    stdout: "public-turn-relay-only.stdout.log",
    stderr: "public-turn-relay-only.stderr.log",
    requiredEnv: ["DISCRYPT_PUBLIC_TURN_ENDPOINT", "DISCRYPT_PUBLIC_TURN_USERNAME", "DISCRYPT_PUBLIC_TURN_CREDENTIAL"],
  },
  {
    id: "public-ipfs-topic-peer",
    adapter: "ipfs",
    command: "cargo",
    args: ["test", "-q", "-p", "discrypt-transport", "--features", "ipfs-pubsub-adapter", "public_ipfs_two_peer_signaling_smoke", "--", "--nocapture"],
    env: { DISCRYPT_PUBLIC_IPFS_E2E: "1" },
    stdout: "public-ipfs-topic-peer.stdout.log",
    stderr: "public-ipfs-topic-peer.stderr.log",
    requiredEnv: ["DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS"],
  },
  {
    id: "public-quic-rendezvous",
    adapter: "quic-rendezvous",
    command: "cargo",
    args: ["test", "-q", "-p", "discrypt-transport", "--features", "discrypt-quic-rendezvous-adapter", "public_quic_two_peer_signaling_smoke", "--", "--nocapture"],
    env: { DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E: "1" },
    stdout: "public-quic-rendezvous.stdout.log",
    stderr: "public-quic-rendezvous.stderr.log",
    requiredEnv: ["DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT"],
  },
];

function valueAfter(flag) {
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : undefined;
}

function rel(path) {
  return path.startsWith(repoRoot) ? path.slice(repoRoot.length + 1) : path;
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function ensureVersionSync() {
  const versions = Object.entries(versionTargets);
  const unique = new Set(versions.map(([, version]) => version));
  if (unique.size !== 1 || unique.has(null) || unique.has(undefined)) {
    throw new Error(`version metadata must match across UI, Tauri config, and desktop Cargo.toml: ${JSON.stringify(versionTargets)}`);
  }
}

function renderCommand(step) {
  return [step.command, ...step.args].join(" ");
}

function writeText(path, text) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, text);
}

function runStep(step, extraEnv = {}) {
  if (step.enabled === false) {
    return { ...step, rendered: renderCommand(step), status: "skipped", reason: step.disabledReason };
  }
  if (dryRun) return { ...step, rendered: renderCommand(step), status: "planned" };
  const cwd = resolve(repoRoot, step.cwd ?? ".");
  const stdoutPath = resolve(artifactRoot, step.stdout ?? `${step.id}.stdout.log`);
  const stderrPath = resolve(artifactRoot, step.stderr ?? `${step.id}.stderr.log`);
  const result = spawnSync(step.command, step.args, {
    cwd,
    encoding: "utf8",
    env: { ...process.env, ...extraEnv, ...(step.env || {}) },
    maxBuffer: 1024 * 1024 * 64,
  });
  writeText(stdoutPath, result.stdout ?? "");
  writeText(stderrPath, result.stderr ?? "");
  if (result.status !== 0) {
    throw new Error(`${step.id} failed with status ${result.status ?? "unknown"}; stdout=${rel(stdoutPath)} stderr=${rel(stderrPath)}`);
  }
  return {
    ...step,
    rendered: renderCommand(step),
    status: "passed",
    stdout: rel(stdoutPath),
    stderr: rel(stderrPath),
    stdoutSha256: sha256(stdoutPath),
    stderrSha256: sha256(stderrPath),
  };
}

function publicStepStatus(step) {
  const missing = step.requiredEnv.filter((name) => !process.env[name]);
  if (missing.length > 0) {
    return {
      ...step,
      rendered: renderCommand(step),
      status: "skipped_missing_external_credentials",
      missingEnv: missing,
      reason: `set ${missing.join(", ")} to run ${step.id}`,
    };
  }
  return runStep(step, step.env);
}

async function sleep(ms) {
  await new Promise((resolveWait) => setTimeout(resolveWait, ms));
}

function childIsRunning(child) {
  return child.exitCode === null && child.signalCode === null;
}

async function waitForExit(child, timeoutMs) {
  if (!childIsRunning(child)) return true;
  return Promise.race([once(child, "exit").then(() => true), sleep(timeoutMs).then(() => false)]);
}

async function launchBuiltProfiles() {
  const binary = resolve(repoRoot, "target/release", process.platform === "win32" ? `${desktopCrate}.exe` : desktopCrate);
  const step = {
    id: "tauri-built-two-profile-launch",
    command: binary,
    args: [],
    rendered: `${rel(binary)} # launched twice with isolated DISCRYPT_APP_STATE_PATH values`,
    profiles: Object.fromEntries(Object.entries(profileState).map(([name, path]) => [name, rel(path)])),
  };
  if (!launchBuilt) return { ...step, status: "skipped", reason: "set --launch-built or DISCRYPT_G010_LAUNCH_BUILT=1 after building the Tauri binary" };
  if (!existsSync(binary)) throw new Error(`built Tauri binary not found at ${binary}; run npm --prefix apps/ui run release:linux or cargo build --release -p discrypt-desktop --features tauri-runtime,local-dev first so DISCRYPT_APP_STATE_PATH profile isolation is honored`);
  if (dryRun) return { ...step, status: "planned" };
  mkdirSync(resolve(profileRoot, "alice"), { recursive: true });
  mkdirSync(resolve(profileRoot, "bob"), { recursive: true });
  const children = Object.entries(profileState).map(([name, statePath]) => {
    const stdout = resolve(artifactRoot, `${name}-tauri.stdout.log`);
    const stderr = resolve(artifactRoot, `${name}-tauri.stderr.log`);
    const stdoutStream = createWriteStream(stdout, { flags: "w" });
    const stderrStream = createWriteStream(stderr, { flags: "w" });
    const child = spawn(binary, [], {
      cwd: repoRoot,
      env: {
        ...process.env,
        DISCRYPT_APP_STATE_PATH: statePath,
        DISCRYPT_RELEASE_HARNESS_PROFILE: name,
        WEBKIT_DISABLE_COMPOSITING_MODE: "1",
      },
      stdio: ["ignore", "pipe", "pipe"],
    });
    child.stdout.pipe(stdoutStream);
    child.stderr.pipe(stderrStream);
    return { name, child, stdout, stderr, stdoutStream, stderrStream };
  });
  await sleep(8000);
  const earlyExit = children.find((entry) => !childIsRunning(entry.child));
  for (const entry of children) {
    if (childIsRunning(entry.child)) entry.child.kill("SIGTERM");
  }
  await Promise.all(children.map((entry) => waitForExit(entry.child, 5000)));
  for (const entry of children) {
    if (childIsRunning(entry.child)) entry.child.kill("SIGKILL");
  }
  await Promise.all(children.map((entry) => waitForExit(entry.child, 2000)));
  await Promise.all(children.flatMap((entry) => [
    new Promise((resolveFinish) => entry.stdoutStream.end(resolveFinish)),
    new Promise((resolveFinish) => entry.stderrStream.end(resolveFinish)),
  ]));
  const logs = Object.fromEntries(children.map((entry) => [entry.name, {
    stdout: rel(entry.stdout),
    stderr: rel(entry.stderr),
    pid: entry.child.pid,
    exitCode: entry.child.exitCode,
    signalCode: entry.child.signalCode,
  }]));
  if (earlyExit) {
    return {
      ...step,
      status: "failed",
      reason: `${earlyExit.name} exited before the 8000ms launch window elapsed`,
      logs,
    };
  }
  return {
    ...step,
    status: "passed",
    logs,
  };
}

function makePlan() {
  ensureVersionSync();
  const plan = {
    schema_version: "discrypt.g010.two_profile_release_harness.v1",
    generated_at: new Date().toISOString(),
    product: {
      name: tauriConfig.productName,
      identifier: tauriConfig.identifier,
      desktopCrate,
      version: versionTargets.uiPackage,
      versionTargets,
    },
    artifactRoot: rel(artifactRoot),
    profiles: Object.fromEntries(Object.entries(profileState).map(([name, path]) => [name, { statePath: rel(path) }])),
    mode: {
      dryRun,
      includePublic,
      launchBuilt,
      runBrowserFlow,
      runRustMatrix,
    },
    localAdapterMatrix: localAdapterMatrix.map((step) => ({ ...step, rendered: renderCommand(step) })),
    publicAdapterMatrix: publicAdapterMatrix.map((step) => ({ ...step, rendered: renderCommand(step), missingEnv: step.requiredEnv.filter((name) => !process.env[name]) })),
    tauriBuiltLaunch: {
      rendered: `target/release/${desktopCrate} # launched concurrently for alice+bob with DISCRYPT_APP_STATE_PATH isolation`,
      envIsolation: "DISCRYPT_APP_STATE_PATH per profile",
      logs: ["alice-tauri.stdout.log", "alice-tauri.stderr.log", "bob-tauri.stdout.log", "bob-tauri.stderr.log"],
    },
    artifactPolicy: {
      stdoutStderr: true,
      playwrightJson: true,
      screenshotsAndTraces: "Playwright output under apps/ui/test-results plus JSON copied to artifactRoot; traces retained by Playwright on failure",
      sensitiveDataBoundary: "Harness artifacts must not include message bodies beyond deterministic test strings, raw credentials, SDP bodies, ICE passwords, MLS secrets, SFrame keys, invite secrets, or raw environment dumps.",
    },
  };
  return plan;
}

async function main() {
  const plan = makePlan();
  mkdirSync(artifactRoot, { recursive: true });
  writeText(resolve(artifactRoot, "plan.json"), `${JSON.stringify(plan, null, 2)}\n`);
  if (dryRun) {
    console.log(JSON.stringify(plan, null, 2));
    return;
  }
  const results = [];
  for (const step of localAdapterMatrix) results.push(runStep(step));
  if (includePublic) {
    for (const step of publicAdapterMatrix) results.push(publicStepStatus(step));
  } else {
    for (const step of publicAdapterMatrix) {
      results.push({ ...step, rendered: renderCommand(step), status: "skipped_public_matrix_disabled", reason: "set --include-public or DISCRYPT_G010_PUBLIC_MATRIX=1" });
    }
  }
  results.push(await launchBuiltProfiles());
  const report = {
    ...plan,
    completed_at: new Date().toISOString(),
    status: "passed",
    results,
  };
  writeText(resolve(artifactRoot, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report, null, 2));
}

main().catch((error) => {
  mkdirSync(artifactRoot, { recursive: true });
  const message = error instanceof Error ? error.message : String(error);
  writeText(resolve(artifactRoot, "failure.json"), `${JSON.stringify({ schema_version: "discrypt.g010.two_profile_release_harness.v1", status: "failed", error: message }, null, 2)}\n`);
  console.error(`release-two-profile-harness-g010: ${message}`);
  process.exit(1);
});

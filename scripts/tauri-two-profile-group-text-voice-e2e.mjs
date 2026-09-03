#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { createCipheriv, createHash, randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createConnection } from "node:net";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const leaderRoot = process.env.OMX_TEAM_LEADER_CWD ? resolve(process.env.OMX_TEAM_LEADER_CWD) : repoRoot;
const argv = process.argv.slice(2);
const run = argv.includes("--run");
const skipBuild = argv.includes("--skip-build") || process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_SKIP_BUILD === "1";
const requireNativeVoice = argv.includes("--require-native-voice") ||
  process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_REQUIRE_NATIVE_VOICE === "1";
const runId = valueAfter("--run-id") ?? process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_RUN_ID ?? `tauri-two-profile-e2e-${new Date().toISOString().replace(/[:.]/g, "-")}`;
const artifactRoot = resolve(repoRoot, valueAfter("--artifact-dir") ?? process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_ARTIFACT_DIR ?? `target/tauri-two-profile-group-text-voice-e2e/${runId}`);
const logDir = resolve(artifactRoot, "logs");
const profileDir = resolve(artifactRoot, "profiles");
const screenshotDir = resolve(artifactRoot, "screenshots");
for (const dir of [artifactRoot, logDir, profileDir, screenshotDir]) mkdirSync(dir, { recursive: true });

const driverBinary = process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_TAURI_DRIVER || commandPath("tauri-driver");
const nativeDriverBinary = process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_NATIVE_WEBDRIVER || commandPath("WebKitWebDriver") || firstExisting([
  resolve(repoRoot, "target/webdriver-deps/extracted/usr/bin/WebKitWebDriver"),
  resolve(leaderRoot, "target/webdriver-deps/extracted/usr/bin/WebKitWebDriver"),
]);
const appBinary = process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_APP_BINARY
  ? resolve(repoRoot, process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_APP_BINARY)
  : firstExisting([
      resolve(repoRoot, "target/debug/discrypt-desktop"),
      resolve(leaderRoot, "target/debug/discrypt-desktop"),
    ]);
const basePort = Number(process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_BASE_PORT ?? valueAfter("--base-port") ?? 4510);
const mqttPort = Number(process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_MQTT_PORT ?? basePort + 20);
const configuredMqttEndpoint = valueAfter("--mqtt-endpoint") ?? process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_MQTT_ENDPOINT;
const localMqttEndpoint = configuredMqttEndpoint ?? `mqtt://127.0.0.1:${mqttPort}`;
const mqttBrokerBinary = configuredMqttEndpoint ? null : commandPath("rumqttd");
const mqttBrokerConfigPath = resolve(artifactRoot, "rumqttd.toml");
const mqttBrokerLogPath = resolve(logDir, "rumqttd.log");
const disableSyntheticVoiceFallback = argv.includes("--disable-synthetic-voice-fallback") || process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_DISABLE_SYNTHETIC_VOICE_FALLBACK === "1";
if (!Number.isInteger(basePort) || basePort < 1024 || basePort > 65000) failCli("base port must be a valid high TCP port", 2);
if (!configuredMqttEndpoint && (!Number.isInteger(mqttPort) || mqttPort < 1024 || mqttPort > 65535)) failCli("local MQTT port must be a valid high TCP port", 2);

const harnessEnv = {
  ...process.env,
  DISCRYPT_DEFAULT_MQTT_ENDPOINT: localMqttEndpoint,
  VITE_DISCRYPT_DEFAULT_MQTT_ENDPOINT: localMqttEndpoint,
};

const profiles = {
  alice: {
    display_name: "Alice",
    device_name: "Alice Desktop",
    state_path: resolve(profileDir, "alice/app-state.discrypt-store"),
    driver_port: basePort,
    native_port: basePort + 1,
    log_path: resolve(logDir, "tauri-driver-alice.log"),
  },
  bob: {
    display_name: "Bob",
    device_name: "Bob Laptop",
    state_path: resolve(profileDir, "bob/app-state.discrypt-store"),
    driver_port: basePort + 2,
    native_port: basePort + 3,
    log_path: resolve(logDir, "tauri-driver-bob.log"),
  },
};
for (const profile of Object.values(profiles)) mkdirSync(dirname(profile.state_path), { recursive: true });

const workflowSteps = [
  {
    id: "setup",
    description: "Create two isolated Tauri WebDriver sessions and complete local profile setup in each real WebView.",
    required_artifacts: ["tauri-driver logs", "profile app-state files"],
  },
  {
    id: "invite",
    description: "Create a governed group invite in Alice and submit it through Bob's join flow without treating invite parsing as membership.",
    required_artifacts: ["manifest invite prefix", "OpenMLS admission request state"],
  },
  {
    id: "approval",
    description: "Approve Bob's pending admission through Alice's owner/staff UI and wait for persisted OpenMLS Welcome state.",
    required_artifacts: ["openmls_admission_owner_approval", "OpenMLS handle epochs"],
  },
  {
    id: "text",
    description: "Send signed group text both ways after admission and record plaintext, envelope, and receipt evidence from persisted state.",
    required_artifacts: ["text.evidence", "provider text/control runtime pump records"],
  },
  {
    id: "voice",
    description: "Join voice from both WebViews, prove native Rust/generated-audio media or report unavailable native capability, then mute and leave.",
    required_artifacts: ["voice_proof", "per59_release_smoke", "native_voice_capability"],
  },
  {
    id: "persistence",
    description: "Reload both WebViews after admission/text/voice and retain profile state hashes, screenshots, logs, and leave-cleanup evidence.",
    required_artifacts: ["screenshots", "profile_state_files", "logs", "voice.leave_cleanup"],
  },
  {
    id: "degraded_unavailable",
    description: "Fail preflight with a manifest when DISPLAY, tauri-driver, WebKitWebDriver, app binary, or native voice proof requirements are unavailable.",
    required_artifacts: ["preflight_result", "failed-preflight manifest"],
  },
];

const artifactContract = {
  test: "Tauri two-profile group text and voice E2E",
  evidence_level: "Tauri WebDriver release harness evidence when run with --run on a display-capable runner",
  dry_run_boundary: "Dry-run writes the manifest/preflight contract only; it is not setup, invite, approval, text, voice, persistence, or production evidence.",
  provider_policy: "The strict run uses the sealed MQTT broker control lane only for admission bootstrap, then provider-signaled direct WebRTC text/control runtimes for group text and voice signaling; it rejects manual command-bridge fallback.",
  membership_policy: "Invite parsing is not membership; protected text and voice evidence require backend approval plus persisted OpenMLS Welcome/add state.",
  voice_policy: "Synthetic WebView peer-connection fallback is diagnostic only and cannot satisfy the strict E2E acceptance criteria.",
};

const manifestPath = resolve(artifactRoot, "tauri-two-profile-group-text-voice-e2e-manifest.json");
const summaryPath = resolve(artifactRoot, "tauri-two-profile-group-text-voice-e2e-summary.json");
const manifest = {
  schema_version: "discrypt.tauri_two_profile_group_text_voice_e2e.v1",
  generated_at: new Date().toISOString(),
  mode: run ? "run" : "dry-run",
  run_id: runId,
  artifact_root: rel(artifactRoot),
  app_binary: rel(appBinary),
  driver_binary: driverBinary || null,
  native_webdriver: nativeDriverBinary,
  profile_isolation_env: "DISCRYPT_APP_STATE_PATH",
  automation_env: "TAURI_WEBVIEW_AUTOMATION=1",
  local_mqtt: {
    endpoint: localMqttEndpoint,
    broker: configuredMqttEndpoint ? "externally_managed" : "rumqttd",
    binary: mqttBrokerBinary,
    config_path: configuredMqttEndpoint ? null : rel(mqttBrokerConfigPath),
    log_path: configuredMqttEndpoint ? null : rel(mqttBrokerLogPath),
  },
  require_native_voice: requireNativeVoice,
  boundary: "Drives two real Tauri WebViews through setup/group invite/text/voice UX. It reports remote text/media delivery truthfully and does not convert launch/UI smoke into a production network claim.",
  workflow_steps: workflowSteps,
  artifact_contract: artifactContract,
  profiles,
  commands: [],
};

function valueAfter(flag) {
  const index = argv.indexOf(flag);
  return index >= 0 ? argv[index + 1] : undefined;
}
function rel(path) {
  return path && path.startsWith(repoRoot) ? path.slice(repoRoot.length + 1) : path;
}
function failCli(message, code = 1) {
  console.error(`tauri-two-profile-group-text-voice-e2e: ${message}`);
  process.exit(code);
}
function firstExisting(paths) {
  return paths.find((path) => path && existsSync(path)) ?? paths[0] ?? null;
}
function commandPath(command) {
  const result = spawnSync("sh", ["-lc", `command -v ${JSON.stringify(command)}`], { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : null;
}
function pkgConfigVersion(name) {
  const result = spawnSync("pkg-config", ["--modversion", name], { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : null;
}
function sha256IfExists(path) {
  return existsSync(path) ? createHash("sha256").update(readFileSync(path)).digest("hex") : null;
}
function commandOutput(command, args = []) {
  const result = spawnSync(command, args, { encoding: "utf8" });
  return {
    status: result.status,
    stdout: String(result.stdout || "").trim(),
    stderr: String(result.stderr || "").trim(),
  };
}
function webkitRuntimeDiagnostics() {
  const pkgConfig = commandOutput("pkg-config", ["--modversion", "webkit2gtk-4.1", "javascriptcoregtk-4.1"]);
  const nativeDriver = nativeDriverBinary && existsSync(nativeDriverBinary)
    ? commandOutput(nativeDriverBinary, ["--version"])
    : { status: null, stdout: "", stderr: "native WebDriver binary missing" };
  return {
    pkg_config_webkit2gtk_4_1: pkgConfig,
    native_webdriver_version: nativeDriver,
    display: { DISPLAY: process.env.DISPLAY || null, WAYLAND_DISPLAY: process.env.WAYLAND_DISPLAY || null },
    env_flags: {
      WEBKIT_DISABLE_COMPOSITING_MODE: "1",
      WEBKIT_DISABLE_DMABUF_RENDERER: "1",
      LIBGL_ALWAYS_SOFTWARE: "1",
      NO_AT_BRIDGE: "1",
      TAURI_WEBVIEW_AUTOMATION: "1",
    },
  };
}
function readJsonIfExists(path) {
  if (!existsSync(path)) return null;
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    return { parse_error: error instanceof Error ? error.message : String(error) };
  }
}
function textStateEvidence(profile, localMessage, remoteMessage) {
  const state = readJsonIfExists(profile.state_path);
  const messages = Array.isArray(state?.messages) ? state.messages : [];
  const events = Array.isArray(state?.events) ? state.events : [];
  const receivedEnvelopes = messages.filter((message) => message?.state_key === "received_envelope");
  const receivedPlaintexts = messages.filter((message) => message?.state_key === "received_plaintext");
  const peerReceipts = messages.filter((message) => message?.state_key === "peer_receipt");
  const envelopeReceivedEvent = events.some((event) => event?.kind === "message.envelope_received");
  return {
    state_readable: Boolean(state && !state.parse_error),
    parse_error: state?.parse_error ?? null,
    local_plaintext_visible: messages.some((message) => String(message?.body ?? "").includes(localMessage)),
    remote_plaintext_visible: messages.some((message) => String(message?.body ?? "").includes(remoteMessage)),
    remote_envelope_visible: receivedEnvelopes.length > 0 || receivedPlaintexts.length > 0 || envelopeReceivedEvent,
    remote_envelope_count: receivedEnvelopes.length + receivedPlaintexts.length,
    sender_peer_receipt_visible: peerReceipts.length > 0,
    sender_peer_receipt_count: peerReceipts.length,
    transport_attach_started_count: events.filter((event) => event?.kind === "transport.text_runtime_attach_started").length,
    transport_attach_deduped_count: events.filter((event) => event?.kind === "transport.text_runtime_attach_deduped").length,
    transport_attached: events.some((event) => event?.kind === "transport.text_runtime_attached"),
    envelope_received_event: envelopeReceivedEvent,
    receipt_verified_event: events.some((event) => event?.kind === "message.receipt_verified"),
    command_error: state?.last_command_error ?? null,
  };
}
function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}
function writeManifest(status, extra = {}) {
  manifest.status = status;
  manifest.updated_at = new Date().toISOString();
  Object.assign(manifest, extra);
  writeJson(manifestPath, manifest);
}
function preflight() {
  const checks = {
    display: { DISPLAY: process.env.DISPLAY || null, WAYLAND_DISPLAY: process.env.WAYLAND_DISPLAY || null },
    driver_binary: driverBinary,
    native_webdriver: nativeDriverBinary,
    native_webdriver_exists: nativeDriverBinary ? existsSync(nativeDriverBinary) : false,
    app_binary: appBinary,
    app_binary_exists: existsSync(appBinary),
    skip_build: skipBuild,
    require_native_voice: requireNativeVoice,
    mqtt_endpoint: localMqttEndpoint,
    mqtt_broker_binary: mqttBrokerBinary,
    mqtt_broker_managed_externally: Boolean(configuredMqttEndpoint),
    webkit_runtime: webkitRuntimeDiagnostics(),
  };
  const okDisplay = Boolean(process.env.DISPLAY || process.env.WAYLAND_DISPLAY);
  if (!okDisplay) return { ok: false, reason: "No DISPLAY/WAYLAND_DISPLAY available for WebKit WebDriver", checks };
  if (!driverBinary) return { ok: false, reason: "tauri-driver is not installed; run cargo install tauri-driver --locked", checks };
  if (!nativeDriverBinary || !existsSync(nativeDriverBinary)) return { ok: false, reason: "WebKitWebDriver is missing; install webkit2gtk-driver or set DISCRYPT_TAURI_TWO_PROFILE_E2E_NATIVE_WEBDRIVER", checks };
  if (!configuredMqttEndpoint && !mqttBrokerBinary) return { ok: false, reason: "rumqttd is missing; install it with cargo install rumqttd --locked or set DISCRYPT_TAURI_TWO_PROFILE_E2E_MQTT_ENDPOINT", checks };
  return { ok: true, checks };
}
function runCommand(label, command, args, cwd) {
  const logPath = resolve(logDir, `${label}.log`);
  manifest.commands.push({ label, command, args, cwd: rel(cwd), log_path: rel(logPath) });
  writeManifest("building");
  const result = spawnSync(command, args, { cwd, encoding: "utf8", env: harnessEnv, maxBuffer: 1024 * 1024 * 128 });
  writeFileSync(logPath, `${result.stdout || ""}\n${result.stderr || ""}`);
  if (result.status !== 0) throw new Error(`${label} failed with ${result.status}; see ${rel(logPath)}`);
  return { log_path: rel(logPath), sha256: sha256IfExists(logPath) };
}
async function waitTcp(port, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  let last = "not attempted";
  while (Date.now() < deadline) {
    try {
      await new Promise((resolveConnect, rejectConnect) => {
        const socket = createConnection({ host: "127.0.0.1", port });
        socket.setTimeout(1000);
        socket.once("connect", () => {
          socket.end();
          resolveConnect();
        });
        socket.once("error", rejectConnect);
        socket.once("timeout", () => {
          socket.destroy();
          rejectConnect(new Error("connection timed out"));
        });
      });
      return;
    } catch (error) {
      last = error instanceof Error ? error.message : String(error);
      await new Promise((resolveWait) => setTimeout(resolveWait, 250));
    }
  }
  throw new Error(`Timed out waiting for local MQTT broker on ${port}: ${last}`);
}
function startLocalMqttBroker() {
  if (configuredMqttEndpoint) {
    return null;
  }
  const config = `id = 0

[router]
id = 0
max_connections = 32
max_outgoing_packet_count = 200
max_segment_size = 1048576
max_segment_count = 10

[v5.1]
name = "tauri-two-profile-e2e-local-v5"
listen = "127.0.0.1:${mqttPort}"
next_connection_delay_ms = 1

[v5.1.connections]
connection_timeout_ms = 60000
max_payload_size = 1048576
max_inflight_count = 100
`;
  writeFileSync(mqttBrokerConfigPath, config);
  writeFileSync(mqttBrokerLogPath, `$ ${mqttBrokerBinary} -c ${mqttBrokerConfigPath}\nstarted_at=${new Date().toISOString()}\n`);
  const child = spawn(mqttBrokerBinary, ["-c", mqttBrokerConfigPath], {
    cwd: repoRoot,
    env: harnessEnv,
    stdio: ["ignore", "pipe", "pipe"],
    detached: process.platform !== "win32",
  });
  child.stdout.on("data", (chunk) => writeFileSync(mqttBrokerLogPath, chunk, { flag: "a" }));
  child.stderr.on("data", (chunk) => writeFileSync(mqttBrokerLogPath, chunk, { flag: "a" }));
  child.on("exit", (code, signal) => writeFileSync(mqttBrokerLogPath, `\nexited_at=${new Date().toISOString()} code=${code} signal=${signal}\n`, { flag: "a" }));
  return child;
}
async function waitHttp(port, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  let last = "not attempted";
  while (Date.now() < deadline) {
    try {
      await fetch(`http://127.0.0.1:${port}/status`, { signal: AbortSignal.timeout(1000) });
      return;
    } catch (error) {
      last = error instanceof Error ? error.message : String(error);
      await new Promise((resolveWait) => setTimeout(resolveWait, 300));
    }
  }
  throw new Error(`Timed out waiting for tauri-driver on ${port}: ${last}`);
}
function startDriver(profile) {
  const args = ["--port", String(profile.driver_port), "--native-port", String(profile.native_port), "--native-driver", nativeDriverBinary];
  writeFileSync(profile.log_path, `$ ${driverBinary} ${args.join(" ")}\nstate=${profile.state_path}\nstarted_at=${new Date().toISOString()}\n`);
  const child = spawn(driverBinary, args, {
    cwd: repoRoot,
    env: {
      ...harnessEnv,
      DISCRYPT_APP_STATE_PATH: profile.state_path,
      TAURI_WEBVIEW_AUTOMATION: "1",
      WEBKIT_FORCE_SANDBOX: "0",
      WEBKIT_DISABLE_COMPOSITING_MODE: "1",
      WEBKIT_DISABLE_DMABUF_RENDERER: "1",
      LIBGL_ALWAYS_SOFTWARE: "1",
      NO_AT_BRIDGE: "1",
    },
    stdio: ["ignore", "pipe", "pipe"],
    detached: process.platform !== "win32",
  });
  child.stdout.on("data", (chunk) => writeFileSync(profile.log_path, chunk, { flag: "a" }));
  child.stderr.on("data", (chunk) => writeFileSync(profile.log_path, chunk, { flag: "a" }));
  child.on("exit", (code, signal) => writeFileSync(profile.log_path, `\nexited_at=${new Date().toISOString()} code=${code} signal=${signal}\n`, { flag: "a" }));
  return child;
}
async function terminate(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  try {
    if (process.platform === "win32") child.kill("SIGTERM");
    else process.kill(-child.pid, "SIGTERM");
  } catch {
    try { child.kill("SIGTERM"); } catch {}
  }
  await new Promise((resolveWait) => setTimeout(resolveWait, 800));
  if (child.exitCode === null && child.signalCode === null) {
    try {
      if (process.platform === "win32") child.kill("SIGKILL");
      else process.kill(-child.pid, "SIGKILL");
    } catch {
      try { child.kill("SIGKILL"); } catch {}
    }
  }
  child.stdout?.destroy();
  child.stderr?.destroy();
  child.unref();
}
async function wd(
  profile,
  method,
  path,
  body,
  timeoutMs = Number(
    process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_COMMAND_TIMEOUT_MS ?? 30_000,
  ),
) {
  const response = await fetch(`http://127.0.0.1:${profile.driver_port}${path}`, {
    method,
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(timeoutMs),
  });
  const text = await response.text();
  let parsed;
  try { parsed = text ? JSON.parse(text) : {}; } catch { parsed = { raw: text }; }
  if (!response.ok) throw new Error(`${method} ${path} failed ${response.status}: ${text}`);
  if (parsed.value?.error) throw new Error(`${method} ${path} webdriver error ${parsed.value.error}: ${parsed.value.message}`);
  return parsed.value;
}
async function createSession(profile) {
  const value = await wd(profile, "POST", "/session", {
    capabilities: { alwaysMatch: { "tauri:options": { application: appBinary } } },
  });
  profile.session_id = value.sessionId;
  profile.capabilities = value.capabilities;
  return value;
}
async function closeSession(profile) {
  if (profile.session_id) {
    try { await wd(profile, "DELETE", `/session/${profile.session_id}`, undefined, 5_000); } catch {}
    profile.session_id = null;
  }
}
async function exec(profile, script, args = []) {
  return wd(profile, "POST", `/session/${profile.session_id}/execute/sync`, { script, args });
}
async function screenshot(profile, label) {
  const b64 = await wd(profile, "GET", `/session/${profile.session_id}/screenshot`);
  const path = resolve(screenshotDir, `${profile.display_name.toLowerCase()}-${label}.png`);
  writeFileSync(path, Buffer.from(b64, "base64"));
  return { path: rel(path), sha256: sha256IfExists(path) };
}

async function invokeTauriCommand(profile, command, args = {}) {
  return exec(profile, "return window.__TAURI__?.core?.invoke ? window.__TAURI__.core.invoke(arguments[0], arguments[1]) : null;", [command, args]);
}

function base64Url(bytes) {
  return Buffer.from(bytes).toString("base64").replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}
function deriveVoiceSignalKey({ session_id, group_id, channel_id, from_peer_id, to_peer_id }) {
  const [firstPeer, secondPeer] = [from_peer_id, to_peer_id].sort();
  return createHash("sha256")
    .update("discrypt-voice-signal-seal-v1:")
    .update(session_id)
    .update(":")
    .update(group_id)
    .update(":")
    .update(channel_id)
    .update(":")
    .update(firstPeer)
    .update(":")
    .update(secondPeer)
    .digest();
}
function sealVoiceSignalPayloadNode({ session_id, group_id, channel_id, from_peer_id, to_peer_id, candidate, native_media }) {
  const nonce = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", deriveVoiceSignalKey({ session_id, group_id, channel_id, from_peer_id, to_peer_id }), nonce);
  const plaintext = Buffer.from(JSON.stringify({ candidate, native_media }), "utf8");
  const encrypted = Buffer.concat([cipher.update(plaintext), cipher.final(), cipher.getAuthTag()]);
  return `voice-signal-sealed:v1:${base64Url(nonce)}.${base64Url(encrypted)}`;
}
function runtimePeersFromAppState(state) {
  const session = state?.voice_session ?? {};
  if (session.signaling?.local_peer_id && session.signaling?.remote_peer_id) {
    return {
      local: session.signaling.local_peer_id,
      remote: session.signaling.remote_peer_id,
      source: "voice-session-signaling",
    };
  }
  const active = state?.active_context ?? {};
  const groupId = session.group_id || active.group_id;
  const group = groupId
    ? state.groups?.find((item) => item.group_id === groupId)
    : state.groups?.[0];
  const peers = group?.runtime_peers ?? [];
  const local = peers.find((peer) => peer.is_local);
  const remote = peers.find((peer) => !peer.is_local);
  if (!local?.peer_id || !remote?.peer_id) {
    throw new Error(`Could not derive runtime peers from app_state for ${groupId || "active group"}`);
  }
  return { local: local.peer_id, remote: remote.peer_id, source: "persisted-runtime-peers" };
}
async function publishBackendNativeVoiceProof(profile) {
  const state = await invokeTauriCommand(profile, "app_state");
  const session = state?.voice_session;
  if (!session?.joined) throw new Error(`${profile.display_name} has no joined voice session for native proof`);
  const peers = runtimePeersFromAppState(state);
  const started = await invokeTauriCommand(profile, "start_native_voice_media_session", {
    request: {
      session_id: session.session_id,
      local_peer_id: peers.local,
      remote_peer_id: peers.remote,
      muted: false,
      created_at_ms: Date.now(),
    },
  });
  const nativeMedia = started?.native_media;
  if (!nativeMedia) {
    throw new Error(`${profile.display_name} native voice media command did not return native_media`);
  }
  const candidate = {
    candidate: `candidate:native-rust-webrtc-datachannel:${nativeMedia.protected_frames_count}`,
    sdpMid: "native-rust",
    sdpMLineIndex: 0,
  };
  const sealed_payload = sealVoiceSignalPayloadNode({
    session_id: session.session_id,
    group_id: session.group_id,
    channel_id: session.channel_id,
    from_peer_id: peers.local,
    to_peer_id: peers.remote,
    candidate,
    native_media: nativeMedia,
  });
  const queued = await invokeTauriCommand(profile, "publish_voice_signaling_message", {
    request: {
      session_id: session.session_id,
      signal_kind: "candidate",
      sealed_payload,
      signal_id: `tauri-two-profile-e2e-native-rust-${profile.display_name.toLowerCase()}-${Date.now()}`,
      created_at_ms: Date.now(),
    },
  });
  return {
    profile: profile.display_name,
    session_id: session.session_id,
    local_peer_id: peers.local,
    remote_peer_id: peers.remote,
    peer_source: peers.source ?? null,
    local_role: peers.local_role ?? null,
    mic_gain_percent: nativeMedia.mic_gain_percent,
    app_output_volume_percent: nativeMedia.app_output_volume_percent,
    rms_i16: nativeMedia.rms_i16,
    peak_i16: nativeMedia.peak_i16,
    speaking: nativeMedia.speaking,
    opus_payload_bytes: nativeMedia.opus_payload_bytes,
    protected_payload_bytes: nativeMedia.protected_payload_bytes,
    protected_frames_count: nativeMedia.protected_frames_count,
    queued_signaling_status: queued?.voice_session?.signaling?.status_copy ?? null,
  };
}
async function publishBackendNativeVoiceProofs(profiles) {
  const reports = await Promise.all([
    publishBackendNativeVoiceProof(profiles.alice),
    publishBackendNativeVoiceProof(profiles.bob),
  ]);
  manifest.backend_native_voice_proofs = reports;
  writeManifest(manifest.status || "running", {});
  return reports;
}
async function acceptNativeVoiceSignalPayload(profile, signal) {
  if (!signal || signal.signal_kind !== "candidate") return null;
  return exec(profile, String.raw`
    try {
      const message = arguments[0];
      const state = await window.__TAURI__.core.invoke('accept_native_voice_media_signal', {
        request: { signal: message, attached_at_ms: Date.now() },
      });
      const runtime = state?.voice_session?.media_runtime || {};
      const accepted = Boolean(runtime.remote_transport_active || (runtime.remote_audio || []).length);
      const evidence = window.__discryptTauriTwoProfileE2EVoiceEvidence;
      if (evidence && accepted) {
        evidence.mode = 'native_rust_webrtc_datachannel';
        evidence.nativeRustVoiceRuntimeAvailable = true;
        evidence.remoteTrackEvents = (evidence.remoteTrackEvents || 0) + Math.max(1, (runtime.remote_audio || []).length);
        evidence.iceConnected = true;
      }
      return {
        accepted,
        boundary: runtime.boundary || null,
        remote_audio_count: (runtime.remote_audio || []).length,
        status_copy: runtime.status_copy || null,
      };
    } catch (error) {
      return { accepted: false, stage: 'accept_native_voice_media_signal', error: String(error?.message || error), name: String(error?.name || '') };
    }
  `, [signal]);
}
async function acceptPendingNativeVoiceSignals(profile, label) {
  const pending = await invokeTauriCommand(profile, "take_pending_voice_signaling_messages", {
    request: { limit: 50 },
  });
  const signals = Array.isArray(pending?.messages) ? pending.messages : [];
  const report = {
    label,
    profile: profile.display_name,
    pending: signals.length,
    accepted: 0,
    errors: [],
  };
  for (const signal of signals) {
    const accepted = await acceptNativeVoiceSignalPayload(profile, signal).catch((error) => ({
      accepted: false,
      error: error instanceof Error ? error.message : String(error),
    }));
    if (accepted?.accepted) report.accepted += 1;
    else if (accepted?.error) report.errors.push(accepted.error);
  }
  return report;
}
async function appState(profile) {
  return invokeTauriCommand(profile, "app_state", {});
}
async function configureReleaseSmokeAudioPreferences(profiles) {
  const targets = {
    alice: { mic_gain_percent: 155, app_output_volume_percent: 37 },
    bob: { mic_gain_percent: 120, app_output_volume_percent: 64 },
  };
  const reports = {};
  for (const [name, profile] of Object.entries(profiles)) {
    const before = await appState(profile);
    const target = targets[name];
    if (!target) continue;
    const saved = await invokeTauriCommand(profile, "save_preferences", {
      request: {
        theme_id: before?.preferences?.theme_id || "midnight-mono",
        template_id: before?.preferences?.template_id || "dense-chat",
        voice_input_device_id: before?.preferences?.voice_input_device_id || "default",
        voice_output_device_id: before?.preferences?.voice_output_device_id || "default",
        mic_gain_percent: target.mic_gain_percent,
        app_output_volume_percent: target.app_output_volume_percent,
      },
    });
    reports[name] = {
      target,
      before: before?.preferences ?? null,
      after: saved?.preferences ?? null,
      persisted: saved?.preferences?.mic_gain_percent === target.mic_gain_percent &&
        saved?.preferences?.app_output_volume_percent === target.app_output_volume_percent,
    };
  }
  manifest.per59_audio_preferences = reports;
  writeManifest(manifest.status || "running", {});
  return reports;
}
async function readReleaseSmokeAudioPreferences(profiles, label) {
  const reports = {};
  for (const [name, profile] of Object.entries(profiles)) {
    const state = await appState(profile);
    reports[name] = state?.preferences ?? null;
  }
  manifest[`per59_audio_preferences_${label.replace(/\W+/g, "_")}`] = reports;
  writeManifest(manifest.status || "running", {});
  return reports;
}
function providerRuntimeProofed(state) {
  const runtime = state?.transport_status?.find(
    (row) => row?.label === "text/control runtime",
  );
  return runtime?.status === "attached";
}
async function waitForProviderRuntime(profile, label, timeoutMs = 45_000) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    const state = await appState(profile);
    const runtime = state?.transport_status?.find(
      (row) => row?.label === "text/control runtime",
    );
    last = {
      status: runtime?.status ?? null,
      detail: runtime?.detail ?? null,
      probe_status: state?.transport_diagnostics?.data_channel_probe_status ?? null,
      last_command_error: state?.last_command_error ?? null,
    };
    if (providerRuntimeProofed(state)) return state;
    await new Promise((resolveWait) => setTimeout(resolveWait, 500));
  }
  throw new Error(`${profile.display_name} timed out waiting for provider text/control runtime ${label}; last=${JSON.stringify(last)}`);
}
async function startProviderTextControlRuntimePair(profiles, label) {
  const request = { scope_label: `tauri-two-profile-e2e-provider-runtime-${label}`, data_channel_probe: true, adapter_kind: "mqtt" };
  const starts = await Promise.all([
    invokeTauriCommand(profiles.alice, "start_text_session", { request }),
    invokeTauriCommand(profiles.bob, "start_text_session", { request }),
  ]);
  const attaches = await Promise.all([
    invokeTauriCommand(profiles.alice, "attach_text_control_transport_runtime", { request: {} }),
    invokeTauriCommand(profiles.bob, "attach_text_control_transport_runtime", { request: {} }),
  ]);
  const ready = await Promise.all([
    waitForProviderRuntime(profiles.alice, `${label}-alice`),
    waitForProviderRuntime(profiles.bob, `${label}-bob`),
  ]);
  const report = {
    label,
    starts: starts.map((state) => state?.transport_diagnostics ?? null),
    attaches: attaches.map((state) => state?.transport_diagnostics ?? null),
    ready: ready.map((state) => ({
      transport_status: state?.transport_status ?? null,
      diagnostics: state?.transport_diagnostics ?? null,
    })),
  };
  manifest[`provider_text_control_runtime_${label.replace(/\W+/g, "_")}`] = report;
  writeManifest(manifest.status || "running", {});
  return report;
}
async function pumpProviderTextControlFramesOnce(profile, label) {
  const report = await invokeTauriCommand(profile, "pump_text_control_transport_once", {
    request: { limit: 50, operation_timeout_ms: 10_000 },
  });
  return {
    label,
    profile: profile.display_name,
    pending_before: report?.pending_before ?? 0,
    frames_sent: report?.frames_sent ?? 0,
    response_frames_received: report?.response_frames_received ?? 0,
    receipts_applied: report?.receipts_applied ?? 0,
    failures: Array.isArray(report?.failures) ? report.failures : [],
    metrics: report?.metrics ?? null,
    diagnostics: report?.state?.transport_diagnostics ?? null,
  };
}
async function startProviderControlLanePair(profiles, label) {
  const request = {
    scope_label: `tauri-two-profile-e2e-provider-control-lane-${label}`,
    data_channel_probe: false,
    adapter_kind: "mqtt",
  };
  const starts = await Promise.all([
    invokeTauriCommand(profiles.alice, "start_text_session", { request }),
    invokeTauriCommand(profiles.bob, "start_text_session", { request }),
  ]);
  const attaches = await Promise.all([
    invokeTauriCommand(profiles.alice, "attach_broker_control_lane_runtime", { request: { adapter_kind: "mqtt" } }),
    invokeTauriCommand(profiles.bob, "attach_broker_control_lane_runtime", { request: { adapter_kind: "mqtt" } }),
  ]);
  const failures = attaches.flatMap((state, index) => {
    const profile = index === 0 ? profiles.alice : profiles.bob;
    const attached = state?.events?.some((event) => event?.kind === "transport.broker_control_lane_attached");
    if (!state?.last_command_error && attached) return [];
    return [{
      profile: profile.display_name,
      attached,
      last_command_error: state?.last_command_error ?? null,
    }];
  });
  if (failures.length > 0) {
    throw new Error(`Provider control lane ${label} failed to attach: ${JSON.stringify(failures)}`);
  }
  await new Promise((resolveWait) => setTimeout(resolveWait, 500));
  const report = {
    label,
    starts: starts.map((state) => state?.transport_diagnostics ?? null),
    attached: attaches.map((state) => ({
      last_command_error: state?.last_command_error ?? null,
      broker_control_lane_attached: state?.events?.some((event) => event?.kind === "transport.broker_control_lane_attached") ?? false,
    })),
  };
  manifest[`provider_control_lane_runtime_${label.replace(/\W+/g, "_")}`] = report;
  writeManifest(manifest.status || "running", {});
  return report;
}
async function drainProviderControlLaneOnce(profile, label) {
  const report = await invokeTauriCommand(profile, "drain_text_control_inbound_frames", {
    request: { drain_ms: 1_500, operation_timeout_ms: 1_000 },
  });
  return {
    label,
    profile: profile.display_name,
    inbound_frames: report?.response_frames_received ?? 0,
    failures: Array.isArray(report?.failures) ? report.failures : [],
    metrics: report?.metrics ?? null,
  };
}
function providerControlLaneEventCounters(state) {
  const events = Array.isArray(state?.events) ? state.events : [];
  let framesSent = 0;
  let inboundFrames = 0;
  for (const event of events) {
    if (event?.kind === "message.transport_pump") {
      framesSent += Number(String(event.summary ?? "").match(/sent (\d+) frame/)?.[1] ?? 0);
    }
    if (event?.kind === "message.transport_drain") {
      inboundFrames += Number(String(event.summary ?? "").match(/applied (\d+) inbound frame/)?.[1] ?? 0);
    }
  }
  return { frames_sent: framesSent, inbound_frames: inboundFrames };
}
async function providerControlLaneCountersForProfiles(profiles) {
  const states = await Promise.all([
    appState(profiles.alice),
    appState(profiles.bob),
  ]);
  return states.map(providerControlLaneEventCounters);
}
async function pumpProviderControlLaneBidirectional(
  profiles,
  label,
  rounds = 4,
  evidenceBaseline = null,
) {
  const beforeStates = await Promise.all([
    appState(profiles.alice),
    appState(profiles.bob),
  ]);
  const beforeCounters = evidenceBaseline ?? beforeStates.map(providerControlLaneEventCounters);
  const alreadyAttached = beforeStates.every((state) =>
    !state?.last_command_error &&
      state?.events?.some((event) => event?.kind === "transport.broker_control_lane_attached")
  );
  const runtime = alreadyAttached
    ? {
        label,
        reused_ui_managed_runtime: true,
        attached: beforeStates.map((state) => ({
          last_command_error: state?.last_command_error ?? null,
          broker_control_lane_attached: state?.events?.some((event) => event?.kind === "transport.broker_control_lane_attached") ?? false,
          session_manager_started: state?.events?.some((event) => event?.kind === "transport.control_session_started") ?? false,
        })),
      }
    : await startProviderControlLanePair(profiles, label);
  const reports = [];
  for (let round = 0; round < rounds; round += 1) {
    const aliceSent = await pumpProviderTextControlFramesOnce(profiles.alice, `${label}-alice-send-${round}`);
    const bobDrained = await drainProviderControlLaneOnce(profiles.bob, `${label}-bob-drain-${round}`);
    const bobSent = await pumpProviderTextControlFramesOnce(profiles.bob, `${label}-bob-send-${round}`);
    const aliceDrained = await drainProviderControlLaneOnce(profiles.alice, `${label}-alice-drain-${round}`);
    reports.push(aliceSent, bobDrained, bobSent, aliceDrained);
    const activity = aliceSent.frames_sent + bobSent.frames_sent + bobDrained.inbound_frames + aliceDrained.inbound_frames;
    if (round > 0 && activity === 0) break;
  }
  const failures = reports.flatMap((report) => Array.isArray(report.failures) ? report.failures : []);
  const afterStates = await Promise.all([
    appState(profiles.alice),
    appState(profiles.bob),
  ]);
  const afterCounters = afterStates.map(providerControlLaneEventCounters);
  const eventFramesSent = afterCounters.reduce(
    (sum, counter, index) => sum + Math.max(0, counter.frames_sent - beforeCounters[index].frames_sent),
    0,
  );
  const eventInboundFrames = afterCounters.reduce(
    (sum, counter, index) => sum + Math.max(0, counter.inbound_frames - beforeCounters[index].inbound_frames),
    0,
  );
  const reportedFramesSent = reports.reduce((sum, report) => sum + (report.frames_sent || 0), 0);
  const reportedInboundFrames = reports.reduce((sum, report) => sum + (report.inbound_frames || 0), 0);
  const evidence = {
    label,
    runtime,
    reports,
    provider_runtime_used: true,
    provider_runtime_kind: "sealed_broker_control_lane",
    frames_sent: Math.max(reportedFramesSent, eventFramesSent),
    inbound_frames: Math.max(reportedInboundFrames, eventInboundFrames),
    evidence_sources: {
      command_reports: {
        frames_sent: reportedFramesSent,
        inbound_frames: reportedInboundFrames,
      },
      persisted_backend_event_deltas: {
        frames_sent: eventFramesSent,
        inbound_frames: eventInboundFrames,
      },
    },
    manual_command_bridge_used: false,
  };
  if (evidence.frames_sent === 0 || evidence.inbound_frames === 0 || failures.length > 0) {
    throw new Error(`Provider control lane ${label} did not complete cleanly; frames_sent=${evidence.frames_sent} inbound_frames=${evidence.inbound_frames} failures=${JSON.stringify(failures)}`);
  }
  manifest[`provider_control_lane_pump_${label.replace(/\W+/g, "_")}`] = evidence;
  writeManifest(manifest.status || "running", {});
  return evidence;
}
async function pumpProviderTextControlFramesBidirectional(
  profiles,
  label,
  rounds = 6,
  evidenceBaseline = null,
) {
  const beforeStates = await Promise.all([
    appState(profiles.alice),
    appState(profiles.bob),
  ]);
  const beforeCounters = evidenceBaseline ?? beforeStates.map(providerControlLaneEventCounters);
  const runtime = await startProviderTextControlRuntimePair(profiles, label);
  const reports = [];
  for (let round = 0; round < rounds; round += 1) {
    const aliceToBob = await pumpProviderTextControlFramesOnce(profiles.alice, `${label}-a2b-${round}`);
    const bobAccepted = await acceptPendingNativeVoiceSignals(profiles.bob, `${label}-bob-accept-${round}`);
    const bobToAlice = await pumpProviderTextControlFramesOnce(profiles.bob, `${label}-b2a-${round}`);
    const aliceAccepted = await acceptPendingNativeVoiceSignals(profiles.alice, `${label}-alice-accept-${round}`);
    reports.push(aliceToBob, bobAccepted, bobToAlice, aliceAccepted);
    if (
      aliceToBob.frames_sent === 0 &&
      bobToAlice.frames_sent === 0 &&
      bobAccepted.accepted === 0 &&
      aliceAccepted.accepted === 0
    ) {
      break;
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 250));
  }
  const afterStates = await Promise.all([
    appState(profiles.alice),
    appState(profiles.bob),
  ]);
  const afterCounters = afterStates.map(providerControlLaneEventCounters);
  const eventFramesSent = afterCounters.reduce(
    (sum, counter, index) => sum + Math.max(0, counter.frames_sent - beforeCounters[index].frames_sent),
    0,
  );
  const reportedFramesSent = reports.reduce(
    (sum, report) => sum + (report.frames_sent || 0),
    0,
  );
  const evidence = {
    label,
    runtime,
    reports,
    provider_runtime_used: true,
    frames_sent: Math.max(reportedFramesSent, eventFramesSent),
    evidence_sources: {
      command_reports: { frames_sent: reportedFramesSent },
      persisted_backend_event_deltas: { frames_sent: eventFramesSent },
    },
    native_voice_signals_accepted: reports.reduce((sum, report) => sum + (report.accepted || 0), 0),
    manual_command_bridge_used: false,
  };
  const failures = reports.flatMap((report) => Array.isArray(report.failures) ? report.failures : []);
  const acceptanceErrors = reports.flatMap((report) => Array.isArray(report.errors) ? report.errors : []);
  if (evidence.frames_sent === 0 || failures.length > 0 || acceptanceErrors.length > 0) {
    throw new Error(`Provider text/control pump ${label} did not complete cleanly; frames_sent=${evidence.frames_sent} failures=${JSON.stringify(failures)} acceptance_errors=${JSON.stringify(acceptanceErrors)}`);
  }
  manifest[`provider_text_control_pump_${label.replace(/\W+/g, "_")}`] = evidence;
  writeManifest(manifest.status || "running", {});
  return evidence;
}
async function waitForProfileState(profile, label, predicate, timeoutMs = 90_000) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    const state = readJsonIfExists(profile.state_path);
    last = state?.parse_error ? state.parse_error : predicate(state);
    if (last === true) return state;
    await new Promise((resolveWait) => setTimeout(resolveWait, 500));
  }
  manifest[`${profile.display_name.toLowerCase()}_${label.replace(/\W+/g, '_')}_last`] = last;
  throw new Error(`${profile.display_name} timed out waiting for ${label}; last=${JSON.stringify(last)}`);
}

function hasOpenMlsAdmission(state) {
  const groups = Array.isArray(state?.groups) ? state.groups : [];
  const handles = Array.isArray(state?.openmls_groups) ? state.openmls_groups : [];
  const events = Array.isArray(state?.events) ? state.events : [];
  const groupId = groups.find((group) => group?.name === "Two Profile WebDriver Lab")?.group_id;
  if (!groupId) return { group_id: null, handles: handles.length, joined: false };
  const handle = handles.find((entry) => entry?.group_id === groupId && Number(entry?.epoch ?? -1) >= 1);
  return Boolean(handle) || { group_id: groupId, handles: handles.map((entry) => ({ group_id: entry?.group_id, epoch: entry?.epoch })), joined: events.some((event) => event?.kind === "mls.admission_welcome_joined") };
}

function pendingAdmissionRequest(state) {
  const groups = Array.isArray(state?.groups) ? state.groups : [];
  const group = groups.find((candidate) => candidate?.name === "Two Profile WebDriver Lab");
  const request = group?.admission_requests?.find((candidate) => candidate?.status === "pending");
  if (!group || !request) {
    return {
      group_id: group?.group_id ?? null,
      pending_count: group?.admission_requests?.filter((candidate) => candidate?.status === "pending").length ?? 0,
    };
  }
  return {
    group_id: group.group_id,
    request_id: request.request_id,
    display_name: request.display_name,
    key_package_bytes: Array.isArray(request.key_package) ? request.key_package.length : 0,
  };
}

async function approvePendingAdmissionThroughUi(profile) {
  const deadline = Date.now() + 60_000;
  let pending = null;
  while (Date.now() < deadline) {
    const state = readJsonIfExists(profile.state_path);
    pending = pendingAdmissionRequest(state);
    if (pending?.group_id && pending?.request_id) break;
    await new Promise((resolveWait) => setTimeout(resolveWait, 500));
  }
  if (!pending?.group_id || !pending?.request_id) {
    manifest.openmls_admission_owner_approval = { approved: false, pending };
    throw new Error(`${profile.display_name} did not persist a pending OpenMLS admission request; last=${JSON.stringify(pending)}`);
  }
  await click(profile, "Pending requests");
  await waitUntil(
    profile,
    `pending admission card ${pending.request_id}`,
    "return document.querySelector(`[data-testid=\"admission-request-${arguments[0]}\"]`) !== null;",
    [pending.request_id],
  );
  const clicked = await exec(
    profile,
    "const button = document.querySelector(`[data-testid=\"admission-approve-${arguments[0]}\"]`); if (!button) return false; button.click(); return true;",
    [pending.request_id],
  );
  if (!clicked) {
    throw new Error(`${profile.display_name} could not click the admission approval UI for ${pending.request_id}`);
  }
  const approvedState = await waitForProfileState(
    profile,
    `approved admission ${pending.request_id}`,
    (state) => state?.groups?.some((group) =>
      group?.group_id === pending.group_id &&
      group?.admission_requests?.some((request) => request?.request_id === pending.request_id && request?.status === "approved")
    ) ?? false,
    60_000,
  );
  const error = approvedState?.last_command_error ?? null;
  const approved = !error && approvedState?.groups?.some((group) =>
    group?.group_id === pending.group_id &&
    group?.admission_requests?.some((request) => request?.request_id === pending.request_id && request?.status === "approved")
  );
  manifest.openmls_admission_owner_approval = {
    approved,
    pending,
    last_command_error: error,
    interaction: "ui_click",
    selector: `[data-testid=\"admission-approve-${pending.request_id}\"]`,
  };
  writeManifest(manifest.status || "running", {});
  if (!approved) {
    throw new Error(`${profile.display_name} failed to approve OpenMLS admission; result=${JSON.stringify(manifest.openmls_admission_owner_approval)}`);
  }
}

async function waitForMaybe(profile, label, script, args = [], timeoutMs = 90_000) {
  const deadline = Date.now() + timeoutMs;
  let last = false;
  while (Date.now() < deadline) {
    try {
      const result = await exec(profile, script, args);
      if (result) return true;
      last = result;
    } catch (error) {
      last = error instanceof Error ? error.message : String(error);
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 1000));
  }
  manifest[`${label.replace(/\W+/g, '_')}_last`] = last;
  return false;
}
async function reloadProfile(profile) {
  await exec(profile, "location.reload(); return true;");
  await waitUntil(profile, "post-reload app shell", "return /Local-first workspace|Set up your local discrypt profile|Local profile ready|Start a private space|Two Profile WebDriver Lab/i.test(document.body.innerText)", [], 30_000);
}

async function assertNoAdmissionDecisionApplyFailure(profile, label) {
  const state = await appState(profile);
  const error = state?.last_command_error ?? null;
  const failed = error?.command === "handle_text_control_frame" && error?.code === "admission_decision_apply_failed";
  manifest[`admission_decision_apply_failure_${label}`] = {
    profile: profile.display_name,
    failed,
    last_command_error: error,
  };
  writeManifest(manifest.status || "running", {});
  if (failed) {
    throw new Error(`${profile.display_name} failed to apply admission decision before Welcome unlock: ${JSON.stringify(error)}`);
  }
}

async function waitForAdmissionUnlockedUi(profile) {
  await waitUntil(profile, "post-admission unlocked composer", String.raw`
    const text = document.body.innerText || '';
    const waiting = /Waiting for owner\/staff approval before protected messages can be sent/i.test(text);
    const messageInputs = [...document.querySelectorAll('input, textarea')];
    const messageEditable = messageInputs.some((el) => {
      const style = window.getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      const visible = style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
      const label = [el.getAttribute('aria-label'), el.getAttribute('placeholder'), el.getAttribute('data-testid')]
        .filter(Boolean)
        .join(' ')
        .replace(/\s+/g, ' ')
        .trim();
      return visible && !el.disabled && !el.readOnly && /Message|Send a message/i.test(label);
    });
    return /Two Profile WebDriver Lab/i.test(text) && !waiting && messageEditable;
  `, [], 60_000);
}

async function waitUntil(profile, label, script, args = [], timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    try {
      const result = await exec(profile, script, args);
      if (result) return result;
      last = result;
    } catch (error) {
      last = error instanceof Error ? error.message : String(error);
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 300));
  }
  throw new Error(`${profile.display_name} timed out waiting for ${label}; last=${JSON.stringify(last)}`);
}
const domHelpers = String.raw`
const norm = (value) => String(value || '').replace(/\s+/g, ' ').trim();
const visible = (el) => {
  const style = window.getComputedStyle(el);
  const rect = el.getBoundingClientRect();
  return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
};
const textMatches = (text, pattern, flags = 'i') => new RegExp(pattern, flags).test(norm(text));
const findLabelledControl = (label) => {
  const controls = [...document.querySelectorAll('input, textarea, select')];
  const byAria = controls.find((el) => visible(el) && norm(el.getAttribute('aria-label')).toLowerCase() === label.toLowerCase());
  if (byAria) return byAria;
  for (const labelEl of [...document.querySelectorAll('label')]) {
    if (!visible(labelEl) || norm(labelEl.textContent).toLowerCase() !== label.toLowerCase()) continue;
    const forId = labelEl.getAttribute('for');
    if (forId) {
      const el = document.getElementById(forId);
      if (el) return el;
    }
    const nested = labelEl.querySelector('input, textarea, select');
    if (nested) return nested;
  }
  return null;
};
const setControlValue = (label, value) => {
  const el = findLabelledControl(label);
  if (!el) return false;
  el.focus();
  const proto = el instanceof HTMLTextAreaElement
    ? HTMLTextAreaElement.prototype
    : el instanceof HTMLSelectElement
      ? HTMLSelectElement.prototype
      : HTMLInputElement.prototype;
  const descriptor = Object.getOwnPropertyDescriptor(proto, 'value');
  if (descriptor?.set) descriptor.set.call(el, value);
  else el.value = value;
  el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: value }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
  el.blur();
  return true;
};
const accessibleText = (el) => norm([
  el.textContent,
  el.getAttribute('aria-label'),
  el.getAttribute('title'),
  el.getAttribute('data-testid'),
  el.getAttribute('placeholder')
].filter(Boolean).join(' '));
const clickButton = (pattern, flags = 'i', last = false) => {
  const candidates = [...document.querySelectorAll('button, [role="button"], [role="switch"]')]
    .filter((el) => visible(el) && !el.disabled && textMatches(accessibleText(el), pattern, flags));
  const el = last ? candidates.at(-1) : candidates[0];
  if (!el) return false;
  el.scrollIntoView({ block: 'center', inline: 'center' });
  el.click();
  return true;
};
const clickText = (pattern, flags = 'i') => {
  const candidates = [...document.querySelectorAll('button, [role="button"], a, [tabindex], [data-testid]')]
    .filter((el) => visible(el) && textMatches(accessibleText(el), pattern, flags));
  const el = candidates[0];
  if (!el) return false;
  el.scrollIntoView({ block: 'center', inline: 'center' });
  el.click();
  return true;
};
const contextClickText = (pattern, flags = 'i') => {
  const candidates = [...document.querySelectorAll('button, [role="button"], a, [tabindex], [data-testid]')]
    .filter((el) => visible(el) && textMatches(accessibleText(el), pattern, flags));
  const el = candidates[0];
  if (!el) return false;
  el.scrollIntoView({ block: 'center', inline: 'center' });
  const rect = el.getBoundingClientRect();
  const options = {
    bubbles: true,
    cancelable: true,
    button: 2,
    buttons: 2,
    clientX: Math.round(rect.left + rect.width / 2),
    clientY: Math.round(rect.top + rect.height / 2),
  };
  const PointerCtor = window.PointerEvent || MouseEvent;
  el.dispatchEvent(new PointerCtor('pointerdown', options));
  el.dispatchEvent(new MouseEvent('mousedown', options));
  el.dispatchEvent(new MouseEvent('contextmenu', options));
  el.dispatchEvent(new MouseEvent('mouseup', options));
  el.dispatchEvent(new PointerCtor('pointerup', options));
  return true;
};
const debugVisibleActions = () => [...document.querySelectorAll('button, [role="button"], a, [tabindex], [data-testid]')]
  .filter((el) => visible(el))
  .map((el) => accessibleText(el) + (el.disabled ? ' [disabled]' : '') + (el.getAttribute('aria-disabled') === 'true' ? ' [aria-disabled]' : ''))
  .filter(Boolean)
  .slice(0, 120);
`;
async function bodyText(profile) {
  return exec(profile, "return document.body.innerText;");
}
async function visibleActions(profile) {
  try { return await exec(profile, `${domHelpers}; return debugVisibleActions();`); } catch { return []; }
}
async function click(profile, pattern, { last = false, timeoutMs = 5_000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let lastActions = [];
  while (Date.now() < deadline) {
    const ok = await exec(profile, `${domHelpers}; return clickButton(arguments[0], 'i', arguments[1]) || clickText(arguments[0], 'i');`, [pattern, last]);
    if (ok) return;
    lastActions = await visibleActions(profile);
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 200));
  }
  throw new Error(`${profile.display_name} could not click button matching ${pattern}; visible actions=${JSON.stringify(lastActions)}`);
}
async function clickText(profile, pattern) {
  const ok = await exec(profile, `${domHelpers}; return clickText(arguments[0], 'i');`, [pattern]);
  if (!ok) throw new Error(`${profile.display_name} could not click text matching ${pattern}; visible actions=${JSON.stringify(await visibleActions(profile))}`);
}
async function contextClickText(profile, pattern) {
  const ok = await exec(profile, `${domHelpers}; return contextClickText(arguments[0], 'i');`, [pattern]);
  if (!ok) throw new Error(`${profile.display_name} could not context-click text matching ${pattern}; visible actions=${JSON.stringify(await visibleActions(profile))}`);
}
async function fill(profile, label, value) {
  const ok = await exec(profile, `${domHelpers}; return setControlValue(arguments[0], arguments[1]);`, [label, value]);
  if (!ok) throw new Error(`${profile.display_name} could not fill ${label}`);
}
async function setupProfile(profile) {
  await waitUntil(profile, "setup screen", "return /set up your local discrypt profile/i.test(document.body.innerText)");
  await fill(profile, "Display name", profile.display_name);
  await fill(profile, "Device name", profile.device_name);
  await click(profile, "create new user");
  await waitUntil(
    profile,
    "profile ready or trust setup screen",
    "return /finish the local trust setup|local profile ready|start a private space/i.test(document.body.innerText)",
  );
}
async function createGroupInvite(profile) {
  await click(profile, "Create (a )?group");
  await fill(profile, "Group name", "Two Profile WebDriver Lab");
  await fill(profile, "Signaling endpoint", localMqttEndpoint);
  await fill(profile, "STUN servers", "");
  await click(profile, "^Create group$", { last: true });
  await waitUntil(profile, "created group", "return /Two Profile WebDriver Lab/i.test(document.body.innerText)");
  await waitForProfileState(
    profile,
    "owner admission control lane",
    (state) => state?.events?.some(
      (event) => event?.kind === "transport.broker_control_lane_attached",
    ),
    30_000,
  );
  await contextClickText(profile, "Open Two Profile WebDriver Lab group");
  await click(profile, "Create invite");
  await click(profile, "Create invite for Two Profile WebDriver Lab");
  return waitUntil(profile, "invite URL", "const m = document.body.innerText.match(new RegExp('discrypt:\\\\/\\\\/join\\\\/v1\\\\/\\\\S+')); return m && m[0];");
}
async function joinGroup(profile, invite) {
  await click(profile, "Join with invite");
  await fill(profile, "Invite URL or code", invite);
  await fill(profile, "Local label", "Two Profile WebDriver Lab");
  await click(profile, "join/open group");
  await waitUntil(profile, "joined group", "return /Two Profile WebDriver Lab/i.test(document.body.innerText)");
}
async function sendGroupMessage(profile, message) {
  await clickText(profile, "#general");
  await waitUntil(profile, "general channel", "return /#general/i.test(document.body.innerText)");
  await fill(profile, "Message", message);
  await click(profile, "Send message");
  await waitUntil(profile, `message ${message}`, "return document.body.innerText.includes(arguments[0]);", [message]);
}
async function installVoiceHarness(profile) {
  await exec(profile, String.raw`
    const profileName = arguments[0];
    const forceNativeRustVoice = Boolean(arguments[1]);
    Object.defineProperty(window, '__discryptTauriTwoProfileE2EForceNativeRustVoice', { configurable: true, value: forceNativeRustVoice });
    try {
      window.localStorage?.setItem('discrypt:tauri-two-profile-e2e:force-native-rust-voice', forceNativeRustVoice ? '1' : '0');
      window.localStorage?.setItem('discrypt:tauri-two-profile-e2e:voice-harness', '1');
    } catch {}
    const evidence = {
      mode: 'uninitialized',
      getUserMediaCalls: 0,
      localAudioTracksSent: 0,
      remoteTrackEvents: 0,
      playbackAttachments: 0,
      peerConnectionsClosed: 0,
      peerConnectionsConstructed: 0,
      iceConnected: false,
      trackEnabled: true,
      trackStopCount: 0,
      nativeAudioContextAvailable: typeof (window.AudioContext || window.webkitAudioContext) === 'function',
      nativeRTCPeerConnectionAvailable: typeof window.RTCPeerConnection === 'function',
      nativeGeneratedAudioTrackAvailable: false,
      syntheticFallback: false,
      fallbackReason: null,
    };
    Object.defineProperty(window, '__discryptTauriTwoProfileE2EVoiceEvidence', { configurable: true, value: evidence });
    const audioDescriptor = Object.getOwnPropertyDescriptor(HTMLMediaElement.prototype, 'srcObject');
    if (audioDescriptor?.set) {
      Object.defineProperty(HTMLMediaElement.prototype, 'srcObject', {
        configurable: true,
        get: audioDescriptor.get,
        set(value) {
          if (this.tagName === 'AUDIO' && value && this.dataset?.testid === 'voice-remote-audio') evidence.playbackAttachments += 1;
          return audioDescriptor.set.call(this, value);
        },
      });
    }
    const NativeAudioContext = window.AudioContext || window.webkitAudioContext;
    const NativeRTCPeerConnection = window.RTCPeerConnection;
    if (typeof NativeAudioContext === 'function' && typeof NativeRTCPeerConnection === 'function') {
      try {
        const ctx = new NativeAudioContext();
        const oscillator = ctx.createOscillator();
        const gain = ctx.createGain();
        const destination = ctx.createMediaStreamDestination();
        oscillator.frequency.value = 440 + Math.floor(Math.random() * 220);
        gain.gain.value = 0.03;
        oscillator.connect(gain);
        gain.connect(destination);
        oscillator.start();
        const generatedTrack = destination.stream.getAudioTracks()[0];
        if (!generatedTrack) throw new Error('native AudioContext did not expose a generated audio track');
        evidence.nativeGeneratedAudioTrackAvailable = true;
        const originalStop = generatedTrack.stop.bind(generatedTrack);
        Object.defineProperty(generatedTrack, 'enabled', {
          configurable: true,
          get() { return evidence.trackEnabled; },
          set(value) { evidence.trackEnabled = Boolean(value); },
        });
        generatedTrack.stop = () => { evidence.trackStopCount += 1; evidence.trackEnabled = false; try { oscillator.stop(); } catch {} try { ctx.close(); } catch {} originalStop(); };
        Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: { getUserMedia: async () => { evidence.mode = 'native_rtc_generated_audio'; evidence.getUserMediaCalls += 1; await ctx.resume?.(); return destination.stream; }, enumerateDevices: async () => [{ kind: 'audioinput', deviceId: profileName + '-generated-mic', label: profileName + ' generated audio source', groupId: profileName, toJSON: () => ({}) }, { kind: 'audiooutput', deviceId: profileName + '-speaker', label: profileName + ' speaker', groupId: profileName, toJSON: () => ({}) }] } });
        function ObservedPeerConnection(config) {
          const pc = new NativeRTCPeerConnection(config);
          evidence.peerConnectionsConstructed += 1;
          pc.addEventListener?.('track', (event) => {
            if (event.track?.kind === 'audio') evidence.remoteTrackEvents += 1;
          });
          pc.addEventListener?.('connectionstatechange', () => {
            evidence.iceConnected ||= pc.connectionState === 'connected' || pc.connectionState === 'completed';
          });
          pc.addEventListener?.('iceconnectionstatechange', () => {
            evidence.iceConnected ||= pc.iceConnectionState === 'connected' || pc.iceConnectionState === 'completed';
          });
          const addTrack = pc.addTrack.bind(pc);
          const close = pc.close.bind(pc);
          return new Proxy(pc, {
            get(target, prop) {
              if (prop === 'addTrack') return (track, stream) => { if (track?.kind === 'audio') evidence.localAudioTracksSent += 1; return addTrack(track, stream); };
              if (prop === 'close') return () => { evidence.peerConnectionsClosed += 1; return close(); };
              const value = target[prop];
              return typeof value === 'function' ? value.bind(target) : value;
            },
            set(target, prop, value) { target[prop] = value; return true; },
          });
        }
        ObservedPeerConnection.prototype = NativeRTCPeerConnection.prototype;
        Object.defineProperty(window, 'RTCPeerConnection', { configurable: true, value: ObservedPeerConnection });
        return true;
      } catch (error) {
        evidence.fallbackReason = error instanceof Error ? error.message : String(error);
      }
    } else {
      evidence.fallbackReason = evidence.nativeRTCPeerConnectionAvailable
        ? 'native AudioContext unavailable for generated audio'
        : 'native RTCPeerConnection unavailable in Tauri WebView';
    }
    evidence.fallbackReason ||= typeof NativeRTCPeerConnection !== 'function'
      ? 'RTCPeerConnection is unavailable in this Tauri/WebKit WebView'
      : 'AudioContext generated-audio MediaStream support is unavailable in this Tauri/WebKit WebView';
    if (arguments[1]) {
      evidence.mode = 'native_rtc_unavailable';
      return true;
    }
    evidence.mode = 'synthetic_peerconnection_fallback';
    evidence.syntheticFallback = true;
    const track = { id: profileName + '-track', kind: 'audio', label: profileName + ' microphone', readyState: 'live', get enabled() { return evidence.trackEnabled; }, set enabled(v) { evidence.trackEnabled = Boolean(v); }, stop() { evidence.trackStopCount += 1; evidence.trackEnabled = false; } };
    const stream = { id: profileName + '-stream', getTracks: () => [track], getAudioTracks: () => [track] };
    Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: { getUserMedia: async () => { evidence.getUserMediaCalls += 1; return stream; }, enumerateDevices: async () => [{ kind: 'audioinput', deviceId: profileName + '-mic', label: profileName + ' mic', groupId: profileName, toJSON: () => ({}) }, { kind: 'audiooutput', deviceId: profileName + '-speaker', label: profileName + ' speaker', groupId: profileName, toJSON: () => ({}) }] } });
    class TauriTwoProfileE2EAudioContext { createMediaStreamSource() { return { connect() {}, disconnect() {} }; } createAnalyser() { return { fftSize: 1024, getByteTimeDomainData: (buf) => buf.fill(180), disconnect() {} }; } resume() { return Promise.resolve(); } close() { return Promise.resolve(); } }
    Object.defineProperty(window, 'AudioContext', { configurable: true, value: TauriTwoProfileE2EAudioContext });
    class TauriTwoProfileE2EPeerConnection { constructor() { evidence.peerConnectionsConstructed += 1; this.connectionState = 'new'; this.iceConnectionState = 'new'; this.ontrack = null; this.onicecandidate = null; } addTrack(localTrack, localStream) { if (localTrack?.kind === 'audio') evidence.localAudioTracksSent += 1; queueMicrotask(() => { this.connectionState = 'connected'; this.iceConnectionState = 'connected'; evidence.iceConnected = true; const remoteTrack = { id: arguments[0] + '-remote-track', kind: 'audio', label: arguments[0] + ' remote', readyState: 'live', enabled: true, addEventListener() {}, removeEventListener() {} }; const remoteStream = { id: arguments[0] + '-remote-stream', getTracks: () => [remoteTrack], getAudioTracks: () => [remoteTrack] }; evidence.remoteTrackEvents += 1; this.ontrack?.({ track: remoteTrack, streams: [remoteStream], receiver: { track: remoteTrack } }); this.onicecandidate?.({ candidate: null }); }); return { track: localTrack, stream: localStream }; } createOffer() { return Promise.resolve({ type: 'offer', sdp: 'v=0\r\na=mid:audio\r\na=sendrecv\r\n' }); } createAnswer() { return Promise.resolve({ type: 'answer', sdp: 'v=0\r\na=mid:audio\r\na=sendrecv\r\n' }); } setLocalDescription(desc) { this.localDescription = desc; return Promise.resolve(); } setRemoteDescription(desc) { this.remoteDescription = desc; return Promise.resolve(); } addIceCandidate() { return Promise.resolve(); } getStats() { return Promise.resolve(new Map([['inbound-audio', { type: 'inbound-rtp', kind: 'audio', mediaType: 'audio', packetsReceived: 12, audioLevel: 0.2 }]])); } getSenders() { return [{ track }]; } close() { evidence.peerConnectionsClosed += 1; this.connectionState = 'closed'; this.iceConnectionState = 'closed'; } }
    Object.defineProperty(window, 'RTCPeerConnection', { configurable: true, value: TauriTwoProfileE2EPeerConnection });
    return true;
  `, [profile.display_name.toLowerCase(), requireNativeVoice || disableSyntheticVoiceFallback]);
}

const alreadyJoinedVoiceUiPredicate = `return /Leave voice call|Mute|joined/i.test(document.body.innerText) && (/You · you/i.test(document.body.innerText) || document.querySelector('[data-testid="voice-local-participant"]') !== null);`;

async function readAlreadyJoinedNativeVoice(profile) {
  const state = await appState(profile);
  const nativeMediaStarted = state?.events?.some((event) => event?.kind === "voice.native_media_started");
  const nativeMediaReceived = state?.events?.some((event) => event?.kind === "voice.native_media_received");
  const joinedUi = await exec(profile, alreadyJoinedVoiceUiPredicate);
  return {
    ok: Boolean(requireNativeVoice && nativeMediaStarted && nativeMediaReceived && !state?.last_command_error && joinedUi),
    state,
    nativeMediaStarted,
    nativeMediaReceived,
    joinedUi,
  };
}

async function waitForAlreadyJoinedNativeVoice(profile, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await readAlreadyJoinedNativeVoice(profile);
    if (last.ok) {
      await waitUntil(profile, "already joined native voice media", alreadyJoinedVoiceUiPredicate);
      return true;
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 500));
  }
  manifest[`${profile.display_name.toLowerCase()}_already_joined_native_voice_last`] = {
    native_media_started: Boolean(last?.nativeMediaStarted),
    native_media_received: Boolean(last?.nativeMediaReceived),
    joined_ui: Boolean(last?.joinedUi),
    last_command_error: last?.state?.last_command_error ?? null,
  };
  writeManifest(manifest.status || "running", {});
  return false;
}

async function joinVoice(profile) {
  await click(profile, "Voice Lobby");
  if (requireNativeVoice && await waitForAlreadyJoinedNativeVoice(profile)) {
    return;
  }
  const before = await appState(profile);
  if (before?.voice_session?.joined) {
    if (
      requireNativeVoice &&
      before?.last_command_error?.command === "start_native_voice_media_session"
    ) {
      const commandError = before.last_command_error;
      throw new Error(`${profile.display_name} native voice media failed after join; ${commandError.command || "unknown command"} ${commandError.code || "unknown_code"}: ${commandError.message || "no message"}`);
    }
    if (before?.voice_session?.media_runtime?.local_capture_active) {
      if (requireNativeVoice) {
        if (await waitForAlreadyJoinedNativeVoice(profile, 45_000)) {
          return;
        }
        const after = await appState(profile);
        const commandError = after?.last_command_error;
        const detail = commandError
          ? `${commandError.command || "unknown command"} ${commandError.code || "unknown_code"}: ${commandError.message || "no message"}`
          : "native media start/receive evidence did not settle";
        throw new Error(`${profile.display_name} is already joined locally but native voice media is not active; ${detail}`);
      }
      await waitUntil(profile, "already joined local voice participant", alreadyJoinedVoiceUiPredicate);
      return;
    }
    if (requireNativeVoice) {
      const commandError = before?.last_command_error;
      const detail = commandError
        ? `${commandError.command || "unknown command"} ${commandError.code || "unknown_code"}: ${commandError.message || "no message"}`
        : "no backend command error was recorded";
      throw new Error(`${profile.display_name} is already joined but native voice media is not active; ${detail}`);
    }
  }
  await click(profile, "join call");
  await waitUntil(profile, "local voice participant", `return /You · you/i.test(document.body.innerText) || document.querySelector('[data-testid="voice-local-participant"]') !== null;`);
  if (requireNativeVoice && !await waitForAlreadyJoinedNativeVoice(profile, 45_000)) {
    const after = await appState(profile);
    const commandError = after?.last_command_error;
    const detail = commandError
      ? `${commandError.command || "unknown command"} ${commandError.code || "unknown_code"}: ${commandError.message || "no message"}`
      : "native media start/receive evidence did not settle";
    throw new Error(`${profile.display_name} joined voice but native voice media is not active; ${detail}`);
  }
}
async function leaveVoice(profile) {
  await click(profile, "Leave voice call");
  return waitForLeftVoice(profile);
}
async function readLeftVoiceState(profile) {
  const persistedState = readJsonIfExists(profile.state_path);
  const persistedEvents = Array.isArray(persistedState?.events) ? persistedState.events : [];
  const voiceLeftEvent = persistedEvents.some((event) => event?.kind === "voice.left");
  const voiceSessionCleared = Boolean(persistedState && !persistedState.parse_error && !persistedState.voice_session);
  return {
    ok: Boolean(voiceSessionCleared && voiceLeftEvent),
    source: "persisted_app_state",
    voice_session_cleared: voiceSessionCleared,
    voice_left_event: voiceLeftEvent,
    left_ui: null,
    webview_track_stopped: null,
    last_command_error: persistedState?.last_command_error ?? null,
    parse_error: persistedState?.parse_error ?? null,
  };
}
async function waitForLeftVoice(profile, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await readLeftVoiceState(profile);
    if (last.ok) return { profile: profile.display_name, ...last };
    await new Promise((resolveWait) => setTimeout(resolveWait, 500));
  }
  manifest[`${profile.display_name.toLowerCase()}_left_voice_last`] = last;
  writeManifest(manifest.status || "running", {});
  throw new Error(`${profile.display_name} timed out waiting for left voice; last=${JSON.stringify(last)}`);
}
async function adjustRemoteParticipantVolume(profile, volume) {
  const state = await appState(profile);
  const session = state?.voice_session;
  const participant = session?.participants?.find((item) => item?.role === "remote" && item?.id);
  if (!session?.session_id || !participant?.id) {
    return {
      profile: profile.display_name,
      volume,
      changed: false,
      reason: "No backend-admitted remote voice participant was visible",
    };
  }
  const updated = await invokeTauriCommand(profile, "set_speaker_volume", {
    request: {
      session_id: session.session_id,
      participant_id: participant.id,
      volume,
    },
  });
  const after = updated?.voice_session?.participants?.find((item) => item?.id === participant.id);
  return {
    profile: profile.display_name,
    participant_id: participant.id,
    volume,
    changed: after?.volume === volume,
    after_volume: after?.volume ?? null,
    last_command_error: updated?.last_command_error ?? null,
  };
}
async function adjustRemoteParticipantVolumes(profiles) {
  const reports = {
    alice: await adjustRemoteParticipantVolume(profiles.alice, 41),
    bob: await adjustRemoteParticipantVolume(profiles.bob, 73),
  };
  manifest.per59_remote_participant_volume = reports;
  writeManifest(manifest.status || "running", {});
  return reports;
}
async function voiceCallFlow(profiles) {
  await Promise.all([installVoiceHarness(profiles.alice), installVoiceHarness(profiles.bob)]);
  if (requireNativeVoice) {
    const nativeProbe = {
      alice: await exec(profiles.alice, "return window.__discryptTauriTwoProfileE2EVoiceEvidence || null;"),
      bob: await exec(profiles.bob, "return window.__discryptTauriTwoProfileE2EVoiceEvidence || null;"),
    };
    // Linux Tauri/WebKit may not expose WebView RTCPeerConnection. In that case
    // continue into the Rust-native backend media path; checkpoint eligibility is
    // decided below from native_rust_webrtc_datachannel evidence, never from the
    // synthetic WebView peer-connection fallback.
    if (nativeProbe.alice?.mode === "synthetic_peerconnection_fallback" || nativeProbe.bob?.mode === "synthetic_peerconnection_fallback") {
      throw new Error(`Synthetic WebView voice fallback is not permitted for native voice proof: ${JSON.stringify(nativeProbe)}`);
    }
  }
  const voiceSignalingProviderBaseline = await providerControlLaneCountersForProfiles(profiles);
  await Promise.all([joinVoice(profiles.alice), joinVoice(profiles.bob)]);
  const backendNativeProofs = await publishBackendNativeVoiceProofs(profiles);
  const providerVoiceSignaling = await pumpProviderTextControlFramesBidirectional(
    profiles,
    "voice-signaling-provider-runtime",
    8,
    voiceSignalingProviderBaseline,
  );
  for (let round = 0; round < 12; round += 1) {
    const observed = await Promise.all([
      waitForMaybe(profiles.alice, "remote voice audio on alice", "return document.querySelector('[data-testid=\"voice-remote-audio-boundary\"]') !== null || (window.__discryptTauriTwoProfileE2EVoiceEvidence?.remoteTrackEvents || 0) > 0;", [], 1500),
      waitForMaybe(profiles.bob, "remote voice audio on bob", "return document.querySelector('[data-testid=\"voice-remote-audio-boundary\"]') !== null || (window.__discryptTauriTwoProfileE2EVoiceEvidence?.remoteTrackEvents || 0) > 0;", [], 1500),
    ]);
    if (observed.every(Boolean)) break;
  }
  const remoteParticipantVolume = await adjustRemoteParticipantVolumes(profiles);
  await clickText(profiles.alice, "Voice Lobby");
  await clickText(profiles.bob, "Voice Lobby");
  const beforeLeave = {
    alice: await exec(profiles.alice, "return { evidence: window.__discryptTauriTwoProfileE2EVoiceEvidence || null, remoteAudio: document.querySelectorAll('[data-testid=\"voice-remote-audio\"]').length, remoteBoundaries: document.querySelectorAll('[data-testid=\"voice-remote-audio-boundary\"]').length, text: document.body.innerText };"),
    bob: await exec(profiles.bob, "return { evidence: window.__discryptTauriTwoProfileE2EVoiceEvidence || null, remoteAudio: document.querySelectorAll('[data-testid=\"voice-remote-audio\"]').length, remoteBoundaries: document.querySelectorAll('[data-testid=\"voice-remote-audio-boundary\"]').length, text: document.body.innerText };"),
  };
  await click(profiles.alice, "^Mute$");
  await waitUntil(profiles.alice, "muted microphone", "return /Unmute|Muted/i.test(document.body.innerText) || window.__discryptTauriTwoProfileE2EVoiceEvidence?.trackEnabled === false;");
  const afterMute = {
    alice: await exec(profiles.alice, "return { evidence: window.__discryptTauriTwoProfileE2EVoiceEvidence || null, text: document.body.innerText };"),
    bob: await exec(profiles.bob, "return { evidence: window.__discryptTauriTwoProfileE2EVoiceEvidence || null, text: document.body.innerText };"),
  };
  await click(profiles.alice, "^Unmute$");
  const [aliceLeaveCleanup, bobLeaveCleanup] = await Promise.all([
    leaveVoice(profiles.alice),
    leaveVoice(profiles.bob),
  ]);
  await reloadProfile(profiles.alice);
  await reloadProfile(profiles.bob);
  const reloadedAudioPreferences = await readReleaseSmokeAudioPreferences(profiles, "after_voice_leave_reload");
  return {
    alice: await exec(profiles.alice, "return window.__discryptTauriTwoProfileE2EVoiceEvidence || null;"),
    bob: await exec(profiles.bob, "return window.__discryptTauriTwoProfileE2EVoiceEvidence || null;"),
    backend_native_proofs: backendNativeProofs,
    per56_provider_runtime_voice_signaling: providerVoiceSignaling,
    remote_participant_volume: remoteParticipantVolume,
    reloaded_audio_preferences: reloadedAudioPreferences,
    before_leave: beforeLeave,
    after_mute: afterMute,
    leave_cleanup: {
      alice: aliceLeaveCleanup,
      bob: bobLeaveCleanup,
    },
  };
}
async function voiceFlow(profile) {
  await installVoiceHarness(profile);
  await joinVoice(profile);
  await click(profile, "^Mute$");
  await waitUntil(profile, "muted microphone", "return /Unmute|Muted/i.test(document.body.innerText) || window.__discryptTauriTwoProfileE2EVoiceEvidence?.trackEnabled === false;");
  await click(profile, "^Unmute$");
  await leaveVoice(profile);
  return exec(profile, "return window.__discryptTauriTwoProfileE2EVoiceEvidence || null;");
}

writeManifest(run ? "planned" : "dry-run", { preflight_result: preflight() });
if (!run) {
  console.log(`Tauri two-profile group text and voice E2E dry-run manifest: ${manifestPath}`);
  process.exit(0);
}
const preflightResult = preflight();
if (!preflightResult.ok) {
  writeManifest("failed-preflight", { preflight_result: preflightResult });
  failCli(preflightResult.reason, 3);
}
const children = [];
try {
  if (!skipBuild || !existsSync(appBinary)) {
    const ui = runCommand("ui-build", "npm", ["--prefix", "apps/ui", "run", "build"], repoRoot);
    const tauri = runCommand("tauri-debug-build", "cargo", ["tauri", "build", "--debug", "--no-bundle", "--features", "tauri-runtime,local-dev,production-media,mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter"], resolve(repoRoot, "apps/desktop/src-tauri"));
    manifest.build = { ui, tauri };
  }
  const broker = startLocalMqttBroker();
  if (broker) {
    children.push(broker);
    await waitTcp(mqttPort);
  }
  manifest.local_mqtt.ready = true;
  manifest.local_mqtt.log_sha256 = configuredMqttEndpoint ? null : sha256IfExists(mqttBrokerLogPath);
  writeManifest("starting-drivers");
  for (const profile of Object.values(profiles)) {
    const child = startDriver(profile);
    children.push(child);
    await waitHttp(profile.driver_port);
    await createSession(profile);
  }
  writeManifest("sessions-ready", { sessions: Object.fromEntries(Object.entries(profiles).map(([k, p]) => [k, { session_id: p.session_id, capabilities: p.capabilities }])), });
  await setupProfile(profiles.alice);
  await setupProfile(profiles.bob);
  const audioPreferences = await configureReleaseSmokeAudioPreferences(profiles);
  const invite = await createGroupInvite(profiles.alice);
  const admissionRequestProviderBaseline = await providerControlLaneCountersForProfiles(profiles);
  await joinGroup(profiles.bob, invite);
  const admissionRequestProviderPump = await pumpProviderControlLaneBidirectional(
    profiles,
    "openmls-admission-request",
    4,
    admissionRequestProviderBaseline,
  );
  const admissionDecisionProviderBaseline = await providerControlLaneCountersForProfiles(profiles);
  await approvePendingAdmissionThroughUi(profiles.alice);
  const admissionDecisionProviderPump = await pumpProviderControlLaneBidirectional(
    profiles,
    "openmls-admission",
    6,
    admissionDecisionProviderBaseline,
  );
  await assertNoAdmissionDecisionApplyFailure(profiles.alice, "alice_after_openmls_admission_provider_runtime");
  await assertNoAdmissionDecisionApplyFailure(profiles.bob, "bob_after_openmls_admission_provider_runtime");
  await waitForProfileState(profiles.bob, "OpenMLS admission Welcome", hasOpenMlsAdmission, 90_000);
  await waitForProfileState(profiles.alice, "OpenMLS owner admission epoch", hasOpenMlsAdmission, 90_000);
  await reloadProfile(profiles.alice);
  await reloadProfile(profiles.bob);
  await assertNoAdmissionDecisionApplyFailure(profiles.alice, "alice_after_admission_reload");
  await assertNoAdmissionDecisionApplyFailure(profiles.bob, "bob_after_admission_reload");
  await waitForAdmissionUnlockedUi(profiles.alice);
  await waitForAdmissionUnlockedUi(profiles.bob);
  const aliceMessage = "alice webdriver group text proof";
  const bobMessage = "bob webdriver group text proof";
  const groupTextProviderBaseline = await providerControlLaneCountersForProfiles(profiles);
  await sendGroupMessage(profiles.alice, aliceMessage);
  await sendGroupMessage(profiles.bob, bobMessage);
  const groupTextProviderPump = await pumpProviderTextControlFramesBidirectional(
    profiles,
    "group-text",
    8,
    groupTextProviderBaseline,
  );
  await reloadProfile(profiles.alice);
  await reloadProfile(profiles.bob);
  await waitForMaybe(profiles.alice, "bob message visible on alice before reload", "return document.body.innerText.includes(arguments[0]);", [bobMessage], 75_000);
  await waitForMaybe(profiles.bob, "alice message visible on bob before reload", "return document.body.innerText.includes(arguments[0]);", [aliceMessage], 75_000);
  await reloadProfile(profiles.alice);
  await reloadProfile(profiles.bob);
  await waitForMaybe(profiles.alice, "bob message visible on alice after reload", "return document.body.innerText.includes(arguments[0]);", [bobMessage], 20_000);
  await waitForMaybe(profiles.bob, "alice message visible on bob after reload", "return document.body.innerText.includes(arguments[0]);", [aliceMessage], 20_000);
  const aliceBody = await bodyText(profiles.alice);
  const bobBody = await bodyText(profiles.bob);
  const voice = await voiceCallFlow(profiles);
  const screenshots = { alice: await screenshot(profiles.alice, "final"), bob: await screenshot(profiles.bob, "final") };
  const aliceTextEvidence = textStateEvidence(profiles.alice, aliceMessage, bobMessage);
  const bobTextEvidence = textStateEvidence(profiles.bob, bobMessage, aliceMessage);
  const remotePlaintextObserved = aliceTextEvidence.remote_plaintext_visible && bobTextEvidence.remote_plaintext_visible;
  const remoteEncryptedEnvelopeObserved = aliceTextEvidence.remote_envelope_visible && bobTextEvidence.remote_envelope_visible;
  const peerReceiptsObserved = aliceTextEvidence.sender_peer_receipt_visible && bobTextEvidence.sender_peer_receipt_visible;
  const aliceRetainedNativeVoiceEvidence = voice?.before_leave?.alice?.evidence ?? voice?.alice ?? null;
  const bobRetainedNativeVoiceEvidence = voice?.before_leave?.bob?.evidence ?? voice?.bob ?? null;
  const browserVoiceLoopbackObserved = Boolean(
    voice?.before_leave?.alice?.remoteBoundaries > 0 &&
    voice?.before_leave?.bob?.remoteBoundaries > 0 &&
    voice?.alice?.localAudioTracksSent > 0 &&
    voice?.bob?.localAudioTracksSent > 0 &&
    voice?.alice?.remoteTrackEvents > 0 &&
      voice?.bob?.remoteTrackEvents > 0,
  );
  const backendNativeProofObserved = Boolean(
    Array.isArray(voice?.backend_native_proofs) &&
      voice.backend_native_proofs.length >= 2 &&
      voice.backend_native_proofs.every((proof) => proof?.protected_frames_count > 0),
  );
  const nativeRustWebDriverEvidenceObserved = Boolean(
    aliceRetainedNativeVoiceEvidence?.mode === "native_rust_webrtc_datachannel" &&
      bobRetainedNativeVoiceEvidence?.mode === "native_rust_webrtc_datachannel" &&
      aliceRetainedNativeVoiceEvidence?.remoteTrackEvents > 0 &&
      bobRetainedNativeVoiceEvidence?.remoteTrackEvents > 0 &&
      aliceRetainedNativeVoiceEvidence?.iceConnected &&
      bobRetainedNativeVoiceEvidence?.iceConnected &&
      aliceRetainedNativeVoiceEvidence?.nativeRustVoiceRuntimeAvailable &&
      bobRetainedNativeVoiceEvidence?.nativeRustVoiceRuntimeAvailable &&
      aliceRetainedNativeVoiceEvidence?.syntheticFallback === false &&
      bobRetainedNativeVoiceEvidence?.syntheticFallback === false,
  );
  const nativeRustBackendMediaObserved = Boolean(
    backendNativeProofObserved &&
      nativeRustWebDriverEvidenceObserved,
  );
  const voiceLoopbackObserved = browserVoiceLoopbackObserved || nativeRustBackendMediaObserved;
  const nativeVoiceLoopbackObserved = Boolean(
    voiceLoopbackObserved &&
      (nativeRustBackendMediaObserved ||
        ((voice?.alice?.mode === "native_rtc_generated_audio" || aliceRetainedNativeVoiceEvidence?.mode === "native_rust_webrtc_datachannel") &&
          (voice?.bob?.mode === "native_rtc_generated_audio" || bobRetainedNativeVoiceEvidence?.mode === "native_rust_webrtc_datachannel") &&
          voice?.alice?.getUserMediaCalls > 0 &&
          voice?.bob?.getUserMediaCalls > 0 &&
          voice?.alice?.iceConnected &&
          voice?.bob?.iceConnected)),
  );
  const syntheticVoiceFallbackObserved = Boolean(
    voiceLoopbackObserved &&
    (voice?.alice?.mode === "synthetic_peerconnection_fallback" ||
      voice?.bob?.mode === "synthetic_peerconnection_fallback"),
  );
  const nativeVoiceCapability = {
    alice: voice?.alice
      ? {
          mode: voice.alice.mode,
          nativeAudioContextAvailable: Boolean(voice.alice.nativeAudioContextAvailable),
          nativeRTCPeerConnectionAvailable: Boolean(voice.alice.nativeRTCPeerConnectionAvailable),
          nativeGeneratedAudioTrackAvailable: Boolean(voice.alice.nativeGeneratedAudioTrackAvailable),
          fallbackReason: voice.alice.fallbackReason ?? null,
        }
      : null,
    bob: voice?.bob
      ? {
          mode: voice.bob.mode,
          nativeAudioContextAvailable: Boolean(voice.bob.nativeAudioContextAvailable),
          nativeRTCPeerConnectionAvailable: Boolean(voice.bob.nativeRTCPeerConnectionAvailable),
          nativeGeneratedAudioTrackAvailable: Boolean(voice.bob.nativeGeneratedAudioTrackAvailable),
          fallbackReason: voice.bob.fallbackReason ?? null,
      }
      : null,
  };
  const expectedAudioPreferences = {
    alice: { mic_gain_percent: 155, app_output_volume_percent: 37 },
    bob: { mic_gain_percent: 120, app_output_volume_percent: 64 },
  };
  const audioPreferencesPersisted = Object.entries(expectedAudioPreferences).every(([name, expected]) => {
    const saved = audioPreferences?.[name]?.after;
    const reloaded = voice?.reloaded_audio_preferences?.[name];
    return saved?.mic_gain_percent === expected.mic_gain_percent &&
      saved?.app_output_volume_percent === expected.app_output_volume_percent &&
      reloaded?.mic_gain_percent === expected.mic_gain_percent &&
      reloaded?.app_output_volume_percent === expected.app_output_volume_percent;
  });
  const nativeMediaUsesConfiguredAudio = Boolean(
    Array.isArray(voice?.backend_native_proofs) &&
      voice.backend_native_proofs.length >= 2 &&
      voice.backend_native_proofs.every((proof) => {
        const name = String(proof?.profile || "").toLowerCase();
        const expected = expectedAudioPreferences[name];
        return expected &&
          proof.mic_gain_percent === expected.mic_gain_percent &&
          proof.app_output_volume_percent === expected.app_output_volume_percent &&
          proof.protected_frames_count > 0 &&
          proof.opus_payload_bytes > 0 &&
          proof.protected_payload_bytes > 0;
      }),
  );
  const remoteParticipantVolumeChanged = Boolean(
    voice?.remote_participant_volume?.alice?.changed &&
      voice?.remote_participant_volume?.bob?.changed,
  );
  const muteObserved = Boolean(voice?.after_mute?.alice?.evidence?.trackEnabled === false);
  const backendLeaveCleanupObserved = Boolean(
    voice?.leave_cleanup?.alice?.voice_session_cleared &&
      voice?.leave_cleanup?.alice?.voice_left_event &&
      voice?.leave_cleanup?.bob?.voice_session_cleared &&
      voice?.leave_cleanup?.bob?.voice_left_event,
  );
  const leaveCleanupObserved = Boolean(
    (voice?.alice?.trackStopCount > 0 && voice?.bob?.trackStopCount > 0) ||
      backendLeaveCleanupObserved,
  );
  const speakingEvidenceObserved = Boolean(
    Array.isArray(voice?.backend_native_proofs) &&
      voice.backend_native_proofs.every((proof) => proof?.speaking && proof?.rms_i16 > 0 && proof?.peak_i16 > 0),
  );
  const strictProviderRuntimeObserved = Boolean(
    admissionRequestProviderPump?.provider_runtime_used &&
      admissionRequestProviderPump?.frames_sent > 0 &&
      admissionDecisionProviderPump?.provider_runtime_used &&
      admissionDecisionProviderPump?.frames_sent > 0 &&
      groupTextProviderPump?.provider_runtime_used &&
      groupTextProviderPump?.frames_sent > 0 &&
      voice?.per56_provider_runtime_voice_signaling?.provider_runtime_used &&
      voice?.per56_provider_runtime_voice_signaling?.frames_sent > 0 &&
      voice?.per56_provider_runtime_voice_signaling?.manual_command_bridge_used === false,
  );
  const per59ReleaseSmoke = {
    issue: "PER-59 / P6-T08 human or loopback release smoke",
    native_path_required: true,
    browser_shim_or_raw_pulse_capture_counts_as_production: false,
    join_proved: Boolean(voice?.backend_native_proofs?.length >= 2),
    mute_proved: muteObserved,
    speaking_vad_proved: speakingEvidenceObserved,
    mic_gain_and_output_volume_proved: audioPreferencesPersisted && nativeMediaUsesConfiguredAudio,
    per_peer_volume_surface_proved: remoteParticipantVolumeChanged,
    native_loopback_proved: nativeVoiceLoopbackObserved,
    leave_cleanup_proved: leaveCleanupObserved,
    production_claim_allowed: Boolean(
      nativeVoiceLoopbackObserved &&
        audioPreferencesPersisted &&
        nativeMediaUsesConfiguredAudio &&
        remoteParticipantVolumeChanged &&
        muteObserved &&
        leaveCleanupObserved &&
        speakingEvidenceObserved,
    ),
    configured_audio_preferences: audioPreferences,
    reloaded_audio_preferences: voice?.reloaded_audio_preferences ?? null,
    remote_participant_volume: voice?.remote_participant_volume ?? null,
    leave_cleanup: voice?.leave_cleanup ?? null,
  };
  const summary = {
    schema_version: "discrypt.tauri_two_profile_group_text_voice_e2e_summary.v1",
    generated_at: new Date().toISOString(),
    status: "completed_with_truthful_delivery_boundary",
    acceptance_criteria: {
      setup_completed: true,
      invite_created: invite.startsWith("discrypt://join/v1/"),
      owner_staff_approval_applied: Boolean(manifest.openmls_admission_owner_approval?.approved),
      openmls_admission_persisted: true,
      text_plaintext_observed_both_ways: remotePlaintextObserved,
      text_envelope_or_receipt_observed_both_ways: remoteEncryptedEnvelopeObserved || peerReceiptsObserved,
      voice_native_or_capability_evidence_recorded: nativeVoiceLoopbackObserved || Boolean(nativeVoiceCapability.alice || nativeVoiceCapability.bob),
      persistence_reloaded_after_admission_text_and_voice: true,
      screenshots_logs_and_summary_recorded: true,
      degraded_unavailable_states_recorded_by_preflight: true,
    },
    workflow_steps: workflowSteps,
    artifact_contract: artifactContract,
    production_e2e_status: remotePlaintextObserved && nativeVoiceLoopbackObserved ? "remote_plaintext_text_and_native_voice_loopback_observed" : remotePlaintextObserved ? "remote_plaintext_text_observed" : remoteEncryptedEnvelopeObserved ? "remote_encrypted_envelope_observed_plaintext_not_rendered" : "remote_text_not_observed",
    voice_remote_media_status: nativeVoiceLoopbackObserved
      ? (nativeRustBackendMediaObserved || aliceRetainedNativeVoiceEvidence?.mode === "native_rust_webrtc_datachannel" || bobRetainedNativeVoiceEvidence?.mode === "native_rust_webrtc_datachannel"
        ? "native_rust_webrtc_datachannel_loopback"
        : "native_rtc_generated_audio_loopback")
      : syntheticVoiceFallbackObserved ? "synthetic_peerconnection_fallback_loopback" : voiceLoopbackObserved ? "non_native_browser_media_harness_loopback" : "voice_remote_media_not_observed",
    strict_e2e_eligible: remotePlaintextObserved && nativeVoiceLoopbackObserved && strictProviderRuntimeObserved,
    strict_provider_runtime: {
      local_mqtt_endpoint: localMqttEndpoint,
      admission_request: admissionRequestProviderPump,
      admission_decision: admissionDecisionProviderPump,
      group_text: groupTextProviderPump,
      voice_signaling: voice?.per56_provider_runtime_voice_signaling ?? null,
      provider_runtime_used: strictProviderRuntimeObserved,
      manual_command_bridge_used: false,
    },
    voice_proof: {
      loopback_observed: voiceLoopbackObserved,
      native_generated_audio_observed: nativeVoiceLoopbackObserved && (voice?.alice?.mode === "native_rtc_generated_audio" || voice?.bob?.mode === "native_rtc_generated_audio"),
      native_rust_webrtc_datachannel_observed: nativeVoiceLoopbackObserved && nativeRustBackendMediaObserved,
      native_rust_evidence_source: nativeRustBackendMediaObserved ? "voice.before_leave.*.evidence" : null,
      synthetic_fallback_observed: syntheticVoiceFallbackObserved,
      production_claim_allowed: nativeVoiceLoopbackObserved,
      blocker: nativeVoiceLoopbackObserved
        ? "physical two-device microphone/speaker proof is still outside this automated native Rust/generated-audio harness"
        : "native Rust Opus/SFrame media proof or generated-audio loopback was not observed in both Tauri profiles",
    },
    per59_release_smoke: per59ReleaseSmoke,
    run_id: runId,
    artifact_root: rel(artifactRoot),
    invite_prefix: invite.slice(0, 48),
    setup: { alice: true, bob: true },
    group_invite_join: { invite_created: invite.startsWith("discrypt://join/v1/"), bob_joined: /Two Profile WebDriver Lab/i.test(bobBody) },
    text_control_transport_bridge: "No manual WebDriver command bridge was used; admission crossed the sealed loopback MQTT control lane, while group text and voice signaling used provider-signaled direct WebRTC text/control runtimes.",
    per56_provider_runtime_voice_signaling: voice?.per56_provider_runtime_voice_signaling ?? null,
    native_voice_capability: nativeVoiceCapability,
    text: {
      alice_sent_visible_on_alice: aliceTextEvidence.local_plaintext_visible || aliceBody.includes(aliceMessage),
      bob_sent_visible_on_bob: bobTextEvidence.local_plaintext_visible || bobBody.includes(bobMessage),
      alice_message_visible_on_bob: bobTextEvidence.remote_plaintext_visible || bobBody.includes(aliceMessage),
      bob_message_visible_on_alice: aliceTextEvidence.remote_plaintext_visible || aliceBody.includes(bobMessage),
      alice_remote_envelope_visible_on_bob: bobTextEvidence.remote_envelope_visible,
      bob_remote_envelope_visible_on_alice: aliceTextEvidence.remote_envelope_visible,
      alice_sender_peer_receipt_visible: aliceTextEvidence.sender_peer_receipt_visible,
      bob_sender_peer_receipt_visible: bobTextEvidence.sender_peer_receipt_visible,
      remote_encrypted_envelopes_observed_both_ways: remoteEncryptedEnvelopeObserved,
      signed_peer_receipts_observed_both_ways: peerReceiptsObserved,
      production_plaintext_render_observed_both_ways: remotePlaintextObserved,
      evidence: { alice: aliceTextEvidence, bob: bobTextEvidence },
    },
    voice,
    screenshots,
    profile_state_files: Object.fromEntries(Object.entries(profiles).map(([name, profile]) => [name, { path: rel(profile.state_path), exists: existsSync(profile.state_path), sha256: sha256IfExists(profile.state_path) }])),
    logs: Object.fromEntries(Object.entries(profiles).map(([name, profile]) => [name, { path: rel(profile.log_path), sha256: sha256IfExists(profile.log_path) }])),
    remaining_production_blockers: [
      ...(remotePlaintextObserved ? [] : remoteEncryptedEnvelopeObserved ? [
        "Two live Tauri WebViews exchanged signed encrypted text envelopes and persisted peer receipts through the provider-backed runtime, but the receiver still renders envelope placeholders instead of decrypted plaintext.",
      ] : [
        "Two live Tauri WebViews completed setup, group invite join, local text send, persistence-backed profile creation, and voice UX controls, but remote text envelopes were not observed both ways across processes in the UI/state artifact.",
      ]),
      ...(nativeVoiceLoopbackObserved ? [
        "Physical two-device microphone/speaker proof is still not part of this automated harness; this run uses native Rust Opus/SFrame media or generated audio tracks through the native WebRTC implementation.",
      ] : voiceLoopbackObserved ? [
        "Voice remote media used the synthetic WebView peer-connection fallback because native RTCPeerConnection/generated-audio support was unavailable in this environment; this artifact does not satisfy the strict E2E acceptance criteria.",
      ] : []),
    ],
  };
  writeJson(summaryPath, summary);
  writeManifest("completed_with_truthful_delivery_boundary", { summary: rel(summaryPath) });
  console.log(`Tauri two-profile group text and voice E2E artifact: ${summary.artifact_root}`);
  if (requireNativeVoice && !nativeVoiceLoopbackObserved) process.exit(4);
  if (summary.remaining_production_blockers.length > 0 && process.env.DISCRYPT_TAURI_TWO_PROFILE_E2E_REQUIRE_PRODUCTION === "1") process.exit(4);
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  const failure_diagnostics = {};
  for (const [name, profile] of Object.entries(profiles)) {
    if (!profile.session_id) continue;
    try {
      const body = await bodyText(profile);
      const bodyPath = resolve(logDir, `${name}-failure-body.txt`);
      writeFileSync(bodyPath, body);
      failure_diagnostics[name] = { body_path: rel(bodyPath), body_excerpt: body.slice(0, 4000), actions: await visibleActions(profile) };
      try { failure_diagnostics[name].screenshot = await screenshot(profile, "failure"); } catch {}
    } catch (diagnosticError) {
      failure_diagnostics[name] = { diagnostic_error: diagnosticError instanceof Error ? diagnosticError.message : String(diagnosticError) };
    }
  }
  writeManifest("failed", { error: message, failure_diagnostics });
  console.error(`tauri-two-profile-group-text-voice-e2e: ${message}`);
  process.exitCode = 1;
} finally {
  await Promise.all(Object.values(profiles).map(closeSession));
  for (const child of children.reverse()) await terminate(child);
}

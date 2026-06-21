#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { createCipheriv, createHash, randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const leaderRoot = process.env.OMX_TEAM_LEADER_CWD ? resolve(process.env.OMX_TEAM_LEADER_CWD) : repoRoot;
const argv = process.argv.slice(2);
const run = argv.includes("--run");
const skipBuild = argv.includes("--skip-build") || process.env.DISCRYPT_G012_WEBDRIVER_SKIP_BUILD === "1";
const requireNativeVoice = argv.includes("--require-native-voice") ||
  process.env.DISCRYPT_G012_REQUIRE_NATIVE_VOICE === "1" ||
  process.env.DISCRYPT_G012_WEBDRIVER_REQUIRE_NATIVE_VOICE === "1";
const runId = valueAfter("--run-id") ?? process.env.DISCRYPT_G012_WEBDRIVER_RUN_ID ?? `g012-webdriver-${new Date().toISOString().replace(/[:.]/g, "-")}`;
const artifactRoot = resolve(repoRoot, valueAfter("--artifact-dir") ?? process.env.DISCRYPT_G012_WEBDRIVER_ARTIFACT_DIR ?? `target/g012-e2e/${runId}`);
const logDir = resolve(artifactRoot, "logs");
const profileDir = resolve(artifactRoot, "profiles");
const screenshotDir = resolve(artifactRoot, "screenshots");
for (const dir of [artifactRoot, logDir, profileDir, screenshotDir]) mkdirSync(dir, { recursive: true });

const driverBinary = process.env.DISCRYPT_G012_TAURI_DRIVER || commandPath("tauri-driver");
const nativeDriverBinary = process.env.DISCRYPT_G012_NATIVE_WEBDRIVER || commandPath("WebKitWebDriver") || firstExisting([
  resolve(repoRoot, "target/webdriver-deps/extracted/usr/bin/WebKitWebDriver"),
  resolve(leaderRoot, "target/webdriver-deps/extracted/usr/bin/WebKitWebDriver"),
]);
const appBinary = process.env.DISCRYPT_G012_APP_BINARY
  ? resolve(repoRoot, process.env.DISCRYPT_G012_APP_BINARY)
  : firstExisting([
      resolve(repoRoot, "target/debug/discrypt-desktop"),
      resolve(leaderRoot, "target/debug/discrypt-desktop"),
    ]);
const basePort = Number(process.env.DISCRYPT_G012_WEBDRIVER_BASE_PORT ?? valueAfter("--base-port") ?? 4510);
const disableSyntheticVoiceFallback = argv.includes("--disable-synthetic-voice-fallback") || process.env.DISCRYPT_G012_WEBDRIVER_DISABLE_SYNTHETIC_VOICE_FALLBACK === "1";
if (!Number.isInteger(basePort) || basePort < 1024 || basePort > 65000) failCli("base port must be a valid high TCP port", 2);

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

const manifestPath = resolve(artifactRoot, "tauri-webdriver-integrated-manifest.json");
const summaryPath = resolve(artifactRoot, "tauri-webdriver-integrated-summary.json");
const manifest = {
  schema_version: "discrypt.g012.tauri_webdriver_integrated.v1",
  generated_at: new Date().toISOString(),
  mode: run ? "run" : "dry-run",
  run_id: runId,
  artifact_root: rel(artifactRoot),
  app_binary: rel(appBinary),
  driver_binary: driverBinary || null,
  native_webdriver: nativeDriverBinary,
  profile_isolation_env: "DISCRYPT_APP_STATE_PATH",
  automation_env: "TAURI_WEBVIEW_AUTOMATION=1",
  require_native_voice: requireNativeVoice,
  boundary: "Drives two real Tauri WebViews through setup/group invite/text/voice UX. It reports remote text/media delivery truthfully and does not convert launch/UI smoke into a production network claim.",
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
  console.error(`g012-tauri-webdriver-integrated: ${message}`);
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
    webkit_runtime: webkitRuntimeDiagnostics(),
  };
  const okDisplay = Boolean(process.env.DISPLAY || process.env.WAYLAND_DISPLAY);
  if (!okDisplay) return { ok: false, reason: "No DISPLAY/WAYLAND_DISPLAY available for WebKit WebDriver", checks };
  if (!driverBinary) return { ok: false, reason: "tauri-driver is not installed; run cargo install tauri-driver --locked", checks };
  if (!nativeDriverBinary || !existsSync(nativeDriverBinary)) return { ok: false, reason: "WebKitWebDriver is missing; install webkit2gtk-driver or set DISCRYPT_G012_NATIVE_WEBDRIVER", checks };
  return { ok: true, checks };
}
function runCommand(label, command, args, cwd) {
  const logPath = resolve(logDir, `${label}.log`);
  manifest.commands.push({ label, command, args, cwd: rel(cwd), log_path: rel(logPath) });
  writeManifest("building");
  const result = spawnSync(command, args, { cwd, encoding: "utf8", env: process.env, maxBuffer: 1024 * 1024 * 128 });
  writeFileSync(logPath, `${result.stdout || ""}\n${result.stderr || ""}`);
  if (result.status !== 0) throw new Error(`${label} failed with ${result.status}; see ${rel(logPath)}`);
  return { log_path: rel(logPath), sha256: sha256IfExists(logPath) };
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
      ...process.env,
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
}
async function wd(profile, method, path, body) {
  const response = await fetch(`http://127.0.0.1:${profile.driver_port}${path}`, {
    method,
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(Number(process.env.DISCRYPT_G012_WEBDRIVER_COMMAND_TIMEOUT_MS ?? 30_000)),
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
    try { await wd(profile, "DELETE", `/session/${profile.session_id}`); } catch {}
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
  const active = state?.active_context ?? {};
  const group = active.group_id
    ? state.groups?.find((item) => item.group_id === active.group_id)
    : state.groups?.[0];
  const peers = group?.runtime_peers ?? [];
  const local = peers.find((peer) => peer.is_local);
  const remote = peers.find((peer) => !peer.is_local);
  if (!local?.peer_id || !remote?.peer_id) {
    throw new Error(`Could not derive runtime peers from app_state for ${active.group_id || "active group"}`);
  }
  return { local: local.peer_id, remote: remote.peer_id };
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
      signal_id: `g012-native-rust-${profile.display_name.toLowerCase()}-${Date.now()}`,
      created_at_ms: Date.now(),
    },
  });
  return {
    profile: profile.display_name,
    session_id: session.session_id,
    local_peer_id: peers.local,
    remote_peer_id: peers.remote,
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
  manifest.g012_backend_native_voice_proofs = reports;
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
      const evidence = window.__discryptG012WebDriverVoiceEvidence;
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
  const diagnostics = state?.transport_diagnostics || {};
  const probe = diagnostics.data_channel_probe || {};
  return diagnostics.data_channel_probe_status === "webrtc-datachannel-proofed" &&
    Boolean(probe.offerer_data_channel_open) &&
    Boolean(probe.answerer_data_channel_open) &&
    Boolean(probe.text_control_frame_roundtrip);
}
async function waitForProviderRuntime(profile, label, timeoutMs = 45_000) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    const state = await appState(profile);
    last = {
      status: state?.transport_diagnostics?.data_channel_probe_status ?? null,
      detail: state?.transport_diagnostics?.data_channel_probe_detail ?? null,
      last_command_error: state?.last_command_error ?? null,
    };
    if (providerRuntimeProofed(state)) return state;
    await new Promise((resolveWait) => setTimeout(resolveWait, 500));
  }
  throw new Error(`${profile.display_name} timed out waiting for provider text/control runtime ${label}; last=${JSON.stringify(last)}`);
}
async function startProviderTextControlRuntimePair(profiles, label) {
  const request = { scope_label: `g012-provider-runtime-${label}`, data_channel_probe: true, adapter_kind: "mqtt" };
  const starts = await Promise.all([
    invokeTauriCommand(profiles.alice, "start_text_session", { request }),
    invokeTauriCommand(profiles.bob, "start_text_session", { request }),
  ]);
  const attaches = await Promise.all([
    invokeTauriCommand(profiles.alice, "attach_text_control_transport_runtime", { request: { derive_from_state: true } }),
    invokeTauriCommand(profiles.bob, "attach_text_control_transport_runtime", { request: { derive_from_state: true } }),
  ]);
  const ready = await Promise.all([
    waitForProviderRuntime(profiles.alice, `${label}-alice`),
    waitForProviderRuntime(profiles.bob, `${label}-bob`),
  ]);
  const report = {
    label,
    starts: starts.map((state) => state?.transport_diagnostics ?? null),
    attaches: attaches.map((state) => state?.transport_diagnostics ?? null),
    ready: ready.map((state) => state?.transport_diagnostics ?? null),
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
async function pumpProviderTextControlFramesBidirectional(profiles, label, rounds = 6) {
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
  const evidence = {
    label,
    runtime,
    reports,
    provider_runtime_used: true,
    frames_sent: reports.reduce((sum, report) => sum + (report.frames_sent || 0), 0),
    native_voice_signals_accepted: reports.reduce((sum, report) => sum + (report.accepted || 0), 0),
    manual_command_bridge_used: false,
  };
  manifest[`provider_text_control_pump_${label.replace(/\W+/g, "_")}`] = evidence;
  writeManifest(manifest.status || "running", {});
  return evidence;
}
async function bridgeTextControlFramesOnce(fromProfile, toProfile, label) {
  const pending = await invokeTauriCommand(fromProfile, "list_pending_text_control_frames", { request: { limit: 50, operation_timeout_ms: 1000 } });
  const frames = Array.isArray(pending?.frames) ? pending.frames : [];
  const report = { label, from: fromProfile.display_name, to: toProfile.display_name, pending: frames.length, delivered: 0, responses: 0, frame_kinds: [] };
  for (const item of frames) {
    if (!item?.frame || !item?.message_id || !item?.frame_sha256) continue;
    report.frame_kinds.push(item.frame.kind || "unknown");
    const handled = await invokeTauriCommand(toProfile, "handle_text_control_frame", { request: { frame: item.frame } });
    if (item.frame.kind === "voice_signal") {
      const nativeAccepted = await acceptNativeVoiceSignalPayload(toProfile, item.frame.signal).catch((error) => ({ error: error instanceof Error ? error.message : String(error) }));
      if (nativeAccepted?.accepted) {
        report.native_media_accepted = (report.native_media_accepted || 0) + 1;
      } else if (nativeAccepted?.error) {
        report.native_media_errors = [...(report.native_media_errors || []), nativeAccepted.error];
      }
    }
    await invokeTauriCommand(fromProfile, "mark_text_control_frame_sent", {
      request: {
        message_id: item.message_id,
        frame_sha256: item.frame_sha256,
        transport_session_id: `g012-webdriver-command-bridge-${label}`,
      },
    });
    report.delivered += 1;
    if (handled?.response_frame) {
      await invokeTauriCommand(fromProfile, "handle_text_control_frame", { request: { frame: handled.response_frame } });
      report.responses += 1;
    }
  }
  return report;
}
async function bridgeTextControlFramesBidirectional(profiles, label, rounds = 6) {
  const reports = [];
  for (let round = 0; round < rounds; round += 1) {
    const aliceToBob = await bridgeTextControlFramesOnce(profiles.alice, profiles.bob, `${label}-a2b-${round}`);
    const bobToAlice = await bridgeTextControlFramesOnce(profiles.bob, profiles.alice, `${label}-b2a-${round}`);
    reports.push(aliceToBob, bobToAlice);
    if (aliceToBob.delivered === 0 && bobToAlice.delivered === 0) break;
    await new Promise((resolveWait) => setTimeout(resolveWait, 250));
  }
  manifest[`text_control_bridge_${label.replace(/\W+/g, "_")}`] = reports;
  manifest[`text_control_bridge_${label.replace(/\W+/g, "_")}_classification`] = "manual_command_bridge_not_per56_provider_runtime_evidence";
  writeManifest(manifest.status || "running", {});
  return reports;
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

async function approvePendingAdmission(profile) {
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
  const approvedState = await invokeTauriCommand(profile, "approve_group_admission_request", {
    request: {
      group_id: pending.group_id,
      request_id: pending.request_id,
    },
  });
  const error = approvedState?.last_command_error ?? null;
  const approved = !error && approvedState?.groups?.some((group) =>
    group?.group_id === pending.group_id &&
    group?.admission_requests?.some((request) => request?.request_id === pending.request_id && request?.status === "approved")
  );
  manifest.openmls_admission_owner_approval = {
    approved,
    pending,
    last_command_error: error,
    command: "approve_group_admission_request",
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
  await click(profile, "^Create group$", { last: true });
  await waitUntil(profile, "created group", "return /Two Profile WebDriver Lab/i.test(document.body.innerText)");
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
    Object.defineProperty(window, '__discryptG012ForceNativeRustVoice', { configurable: true, value: forceNativeRustVoice });
    try {
      window.localStorage?.setItem('discrypt:g012:force-native-rust-voice', forceNativeRustVoice ? '1' : '0');
      window.localStorage?.setItem('discrypt:g012:webdriver-voice-harness', '1');
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
    Object.defineProperty(window, '__discryptG012WebDriverVoiceEvidence', { configurable: true, value: evidence });
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
    class G012AudioContext { createMediaStreamSource() { return { connect() {}, disconnect() {} }; } createAnalyser() { return { fftSize: 1024, getByteTimeDomainData: (buf) => buf.fill(180), disconnect() {} }; } resume() { return Promise.resolve(); } close() { return Promise.resolve(); } }
    Object.defineProperty(window, 'AudioContext', { configurable: true, value: G012AudioContext });
    class G012PeerConnection { constructor() { evidence.peerConnectionsConstructed += 1; this.connectionState = 'new'; this.iceConnectionState = 'new'; this.ontrack = null; this.onicecandidate = null; } addTrack(localTrack, localStream) { if (localTrack?.kind === 'audio') evidence.localAudioTracksSent += 1; queueMicrotask(() => { this.connectionState = 'connected'; this.iceConnectionState = 'connected'; evidence.iceConnected = true; const remoteTrack = { id: arguments[0] + '-remote-track', kind: 'audio', label: arguments[0] + ' remote', readyState: 'live', enabled: true, addEventListener() {}, removeEventListener() {} }; const remoteStream = { id: arguments[0] + '-remote-stream', getTracks: () => [remoteTrack], getAudioTracks: () => [remoteTrack] }; evidence.remoteTrackEvents += 1; this.ontrack?.({ track: remoteTrack, streams: [remoteStream], receiver: { track: remoteTrack } }); this.onicecandidate?.({ candidate: null }); }); return { track: localTrack, stream: localStream }; } createOffer() { return Promise.resolve({ type: 'offer', sdp: 'v=0\r\na=mid:audio\r\na=sendrecv\r\n' }); } createAnswer() { return Promise.resolve({ type: 'answer', sdp: 'v=0\r\na=mid:audio\r\na=sendrecv\r\n' }); } setLocalDescription(desc) { this.localDescription = desc; return Promise.resolve(); } setRemoteDescription(desc) { this.remoteDescription = desc; return Promise.resolve(); } addIceCandidate() { return Promise.resolve(); } getStats() { return Promise.resolve(new Map([['inbound-audio', { type: 'inbound-rtp', kind: 'audio', mediaType: 'audio', packetsReceived: 12, audioLevel: 0.2 }]])); } getSenders() { return [{ track }]; } close() { evidence.peerConnectionsClosed += 1; this.connectionState = 'closed'; this.iceConnectionState = 'closed'; } }
    Object.defineProperty(window, 'RTCPeerConnection', { configurable: true, value: G012PeerConnection });
    return true;
  `, [profile.display_name.toLowerCase(), requireNativeVoice || disableSyntheticVoiceFallback]);
}

async function joinVoice(profile) {
  await click(profile, "Voice Lobby");
  await click(profile, "join call");
  await waitUntil(profile, "local voice participant", `return /You · you/i.test(document.body.innerText) || document.querySelector('[data-testid="voice-local-participant"]') !== null;`);
}
async function leaveVoice(profile) {
  await click(profile, "leave call");
  await waitUntil(profile, "left voice", "return /not joined/i.test(document.body.innerText) || window.__discryptG012WebDriverVoiceEvidence?.trackStopCount > 0;");
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
      alice: await exec(profiles.alice, "return window.__discryptG012WebDriverVoiceEvidence || null;"),
      bob: await exec(profiles.bob, "return window.__discryptG012WebDriverVoiceEvidence || null;"),
    };
    // Linux Tauri/WebKit may not expose WebView RTCPeerConnection. In that case
    // continue into the Rust-native backend media path; checkpoint eligibility is
    // decided below from native_rust_webrtc_datachannel evidence, never from the
    // synthetic WebView peer-connection fallback.
    if (nativeProbe.alice?.mode === "synthetic_peerconnection_fallback" || nativeProbe.bob?.mode === "synthetic_peerconnection_fallback") {
      throw new Error(`Synthetic WebView voice fallback is not permitted for native voice proof: ${JSON.stringify(nativeProbe)}`);
    }
  }
  await Promise.all([joinVoice(profiles.alice), joinVoice(profiles.bob)]);
  const backendNativeProofs = await publishBackendNativeVoiceProofs(profiles);
  let providerVoiceSignaling = null;
  try {
    providerVoiceSignaling = await pumpProviderTextControlFramesBidirectional(profiles, "voice-signaling-provider-runtime", 8);
  } catch (error) {
    providerVoiceSignaling = {
      provider_runtime_used: false,
      manual_command_bridge_used: false,
      error: error instanceof Error ? error.message : String(error),
    };
    manifest.per56_provider_runtime_voice_signaling = providerVoiceSignaling;
    writeManifest(manifest.status || "running", {});
  }
  for (let round = 0; round < 12; round += 1) {
    if (!providerVoiceSignaling?.provider_runtime_used || providerVoiceSignaling.frames_sent === 0) {
      await bridgeTextControlFramesBidirectional(profiles, `voice-signaling-${round}`, 4);
    }
    const observed = await Promise.all([
      waitForMaybe(profiles.alice, "remote voice audio on alice", "return document.querySelector('[data-testid=\"voice-remote-audio-boundary\"]') !== null || (window.__discryptG012WebDriverVoiceEvidence?.remoteTrackEvents || 0) > 0;", [], 1500),
      waitForMaybe(profiles.bob, "remote voice audio on bob", "return document.querySelector('[data-testid=\"voice-remote-audio-boundary\"]') !== null || (window.__discryptG012WebDriverVoiceEvidence?.remoteTrackEvents || 0) > 0;", [], 1500),
    ]);
    if (observed.every(Boolean)) break;
  }
  const remoteParticipantVolume = await adjustRemoteParticipantVolumes(profiles);
  await reloadProfile(profiles.alice);
  await reloadProfile(profiles.bob);
  const reloadedAudioPreferences = await readReleaseSmokeAudioPreferences(profiles, "after_voice_reload");
  await Promise.all([
    waitForMaybe(profiles.alice, "remote voice audio on alice", "return document.querySelector('[data-testid=\"voice-remote-audio-boundary\"]') !== null || (window.__discryptG012WebDriverVoiceEvidence?.remoteTrackEvents || 0) > 0;", [], 45_000),
    waitForMaybe(profiles.bob, "remote voice audio on bob", "return document.querySelector('[data-testid=\"voice-remote-audio-boundary\"]') !== null || (window.__discryptG012WebDriverVoiceEvidence?.remoteTrackEvents || 0) > 0;", [], 45_000),
  ]);
  await clickText(profiles.alice, "Voice Lobby");
  await clickText(profiles.bob, "Voice Lobby");
  const beforeLeave = {
    alice: await exec(profiles.alice, "return { evidence: window.__discryptG012WebDriverVoiceEvidence || null, remoteAudio: document.querySelectorAll('[data-testid=\"voice-remote-audio\"]').length, remoteBoundaries: document.querySelectorAll('[data-testid=\"voice-remote-audio-boundary\"]').length, text: document.body.innerText };"),
    bob: await exec(profiles.bob, "return { evidence: window.__discryptG012WebDriverVoiceEvidence || null, remoteAudio: document.querySelectorAll('[data-testid=\"voice-remote-audio\"]').length, remoteBoundaries: document.querySelectorAll('[data-testid=\"voice-remote-audio-boundary\"]').length, text: document.body.innerText };"),
  };
  await click(profiles.alice, "mute my microphone");
  await waitUntil(profiles.alice, "muted microphone", "return /muted/i.test(document.body.innerText) || window.__discryptG012WebDriverVoiceEvidence?.trackEnabled === false;");
  const afterMute = {
    alice: await exec(profiles.alice, "return { evidence: window.__discryptG012WebDriverVoiceEvidence || null, text: document.body.innerText };"),
    bob: await exec(profiles.bob, "return { evidence: window.__discryptG012WebDriverVoiceEvidence || null, text: document.body.innerText };"),
  };
  await click(profiles.alice, "mute my microphone");
  await Promise.all([leaveVoice(profiles.alice), leaveVoice(profiles.bob)]);
  return {
    alice: await exec(profiles.alice, "return window.__discryptG012WebDriverVoiceEvidence || null;"),
    bob: await exec(profiles.bob, "return window.__discryptG012WebDriverVoiceEvidence || null;"),
    backend_native_proofs: backendNativeProofs,
    per56_provider_runtime_voice_signaling: providerVoiceSignaling,
    remote_participant_volume: remoteParticipantVolume,
    reloaded_audio_preferences: reloadedAudioPreferences,
    before_leave: beforeLeave,
    after_mute: afterMute,
  };
}
async function voiceFlow(profile) {
  await installVoiceHarness(profile);
  await joinVoice(profile);
  await click(profile, "mute my microphone");
  await waitUntil(profile, "muted microphone", "return /muted/i.test(document.body.innerText) || window.__discryptG012WebDriverVoiceEvidence?.trackEnabled === false;");
  await click(profile, "mute my microphone");
  await leaveVoice(profile);
  return exec(profile, "return window.__discryptG012WebDriverVoiceEvidence || null;");
}

writeManifest(run ? "planned" : "dry-run", { preflight_result: preflight() });
if (!run) {
  console.log(`G012 Tauri WebDriver integrated dry-run manifest: ${manifestPath}`);
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
  await joinGroup(profiles.bob, invite);
  await bridgeTextControlFramesBidirectional(profiles, "openmls-admission-request", 4);
  await approvePendingAdmission(profiles.alice);
  await bridgeTextControlFramesBidirectional(profiles, "openmls-admission", 8);
  await assertNoAdmissionDecisionApplyFailure(profiles.alice, "alice_after_openmls_admission_bridge");
  await assertNoAdmissionDecisionApplyFailure(profiles.bob, "bob_after_openmls_admission_bridge");
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
  await sendGroupMessage(profiles.alice, aliceMessage);
  await sendGroupMessage(profiles.bob, bobMessage);
  await bridgeTextControlFramesBidirectional(profiles, "group-text", 8);
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
  const nativeRustBackendMediaObserved = Boolean(
    voice?.before_leave?.alice?.remoteBoundaries > 0 &&
    voice?.before_leave?.bob?.remoteBoundaries > 0 &&
      backendNativeProofObserved,
  );
  const voiceLoopbackObserved = browserVoiceLoopbackObserved || nativeRustBackendMediaObserved;
  const nativeVoiceLoopbackObserved = Boolean(
    voiceLoopbackObserved &&
      (nativeRustBackendMediaObserved ||
        ((voice?.alice?.mode === "native_rtc_generated_audio" || voice?.alice?.mode === "native_rust_webrtc_datachannel") &&
          (voice?.bob?.mode === "native_rtc_generated_audio" || voice?.bob?.mode === "native_rust_webrtc_datachannel") &&
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
  const leaveCleanupObserved = Boolean(voice?.alice?.trackStopCount > 0 && voice?.bob?.trackStopCount > 0);
  const speakingEvidenceObserved = Boolean(
    Array.isArray(voice?.backend_native_proofs) &&
      voice.backend_native_proofs.every((proof) => proof?.speaking && proof?.rms_i16 > 0 && proof?.peak_i16 > 0),
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
  };
  const summary = {
    schema_version: "discrypt.g012.tauri_webdriver_integrated_summary.v3",
    generated_at: new Date().toISOString(),
    status: "completed_with_truthful_delivery_boundary",
    production_e2e_status: remotePlaintextObserved && nativeVoiceLoopbackObserved ? "remote_plaintext_text_and_native_voice_loopback_observed" : remotePlaintextObserved ? "remote_plaintext_text_observed" : remoteEncryptedEnvelopeObserved ? "remote_encrypted_envelope_observed_plaintext_not_rendered" : "remote_text_not_observed",
    voice_remote_media_status: nativeVoiceLoopbackObserved
      ? (nativeRustBackendMediaObserved || voice?.alice?.mode === "native_rust_webrtc_datachannel" || voice?.bob?.mode === "native_rust_webrtc_datachannel"
        ? "native_rust_webrtc_datachannel_loopback"
        : "native_rtc_generated_audio_loopback")
      : syntheticVoiceFallbackObserved ? "synthetic_peerconnection_fallback_loopback" : voiceLoopbackObserved ? "non_native_browser_media_harness_loopback" : "voice_remote_media_not_observed",
    g012_checkpoint_eligible: remotePlaintextObserved && nativeVoiceLoopbackObserved,
    voice_proof: {
      loopback_observed: voiceLoopbackObserved,
      native_generated_audio_observed: nativeVoiceLoopbackObserved && (voice?.alice?.mode === "native_rtc_generated_audio" || voice?.bob?.mode === "native_rtc_generated_audio"),
      native_rust_webrtc_datachannel_observed: nativeVoiceLoopbackObserved && (nativeRustBackendMediaObserved || voice?.alice?.mode === "native_rust_webrtc_datachannel" || voice?.bob?.mode === "native_rust_webrtc_datachannel"),
      synthetic_fallback_observed: syntheticVoiceFallbackObserved,
      production_claim_allowed: nativeVoiceLoopbackObserved,
      blocker: nativeVoiceLoopbackObserved
        ? "physical two-device microphone/speaker proof is still outside this automated native Rust/generated-audio harness"
        : "native RTCPeerConnection generated-audio loopback was not observed in both Tauri WebViews",
    },
    per59_release_smoke: per59ReleaseSmoke,
    run_id: runId,
    artifact_root: rel(artifactRoot),
    invite_prefix: invite.slice(0, 48),
    setup: { alice: true, bob: true },
    group_invite_join: { invite_created: invite.startsWith("discrypt://join/v1/"), bob_joined: /Two Profile WebDriver Lab/i.test(bobBody) },
    text_control_transport_bridge: "Manual WebDriver command bridge may move signed backend text/control frames only as fallback evidence; it is not PER-56 provider-runtime evidence.",
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
        "Voice remote media used the synthetic WebView peer-connection fallback because native RTCPeerConnection/generated-audio support was unavailable in this environment; this artifact is not eligible to checkpoint G012 as production voice.",
      ] : []),
    ],
  };
  writeJson(summaryPath, summary);
  writeManifest("completed_with_truthful_delivery_boundary", { summary: rel(summaryPath) });
  console.log(`G012 Tauri WebDriver integrated artifact: ${summary.artifact_root}`);
  if (requireNativeVoice && !nativeVoiceLoopbackObserved) process.exit(4);
  if (summary.remaining_production_blockers.length > 0 && process.env.DISCRYPT_G012_WEBDRIVER_REQUIRE_PRODUCTION === "1") process.exit(4);
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
  console.error(`g012-tauri-webdriver-integrated: ${message}`);
  process.exitCode = 1;
} finally {
  for (const profile of Object.values(profiles)) await closeSession(profile);
  for (const child of children.reverse()) await terminate(child);
}

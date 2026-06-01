#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const argv = process.argv.slice(2);
const run = argv.includes("--run");
const skipBuild = argv.includes("--skip-build") || process.env.DISCRYPT_G012_WEBDRIVER_SKIP_BUILD === "1";
const runId = valueAfter("--run-id") ?? process.env.DISCRYPT_G012_WEBDRIVER_RUN_ID ?? `g012-webdriver-${new Date().toISOString().replace(/[:.]/g, "-")}`;
const artifactRoot = resolve(repoRoot, valueAfter("--artifact-dir") ?? process.env.DISCRYPT_G012_WEBDRIVER_ARTIFACT_DIR ?? `target/g012-e2e/${runId}`);
const logDir = resolve(artifactRoot, "logs");
const profileDir = resolve(artifactRoot, "profiles");
const screenshotDir = resolve(artifactRoot, "screenshots");
for (const dir of [artifactRoot, logDir, profileDir, screenshotDir]) mkdirSync(dir, { recursive: true });

const driverBinary = process.env.DISCRYPT_G012_TAURI_DRIVER || commandPath("tauri-driver");
const nativeDriverBinary = process.env.DISCRYPT_G012_NATIVE_WEBDRIVER || commandPath("WebKitWebDriver") || resolve(repoRoot, "target/webdriver-deps/extracted/usr/bin/WebKitWebDriver");
const appBinary = resolve(repoRoot, process.env.DISCRYPT_G012_APP_BINARY || "target/debug/discrypt-desktop");
const basePort = Number(process.env.DISCRYPT_G012_WEBDRIVER_BASE_PORT ?? valueAfter("--base-port") ?? 4510);
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
function commandPath(command) {
  const result = spawnSync("sh", ["-lc", `command -v ${JSON.stringify(command)}`], { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : null;
}
function sha256IfExists(path) {
  return existsSync(path) ? createHash("sha256").update(readFileSync(path)).digest("hex") : null;
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
  await waitUntil(profile, "post-reload app shell", "return /Local-first workspace|Set up your local discrypt profile/i.test(document.body.innerText)", [], 30_000);
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
async function click(profile, pattern, { last = false } = {}) {
  const ok = await exec(profile, `${domHelpers}; return clickButton(arguments[0], 'i', arguments[1]);`, [pattern, last]);
  if (!ok) throw new Error(`${profile.display_name} could not click button matching ${pattern}; visible actions=${JSON.stringify(await visibleActions(profile))}`);
}
async function clickText(profile, pattern) {
  const ok = await exec(profile, `${domHelpers}; return clickText(arguments[0], 'i');`, [pattern]);
  if (!ok) throw new Error(`${profile.display_name} could not click text matching ${pattern}; visible actions=${JSON.stringify(await visibleActions(profile))}`);
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
  await waitUntil(profile, "trust setup screen", "return /finish the local trust setup/i.test(document.body.innerText)");
}
async function createGroupInvite(profile) {
  await click(profile, "Create (a )?group");
  await fill(profile, "Group name", "Two Profile WebDriver Lab");
  await click(profile, "^Create group$", { last: true });
  await waitUntil(profile, "created group", "return /Two Profile WebDriver Lab/i.test(document.body.innerText)");
  await click(profile, "Create invite");
  return waitUntil(profile, "invite URL", "const m = document.body.innerText.match(new RegExp('discrypt:\\\\/\\\\/join\\\\/v1\\\\/\\\\S+')); return m && m[0];");
}
async function joinGroup(profile, invite) {
  await click(profile, "Join group");
  await fill(profile, "Invite URL or code", invite);
  await fill(profile, "Joined group/contact label", "Two Profile WebDriver Lab");
  await click(profile, "join/open group");
  await waitUntil(profile, "joined group", "return /Two Profile WebDriver Lab/i.test(document.body.innerText)");
}
async function sendGroupMessage(profile, message) {
  await clickText(profile, "#general");
  await waitUntil(profile, "general channel", "return /#general/i.test(document.body.innerText)");
  await fill(profile, "Message", message);
  await click(profile, "^Send message$");
  await waitUntil(profile, `message ${message}`, "return document.body.innerText.includes(arguments[0]);", [message]);
}
async function installVoiceHarness(profile) {
  await exec(profile, String.raw`
    const profileName = arguments[0];
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
  `, [profile.display_name.toLowerCase()]);
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
async function voiceCallFlow(profiles) {
  await Promise.all([installVoiceHarness(profiles.alice), installVoiceHarness(profiles.bob)]);
  await Promise.all([joinVoice(profiles.alice), joinVoice(profiles.bob)]);
  await Promise.all([
    waitForMaybe(profiles.alice, "remote voice audio on alice", "return document.querySelector('[data-testid=\"voice-remote-audio-boundary\"]') !== null || (window.__discryptG012WebDriverVoiceEvidence?.remoteTrackEvents || 0) > 0;", [], 45_000),
    waitForMaybe(profiles.bob, "remote voice audio on bob", "return document.querySelector('[data-testid=\"voice-remote-audio-boundary\"]') !== null || (window.__discryptG012WebDriverVoiceEvidence?.remoteTrackEvents || 0) > 0;", [], 45_000),
  ]);
  const beforeLeave = {
    alice: await exec(profiles.alice, "return { evidence: window.__discryptG012WebDriverVoiceEvidence || null, remoteAudio: document.querySelectorAll('[data-testid=\"voice-remote-audio\"]').length, remoteBoundaries: document.querySelectorAll('[data-testid=\"voice-remote-audio-boundary\"]').length, text: document.body.innerText };"),
    bob: await exec(profiles.bob, "return { evidence: window.__discryptG012WebDriverVoiceEvidence || null, remoteAudio: document.querySelectorAll('[data-testid=\"voice-remote-audio\"]').length, remoteBoundaries: document.querySelectorAll('[data-testid=\"voice-remote-audio-boundary\"]').length, text: document.body.innerText };"),
  };
  await click(profiles.alice, "mute my microphone");
  await waitUntil(profiles.alice, "muted microphone", "return /muted/i.test(document.body.innerText) || window.__discryptG012WebDriverVoiceEvidence?.trackEnabled === false;");
  await click(profiles.alice, "mute my microphone");
  await Promise.all([leaveVoice(profiles.alice), leaveVoice(profiles.bob)]);
  return {
    alice: await exec(profiles.alice, "return window.__discryptG012WebDriverVoiceEvidence || null;"),
    bob: await exec(profiles.bob, "return window.__discryptG012WebDriverVoiceEvidence || null;"),
    before_leave: beforeLeave,
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
    const tauri = runCommand("tauri-debug-build", "cargo", ["tauri", "build", "--debug", "--no-bundle", "--features", "tauri-runtime,local-dev,mqtt-adapter,nostr-adapter"], resolve(repoRoot, "apps/desktop/src-tauri"));
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
  const invite = await createGroupInvite(profiles.alice);
  await joinGroup(profiles.bob, invite);
  await waitForProfileState(profiles.bob, "OpenMLS admission Welcome", hasOpenMlsAdmission, 90_000);
  await waitForProfileState(profiles.alice, "OpenMLS owner admission epoch", hasOpenMlsAdmission, 90_000);
  const aliceMessage = "alice webdriver group text proof";
  const bobMessage = "bob webdriver group text proof";
  await sendGroupMessage(profiles.alice, aliceMessage);
  await sendGroupMessage(profiles.bob, bobMessage);
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
  const voiceLoopbackObserved = Boolean(
    voice?.before_leave?.alice?.remoteBoundaries > 0 &&
    voice?.before_leave?.bob?.remoteBoundaries > 0 &&
    voice?.alice?.localAudioTracksSent > 0 &&
    voice?.bob?.localAudioTracksSent > 0 &&
    voice?.alice?.remoteTrackEvents > 0 &&
    voice?.bob?.remoteTrackEvents > 0,
  );
  const nativeVoiceLoopbackObserved = Boolean(
    voiceLoopbackObserved &&
    voice?.alice?.mode === "native_rtc_generated_audio" &&
    voice?.bob?.mode === "native_rtc_generated_audio",
  );
  const syntheticVoiceFallbackObserved = Boolean(
    voiceLoopbackObserved &&
    (voice?.alice?.mode === "synthetic_peerconnection_fallback" ||
      voice?.bob?.mode === "synthetic_peerconnection_fallback"),
  );
  const summary = {
    schema_version: "discrypt.g012.tauri_webdriver_integrated_summary.v2",
    generated_at: new Date().toISOString(),
    status: "completed_with_truthful_delivery_boundary",
    production_e2e_status: remotePlaintextObserved && nativeVoiceLoopbackObserved ? "remote_plaintext_text_and_native_voice_loopback_observed" : remotePlaintextObserved ? "remote_plaintext_text_observed" : remoteEncryptedEnvelopeObserved ? "remote_encrypted_envelope_observed_plaintext_not_rendered" : "remote_text_not_observed",
    voice_remote_media_status: nativeVoiceLoopbackObserved ? "native_rtc_generated_audio_loopback" : syntheticVoiceFallbackObserved ? "synthetic_peerconnection_fallback_loopback" : voiceLoopbackObserved ? "browser_media_harness_loopback" : "voice_remote_media_not_observed",
    g012_checkpoint_eligible: remotePlaintextObserved && nativeVoiceLoopbackObserved,
    voice_proof: {
      loopback_observed: voiceLoopbackObserved,
      native_generated_audio_observed: nativeVoiceLoopbackObserved,
      synthetic_fallback_observed: syntheticVoiceFallbackObserved,
      production_claim_allowed: nativeVoiceLoopbackObserved,
      blocker: nativeVoiceLoopbackObserved
        ? "physical two-device microphone/speaker proof is still outside this automated generated-audio harness"
        : "native RTCPeerConnection generated-audio loopback was not observed in both Tauri WebViews",
    },
    run_id: runId,
    artifact_root: rel(artifactRoot),
    invite_prefix: invite.slice(0, 48),
    setup: { alice: true, bob: true },
    group_invite_join: { invite_created: invite.startsWith("discrypt://join/v1/"), bob_joined: /Two Profile WebDriver Lab/i.test(bobBody) },
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
      ...(voiceLoopbackObserved ? [] : [
        "Voice remote media was not observed in both live Tauri WebViews.",
      ]),
      ...(nativeVoiceLoopbackObserved ? [
        "Physical two-device microphone/speaker proof is still not part of this automated harness; this run uses generated audio tracks through the native WebRTC implementation.",
      ] : voiceLoopbackObserved ? [
        "Voice remote media used the synthetic WebView peer-connection fallback because native RTCPeerConnection/generated-audio support was unavailable in this environment; this artifact is not eligible to checkpoint G012 as production voice.",
      ] : []),
    ],
  };
  writeJson(summaryPath, summary);
  writeManifest("completed_with_truthful_delivery_boundary", { summary: rel(summaryPath) });
  console.log(`G012 Tauri WebDriver integrated artifact: ${summary.artifact_root}`);
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

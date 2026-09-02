#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const failures = [];

for (const configPath of [
  "apps/desktop/src-tauri/tauri.conf.json",
  "apps/desktop/src-tauri/tauri.release.conf.json",
]) {
  const config = JSON.parse(read(configPath));
  const mainWindow = config.app?.windows?.find((window) => window.label === "main");
  if (!mainWindow || mainWindow.url === "about:blank") {
    failures.push(`${configPath} must load the configured frontend directly`);
  }
  const debDepends = config.bundle?.linux?.deb?.depends ?? [];
  for (const dependency of [
    "gstreamer1.0-alsa",
    "gstreamer1.0-plugins-bad",
    "gstreamer1.0-nice",
    "gstreamer1.0-pulseaudio",
  ]) {
    if (!debDepends.includes(dependency)) {
      failures.push(`${configPath} missing Debian voice runtime dependency: ${dependency}`);
    }
  }
  const rpmDepends = config.bundle?.linux?.rpm?.depends ?? [];
  for (const dependency of [
    "gstreamer1-plugins-base",
    "gstreamer1-plugins-good",
    "gstreamer1-plugins-bad-free",
    "libnice-gstreamer1",
  ]) {
    if (!rpmDepends.includes(dependency)) {
      failures.push(`${configPath} missing RPM voice runtime dependency: ${dependency}`);
    }
  }
}

const desktopSource = read("apps/desktop/src-tauri/src/lib.rs");
for (const token of [
  "settings.set_enable_webrtc(true)",
  "settings.set_enable_media_stream(true)",
  "webview.connect_permission_request",
  "start_native_voice_stream",
  "send_native_voice_audio_frame",
  "take_native_voice_playback_frames",
  "derive_native_voice_runtime_inputs",
  "IceServerConfig::new(base.ice_config.stun_servers, Vec::new())",
  "start_provider_webrtc_text_control_offer_runtime",
  "start_provider_webrtc_text_control_answer_runtime_with_answerer",
]) {
  if (!desktopSource.includes(token)) {
    failures.push(`desktop native voice runtime missing token: ${token}`);
  }
}

const voiceMediaSource = read("apps/ui/src/voice-media.ts");
for (const token of [
  "createMediaStreamSource(options.localStream)",
  "createScriptProcessor",
  "sendNativeVoiceAudioFrame",
  "takeNativeVoicePlaybackFrames",
  "context.createBuffer",
]) {
  if (!voiceMediaSource.includes(token)) {
    failures.push(`WebAudio native voice bridge missing token: ${token}`);
  }
}

const preflight = read("scripts/g012-docker-tauri-preflight.sh");
for (const dependency of [
  "gstreamer1.0-alsa",
  "gstreamer1.0-plugins-bad",
  "gstreamer1.0-nice",
  "gstreamer1.0-pulseaudio",
]) {
  if (!preflight.includes(dependency)) {
    failures.push(`G012 desktop preflight missing voice runtime dependency: ${dependency}`);
  }
}

if (failures.length > 0) {
  console.error("linux voice runtime contract check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("linux voice runtime contract check passed");

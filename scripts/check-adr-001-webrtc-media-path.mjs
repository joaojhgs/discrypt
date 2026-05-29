#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const adr = read("docs/adr/adr-001-webrtc-media-path.md");
const transport = read("crates/media/src/transport.rs");
const bridge = read("crates/media/src/transform_bridge.rs");
const uiBridge = read("apps/ui/src/media/transform.ts");
const capture = read("crates/media/src/capture.rs");
const cargo = read("Cargo.toml");
const transportCargo = read("crates/transport/Cargo.toml");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "# ADR-001: WebRTC media path and Rust-owned SFrame boundary",
  "Status: accepted",
  "WebView `RTCPeerConnection` + Encoded Transform",
  "Rust native `webrtc` crate contingency",
  "WebView `getUserMedia`",
  "WebView audio output selection",
  "Rust `libopus-rs` pipeline",
  "`cpal` is not selected for the production desktop path",
  "JavaScript receives only encoded frame bytes, KIDs, and counters",
  "NativeWebRtcRsContingency",
]) requireText("ADR-001", adr, token);

for (const token of [
  "pub struct WebRtcMediaPathDecision",
  "pub enum VoiceCaptureBackend",
  "pub enum VoicePlaybackBackend",
  "pub enum VoiceCodecBackend",
  "WebviewGetUserMedia",
  "WebviewAudioOutput",
  "AndroidNativeWebRtcRs",
  "RustLibopusRs",
  "WebRtcRuntimeOpus",
  "js_raw_key_export_allowed: false",
  "preserves_rust_sframe_boundary",
  "adr_001_desktop_decision_uses_webview_peer_connection_and_rust_sframe_boundary",
  "adr_001_android_without_encoded_transform_uses_native_webrtc_rs_and_libopus",
]) requireText("media transport", transport, token);

for (const token of ["RustTransformBridge", "SFrameSender", "SFrameReceiver", "BridgeProtectedFrame"]) requireText("transform bridge", bridge, token);
for (const token of ["SFrame keys stay in Rust", "KIDs", "counters", "protectEncoded", "openEncoded"]) requireText("UI transform", uiBridge, token);
for (const token of ["libopus_rs", "OpusAudioEncoder", "OpusAudioDecoder", "VoiceCaptureSFramePipeline"]) requireText("capture", capture, token);
requireText("workspace cargo", cargo, "libopus-rs");
requireText("transport cargo", transportCargo, "webrtc = { version");

if (/TODO|FIXME|unimplemented!|todo!/i.test(adr)) failures.push("ADR-001 contains unfinished-work marker");

if (failures.length === 0) {
  const commands = [
    ["cargo", ["test", "-p", "discrypt-media", "transport", "--quiet"]],
    ["cargo", ["test", "-p", "discrypt-media", "transform_bridge", "--quiet"]],
    ["cargo", ["test", "-p", "external-signaling", "two_process_webrtc_paths_pass_with_ciphertext_only_pcap_audit", "--quiet"]],
  ];
  for (const [cmd, args] of commands) {
    const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
    if (run.status !== 0) {
      failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
      break;
    }
  }
}

if (failures.length > 0) {
  console.error("ADR-001 WebRTC media path check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("ADR-001 WebRTC media path check passed");

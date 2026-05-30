# ADR-001: WebRTC media path and Rust-owned SFrame boundary

Status: accepted  
Date: 2026-05-29

## Context

The original Discrypt plan requires voice over WebRTC, relay/TURN ciphertext-only media, Android coverage, and a hard key boundary: raw MLS exporter material and SFrame keys stay in Rust. The production launch hint asks for an explicit choice between desktop WebView `RTCPeerConnection` with Encoded Transform and a full native `webrtc-rs` media stack, plus the exact capture/playback and codec ownership model.

## Decision

Discrypt uses a split media architecture:

| Runtime | WebRTC engine | Capture backend | Playback backend | Opus owner | SFrame/key owner |
| --- | --- | --- | --- | --- | --- |
| Linux/macOS/Windows desktop WebView | WebView `RTCPeerConnection` + Encoded Transform | WebView `getUserMedia` | WebView audio output selection | WebRTC runtime Opus frames | Rust `RustTransformBridge`, `SFrameSender`, `SFrameReceiver` |
| Android with Encoded Transform support | WebView `RTCPeerConnection` + Encoded Transform | WebView `getUserMedia` | WebView audio output selection | WebRTC runtime Opus frames | Rust `RustTransformBridge`, `SFrameSender`, `SFrameReceiver` |
| Android without Encoded Transform support | Rust native `webrtc` crate contingency | `webrtc` crate native Android media path | `webrtc` crate native Android media path | Rust `libopus-rs` pipeline | Rust `RustTransformBridge`, `SFrameSender`, `SFrameReceiver` |

`cpal` is not selected for the production desktop path because the WebView owns microphone and speaker devices there. It is also not selected for ADR-001 Android contingency because the accepted fallback is the native `webrtc` crate path plus the Rust `libopus-rs` encode/decode pipeline already present in `crates/media/src/capture.rs`. If a future native-desktop capture route is accepted, it must be a new ADR and must not weaken this key boundary.

## Rust and TypeScript integration contract

- `crates/media/src/transport.rs` exposes `WebRtcMediaPathDecision` so runtime probes can report the selected path without guessing.
- Desktop and Android-with-transform paths require Encoded Transform support and use the keyless TypeScript bridge in `apps/ui/src/media/transform.ts`.
- JavaScript receives only encoded frame bytes, KIDs, and counters. It cannot request raw SFrame keys, MLS exporter output, content keys, or media key material.
- Android without Encoded Transform uses `NativeWebRtcRsContingency`; it must have microphone permission, a selected input device, at least one STUN/TURN ICE endpoint, native capture/playback enabled, and Rust SFrame enabled before media transit.
- Rust transforms protect encoded frames before relay/TURN/network transit and verify sender binding before playback. The selected path does not change the sender-binding requirement.

## Consequences

- The default desktop implementation avoids a second native WebRTC media stack and keeps OS media permissions inside the WebView/Tauri shell.
- Android has a deterministic native fallback for WebViews that cannot expose safe encoded-frame hooks.
- The Rust `webrtc` crate remains mandatory for the native contingency and process-level WebRTC path verification; `libopus-rs` remains the pinned Rust Opus codec for native/synthetic media frames.
- Release verification must keep proving that signaling, relay, TURN, logs, crash reports, and JS command payloads never contain raw SDP secrets, ICE credentials, SFrame keys, MLS exporter secrets, or audio plaintext.

## Evidence

- `crates/media/src/transport.rs` — `WebRtcMediaPathDecision`, `AndroidVoiceContingency`, `NativeWebRtcRsContingency`.
- `crates/media/src/transform_bridge.rs` — keyless encoded-frame bridge that exposes protected bytes, KIDs, and counters only.
- `apps/ui/src/media/transform.ts` — TypeScript boundary that rejects raw-key handling.
- `crates/media/src/capture.rs` — Rust `libopus-rs` capture/encode/decode path for native and harness media frames.
- `crates/transport/src/webrtc_negotiation.rs` and `../discrypt-signaling/tests/process_webrtc_transport_paths.rs` — native `webrtc` crate negotiation and process verification.

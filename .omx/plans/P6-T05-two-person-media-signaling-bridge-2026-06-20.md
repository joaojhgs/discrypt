# P6-T05 - Two-Person Media Signaling Bridge

## Requirements Summary

Source: Multica PER-56 / Phase 6 two-person voice chat production path.

Acceptance criterion: voice offer/answer/candidate/native media proof signals must traverse the provider-signaled text/control runtime rather than being copied between profiles by harness code. Providers remain signaling/rendezvous only: they may carry sealed WebRTC negotiation payloads, but never application text frames, media frames, plaintext, raw SDP/ICE credentials, SFrame keys, MLS exporter material, or audio bytes.

Relevant context:
- `docs/release/handoff-2026-06-10-current-state.md`: release reset and backend-truth boundary; WebRTC/media state requires route evidence.
- `.omc/plans/discrypt-plan.md`: Phase 6 signaling scope and locked voice/WebRTC/SFrame invariants.
- `docs/adr/adr-001-webrtc-media-path.md`: Rust-owned SFrame boundary and WebRTC media path.
- `docs/phase-6-connectivity-signaling-push-metadata.md`: provider-visible signaling remains opaque and content-blind.
- Existing adjacent plans: `P6-T01` through `P6-T04` establish non-persisted voice state, device selection, voice UX, and native Rust media session boundary.

Relevant code paths:
- `crates/transport/src/provider_adapters.rs`: provider-signaled WebRTC text/control runtime constructors and local conformance provider.
- `crates/transport/src/signaling.rs`: adapter/rendezvous contract for sealed WebRTC negotiation.
- `apps/desktop/src-tauri/src/lib.rs`: voice signaling outbox/inbox, text/control pump, native media proof commands, and backend tests.
- `scripts/g012-tauri-webdriver-integrated.mjs`: native voice proof script currently has a direct frame bridge fallback and must not be used as production evidence for this task unless provider runtime evidence is present.

## Implementation Steps

1. Add a harness-only transport constructor that starts a live provider-signaled WebRTC text/control runtime pair over `LocalConformanceProviderAdapter`, with explicit offerer/answerer peer ids and an answerer callback.
2. Ensure the desktop harness feature enables the transport harness path without changing production feature gates.
3. Add a Tauri backend regression that:
   - creates two isolated profiles and joins the same voice channel,
   - starts native Rust voice media proof on Alice,
   - queues the proof as a sealed `VoiceSignal`,
   - pumps it through the local provider-signaled WebRTC DataChannel runtime into Bob,
   - accepts the remote native media proof on Bob,
   - verifies Bob records remote media evidence and Alice marks the voice signal sent.
4. Update release evidence docs/scripts only as needed to distinguish provider-runtime evidence from the older manual WebDriver bridge fallback.
5. Run targeted Rust tests and formatting. Attempt the required Tauri WebDriver native voice proof; if the environment cannot run it, record the exact blocker and classify the backend proof as harness/local evidence, not production two-machine proof.

## Failure Modes And Safety Behavior

- If provider/runtime negotiation is unavailable, the proof must fail closed with no remote media state and no joined/connected claim.
- If a voice signal is not sealed, contains raw SDP/ICE markers, targets the wrong peer, or comes from a local participant, existing validation must reject it before persistence or playback state.
- Provider adapters must remain unable to send app payload/media as fallback; only the WebRTC DataChannel transport may carry the serialized `VoiceSignal` text/control frame.
- The local conformance provider is harness-only and must not weaken production feature-gated adapter behavior.

## Acceptance Criteria

- A focused backend/Tauri test proves sealed voice signaling moves through a provider-signaled WebRTC DataChannel runtime, not direct `handle_text_control_frame` copy.
- The receiver can accept a remote native Rust media proof only after it arrives as a sealed backend voice signal.
- Provider logs/evidence show the adapter was used for rendezvous/signaling, while text/control payload bytes crossed the WebRTC DataChannel.
- No UI or backend state claims remote audio before the remote proof is accepted.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per56 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --features harness native_voice_signal_traverses_provider_signaled_text_control_runtime --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per56 cargo test -p discrypt-transport --features harness live_provider_text_control_runtime_pair_carries_multiple_opaque_frames -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-per56 cargo fmt --check`
- Attempt: `DISCRYPT_G012_REQUIRE_NATIVE_VOICE=1 RUSTUP_TOOLCHAIN=1.89.0 node scripts/g012-tauri-webdriver-integrated.mjs --run --require-native-voice --artifact-dir target/per56-g012-native-voice`

Evidence classification: the targeted Rust/Tauri tests are harness/local provider-signaled runtime evidence. The WebDriver command is the required native voice proof only if it runs successfully in this environment and records provider-runtime voice signaling evidence.

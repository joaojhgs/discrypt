# P8-T09 Overlay Voice Fanout Plan

## Requirements Summary
- Source: PER-74 / P8-T09 from Phase 8 adaptive encrypted peer-relay overlay.
- Product invariant: voice/media may fan out over direct WebRTC, configured TURN-backed WebRTC, or peer-assisted overlay only; public signaling providers must not relay media.
- Acceptance: a 3-member audio frame proof shows one protected voice frame delivered over direct plus peer-assisted relay graph, with relay-visible bytes containing only SFrame ciphertext and non-secret metadata.

## Code Paths
- `crates/media/src/transport.rs`: add media-side fanout planner and local harness tests.
- `crates/media/src/lib.rs`: export the planner types.
- `crates/media/Cargo.toml`: depend on `discrypt-transport` for route/overlay evidence types.
- Existing transport substrate: `crates/transport/src/peer_overlay.rs` provides route selection, relay authority, and opaque media forwarding.
- Existing media substrate: `crates/media/src/sframe.rs` and `crates/media/src/transform_bridge.rs` provide SFrame-protected frame bytes with no raw key export.

## Implementation Steps
1. Add a typed `VoiceOverlayFanoutInput` and `build_voice_overlay_fanout` in `crates/media/src/transport.rs`.
2. Convert each `PeerOverlayRouteSelection` into a per-recipient delivery:
   - direct WebRTC or configured TURN emits the protected SFrame frame directly to the destination.
   - peer-assisted overlay builds a `PeerOverlayFrame` with `PeerOverlayPayloadKind::Media`, then calls `build_peer_overlay_forwarding_plan`.
3. Validate route selections are source-bound, destination-unique, and provider-relay-free.
4. Add a 3-member test: Alice sends one protected Opus-like frame directly to Bob and through Bob as relay to Carol; Bob-as-relay sees ciphertext only, while Bob/Carol receivers can open the SFrame frame after registry verification.
5. Add negative tests for forbidden plaintext marker leakage and provider application relay rejection.

## Failure Modes And Safety
- Missing route evidence returns a media transport error instead of claiming delivery.
- Provider application relay carriers are rejected by transport validation and never converted into media deliveries.
- Relay forwarding uses `PeerOverlayForwardingPolicy` forbidden markers to prove plaintext/key material is absent from relay-visible bytes.
- The planner does not open sockets, decrypt as a relay, or expose raw SFrame keys; evidence is local harness/model proof, not split-machine production audio evidence.

## Verification
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-media voice_overlay_fanout -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

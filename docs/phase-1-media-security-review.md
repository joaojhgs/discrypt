# Phase 1 media security review

G002 implements the Phase 1 media-security slice as a Rust-owned, SFrame-like AEAD boundary. It is still a compact facade rather than a full RFC-9605 implementation, but it locks the architectural contracts that later OpenMLS/SFrame integration must preserve.

## Preserved security contracts

- **No raw JavaScript media keys:** web/React bridge code exchanges encoded payloads, KIDs, counters, and protected ciphertext only. `SFrameKey`, sender state, receiver registries, and MLS exporter material stay in Rust.
- **Sender binding is explicit:** media keys derive from MLS exporter material plus KID, MLS leaf index, and device id. Duplicate KIDs and invalid bindings are rejected.
- **Authenticated relay-visible frames:** media ciphertext is produced with AES-GCM; KID/counter/sender binding context are authenticated through nonce/AAD construction.
- **Receiver-owned replay defense:** `ReplayWindow` rejects duplicate and stale counters per KID after authentication.
- **Relay content blindness:** relay helpers and the multinode harness verify relays see ciphertext, not voice-frame plaintext.
- **Android contingency remains native:** `MediaTransportPath::NativeWebRtcRsContingency` records the fallback when Android webviews lack safe encoded transform support.

## Current evidence

| Area | Current files | Evidence |
| --- | --- | --- |
| Sender-bound media keys | `crates/media/src/sframe.rs` | `SFrameKey::derive` binds MLS exporter output to KID, leaf index, and device id; tests reject binding mismatch and duplicate KIDs. |
| Authenticated media frames | `crates/media/src/sframe.rs` | AES-GCM protection rejects ciphertext tamper without consuming replay state. |
| Replay semantics | `crates/media/src/sframe.rs` | Bounded per-KID window accepts out-of-order frames once and rejects duplicates/stale counters. |
| Transform bridge | `crates/media/src/transform_bridge.rs`, `apps/ui/src/media/transform.ts` | bridge APIs expose only encoded/protected frame metadata; raw key fields are forbidden on the TypeScript boundary. |
| Relay adversary smokes | `crates/relay-overlay/src/integrity.rs`, `harness/multinode/src/lib.rs` | relay helpers model forward/tamper behavior; harness checks passive relay opacity, replay rejection, and tamper rejection. |
| Platform contingency | `crates/media/src/transport.rs` | Android without encoded transforms selects the native `webrtc-rs` contingency skeleton. |

## Remaining production hardening

- Replace the compact facade with audited RFC-9605 SFrame packet framing while preserving the Rust-owned key and sender-binding contracts.
- Wire sender-binding authority to the later MLS-signed room-state delivery and fork-repair implementation.
- Add browser and Android device/runtime tests once Tauri/mobile shells are beyond the Phase 0/1 skeleton.
- Add lossy-network media E2E tests when relay topology/failover lands in G003.

## Verification used for G002

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p discrypt-multinode-harness --quiet
cargo check --workspace --target aarch64-linux-android
cargo audit
cargo deny check
cargo sbom --output-format spdx_json_2_3
cd apps/ui && npm ci && npm run typecheck && npm run build && npm audit --audit-level=moderate
```

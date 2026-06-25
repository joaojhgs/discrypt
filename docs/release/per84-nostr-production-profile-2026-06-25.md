# PER-84 / P10-T03 Nostr Production Profile Evidence

Date: 2026-06-25
Branch: `multica/P10-T03-nostr-production-profile`

## Scope

This is Phase 10 Nostr production-profile evidence for public/custom relay list handling, privacy-safe Nostr tags/payloads, payload-size fail-closed behavior, and provider-as-signaling-only boundaries.

It is not full production readiness for the app, invite admission, OpenMLS membership, installed Tauri GUI behavior, voice audio, or packaging.

## Implemented

- Nostr profile relay extraction now deduplicates configured relay URLs while preserving order before connecting.
- Nostr publish validates serialized provider envelopes against the profile's bounded provider message cap and fails closed with `provider_message_too_large` diagnostics.
- Desktop default Nostr profile generation now supports `DISCRYPT_DEFAULT_NOSTR_ENDPOINTS` as a comma-separated public/custom relay list, keeps legacy `DISCRYPT_DEFAULT_NOSTR_ENDPOINT`, and defaults to `wss://relay.damus.io` plus `wss://nos.lol` when no override is present.
- Generated profile allowlist commitments now cover every endpoint in a relay list.
- `crates/transport/tests/public_signaling_e2e.rs` now includes a test-local loopback Nostr relay that accepts real WebSocket Nostr `REQ`/`EVENT` traffic and exercises a two-peer presence plus sealed WebRTC signaling roundtrip through `NostrProviderAdapter`.

## Privacy And Boundary Evidence

Provider-visible Nostr material remains limited to:

- relay endpoint metadata;
- the custom Discrypt Nostr event kind;
- one `d` tag containing the derived hashed rendezvous topic;
- base64-encoded Discrypt wire envelopes containing already-opaque presence or sealed WebRTC negotiation payloads.

The provider does not receive room names, display names, invite secrets, raw SDP, ICE credentials, TURN credentials, MLS/SFrame/content keys, message plaintext, or media bytes. `broadcast_control` and `take_control_payloads` still fail closed for provider application relay.

## Verification

Passed:

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features nostr-adapter nostr_ -- --test-threads=1`
  - Included local/unit Nostr coverage for multi-relay preservation/dedupe, public cleartext relay rejection, privacy-safe event content/tag shape, typed oversized-envelope failure, structured provider failure mapping, and a live loopback Nostr relay roundtrip using `ws://127.0.0.1...` with two adapter sessions exchanging opaque presence and sealed WebRTC offer payloads.
- `DISCRYPT_PUBLIC_NOSTR_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features nostr-adapter public_nostr_two_peer_presence_and_signal_roundtrip -- --test-threads=1 --nocapture`
  - Live public relay evidence: 1 passed, using `wss://nos.lol`.
- `DISCRYPT_PUBLIC_NOSTR_MULTI_RELAY_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINTS=wss://nos.lol,wss://relay.damus.io,wss://discrypt-degraded-relay.invalid RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features nostr-adapter public_nostr_multi_relay_degraded_fallback_soak -- --test-threads=1 --nocapture`
  - Live public/custom relay-list evidence: 1 passed, with two public relays and one intentionally degraded relay.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --features nostr-adapter default_profiles_ -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy --workspace --all-targets -- -D warnings`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-transport --features nostr-adapter --lib -- -D warnings`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --features nostr-adapter --lib -- -D warnings`

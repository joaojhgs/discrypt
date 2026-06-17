# Nostr signaling adapter readiness note

Status: real provider client wired behind `nostr-adapter`; public relay WebRTC/DataChannel E2E passed against `wss://nos.lol`; profile-level multi-relay wiring is implemented and a degraded multi-relay public fallback soak passed with two public relays plus one intentionally invalid relay.
Scope: Discrypt serverless signaling adapter `nostr` / Cargo feature `nostr-adapter`

## Current contract

The `nostr` adapter is registered in the transport adapter registry and is selectable when the `nostr-adapter` Cargo feature is compiled. It uses `nostr-sdk` to connect to configured `wss://` relay endpoints and publishes Discrypt-specific Nostr events containing only already-sealed Discrypt signaling envelopes. When a profile contains multiple relay endpoints, the room join path now adds all configured relays and publishes/subscribes against that relay set instead of silently using only the first endpoint.

Verified guard:

```bash
cargo test -q -p discrypt-transport --features nostr-adapter \
  nostr_adapter_feature_is_selectable_with_real_relay_client
cargo test -q -p discrypt-transport --features nostr-adapter \
  nostr_profile_preserves_all_configured_relays_for_room_join
DISCRYPT_PUBLIC_NOSTR_MULTI_RELAY_E2E=1 cargo test -q -p discrypt-transport \
  --features nostr-adapter public_nostr_multi_relay_degraded_fallback_soak -- --nocapture
DISCRYPT_PUBLIC_NOSTR_REJECTION_E2E=1 cargo test -q -p discrypt-transport \
  --features nostr-adapter public_nostr_blocked_relay_maps_to_auth_required -- --nocapture
cargo test -q -p discrypt-transport \
  provider_failure_classes_map_to_typed_health_states -- --nocapture
DISCRYPT_PUBLIC_NOSTR_WEBRTC_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol \
  cargo test -q -p discrypt-transport --features nostr-adapter \
  --test public_webrtc_datachannel_e2e \
  public_nostr_signals_real_webrtc_datachannel_roundtrip -- --nocapture
```

## Provider-visible shape

- Event kind: Discrypt custom regular event kind `31733`.
- Topic tag: `d=<RendezvousCapability.topic>` where the capability topic is already random/hashed policy metadata.
- Event signer: scoped relay identity derived from the rendezvous topic and redacted local peer id, not the user's MLS/account identity key.
- Event content: base64url/no-pad JSON envelope carrying only:
  - encrypted presence bytes,
  - `SealedWebRtcNegotiationPayload` offer/answer/candidate bytes.

The relay must not receive group names, channel names, display names, safety numbers, raw room seeds, raw SDP, ICE ufrag/passwords, TURN credentials, plaintext messages, application data frames, receipts, or audio metadata.

## Still required for production completion

- Keep the opt-in public relay two-peer smoke tests (`DISCRYPT_PUBLIC_NOSTR_E2E=1` and `DISCRYPT_PUBLIC_NOSTR_WEBRTC_E2E=1`) in release verification; latest WebRTC/DataChannel pass used `wss://nos.lol`, while an earlier relay tried at `wss://nostr.oxtr.dev` returned `blocked`.
- Public multi-relay fallback evidence now exists: `DISCRYPT_PUBLIC_NOSTR_MULTI_RELAY_E2E=1 cargo test -q -p discrypt-transport --features nostr-adapter public_nostr_multi_relay_degraded_fallback_soak -- --nocapture` passed on 2026-05-30 with the default relay set `wss://nos.lol,wss://relay.damus.io,wss://discrypt-degraded-relay.invalid`, proving a configured degraded relay does not prevent two peers from exchanging sealed presence and WebRTC negotiation via the healthy relay set.
- Map relay auth, rate-limit, message-too-large, unhealthy relay, and trust mismatch failures to typed `SignalingHealthState`/`AdapterReadinessState` values instead of a generic signaling error where possible. Conservative string classification, structured `NOTICE`/`CLOSED`/negative `OK` relay-message extraction, and all-relay Nostr publish/subscribe failure labeling are implemented. Public auth/block evidence now exists: `DISCRYPT_PUBLIC_NOSTR_REJECTION_E2E=1 cargo test -q -p discrypt-transport --features nostr-adapter public_nostr_blocked_relay_maps_to_auth_required -- --nocapture` passed on 2026-05-30 against `wss://nostr.oxtr.dev` with `failure_class=provider_auth_required` and no payload leakage. Explicit reproducible public rate-limit evidence remains opportunistic because public relays do not provide deterministic test accounts for rate-limit triggering.
- Provider-visible capture scans are covered by G133 (`npm --prefix apps/ui run test:provider-metadata-capture-g133`) for event tags/content/log-adjacent adapter captures. External host packet capture remains a separate release-run artifact.
- Wire this adapter through the Tauri runtime factory and UI selection path for actual app use, not only transport-level conformance.

## Dependency note

`nostr-sdk` 0.44.1 is MIT licensed and supports the client operations Discrypt needs: relay management, subscriptions, custom event builders, and notification handling. The crate is feature-gated and not part of default builds.

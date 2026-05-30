# Nostr signaling adapter readiness note

Status: groundwork only; not production-ready  
Scope: Discrypt serverless signaling adapter `nostr` / Cargo feature `nostr-adapter`

## Current contract

The `nostr` adapter is registered in the transport adapter registry and remains fail-closed until a real audited relay client is wired. Compiling with `nostr-adapter` currently changes the boundary from `feature_disabled` to `implementation_unavailable`; it must not make fallback selection treat Nostr as usable.

Verified guard:

```bash
cargo test -q -p discrypt-transport --features nostr-adapter \
  nostr_feature_gate_remains_fail_closed_until_real_relay_client_is_wired
```

## Required production implementation checklist

- Choose and audit a Rust Nostr client/runtime dependency before adding it to default builds.
- Connect only to configured `wss://` relay allowlists from app/DM/group/channel policy or signed invite bootstrap metadata.
- Use a scoped relay identity for event signing; do not reuse the user's MLS/account identity key unless a future ADR explicitly approves it.
- Derive random/hashed rendezvous tags from `RendezvousCapability`; never publish group names, channel names, display names, safety numbers, raw room seeds, raw SDP, ICE ufrag/passwords, TURN credentials, plaintext messages, or audio metadata.
- Publish/subscribe the existing sealed provider payload types: presence, WebRTC offer/answer/candidate envelopes, and room control envelopes.
- Map relay auth, rate-limit, message-too-large, unhealthy relay, and trust mismatch failures to typed `SignalingHealthState`/`AdapterReadinessState` values.
- Add local relay integration tests plus opt-in public relay smoke/soak tests before enabling Nostr as a selectable default.
- Add provider-visible capture scans for event tags/content/logs before any production-ready claim.

## Why this is not using a fake adapter

The local conformance bus can prove the shared `SignalingAdapter` trait shape, but it is not a Nostr relay client. Production readiness requires real Nostr event signing, relay subscription filters, relay error mapping, and public/local relay E2E evidence.

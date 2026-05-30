# Separate Rust QUIC rendezvous adapter readiness note

Status: groundwork only; not production-ready  
Scope: Discrypt self-hosted signaling adapter `discrypt_quic_rendezvous` / Cargo feature `discrypt-quic-rendezvous-adapter`

## Current contract

The `discrypt_quic_rendezvous` adapter is registered in the transport adapter registry and remains fail-closed until a real client for the sibling signaling service is wired. The server/service source must stay outside this repository. Compiling with `discrypt-quic-rendezvous-adapter` currently reports `implementation_unavailable` and must not make fallback selection treat the QUIC service as usable.

Verified guard:

```bash
cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter \
  quic_rendezvous_feature_gate_remains_fail_closed_until_sibling_client_is_wired
```

## Required production implementation checklist

- Keep the signaling service in the sibling repository and depend only on an audited client protocol/API from Discrypt.
- Accept only signed `quic://`/`https://` endpoint descriptors from app/DM/group/channel policy or signed invite bootstrap metadata.
- Validate service identity/trust fingerprint, ALPN, protocol version, expiry, max payload, abuse/rate-limit policy, and endpoint allowlist before use.
- Transport only sealed rendezvous, WebRTC offer/answer/candidate, and control envelopes. QUIC rendezvous does not replace WebRTC data/audio.
- Map trust mismatch, version mismatch, rate-limit, payload-too-large, outage, and provider-unhealthy states to typed health/readiness.
- Add a local sibling-service harness and staged/deployed service E2E before enabling as selectable.
- Add provider-visible capture scans proving no raw SDP/ICE/TURN credentials/room seeds/names/plaintext enter the rendezvous service.

## Why this is not using a fake adapter

A loopback or in-memory conformance adapter cannot prove QUIC identity, ALPN/version negotiation, sibling service trust, deployment health, or outage fallback. Production readiness requires the real sibling service client path plus local and staged service evidence.

# Separate Rust QUIC rendezvous adapter readiness note

Status: sibling-service HTTP API client wired with signed endpoint-fingerprint enforcement; native `quic://` transport still reserved; not production-ready  
Scope: Discrypt self-hosted signaling adapter `discrypt_quic_rendezvous` / Cargo feature `discrypt-quic-rendezvous-adapter`

## Current contract

The `discrypt_quic_rendezvous` adapter is registered in the transport adapter registry and now wires a real content-blind client for the sibling `discrypt-signaling` service API. The server/service source stays outside this repository. Compiling with `discrypt-quic-rendezvous-adapter` reports `implementation_available` and can run the adapter contract against the sibling service over validated `https://` or loopback `http://127.0.0.1` endpoints.

Native `quic://` is still reserved by the sibling service ADR and is rejected with an explicit error until an audited native QUIC client exists. This adapter does not replace WebRTC media/data paths; it is only a rendezvous/signaling provider.

Verified guard:

```bash
cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter \
  quic_rendezvous_feature_gate_is_selectable_but_rejects_reserved_native_quic_scheme

cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter \
  discrypt_rendezvous_sibling_service_roundtrip_when_binary_is_available -- --nocapture

cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter \
  quic_rendezvous_rejects_https_endpoint_without_signed_trust_fingerprint -- --nocapture

cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter \
  quic_rendezvous_rejects_mismatched_signed_trust_fingerprint -- --nocapture

cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter \
  quic_rendezvous_health_requires_matching_public_base_for_production -- --nocapture
```

## Required production implementation checklist

- Keep the signaling service in the sibling repository and depend only on the audited content-blind `/v1/signals/*` client protocol/API from Discrypt.
- Accept only signed `https://` endpoint descriptors from app/DM/group/channel policy or signed invite bootstrap metadata for production; `quic://` remains reserved until native QUIC client support lands.
- Require the signed endpoint trust fingerprint from app/DM/group/channel policy or invite bootstrap metadata before any production/self-hosted endpoint is used, and reject mismatched fingerprints before health probes.
- Validate `/healthz` status, service label, and advertised `public_base_url`; production/self-hosted service health must advertise the same normalized endpoint being used.
- Still add TLS certificate/public-key pin validation, ALPN, protocol version, expiry, max payload, abuse/rate-limit policy, and endpoint allowlist proof before production release.
- Transport only sealed rendezvous, WebRTC offer/answer/candidate, and control envelopes. QUIC rendezvous does not replace WebRTC data/audio.
- Map trust mismatch, version mismatch, rate-limit, payload-too-large, outage, and provider-unhealthy states to typed health/readiness.
- Extend the local sibling-service harness into staged/deployed service E2E with TLS-edge identity/fingerprint checks.
- Add provider-visible capture scans proving no raw SDP/ICE/TURN credentials/room seeds/names/plaintext enter the rendezvous service.

## Why this is not using a fake adapter

A loopback or in-memory conformance adapter cannot prove TLS certificate identity, future QUIC ALPN/version negotiation, sibling service trust, deployment health, or outage fallback. The current adapter uses the real sibling service API when the external binary is available and enforces the signed endpoint fingerprint carried by policy/invites; production readiness still requires staged service evidence, TLS certificate/public-key pinning, and native QUIC proof if `quic://` endpoints are enabled.

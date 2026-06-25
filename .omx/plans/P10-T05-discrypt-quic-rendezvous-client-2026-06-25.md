# P10-T05 Discrypt QUIC Rendezvous Client Plan

Source issue: PER-86 / P10-T05, Phase 10 signaling adapters, public profiles,
and abuse/privacy.

## Requirements Summary

- Harden the `discrypt_quic_rendezvous` transport adapter as a
  signaling/rendezvous client for the sibling Discrypt signaling service.
- Validate signed endpoint trust fingerprints and service health
  schema/protocol/version material before using production or self-hosted
  endpoints.
- Fail closed for missing, mismatched, malformed, or stale trust material.
- Preserve the provider invariant: the rendezvous service can carry only
  presence and sealed WebRTC negotiation envelopes, never application
  text/control frames, receipts, plaintext, raw SDP/ICE, TURN credentials,
  keys, or media.

## Code Paths

- `crates/transport/src/provider_adapters.rs`: Discrypt QUIC rendezvous adapter,
  sibling-service health validation, provider-visible wire envelope encoding,
  and adapter tests.
- `docs/adapters/quic-rendezvous-adapter-readiness.md`: adapter contract and
  remaining production gaps.
- `docs/release/public-signaling-production-status.md`: current release boundary
  for public signaling adapters.

## Acceptance Criteria

- Production/self-hosted rendezvous profiles require a signed endpoint trust
  fingerprint and reject mismatch before health probes.
- `/healthz` must advertise supported schema/protocol version, matching
  `public_base_url`, bounded body/rate policy, matching service identity,
  accepted ALPN, future expiry, rotation policy, and endpoint allowlist
  commitment.
- Provider-visible wire envelopes reject unsupported schema/kind combinations,
  stale service responses, plaintext markers, and any application/control relay
  envelope.
- `broadcast_control` and `take_control_payloads` remain fail-closed.
- Local sibling-service tests remain the evidence level unless a staged HTTPS/WSS
  service endpoint is explicitly supplied.

## Failure Modes And Safety

- Missing or mismatched endpoint trust: fail before network health probing.
- Malformed or expired service identity proof: fail during health validation and
  do not open a provider session.
- Service response with expired signals, unsupported wire schema, or mismatched
  signal kind: ignored or rejected before surfacing to callers.
- Application relay attempt: rejected with the existing provider-relay-disabled
  error; no fallback over the rendezvous service is added.

Rollback is low risk: changes are scoped to transport validation/tests and docs.
No storage, OpenMLS, UI, voice, overlay, or app-config signing behavior changes.

## Verification Strategy

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- Targeted transport tests:
  `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features discrypt-quic-rendezvous-adapter quic_rendezvous_ -- --test-threads=1`
- Local sibling service harness:
  `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features discrypt-quic-rendezvous-adapter discrypt_rendezvous_sibling_service_roundtrip_when_binary_is_available -- --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-transport --features discrypt-quic-rendezvous-adapter --lib -- -D warnings`
- `git diff --check`

Staged/deployed HTTPS/WSS proof is intentionally not claimed unless an external
endpoint is supplied. Native `quic://` remains reserved until audited.

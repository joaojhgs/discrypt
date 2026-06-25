# PER-86 Discrypt QUIC Rendezvous Client Evidence

Issue: PER-86 / P10-T05  
Scope: `discrypt_quic_rendezvous` transport adapter behind
`discrypt-quic-rendezvous-adapter`

## Result

The Discrypt rendezvous adapter remains a signaling-only client for the sibling
service HTTP API. It now validates provider-visible wire envelopes before
publish and after take:

- presence envelopes must use the current rendezvous wire schema and non-zero
  TTL;
- sealed WebRTC negotiation envelopes must use the current wire schema and
  sealed payload version, and their payload kind must match the sibling service
  signal bucket;
- plaintext markers such as raw SDP, ICE credentials, TURN credentials, content
  keys, MLS exporter material, room names, and message plaintext are rejected;
- application/control relay envelopes are rejected with the existing
  provider-application-relay-disabled path.

The existing production/self-hosted health gate remains in force: signed
endpoint trust fingerprints are required, mismatches fail before health probes,
and `/healthz` must advertise supported schema/protocol version, matching
public base URL, body/rate policy, service identity, accepted ALPN, future
expiry, rotation policy, and endpoint allowlist commitment.

## Evidence Level

This is local Rust transport/harness evidence, not production-ready deployed
service evidence. Native `quic://` remains reserved until a native client is
audited. A staged/deployed HTTPS/WSS endpoint with external TLS/public-key pin
capture evidence is still required before claiming production readiness.

## Verification

Local results from this branch:

```bash
RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check
# passed

RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features discrypt-quic-rendezvous-adapter quic_rendezvous_ -- --test-threads=1
# passed: 12 unit tests plus 3 opt-in public QUIC rendezvous tests in skipped/green mode

RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features discrypt-quic-rendezvous-adapter discrypt_rendezvous_sibling_service_roundtrip_when_binary_is_available -- --nocapture
# passed with explicit skip: sibling binary ../discrypt-signaling/target/debug/discrypt-signaling-server is not built

RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-transport --features discrypt-quic-rendezvous-adapter --lib -- -D warnings
# passed

git diff --check
# passed
```

The sibling service roundtrip skips explicitly when
`../discrypt-signaling/target/debug/discrypt-signaling-server` is not present.

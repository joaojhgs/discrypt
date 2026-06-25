# PER-88 Provider-Visible Privacy Captures Evidence - 2026-06-25

## Scope

PER-88/P10-T07 adds retained provider-visible privacy capture evidence for the
Phase 10 signaling adapters: MQTT, Nostr, IPFS/libp2p PubSub, and Discrypt QUIC
rendezvous.

This is repository-local deterministic provider-visible capture evidence. It
does not claim production-ready external host packet capture, staged QUIC
service capture, public IPFS swarm availability, installed-app two-profile E2E,
OpenMLS admission, voice/media, packaging, or full release readiness.
external host packet capture remains required before the final production
release claim.

## Implemented Behavior

- `local_conformance_adapters_deliver_opaque_dm_payloads_without_plaintext_leaks`
  exercises provider-visible presence and sealed WebRTC negotiation material for
  all four adapter boundaries.
- The test scans provider-visible bytes and redacted observability for display
  names, group/channel names, raw SDP, ICE ufrag/pwd/candidates, TURN
  credentials, private/key material, plaintext messages, media markers, MLS
  exporter material, SFrame keys, content keys, and production-overclaim tokens.
- The npm gate sets `DISCRYPT_PROVIDER_METADATA_CAPTURE_OUT` and retains:
  `target/provider-visible-captures/per88-g133-provider-visible-capture.json`.
- The retained artifact records adapter kind, row count, byte total, SHA-256
  digests, and per-adapter scan status. It intentionally does not retain raw
  provider-visible payload bytes.
- Plaintext/raw SDP/ICE provider payload attempts continue to fail closed before
  provider-visible relay state.
- Providers remain signaling/rendezvous only; application text/control and media
  relay attempts remain rejected.

## Artifact

- `target/provider-visible-captures/per88-g133-provider-visible-capture.json`

Expected artifact properties:

- `captures` contains `mqtt`, `nostr`, `ipfs_pubsub`, and
  `discrypt_quic_rendezvous`.
- Every capture has positive provider-visible row and byte counts.
- Every capture has `forbidden_field_scan: "passed"`.
- Every capture has `raw_payloads_retained: false`.

## Verification

Commands for this branch:

```bash
RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check
RUSTUP_TOOLCHAIN=1.89.0 cargo test -q -p discrypt-transport local_conformance_adapters_deliver_opaque_dm_payloads_without_plaintext_leaks -- --nocapture
RUSTUP_TOOLCHAIN=1.89.0 cargo test -q -p discrypt-transport local_conformance_adapter_rejects_plaintext_sdp_and_ice_markers -- --nocapture
npm --prefix apps/ui run test:provider-metadata-capture-g133
git diff --check
```

The npm gate validates the release/security docs, runs the two Rust gates, writes
the artifact, and fails if the artifact is missing, lacks any required adapter,
retains raw payloads, or contains forbidden-field tokens.

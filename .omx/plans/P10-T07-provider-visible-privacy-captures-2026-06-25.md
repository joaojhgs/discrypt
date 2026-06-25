# P10-T07 Provider-Visible Privacy Captures Plan - 2026-06-25

## Source Scope

- Issue: PER-88 / P10-T07, Phase 10 signaling adapters, public profiles, and abuse/privacy.
- Source plan context from the issue body: provider-visible privacy captures must prove no names, raw SDP, ICE passwords, TURN credentials, keys, plaintext, or media payloads appear in provider payloads/logs.
- The named master plan file is not present in this checkout; adjacent Phase 10 plans use the issue body plus current release docs as the authoritative task scope.
- Current release context: `docs/release/handoff-2026-06-10-current-state.md`, `docs/release/public-signaling-production-status.md`, and `docs/release/release-verification-matrix.md` require fresh evidence and honest evidence-level labels.
- Code anchors: `crates/transport/src/provider_adapters.rs`, `scripts/check-provider-metadata-capture-g133.mjs`, `docs/security/g133-provider-visible-metadata-capture.md`, and `docs/release/public-signaling-production-status.md`.

## Acceptance Criteria

- The provider-visible capture gate covers MQTT, Nostr, IPFS/libp2p PubSub, and Discrypt QUIC rendezvous boundaries.
- The gate scans for display/group/channel names, raw SDP, ICE ufrag/pwd/candidates, TURN credentials, private/key material, message plaintext, media payload markers, MLS/SFrame/content-key material, and sensitive auth tokens.
- The gate retains a deterministic artifact under `target/provider-visible-captures/` with adapter coverage and scan status, but no raw provider-visible payload bytes.
- Provider adapters remain signaling/rendezvous only; application text/control/media relay remains fail-closed.
- Release docs state this is repository-local deterministic evidence and that external host packet capture remains a separate release-run artifact.

## Implementation Steps

1. Extend the existing local conformance provider-visible test to optionally write a sanitized JSON capture artifact when `DISCRYPT_PROVIDER_METADATA_CAPTURE_OUT` is set.
2. Update `scripts/check-provider-metadata-capture-g133.mjs` to set that output path, run the existing conformance and plaintext-rejection tests, and validate artifact coverage/no-raw/no-forbidden-token properties.
3. Update G133 and PER-88 release evidence docs with commands, artifact path, skipped external evidence, and signaling-only boundaries.
4. Run the G133 npm gate, targeted Rust tests, formatting, and diff checks.

## Failure Modes And Safety

- Forbidden marker found in provider-visible bytes: the Rust test fails before the script records a passed artifact.
- Artifact missing or incomplete: the npm gate fails and reports the missing adapter or row.
- Raw payload retention accidentally enabled: the npm gate fails on `raw_payloads_retained`.
- Public/deployed provider capture is unavailable in this environment: docs keep that as an explicit release-run artifact gap, not a production-ready claim.
- Rollback is low risk: changes are scoped to tests/scripts/docs and do not alter production adapter runtime behavior.

## Verification Strategy

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -q -p discrypt-transport local_conformance_adapters_deliver_opaque_dm_payloads_without_plaintext_leaks -- --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -q -p discrypt-transport local_conformance_adapter_rejects_plaintext_sdp_and_ice_markers -- --nocapture`
- `npm --prefix apps/ui run test:provider-metadata-capture-g133`
- `git diff --check`

This is local deterministic provider-visible capture evidence only. It does not claim external libpcap/tcpdump packet capture, staged QUIC service capture, public IPFS swarm availability, installed two-profile app E2E, OpenMLS admission readiness, voice/media readiness, or full production readiness.

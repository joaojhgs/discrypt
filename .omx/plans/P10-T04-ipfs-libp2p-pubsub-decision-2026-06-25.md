# P10-T04 IPFS/libp2p PubSub Decision Plan

Source issue: PER-85 / P10-T04, Phase 10 signaling adapters, public profiles,
and abuse/privacy.

## Requirements Summary

- Decide whether IPFS/libp2p PubSub is production-eligible for Discrypt
  signaling now.
- Preserve the transport invariant that providers carry only rendezvous,
  presence, and sealed WebRTC negotiation envelopes; providers never relay
  application text/control frames, receipts, or media.
- Either provide multi-node harness evidence for a real adapter or keep IPFS
  PubSub explicitly non-default/blocked with documented operator implications.

## Decision

IPFS/libp2p PubSub remains non-default for production unless an operator
explicitly configures direct Discrypt topic-peer multiaddrs that include
`/p2p/<peer-id>`. Generic DNS/bootstrap peer discovery remains blocked because it
does not prove a reachable subscribed Discrypt topic mesh and can make the UI or
backend overstate provider readiness.

## Code References

- `crates/transport/src/policy.rs`: validates IPFS production endpoints as
  explicit direct peer multiaddrs and rejects DNS/bootstrap defaults.
- `crates/transport/src/provider_adapters.rs`: feature-gated rust-libp2p
  adapter, bounded gossipsub config, direct-topic-peer harness tests, typed
  health failures, and provider application relay rejection.
- `apps/desktop/src-tauri/src/lib.rs`: default profile generation omits IPFS
  when no bootstrap endpoints are configured and validates configured defaults
  through the transport profile path.
- `docs/release/handoff-2026-06-10-current-state.md`: current release evidence
  boundary and backend-truth invariants.

## Acceptance Criteria

- Unconfigured IPFS/libp2p PubSub is omitted from default desktop signaling
  profiles.
- Configured IPFS defaults must validate through the transport policy path and
  fail closed for DNS/bootstrap or bare dialable endpoints.
- The feature-gated adapter continues to prove local/direct-topic-peer
  signaling-only behavior with opaque presence and sealed WebRTC negotiation
  envelopes.
- Release evidence documents the non-default/block decision, root cause,
  operator implications, verification commands, and skipped public checks.

## Failure Modes And Safety

- DNS/bootstrap endpoint: rejected before production profile use; no provider
  connection attempt is required to fail safely.
- Bare dialable multiaddr without `/p2p/<peer-id>`: rejected because it does not
  identify the Discrypt topic peer.
- No peers subscribed to the topic: maps to typed provider-unhealthy diagnostics
  and does not claim connected delivery.
- Oversized sealed provider envelope: maps to
  `provider_message_too_large`.
- Application relay attempt: `broadcast_control` and `take_control_payloads`
  fail closed.

Rollback is low risk: this task adds a regression test and release evidence
around existing fail-closed behavior. It does not change storage, OpenMLS,
membership, UI delivery claims, or provider relay behavior.

## Verification Strategy

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_ -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml default_ipfs_profile_requires_explicit_direct_topic_peer_endpoint default_profiles_omit_unconfigured_ipfs_quic_placeholder_endpoints -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-transport --features ipfs-pubsub-adapter --lib -- -D warnings`
- `git diff --check`

Public IPFS/libp2p E2E is intentionally not required unless an operator supplies
`DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS` containing direct topic-peer
multiaddrs. Without that, the release result is default-off/block evidence, not
production-ready public IPFS evidence.

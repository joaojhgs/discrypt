# PER-85 / P10-T04 IPFS/libp2p PubSub Decision Evidence

Date: 2026-06-25
Branch: `multica/P10-T04-ipfs-libp2p-pubsub`

## Scope

This is Phase 10 IPFS/libp2p PubSub adapter decision evidence. It covers the
provider signaling boundary, default-off behavior, direct-topic-peer bootstrap
requirements, and local multi-node harness evidence for the feature-gated
adapter.

It is not a production-ready public IPFS default claim, installed-app
split-machine proof, OpenMLS membership proof, text/media delivery proof, or
packaging evidence.

## Decision

IPFS/libp2p PubSub stays non-default for production. It can be selected only
when explicitly configured with direct Discrypt topic-peer multiaddrs that
include `/p2p/<peer-id>`. Generic DNS bootstrap and bare public libp2p peers
remain blocked until audited topic-peer discovery can prove reachable subscribed
Discrypt peers without overstating readiness.

## Current Implementation Boundary

- No built-in IPFS bootstrap endpoints are configured by default.
- Desktop default signaling profiles omit IPFS/libp2p PubSub unless
  `DISCRYPT_DEFAULT_IPFS_BOOTSTRAP_ENDPOINTS` is set.
- Transport policy accepts production IPFS endpoints only when they are explicit
  direct peer multiaddrs.
- The feature-gated rust-libp2p adapter uses bounded gossipsub resource policy
  and strict validation.
- Provider-visible data is limited to hashed rendezvous topic metadata plus
  opaque Discrypt envelopes for presence and sealed WebRTC negotiation.
- Application text/control and media relay over the provider remains disabled:
  `broadcast_control` and `take_control_payloads` fail closed.

## Block Rationale

Generic IPFS/libp2p bootstrap peers do not prove that another Discrypt peer is
subscribed to the same gossipsub topic. Treating generic DNS/bootstrap discovery
as production readiness would violate the release invariant that connected,
online, and delivery states require backend/transport evidence. The safe
production posture is therefore explicit direct topic-peer configuration only,
with public default selection blocked.

## Operator Implications

Operators who want IPFS/libp2p PubSub must provide direct topic-peer multiaddrs
through `DISCRYPT_DEFAULT_IPFS_BOOTSTRAP_ENDPOINTS` or an equivalent signed
profile. Endpoints must identify reachable Discrypt peers with `/p2p/<peer-id>`.
If no such endpoint is configured, IPFS is omitted from defaults and fallback
selection uses other configured signaling providers.

## Verification

Passed on this branch:

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_ -- --test-threads=1`
  - 14 targeted IPFS tests passed, including bounded/default bootstrap policy,
    DNS/non-topic-peer rejection, local two-peer presence/signal roundtrip,
    direct-topic-peer roundtrip, WebRTC text/control proof, runtime-pair
    text/control proof, unreachable bootstrap health mapping, duplicate storm
    health mapping, and oversized envelope failure mapping.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml default_profiles_ -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml default_ipfs_profile_requires_explicit_direct_topic_peer_endpoint -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-transport --features ipfs-pubsub-adapter --lib -- -D warnings`
- `git diff --check`

## Skipped

- Public IPFS/libp2p E2E was not run because this runner has no
  operator-supplied `DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS` direct
  topic-peer set. That skip is intentional for this decision: absent explicit
  direct topic-peer endpoints, IPFS remains non-default/blocked rather than
  promoted as a public default.

## Evidence Boundary

Passing local direct-topic-peer harnesses prove that the feature-gated adapter
can exchange opaque presence and sealed WebRTC negotiation envelopes between
local libp2p nodes and that provider application relay attempts fail closed.
They do not prove a safe public IPFS default. Public IPFS evidence requires an
operator-supplied direct topic-peer set and a fresh opt-in run with
`DISCRYPT_PUBLIC_IPFS_E2E=1` or the WebRTC-specific IPFS gates.

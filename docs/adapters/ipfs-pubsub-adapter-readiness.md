# IPFS/libp2p PubSub signaling adapter readiness note

Status: groundwork only; not production-ready  
Scope: Discrypt serverless signaling adapter `ipfs_pubsub` / Cargo feature `ipfs-pubsub-adapter`

## Current contract

The `ipfs_pubsub` adapter is registered in the transport adapter registry and remains fail-closed until a real audited libp2p PubSub runtime is wired. Compiling with `ipfs-pubsub-adapter` currently reports `implementation_unavailable` and must not make fallback selection treat IPFS as usable.

Verified guard:

```bash
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_feature_gate_remains_fail_closed_until_real_pubsub_runtime_is_wired
```

## Required production implementation checklist

- Decide Rust-native `libp2p` versus a constrained JS/WebView bridge in an ADR before adding production dependencies.
- Configure bootstrap peers, resource limits, discovery timeout, topic TTL/cleanup, unsubscribe, and duplicate suppression.
- Derive PubSub topics from `RendezvousCapability`; never publish group names, channel names, display names, safety numbers, raw room seeds, raw SDP, ICE ufrag/passwords, TURN credentials, plaintext messages, or audio metadata.
- Publish/subscribe only sealed presence, sealed WebRTC negotiation envelopes, and sealed control payloads.
- Map bootstrap failure, swarm/resource exhaustion, message-too-large, duplicate storm, and provider-unhealthy states to typed health/readiness.
- Add local multi-node libp2p harness evidence before any public/default enablement.
- Add public/realistic bootstrap smoke only if it is reliable enough to be release-gated; otherwise keep IPFS non-default with explicit blocker evidence.

## Why this is not using a fake adapter

The local conformance bus proves the shared provider trait shape, but it does not exercise libp2p discovery, PubSub filters, peer scoring/resource limits, or public bootstrap behavior. Production readiness requires real runtime E2E and provider-visible privacy captures.

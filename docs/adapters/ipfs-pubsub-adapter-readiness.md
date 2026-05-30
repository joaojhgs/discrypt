# IPFS/libp2p PubSub signaling adapter readiness note

Status: real rust-libp2p adapter wired; not production-default yet
Scope: Discrypt serverless signaling adapter `ipfs_pubsub` / Cargo feature `ipfs-pubsub-adapter`

## Current contract

The `ipfs_pubsub` adapter is now a real feature-gated rust-libp2p gossipsub provider client. When compiled with `ipfs-pubsub-adapter`, it is selectable in the transport adapter registry and uses configured libp2p multiaddrs as bootstrap peers. It does **not** use a Kubo HTTP API, and it does not publish raw SDP, ICE credentials, room names, display names, invite secrets, message plaintext, or audio plaintext.

Current behavior:

- `IpfsPubsubProviderAdapter` is compiled behind Cargo feature `ipfs-pubsub-adapter`.
- The adapter validates `SignalingAdapterProfile` endpoints as libp2p multiaddrs.
- Each room subscribes to a gossipsub topic derived from `RendezvousCapability.topic`.
- Published gossipsub messages are JSON envelopes containing only sealed/opaque Discrypt signaling payloads.
- Local duplicate suppression is enabled by message fingerprint.
- `leave()` unsubscribes from the topic and stops the swarm task.

Verified commands:

```bash
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_adapter_feature_is_selectable_with_real_libp2p_client

cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_local_two_peer_presence_signal_and_control_roundtrip -- --nocapture

cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  public_ipfs_two_peer_signaling_smoke -- --nocapture
```

The public smoke is still opt-in. It skips unless `DISCRYPT_PUBLIC_IPFS_E2E=1` and `DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS` contains comma-separated bootstrap multiaddrs.

## Remaining production hardening checklist

- Add public/default bootstrap peer policy: decide which public libp2p bootstrap peers are acceptable, versioned, and rotated.
- Add resource-limit and peer-score configuration before making IPFS a default route.
- Add typed health mapping for bootstrap failures, resource exhaustion, message-too-large, duplicate storms, and provider-unhealthy states.
- Add provider-visible metadata capture scans for gossipsub topics and payloads.
- Add public/realistic bootstrap evidence with `DISCRYPT_PUBLIC_IPFS_E2E=1`; if public peers are unreliable, keep IPFS non-default and require explicit group/DM configuration.
- Wire this adapter through the Tauri app runtime path and two-profile app E2E; current proof is at the transport adapter boundary.

## Why this is no longer a fake adapter

The old fail-closed guard has been replaced by a real rust-libp2p gossipsub client, a local two-node roundtrip, and an opt-in public bootstrap smoke. The remaining blockers are production hardening, public bootstrap policy, app/runtime integration, and full two-installed-app E2E.

# IPFS/libp2p PubSub signaling adapter readiness note

Status: real rust-libp2p adapter wired with bounded bootstrap/resource policy; not production-default yet
Scope: Discrypt serverless signaling adapter `ipfs_pubsub` / Cargo feature `ipfs-pubsub-adapter`

## Current contract

The `ipfs_pubsub` adapter is now a real feature-gated rust-libp2p gossipsub provider client. When compiled with `ipfs-pubsub-adapter`, it is selectable in the transport adapter registry and uses configured libp2p multiaddrs as bootstrap peers. It does **not** use a Kubo HTTP API, and it does not publish raw SDP, ICE credentials, room names, display names, invite secrets, message plaintext, or audio plaintext.

Current behavior:

- `IpfsPubsubProviderAdapter` is compiled behind Cargo feature `ipfs-pubsub-adapter`.
- The adapter validates `SignalingAdapterProfile` endpoints as libp2p multiaddrs.
- Each room subscribes to a gossipsub topic derived from `RendezvousCapability.topic`.
- Published gossipsub messages are JSON envelopes containing only sealed/opaque Discrypt signaling payloads.
- A versioned explicit bootstrap policy (`IPFS_PUBSUB_BOOTSTRAP_POLICY_VERSION=1`) defines allowed public libp2p bootstrap seeds as discovery seeds only, not guaranteed topic relays.
- Resource limits are enforced in code: max 16 bootstrap endpoints, duplicate endpoint rejection, 64 KiB max transmit/envelope size, bounded command queue, strict validation, flood-publish disabled, and bounded gossipsub mesh/history/duplicate-cache settings.
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

cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_resource_policy_is_bounded_and_default_bootstrap_is_parseable -- --nocapture
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_bootstrap_policy_rejects_duplicates_and_overflow -- --nocapture
```

The public smoke is still opt-in. It skips unless `DISCRYPT_PUBLIC_IPFS_E2E=1` and `DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS` contains comma-separated bootstrap multiaddrs.

## Remaining production hardening checklist

- Public/default bootstrap peer policy is now versioned and parse-tested, with generic `bootstrap.libp2p.io` seed(s) documented as discovery-only and capped by a resource policy. Rotation remains a release-management task before IPFS becomes a default route.
- Resource-limit configuration is implemented for the current adapter boundary: bounded bootstrap endpoint count, duplicate rejection, 64 KiB transmit/envelope limit, bounded command queue, strict gossipsub validation, flood-publish disabled, and bounded mesh/history/duplicate-cache settings. Full peer-score tuning remains future hardening before default enablement.
- Add typed health mapping for remaining bootstrap failures, resource exhaustion beyond local policy rejection, duplicate storms, and provider-unhealthy states.
- Add provider-visible metadata capture scans for gossipsub topics and payloads.
- Add public/realistic bootstrap evidence with `DISCRYPT_PUBLIC_IPFS_E2E=1`; if public peers are unreliable, keep IPFS non-default and require explicit group/DM configuration.
- Wire this adapter through the Tauri app runtime path and two-profile app E2E; current proof is at the transport adapter boundary.

## Why this is no longer a fake adapter

The old fail-closed guard has been replaced by a real rust-libp2p gossipsub client, a local two-node roundtrip, an opt-in public bootstrap smoke, and bounded bootstrap/resource policy tests. The remaining blockers are topic-peer public-swarm evidence, full provider health mapping, app/runtime integration, and full two-installed-app E2E.

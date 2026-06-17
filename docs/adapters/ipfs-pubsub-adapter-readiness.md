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
- A versioned explicit bootstrap policy (`IPFS_PUBSUB_BOOTSTRAP_POLICY_VERSION=1`) currently defines an empty public default while the libp2p/Hickory DNS stack is audit-blocked; production profiles must provide explicit direct `/ip4` or `/ip6` multiaddrs as discovery/topic-peer seeds.
- Resource limits are enforced in code: max 16 bootstrap endpoints, duplicate endpoint rejection, 64 KiB max transmit/envelope size, bounded command queue, strict validation, flood-publish disabled, and bounded gossipsub mesh/history/duplicate-cache settings.
- Local duplicate suppression is enabled by message fingerprint.
- `leave()` unsubscribes from the topic and stops the swarm task.

Verified commands:

```bash
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_adapter_feature_is_selectable_with_real_libp2p_client

cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_local_two_peer_presence_and_signal_roundtrip -- --nocapture

cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  public_ipfs_two_peer_signaling_smoke -- --nocapture

cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_resource_policy_is_bounded_and_default_bootstrap_is_parseable -- --nocapture
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_bootstrap_policy_rejects_duplicates_and_overflow -- --nocapture
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_oversized_envelope_maps_to_typed_health -- --nocapture
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_insufficient_peers_reports_actionable_topic_mesh_error -- --nocapture
```

The public smoke is still opt-in. It skips unless `DISCRYPT_PUBLIC_IPFS_E2E=1` and `DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS` contains comma-separated explicit direct topic-peer multiaddrs. When the gate is enabled with generic DNS/bootstrap or bare dialable endpoints, it now fails fast with a typed connectivity-policy error instead of pretending the route is acceptable.

## Remaining production hardening checklist

- Public/default bootstrap peer policy is now versioned and parse-tested, but public defaults are intentionally empty while the libp2p/Hickory DNS stack remains audit-blocked. Explicit direct bootstrap multiaddrs are still capped by the resource policy. Rotation plus topic-peer discovery remains a release-management task before IPFS becomes a default route.
- Resource-limit configuration is implemented for the current adapter boundary: bounded bootstrap endpoint count, duplicate rejection, 64 KiB transmit/envelope limit, bounded command queue, strict gossipsub validation, flood-publish disabled, and bounded mesh/history/duplicate-cache settings. Full peer-score tuning remains future hardening before default enablement.
- Typed health mapping covers oversized envelopes (`provider_message_too_large`), topic mesh unavailability, unreachable bootstrap connection, duplicate-envelope storms, and libp2p listener/runtime failures as redacted `failure_class`/`health_state` details.
- Provider-visible metadata capture is covered by G133 (`npm --prefix apps/ui run test:provider-metadata-capture-g133`) for MQTT, Nostr, IPFS/libp2p, and QUIC adapter boundaries. External host packet captures remain a release-run artifact.
- Remaining production blocker: add public/realistic direct-bootstrap evidence with `DISCRYPT_PUBLIC_IPFS_E2E=1` and explicit direct topic-peer multiaddrs; keep IPFS non-default until this passes on real public peers without DNS bootstrap.
- Tauri runtime can select the adapter through the shared adapter factory when compiled with `ipfs-pubsub-adapter` and given explicit IPFS endpoints, but the remaining app proof is two-profile installed-app E2E over a real public/topic-peer IPFS route.

## Why this is no longer a fake adapter

The old fail-closed guard has been replaced by a real rust-libp2p gossipsub client, a local two-node roundtrip, an opt-in public bootstrap smoke, bounded bootstrap/resource policy tests, typed provider-health failure coverage, and deterministic G133 metadata capture. The remaining blockers are accepted public-default semantics, topic-peer public-swarm evidence, and full two-installed-app E2E over that route.

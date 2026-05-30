# Public signaling production status

_Last updated: 2026-05-30_

## Executive status

Discrypt is **not production-complete** for the full serverless P2P encrypted app target yet. This update has real MQTT, Nostr, and IPFS/libp2p signaling paths at the Rust transport boundary, but it is still not a complete installed-app proof:

- **MQTT public signaling: implemented behind `mqtt-adapter` and latest reruns passed against a real public broker after the adapter began waiting for broker subscription acknowledgements before publishing.**
- **Nostr signaling: real relay client is wired behind `nostr-adapter` and verified against a public relay.**
- **IPFS/libp2p PubSub signaling: real rust-libp2p gossipsub client is wired behind `ipfs-pubsub-adapter` and verified with a local two-node transport roundtrip; public bootstrap smoke exists but still requires configured bootstrap multiaddrs.**
- **Separate Rust QUIC rendezvous adapter: fail-closed groundwork is locked by `discrypt-quic-rendezvous-adapter` feature tests; intended to point at the sibling service once the external adapter client is wired.**
- **Full app-level two-Tauri-instance DM/group text + voice E2E over those adapters: not done.** Current proof is at the transport signaling adapter layer plus a Tauri command/UI-triggered provider-adapter roundtrip probe; it is still not the complete UI/backend/media plane.

## What was implemented now

### MQTT real adapter

Files:

- `crates/transport/src/provider_adapters.rs`
- `crates/transport/src/lib.rs`
- `crates/transport/Cargo.toml`
- `crates/transport/tests/public_signaling_e2e.rs`

Behavior:

- Adds `MqttProviderAdapter` behind Cargo feature `mqtt-adapter`.
- Uses `rumqttc` with TLS-capable MQTT URLs.
- Accepts validated `SignalingAdapterProfile` values with `SignalingAdapterKind::Mqtt`.
- Joins a pre-derived `RendezvousCapability` and creates provider-visible topics under:
  - `discrypt/v1/rendezvous/{hashed-topic}/presence`
  - `discrypt/v1/rendezvous/{hashed-topic}/signal/{peer-id}`
  - `discrypt/v1/rendezvous/{hashed-topic}/control`
- Publishes only sealed/opaque payload envelopes:
  - encrypted presence bytes
  - `SealedWebRtcNegotiationPayload` for offer/answer/candidate signaling
  - opaque room control bytes
- Keeps the public broker away from raw SDP, ICE credentials, display names, group names, invite secrets, message plaintext, and audio plaintext.
- Marks MQTT boundary readiness as `implementation_available` only when compiled with `mqtt-adapter`.
- Leaves the generic `FeatureGatedProviderAdapter` fail-closed; production code should instantiate `MqttProviderAdapter` for MQTT.
- **UI state integration:** command state now surfaces transport/join/voice status cards from command state and keeps route/media claims policy-only when proof is absent.

### Nostr and IPFS real adapters plus remaining fail-closed QUIC readiness groundwork

Files:

- `docs/adapters/nostr-adapter-readiness.md`
- `docs/adapters/ipfs-pubsub-adapter-readiness.md`
- `docs/adapters/quic-rendezvous-adapter-readiness.md`

Commands:

```bash
cargo test -q -p discrypt-transport --features nostr-adapter \
  nostr_adapter_feature_is_selectable_with_real_relay_client
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_adapter_feature_is_selectable_with_real_libp2p_client

cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_local_two_peer_presence_signal_and_control_roundtrip -- --nocapture
cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter \
  quic_rendezvous_feature_gate_remains_fail_closed_until_sibling_client_is_wired
```

Result: Nostr is selectable when feature-gated and backed by `nostr-sdk`; IPFS/libp2p is selectable when feature-gated and backed by rust-libp2p gossipsub; QUIC still passes fail-closed guards proving it remains non-selectable until the sibling-service client is implemented and tested.


### Tauri runtime adapter probe

Files:

- `crates/transport/src/provider_adapters.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/ui/src/commands.ts`
- `apps/ui/src/main.tsx`

Behavior:

- Adds `probe_provider_adapter_roundtrip(...)`, a reusable transport-layer probe that connects two local peers through the selected real provider adapter and verifies opaque presence, sealed WebRTC-negotiation payload, and sealed control broadcast delivery.
- Extends `start_signaling_session` with `adapter_probe=true` and optional `adapter_kind` so the Tauri backend can run the selected DM/group/invite signaling profile instead of only showing static readiness.
- Persists structured `adapter_probe_status`, `adapter_probe_detail`, and redacted probe evidence into transport diagnostics.
- Adds a UI "Probe adapter" action in the transport status panel.
- Uses public Nostr (`wss://relay.damus.io`) first and public MQTT (`mqtts://broker.emqx.io:8883`) second as zero-config default endpoint candidates when no `DISCRYPT_DEFAULT_*`/`VITE_DISCRYPT_DEFAULT_*` override is supplied; IPFS and QUIC still require explicit endpoint configuration because no production default bootstrap/self-hosted endpoint has been accepted yet.
- Keeps route/media claims separate: a successful adapter probe proves provider rendezvous only; it does not mark ICE, data-channel, or voice media as connected.

Verification:

```bash
cargo test -q -p discrypt-transport provider_adapter_roundtrip_probe_quic_fails_closed
cargo test -q -p discrypt-desktop signaling_adapter_probe_surfaces_runtime_blocker_without_route_claim
npm --prefix apps/ui run typecheck
cargo check -q -p discrypt-desktop --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter
```

### Public real-network tests

File:

- `crates/transport/tests/public_signaling_e2e.rs`

MQTT command:

```bash
DISCRYPT_PUBLIC_SIGNALING_E2E=1 \
  cargo test -q -p discrypt-transport --features mqtt-adapter \
  public_mqtt_two_peer_presence_signal_and_control_roundtrip -- --nocapture
```

MQTT status:

- Latest reruns passed against default public broker `mqtts://broker.emqx.io:8883` after the adapter started waiting for all broker `SUBACK`s before treating a joined room as ready.
- Prior failures on `broker.emqx.io` timed out waiting for peer delivery and are treated as a subscription-readiness race that this fix targets; `test.mosquitto.org` still has TLS certificate incompatibility and `broker.hivemq.com` still hit network timeout in this environment.

Nostr command:

```bash
DISCRYPT_PUBLIC_NOSTR_E2E=1 \
DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://relay.damus.io \
  cargo test -q -p discrypt-transport --features nostr-adapter \
  public_nostr_two_peer_presence_signal_and_control_roundtrip -- --nocapture
```

Nostr status:

- Latest rerun passed against `wss://relay.damus.io`.
- The test creates two independent transport sessions (`alice-device` and `bob-device`) on the same hashed DM rendezvous topic.
- It verifies opaque provider roundtrip only:
  1. Alice publishes sealed presence and Bob receives it.
  2. Alice sends a sealed WebRTC offer envelope to Bob and Bob receives it.
  3. Bob broadcasts sealed control and Alice receives it.

These are real public signaling proofs at the provider adapter boundary, but they are **not** full two-installed-app data-channel or media proofs.

## What remains open before production

### P0: adapter support gaps

- [x] Lock Nostr feature-gate readiness and document production requirements.
- [x] Implement real Nostr adapter boundary behind `nostr-adapter`:
  - connects to configured `wss://` relays,
  - signs Nostr events with scoped relay identities,
  - uses hashed/random rendezvous tags only,
  - receives/filters by rendezvous topic.
- [ ] Complete Nostr production hardening:
  - map relay failures/rate limits/auth requirements to typed `SignalingHealthState`,
  - add multi-relay soak/fallback evidence beyond the single public relay smoke,
  - add provider-visible capture scans.
- [x] Lock IPFS/libp2p feature-gate/fail-closed readiness and document production requirements.
- [x] Implement real IPFS/libp2p PubSub adapter with rust-libp2p gossipsub, derived topics, opaque envelopes, unsubscribe, duplicate suppression, and local two-node transport E2E.
- [ ] Complete IPFS/libp2p production hardening:
  - configure public/default bootstrap peer policy and resource limits,
  - map libp2p bootstrap/resource/message failures to typed health,
  - define what “public default IPFS” means without requiring a user-hosted Kubo API,
  - run public-swarm E2E with configured bootstrap multiaddrs,
  - add provider-visible metadata capture scans.
- [x] Lock separate Rust QUIC rendezvous feature-gate/fail-closed readiness and document production requirements.
- [ ] Wire separate Rust QUIC rendezvous adapter:
  - use the sibling signaling service as an explicit/self-hosted adapter,
  - validate QUIC endpoint identity/fingerprint from policy/invite,
  - add local-network and deployed-service E2E.

### P0: app integration gaps

- [ ] Add an adapter registry/factory used by Tauri/backend runtime, not only transport tests.
- [ ] Make per-DM/per-group/per-channel connectivity policy select the real adapter implementation.
- [ ] Carry selected adapter state into UI status honestly: selected provider, health, fallback state, and failure class.
- [ ] Run two actual app profiles/instances through:
  - setup/recovery,
  - DM invite generation/acceptance,
  - group invite generation/join,
  - text channel send/receive,
  - voice negotiation/join/leave/mute/speaker controls,
  - adapter fallback.

### P0: WebRTC/media/data-plane gaps

- [ ] Use the signaling adapters to exchange real WebRTC offer/answer/candidate payloads generated by the app.
- [ ] Establish data channel for text delivery across two real profiles/devices.
- [ ] Establish audio media path and prove speaking/mute/volume UI state reflects real media state.
- [ ] Prove STUN works in normal NAT scenarios.
- [ ] Prove hard NAT fails honestly without TURN and succeeds with a configured TURN service.

### P0: security/release gaps

- [ ] Dependency/security audit for `rumqttc` and any Nostr/IPFS/libp2p dependencies.
- [ ] Public provider allowlist/versioning and rotation policy.
- [ ] Connect STUN/TURN fallback and provider-privacy proof into a dedicated release gate (G132)
  for deterministic harness evidence and optional public-provider MQTT validation.
- [ ] Provider-visible metadata capture/PCAP tests for MQTT, Nostr, IPFS, and QUIC.
- [ ] Abuse/rate-limit handling against public relays/brokers.
- [ ] Full release matrix across Linux desktop package and Android once mobile exists.

#### G132 status

- Added local deterministic proof for STUN→overlay→TURN behavior and provider-privacy hygiene in:
  - `harness/multinode/src/lib.rs`
  - `crates/transport/tests/connectivity_flows.rs`
- Added release gate script:
  - `npm --prefix apps/ui run test:stun-turn-provider-privacy-g132`
- Public-provider smoke remains optional to keep default CI deterministic:
  - set `DISCRYPT_PUBLIC_SIGNALING_E2E=1` for MQTT reruns and `DISCRYPT_PUBLIC_NOSTR_E2E=1` for Nostr reruns; latest MQTT and Nostr public adapter-boundary proofs are green against their configured default public providers.

### G132 production evidence matrix

#### Two-profile signaling verification matrix (required + planned)

| Gate slice | Command | Evidence target |
| --- | --- | --- |
| STUN overlay ordering and TURN fallback determinism | `cargo test -p discrypt-multinode-harness connectivity_signaling_push_smoke_covers_phase6_gates --quiet` | `ConnectivitySignalingPushSmoke` flags: `fallback_chain_covered`, `owner_overrides_used`, `metadata_matrix_validated`, `relays_ciphertext_only`, `ac_metadata_matrix_validated` |
| Transport policy/ciphertext-only routing | `cargo test -p discrypt-transport valid_direct_overlay_and_turn_flows_select_expected_leg --quiet` | Test-asserted route ordering and relay leg ciphertext-only constraints |
| Optional public MQTT proof (provider-visible real smoke) | `DISCRYPT_PUBLIC_SIGNALING_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=<mqtts://...> cargo test -q -p discrypt-transport --features mqtt-adapter public_mqtt_two_peer_presence_signal_and_control_roundtrip -- --nocapture` | Latest reruns passed against `mqtts://broker.emqx.io:8883` after broker `SUBACK` readiness was enforced; `test.mosquitto.org` certificate incompatibility and `broker.hivemq.com` network timeout remain provider-specific caveats. |
| Nostr public-provider proof | `DISCRYPT_PUBLIC_NOSTR_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://relay.damus.io cargo test -p discrypt-transport --features nostr-adapter public_nostr_two_peer_presence_signal_and_control_roundtrip -- --nocapture` | Latest rerun passed against `wss://relay.damus.io`; `wss://nostr.oxtr.dev` returned blocked |
| IPFS local libp2p proof | `cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_local_two_peer_presence_signal_and_control_roundtrip -- --nocapture` | Passed locally with two rust-libp2p gossipsub nodes over loopback; opaque presence/signal/control only |
| IPFS public-provider proof | `DISCRYPT_PUBLIC_IPFS_E2E=1 DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS=<multiaddr,...> cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter public_ipfs_two_peer_signaling_smoke -- --nocapture` | Test exists; public bootstrap run still missing until bootstrap endpoints are selected/configured |
| Planned QUIC public-provider proof | `cargo test -p discrypt-transport public_quic_two_peer_signaling_smoke --quiet` | **Missing (planned)** |

- Real producer/adapter route proofs still missing in this release gate: multi-relay Nostr soak, live IPFS public-bootstrap proof, live QUIC public-provider proof, and end-to-end mobile/installed-app transport smoke (tracked separately).
- Missing adapter check status is intentionally exposed as blockers instead of fake green signals in this phase.

## How to rerun the current real MQTT proof

Default public broker:

```bash
DISCRYPT_PUBLIC_SIGNALING_E2E=1 \
  cargo test -q -p discrypt-transport --features mqtt-adapter \
  public_mqtt_two_peer_presence_signal_and_control_roundtrip -- --nocapture
```

Custom public broker:

```bash
DISCRYPT_PUBLIC_SIGNALING_E2E=1 \
DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 \
  cargo test -q -p discrypt-transport --features mqtt-adapter \
  public_mqtt_two_peer_presence_signal_and_control_roundtrip -- --nocapture
```

The test is intentionally environment-gated so normal unit tests do not depend on public network availability.

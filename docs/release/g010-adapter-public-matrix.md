# G010 adapter/public matrix

G010 wires release-harness automation for local deterministic adapter gates and public-provider opt-in gates. It is harness automation only: **G011/G012 are not claimed**, and public-provider rows are skipped unless their explicit environment gates are set.

## Local deterministic gates

| Slice | Command | CI status |
| --- | --- | --- |
| STUN direct + relay-overlay + TURN fallback policy | `cargo test -p discrypt-multinode-harness connectivity_signaling_push_smoke_covers_phase6_gates --quiet` | Required by `npm --prefix apps/ui run test:signaling-e2e-matrix-g132` and wrapped by `npm --prefix apps/ui run test:g010-adapter-public-matrix` |
| Transport adapter fallback route selection | `cargo test -p discrypt-transport valid_direct_overlay_and_turn_flows_select_expected_leg --quiet` | Required by `npm --prefix apps/ui run test:signaling-e2e-matrix-g132` and wrapped by `npm --prefix apps/ui run test:g010-adapter-public-matrix` |
| Static adapter/public contract | `npm --prefix apps/ui run test:g010-adapter-public-matrix` | Required in CI; verifies package/docs/CI wiring plus explicit skip reporting |

## Public adapter matrix

| Adapter/public proof | Environment gate | Command | Default local behavior |
| --- | --- | --- | --- |
| MQTT public signaling | `DISCRYPT_PUBLIC_MQTT_E2E=1` or legacy `DISCRYPT_PUBLIC_SIGNALING_E2E=1`; optional `DISCRYPT_PUBLIC_MQTT_ENDPOINT=<mqtts://...>` | `cargo test -q -p discrypt-transport --features mqtt-adapter public_mqtt_two_peer_presence_signal_and_control_roundtrip -- --nocapture` | Skipped with an explicit message |
| Nostr public signaling | `DISCRYPT_PUBLIC_NOSTR_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=<wss://...>` | `cargo test -q -p discrypt-transport --features nostr-adapter public_nostr_two_peer_presence_signal_and_control_roundtrip -- --nocapture` | Skipped with an explicit message |
| IPFS direct topic-peer signaling | `DISCRYPT_PUBLIC_IPFS_E2E=1 DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS=<direct-topic-peer-multiaddr,...>` | `cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter public_ipfs_two_peer_signaling_smoke -- --nocapture` | Skipped with an explicit message; generic public bootstrap is not accepted as production proof |
| Discrypt QUIC rendezvous service | `DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E=1 DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=<https://...>` | `cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter public_quic_two_peer_signaling_smoke -- --nocapture` | Skipped with an explicit message until a deployed service endpoint is supplied |
| Relay-only TURN WebRTC DataChannel | `DISCRYPT_PUBLIC_TURN_E2E=1 DISCRYPT_PUBLIC_TURN_ENDPOINT=<turns://...> DISCRYPT_PUBLIC_TURN_USERNAME=<user> DISCRYPT_PUBLIC_TURN_CREDENTIAL=<secret>` | `cargo test -q -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_relay_only_turn_fallback_roundtrip_when_configured -- --nocapture` | Skipped with an explicit message until real TURN credentials are supplied |

## Boundary

This matrix proves that release automation can run deterministic local gates and can expose opt-in public MQTT/Nostr/IPFS/QUIC/TURN gates without silently claiming them. It does **not** prove final production readiness, broad hard-NAT closure, or two installed Tauri users completing text and voice flows; those remain G011/G012 evidence requirements.

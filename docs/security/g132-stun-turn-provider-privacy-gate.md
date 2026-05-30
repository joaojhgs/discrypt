# G132 STUN/TURN provider-privacy and fallback harness gate

G132 adds a dedicated release gate for the phase-6 connectivity/privacy boundary: deterministic coverage for ordered STUN → relay-overlay → TURN fallback, route evidence honesty, and provider-visible content hygiene across signaling + ICE artifacts.

This gate is intentionally narrow:

- It validates local process harness conformance for STUN/overlay/TURN ordering and evidence reporting.
- It validates transport-level fallback policy selection and relay-leg ciphertext-only obligations.
- It optionally validates a real public-provider MQTT signaling path when `DISCRYPT_PUBLIC_SIGNALING_E2E=1` is set as a public-provider proof.

## Acceptance criteria

- **AC13:** direct STUN succeeds first, overlay is used when STUN is unavailable, TURN is used when both STUN and overlay are unavailable.
- **Provider privacy:** signaling path artifacts and route reports do not leak forbidden names/content tokens (`alice`, `bob`, room/topology identifiers, message/plaintext tokens).
- **Route report honesty:** fallback chain and relay/TURN ciphertext-only assertions remain unchanged under route-report and transport conformance checks.
- **Optional public-provider proof (opt-in):** public MQTT signaling smoke can prove opaque transport/signaling behavior on a real broker when explicitly enabled.

## Test entry points

- `npm --prefix apps/ui run test:stun-turn-provider-privacy-g132`
- `cargo test -p discrypt-multinode-harness connectivity_signaling_push_smoke_covers_phase6_gates --quiet`
- `cargo test -p discrypt-transport valid_direct_overlay_and_turn_flows_select_expected_leg --quiet`
- Optional public-provider verification:
  - `DISCRYPT_PUBLIC_SIGNALING_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=<mqtts://...> cargo test -q -p discrypt-transport --features mqtt-adapter public_mqtt_two_peer_presence_signal_and_control_roundtrip -- --nocapture`

## Current evidence notes

- Local harness proof comes from `harness/multinode/src/lib.rs` (`ConnectivitySignalingPushSmoke`) and transport fallback policy tests in `crates/transport/tests/connectivity_flows.rs`.
- Public-provider proof is intentionally opt-in to keep CI deterministic while preserving reproducible release evidence from real-broker smoke runs.
- Adapter fallback behavior is tracked as a required matrix gate in the table below.

## Real-provider verification matrix (two-profile)

| Slice | Command | Status |
| --- | --- | --- |
| STUN direct path | `cargo test -p discrypt-multinode-harness connectivity_signaling_push_smoke_covers_phase6_gates --quiet` | Required local gate |
| TURN relay path / fallback chain | `cargo test -p discrypt-transport valid_direct_overlay_and_turn_flows_select_expected_leg --quiet` | Required local gate |
| Adapter fallback observability | `cargo test -p discrypt-transport valid_direct_overlay_and_turn_flows_select_expected_leg --quiet` | Required local gate |
| Public MQTT two-profile signal/control | `DISCRYPT_PUBLIC_SIGNALING_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=<mqtts://...> cargo test -q -p discrypt-transport --features mqtt-adapter public_mqtt_two_peer_presence_signal_and_control_roundtrip -- --nocapture` | Optional (real provider) |
| Public Nostr two-profile signal/control | `cargo test -p discrypt-transport public_nostr_two_peer_signaling_smoke --quiet` | **Missing (planned)** |
| Public IPFS two-profile signal/control | `cargo test -p discrypt-transport public_ipfs_two_peer_signaling_smoke --quiet` | **Missing (planned)** |
| Public QUIC two-profile signal/control | `cargo test -p discrypt-transport public_quic_two_peer_signaling_smoke --quiet` | **Missing (planned)** |

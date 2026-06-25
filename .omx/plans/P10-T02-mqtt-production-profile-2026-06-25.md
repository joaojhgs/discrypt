# P10-T02 MQTT Production Profile Plan - 2026-06-25

## Source Scope

- Issue: PER-83 / P10-T02, Phase 10 MQTT production profile.
- Acceptance: public/default and custom WSS/TLS MQTT profiles work; payload limits and backoff are mapped.
- Relevant code: `crates/transport/src/policy.rs`, `crates/transport/src/provider_adapters.rs`, `apps/desktop/src-tauri/src/lib.rs`, `apps/ui/src/commands.ts`, `crates/transport/tests/public_signaling_e2e.rs`.
- Invariants: MQTT is signaling/rendezvous only; provider-visible data remains hashed topic metadata plus sealed presence/WebRTC negotiation envelopes, never app text/control/media payloads.

## Acceptance Criteria

- Default MQTT profile remains `mqtts://broker.emqx.io:8883` unless overridden.
- Custom production MQTT endpoints accept `mqtts://` and `wss://`; loopback `mqtt://127.0.0.1:*` remains local-dev only.
- MQTT endpoint/profile metadata carries a bounded max provider message size and bounded retry/backoff settings.
- MQTT publish rejects oversized sealed provider envelopes before broker publish with a typed `provider_message_too_large` failure class and without echoing payload bytes.
- Public MQTT and local Mosquitto verification commands are documented with exact commands and evidence boundaries.

## Implementation Steps

1. Add endpoint-level retry/backoff metadata and validation in transport policy.
2. Map payload cap and backoff fields through desktop/UI signaling profile views.
3. Configure MQTT packet limits from the selected endpoint and fail closed before publish when serialized sealed envelopes exceed the profile cap.
4. Add focused Rust tests for policy metadata and MQTT payload-limit failure classification.
5. Run formatting plus targeted transport/UI checks and, where network/local broker access permits, local Mosquitto and public MQTT opt-in tests.

## Risks And Mitigations

- Risk: treating a successful broker roundtrip as message delivery. Mitigation: docs and tests keep this as provider signaling evidence only; text/control delivery remains WebRTC DataChannel evidence.
- Risk: oversized payload diagnostics leak content. Mitigation: error includes byte counts and failure class only.
- Risk: older persisted profiles lack new fields. Mitigation: serde defaults preserve load compatibility.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport provider_endpoint_defaults_payload_cap_and_bounded_backoff`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter mqtt_publish_payload_limit_maps_to_typed_provider_failure`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter public_mqtt_two_peer_presence_and_signal_roundtrip -- --nocapture` with `DISCRYPT_PUBLIC_MQTT_E2E=1` or legacy `DISCRYPT_PUBLIC_SIGNALING_E2E=1`.
- Local Mosquitto equivalent: set `DISCRYPT_PUBLIC_SIGNALING_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtt://127.0.0.1:1883` against a local broker.


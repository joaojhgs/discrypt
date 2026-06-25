# PER-83 MQTT production profile evidence - 2026-06-25

## Scope

PER-83 / P10-T02 hardens the MQTT signaling profile boundary. This is provider
signaling evidence only: MQTT carries hashed rendezvous topics plus sealed
presence/WebRTC negotiation envelopes. It does not relay application
text/control frames, receipts, media, or plaintext.

## Implemented

- Default and UI-created MQTT signaling profiles now carry:
  - `max_message_bytes = 65536`
  - `backoff_initial_ms = 250`
  - `backoff_max_ms = 5000`
  - `backoff_multiplier = 2`
  - `backoff_max_attempts = 5`
- Custom production MQTT endpoints still require `mqtts://` or `wss://`.
- Local-dev MQTT endpoints accept loopback `mqtt://127.0.0.1:*` / `mqtt://[::1]:*`
  for Mosquitto harness runs only.
- MQTT publish configures the provider packet cap from the selected endpoint and
  rejects oversized serialized sealed envelopes before broker publish with
  `failure_class=provider_message_too_large`.
- The public MQTT test gate now accepts `DISCRYPT_PUBLIC_MQTT_E2E=1` and the
  legacy `DISCRYPT_PUBLIC_SIGNALING_E2E=1`.

## Verification Run

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check` - passed.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport provider_endpoint_defaults_payload_cap_and_bounded_backoff` - passed.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter mqtt_publish_payload_limit_maps_to_typed_provider_failure` - passed.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --lib` - passed.
- `npm --prefix apps/ui ci --cache /tmp/discrypt-npm-cache` - passed.
- `npm --prefix apps/ui run typecheck` - passed.
- `DISCRYPT_PUBLIC_MQTT_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter public_mqtt_two_peer_presence_and_signal_roundtrip -- --nocapture` - passed.

## Skipped

- Local Mosquitto run was not available in this runner: `mosquitto` is not
  installed, and Docker access to `/var/run/docker.sock` is denied, so a
  short-lived `eclipse-mosquitto:2` broker could not be started. The harness is
  now wired for `DISCRYPT_PUBLIC_SIGNALING_E2E=1
  DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtt://127.0.0.1:1883` once Mosquitto is
  available.

## Evidence Boundary

This is production-profile and provider-boundary evidence. It proves TLS/WSS
public MQTT profile validation, explicit payload/backoff metadata, fail-closed
payload cap behavior, and a live public broker opaque signaling roundtrip. It is
not installed-app two-profile message/voice evidence and does not claim MQTT is
an application relay.


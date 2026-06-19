# P3-T09 Configured TURN Proof

Issue: PER-30.

## Verdict

Implemented a release-bound configured TURN proof path. The live CI artifact is produced by Chromium native `RTCPeerConnection` with `iceTransportPolicy: "relay"` against coturn, because the current Rust `webrtc` 0.20.0-alpha.1 Sans-I/O gatherer does not generate TURN relay candidates.

This is transport harness evidence. It is not production-ready installed-app evidence, not OpenMLS admission proof, not overlay proof, and not physical voice/media proof.

## Behavior Implemented

- `public_mqtt_relay_only_turn_fallback_roundtrip_when_configured` now requires `WebRtcIceTransportPolicy::RelayOnly` for the configured TURN proof.
- The test still requires explicit `DISCRYPT_PUBLIC_TURN_E2E=1`, `DISCRYPT_PUBLIC_TURN_ENDPOINT`, `DISCRYPT_PUBLIC_TURN_USERNAME`, and `DISCRYPT_PUBLIC_TURN_CREDENTIAL`.
- On live success it asserts both peers opened the WebRTC DataChannel, both peers have configured TURN servers, both peers report TURN fallback readiness, both sides observed relay candidate evidence, and opaque text/control request plus opaque receipt round-tripped.
- On live success it writes a redacted JSON artifact to `target/e2e/per-30-configured-turn-proof/public-turn-relay-only.json` unless `DISCRYPT_PUBLIC_TURN_ARTIFACT_PATH` overrides it.
- `scripts/per30-browser-turn-proof.mjs` performs the branch CI live proof through Chromium WebRTC and writes the same redacted artifact schema.
- The Rust cargo harness remains skip-safe locally when credentials are absent. When forced with credentials against the current Rust dependency, relay candidate gathering is blocked by the dependency and must not be counted as a product TURN success.

## Provider Boundary

The MQTT provider remains signaling/rendezvous only. Provider-visible material is limited to endpoint label, derived hashed rendezvous topic, and sealed WebRTC offer/answer/candidate envelopes. Application text/control frames and receipt bytes cross the WebRTC DataChannel, not the MQTT provider.

The artifact records `provider_application_relay_used=false`.

## Credential Redaction

The artifact does not contain raw TURN endpoint, username, credential, SDP, ICE candidate lines, or text/control payload bytes. It records only:

- A hashed TURN endpoint label.
- Credential presence/redaction booleans.
- Configured TURN server counts.
- Relay candidate counts.
- DataChannel and opaque frame/receipt roundtrip booleans.
- SHA-256 digests of the opaque request and receipt frames.
- Redacted diagnostic timelines.

## Evidence

CI live coturn proof on branch `multica/P3-T09-configured-turn-proof`:

- `.github/workflows/ci.yml` runs `PER-30 configured TURN proof` only for this task branch.
- The job starts loopback coturn with run-scoped CI-only credentials, sets `DISCRYPT_PUBLIC_TURN_E2E=1`, runs `scripts/per30-browser-turn-proof.mjs`, runs the static artifact redaction gate, and uploads `per30-configured-turn-proof-<run>-<attempt>`.
- The uploaded directory contains the redacted artifact at `public-turn-relay-only.json` plus `coturn.log`.

Static and skip-safe checks:

- `node scripts/check-configured-turn-proof-p3-t09.mjs`
- `npm --prefix apps/ui run test:p3-t09-configured-turn-proof`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_relay_only_turn_fallback_roundtrip_when_configured -- --nocapture`

Credentialed live proof when TURN credentials are available:

- `DISCRYPT_PUBLIC_TURN_E2E=1 DISCRYPT_PUBLIC_TURN_ENDPOINT=<redacted> DISCRYPT_PUBLIC_TURN_USERNAME=<redacted> DISCRYPT_PUBLIC_TURN_CREDENTIAL=<redacted> node scripts/per30-browser-turn-proof.mjs`
- `DISCRYPT_PUBLIC_TURN_E2E=1 DISCRYPT_PUBLIC_TURN_ENDPOINT=<redacted> DISCRYPT_PUBLIC_TURN_USERNAME=<redacted> DISCRYPT_PUBLIC_TURN_CREDENTIAL=<redacted> RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_relay_only_turn_fallback_roundtrip_when_configured -- --nocapture`

Retained live artifact path:

- `target/e2e/per-30-configured-turn-proof/public-turn-relay-only.json`

If credentials are not present, this task provides code/static evidence and an honest skipped live gate. It does not claim configured TURN has been proven in the local runtime without that artifact.

Known Rust transport dependency blocker:

- `webrtc` 0.20.0-alpha.1 passes `RTCIceTransportPolicy::Relay` into configuration, but its Sans-I/O `RTCIceGatherer` currently gathers host candidates and STUN server-reflexive candidates only; TURN client support is not implemented in that gatherer.
- Until Discrypt patches or replaces that dependency path, the browser coturn artifact is valid live WebRTC/TURN evidence but not proof that the Rust text/control transport stack can gather TURN relay candidates.

## Remaining Verification

Before promoting the Rust transport row to fully verified, patch or replace the Rust WebRTC dependency path and rerun the credentialed Rust cargo proof against coturn or an approved public TURN service. The browser artifact verifies live configured TURN relay-only WebRTC behavior and artifact redaction.

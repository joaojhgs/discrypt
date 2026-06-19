# P3-T09 Configured TURN Proof

Issue: PER-30 / P3-T09.

Source context:
- PER-30 requires configured TURN proof for Phase 3 provider-signaled WebRTC text/control reliability.
- `docs/release/handoff-2026-06-10-current-state.md` keeps WebRTC route evidence as a release blocker and says signaling providers are not application relays.
- `docs/release/release-gap-matrix-2026-06-15.md` requires direct WebRTC, configured TURN-backed WebRTC, or future approved relay route evidence before route claims are promoted.
- `.omx/plans/P3-T08-turn-needed-fail-closed-path-2026-06-19.md` covers the adjacent no-TURN fail-closed path and deliberately excludes configured TURN success.
- `.omc/plans/discrypt-plan.md` locks the connectivity chain to STUN -> peer relay overlay -> TURN, with TURN carrying ciphertext only.
- The named `.omx/plans/production-release-master-plan-2026-06-10.md` is absent in this checkout; the issue body, metadata, current release docs, and adjacent P3 plans are the local authority.

Scope:
- Strengthen the existing env-gated public MQTT + TURN WebRTC DataChannel test so it proves relay-only TURN policy, relay candidate evidence, DataChannel open, and bidirectional opaque text/control receipt.
- Emit a redacted proof artifact only when live TURN credentials are explicitly provided.
- Add a static release gate and release report for PER-30. The P3-T09 configured TURN proof must preserve provider signaling-only policy throughout.
- Do not implement overlay relay, voice/media microphone proof, UI redesign, OpenMLS admission, or broader release gates.

Acceptance criteria:
- With `DISCRYPT_PUBLIC_TURN_E2E=1` and configured TURN credentials, the test runs with `WebRtcIceTransportPolicy::RelayOnly`.
- Both peers report DataChannel open, configured TURN server counts greater than zero, TURN fallback readiness, and at least one relay candidate observed locally or remotely.
- The opaque request and opaque receipt cross the WebRTC DataChannel.
- The generated artifact redacts TURN endpoint identity to a hash label and never writes raw TURN username, credential, SDP, ICE candidate lines, or text/control payload bytes.
- Provider-visible material remains limited to endpoint label, derived hashed rendezvous topic, and sealed WebRTC negotiation envelopes; provider application relay is false.

Implementation steps:
1. Update `crates/transport/tests/public_webrtc_datachannel_e2e.rs` to assert relay-only policy and write a redacted PER-30 artifact on live success.
2. Add `docs/release/p3-t09-configured-turn-proof-2026-06-19.md` describing the proof, commands, artifact path, skipped-credential behavior, and claim boundary.
3. Add `scripts/check-configured-turn-proof-p3-t09.mjs` plus an `apps/ui/package.json` wrapper to statically validate the test/report contract and any retained artifact.
4. Run static checks and targeted transport tests locally where available; mark credentialed live TURN as skipped unless TURN credentials are present.
5. Commit, push, open/update PR, pin PR metadata, and hand off to QA with exactly one `@discrypt-qa-tester` mention.

Failure modes and safety:
- Missing TURN credentials must skip the live test or fail with a typed credential setup error; it must not fall back to direct-only evidence and claim configured TURN.
- Relay readiness must require configured TURN counts and relay candidate evidence; TURN booleans alone are insufficient.
- Providers must not carry app plaintext, ciphertext text/control frames, or media frames as a fallback.
- Diagnostic artifacts must remain redacted; raw candidate lines and TURN credentials stay out of docs and JSON.

Verification:
- `node scripts/check-configured-turn-proof-p3-t09.mjs`
- `npm --prefix apps/ui run test:p3-t09-configured-turn-proof`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_relay_only_turn_fallback_roundtrip_when_configured -- --nocapture` without TURN envs should skip honestly.
- With credentials available: `DISCRYPT_PUBLIC_TURN_E2E=1 DISCRYPT_PUBLIC_TURN_ENDPOINT=<redacted> DISCRYPT_PUBLIC_TURN_USERNAME=<redacted> DISCRYPT_PUBLIC_TURN_CREDENTIAL=<redacted> RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_relay_only_turn_fallback_roundtrip_when_configured -- --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Stop condition:
- Branch `multica/P3-T09-configured-turn-proof` contains code/docs/evidence updates.
- A PR is opened/updated and pinned in issue metadata if publishing succeeds.
- QA handoff comment includes exactly one `@discrypt-qa-tester` mention, branch/PR, changed files, exact verification, artifacts, known gaps, and QA focus.

# P3-T08 TURN-Needed Fail-Closed Path

Issue: PER-29.

## Verdict

Implemented after QA rejection of the first PR revision. The code now models a direct-failed/TURN-required route as a failed transport session with diagnostic route evidence, and route proof only accepts TURN success when the probe reports TURN readiness, both peers report configured TURN server counts greater than zero, and the active connectivity policy contains a real TURN endpoint.

This is backend/transport harness evidence. It is not production-ready installed-app NAT proof, overlay proof, voice proof, or OpenMLS admission proof.

## Behavior Implemented

- `crates/transport/src/session.rs` adds `TransportSession::fail_direct_path_turn_required`.
- The failed snapshot stores attempted STUN then TURN diagnostics, sets selected route to TURN for user-visible recovery, keeps `connected() == false`, and records `direct WebRTC failed and TURN is required but not configured`.
- `apps/desktop/src-tauri/src/lib.rs` treats a provider-signaled DataChannel probe with no direct readiness and no configured TURN route as `webrtc-datachannel-failed`.
- TURN route proof no longer trusts TURN fallback booleans alone and no longer synthesizes `turn:configured-turn.proofed` for a success route when no TURN server is configured.
- Desktop diagnostics report `turn_required=turn-required`, `provider_application_relay_used=false`, and a `direct_failed_turn_required` command error/recovery hint.
- The text session is marked `failed`; no delivery receipt is created from this path.

## Provider Boundary

Signaling providers remain signaling/rendezvous only. This change does not introduce provider-carried application payload fallback. The diagnostic copy explicitly records `provider_application_relay_used=false` for the failed path.

## Evidence

Retained local artifact:

- `target/e2e/per-29-turn-needed-fail-closed/local-harness-attempt.json`

Targeted tests added:

- `crates/transport/src/session.rs::turn_needed_fail_closed_snapshot_is_not_connected`
- `apps/desktop/src-tauri/src/lib.rs::turn_needed_fail_closed_probe_marks_failed_without_delivery_claim`
- `apps/desktop/src-tauri/src/lib.rs::turn_ready_booleans_without_configured_turn_still_fail_closed`

Commands attempted locally:

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check` - blocked: `cargo` not found.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport turn_needed_fail_closed -- --nocapture` - blocked: `cargo` not found.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml turn_needed_fail_closed --lib -- --test-threads=1 --nocapture` - blocked: `cargo` not found.
- `git diff --check` - passed.

QA rejection addressed:

- QA found that direct-false probes with both TURN fallback booleans true but zero configured TURN server counts could still select a TURN route.
- The fix requires the same configured-TURN predicate for diagnostics and route proof, and requires an active configured TURN endpoint for TURN success.

## Remaining Verification

Required before promoting the row to verified:

- Run the targeted Rust tests on CI or a Rust-capable host after the QA fix.
- Run a NAT-blocked or equivalent isolated harness where direct route is impossible and no TURN is configured.
- Confirm diagnostics export includes the failed route details without raw SDP, ICE credentials, TURN secrets, or message content.

# P11-T03 - ICE/DTLS/Provider Report

Issue: PER-92 / P11-T03.

## Requirements Summary

Source context:
- PER-92 requires an ICE/DTLS/provider diagnostic report for support bundles.
- `docs/release/handoff-2026-06-10-current-state.md` keeps Discrypt not production-ready after transport regressions.
- `.omc/plans/discrypt-plan.md` locks STUN -> peer relay overlay -> TURN and keeps MQTT/Nostr/IPFS/QUIC providers as signaling/rendezvous only.
- Adjacent plans `P3-T02`, `P3-T08`, and `P7-T05` already added redacted WebRTC timelines, TURN-required fail-closed state, and route diagnostics export.
- The named production master plan path is absent in this checkout; issue body, metadata, current release docs, and adjacent plans are the active constraints.

## Acceptance Criteria

- Diagnostics distinguish these synthetic failure classes: provider missing, offer missing, answer missing, candidate missing, ICE failed, DTLS failed, DataChannel failed, and TURN required.
- Reports are backend/transport-derived and do not expose raw SDP, ICE credentials, TURN endpoints, frame bytes, message bodies, or provider URLs.
- Reports explicitly keep `provider_application_relay_used=false`; no provider application relay behavior is added.
- TURN-required remains a failure/reporting state unless configured TURN route evidence exists.

## Implementation Steps

1. Add transport-level serializable report types and classifier helpers near the existing provider-signaled WebRTC probe evidence.
2. Classify from selected provider state, redacted offer/answer/candidate timeline counts, ICE/DTLS/DataChannel states, direct/TURN readiness, and configured TURN availability.
3. Surface the report through Tauri transport diagnostics and support-bundle export without changing UI success semantics.
4. Add synthetic Rust tests for every required failure class plus redaction/provider-relay invariants.
5. Add a release evidence note under `docs/release/` and run targeted transport/backend tests plus formatting/diff checks.

## Failure Modes And Safety

- Missing report inputs must produce an explicit missing/unavailable class rather than a success state.
- Provider failure is not a delivery fallback. Public signaling providers remain limited to sealed negotiation and rendezvous metadata.
- DataChannel open alone is not enough for route proof when direct/TURN route evidence is absent.
- Synthetic diagnostics are local harness evidence only and must not be labeled production-ready installed-app proof.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport ice_dtls_provider_report`
- Targeted Tauri backend test for support-bundle/report export if the app diagnostics surface changes.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local synthetic backend/transport diagnostics evidence, not production-ready split-machine or installed-app evidence.

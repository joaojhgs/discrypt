# P7-T05 - Route Diagnostics Export

Issue: PER-64 / P7-T05

## Requirements Summary

Source context: PER-64 follows `.omx/plans/P7-T01-route-graph-data-model-2026-06-21.md`, `.omx/plans/P7-T02-runtime-map-2026-06-21.md`, `.omx/plans/P7-T03-per-peer-direct-turn-attach-2026-06-21.md`, and `.omx/plans/P7-T04-message-fanout-dedup-2026-06-21.md`. The production master plan file named in the issue is absent from this checkout; the issue body, metadata, `docs/release/handoff-2026-06-10-current-state.md`, and `.omc/plans/discrypt-plan.md` are the active constraints.

Acceptance:
- Support bundle diagnostics export includes a route graph view with redacted group/channel/local/remote peer refs.
- Each edge reports backend-derived runtime state: attached, pending, missing, unavailable/error, sent, receipted, and pending receipt evidence where available.
- Each edge reports ICE/DTLS/DataChannel/TURN fields from existing runtime/probe evidence without raw SDP, ICE credentials, TURN secrets, provider endpoint URLs, frame bytes, or message bodies.
- Providers remain signaling/rendezvous only; diagnostics must explicitly keep `provider_application_relay_used=false`.

## Implementation Steps

1. Add backend-only serializable route diagnostics structs in `apps/desktop/src-tauri/src/lib.rs`.
2. Derive route graph diagnostics from admitted non-local group members, active text session state, per-peer pending/attached runtime maps, and text-control outbox route tracking.
3. Use existing runtime evidence and DataChannel metrics where available; report unavailable/not-observed states when no live runtime evidence exists.
4. Add the route graph diagnostics to `export_diagnostics_log()` only, leaving normal UI state unchanged and avoiding default debug clutter.
5. Add focused Tauri backend tests that parse the exported support bundle and assert redaction, edge state, ICE/DTLS/DataChannel/TURN fields, and no provider relay fallback or raw secret leakage.

## Failure Modes And Safety

- Missing active text session must not imply connectivity; exported edges remain `missing_runtime` or `pending_runtime`.
- Pending, revoked, migration-default, local-loopback, or duplicate peers must not create connected route edges.
- Runtime metrics can fail or be unavailable; export must surface an unavailable diagnostic state rather than suppressing or fabricating success.
- Provider signaling remains limited to SDP/candidate/control rendezvous. The support bundle must not include application payload relay evidence or copy.

## Verification

- Targeted desktop backend support-bundle test for PER-64 route diagnostics export.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local Rust/Tauri backend unit/support-bundle evidence. This is not split-machine, public-provider, or production route evidence.

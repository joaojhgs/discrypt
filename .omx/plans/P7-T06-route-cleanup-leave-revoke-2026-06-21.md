# P7-T06 - Route Cleanup On Leave/Revoke

Issue: PER-65 / P7-T06

## Requirements Summary

Source context: PER-65 follows `.omx/plans/P7-T01-route-graph-data-model-2026-06-21.md`, `.omx/plans/P7-T02-runtime-map-2026-06-21.md`, `.omx/plans/P7-T03-per-peer-direct-turn-attach-2026-06-21.md`, `.omx/plans/P7-T04-message-fanout-dedup-2026-06-21.md`, and `.omx/plans/P7-T05-route-diagnostics-export-2026-06-21.md`. The production master plan file named in the issue is absent from this checkout; the issue body, metadata, `docs/release/handoff-2026-06-10-current-state.md`, and `.omc/plans/discrypt-plan.md` are the active constraints.

Acceptance:
- Removed, revoked, pending, migration-default, offline, duplicate, or local-loopback route peers have pending and attached direct/TURN WebRTC routes closed.
- Stale outbound retries and inbound frames from removed or revoked peers are rejected fail-closed.
- Valid admitted peer routes remain attached and eligible for fanout.
- Route diagnostics/support bundle state reflects closed/missing routes without provider application relay.

## Implementation Steps

1. Add backend-only helpers in `apps/desktop/src-tauri/src/lib.rs` to derive the current admitted route-peer set for a group text target from signed backend membership/runtime peer state.
2. Prune per-peer pending/attached text/control runtime map entries whose remote peer is no longer admitted for the active group target, leaving other valid peers intact.
3. Prune unresolved outbox route tracking for no-longer-admitted peers so stale retries do not send to removed routes; keep already-receipted messages honest and do not fabricate delivery.
4. Reject inbound group envelopes, receipts, and voice signaling when sender/recipient route peers are no longer admitted for the target group.
5. Add targeted Tauri backend regression tests for revoke/leave cleanup, stale frame rejection, and valid peer preservation.

## Failure Modes And Safety

- Missing or stale group membership evidence fails closed; cleanup does not infer admission from invite parsing.
- Provider adapters remain signaling/rendezvous only. Cleanup never falls back to provider application payload relay.
- Route cleanup only removes per-peer runtime/outbox state proven stale by current backend membership; unrelated sessions remain untouched.
- Diagnostics are derived from the same admitted roster and runtime map, so removed peers disappear from route graph edges or surface as unavailable without connected claims.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib route_cleanup -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local Rust/Tauri backend unit/harness evidence for route cleanup and stale frame rejection. This is not split-machine or production route evidence.

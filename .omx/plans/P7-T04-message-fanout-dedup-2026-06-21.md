# P7-T04 - Message Fanout/Dedup

Issue: PER-63 / P7-T04

## Requirements Summary

Source context: PER-63 follows `.omx/plans/P7-T01-route-graph-data-model-2026-06-21.md`, `.omx/plans/P7-T02-runtime-map-2026-06-21.md`, and `.omx/plans/P7-T03-per-peer-direct-turn-attach-2026-06-21.md`. The production master plan file named in the issue is absent from this checkout; the issue body, metadata, release handoff, and original `.omc/plans/discrypt-plan.md` are the active constraints.

Acceptance:
- Group text sends one protected text/control envelope to every admitted non-local per-peer runtime route for the active group.
- Missing, pending, revoked, duplicate, failed, or unavailable routes fail closed with explicit per-peer errors and do not block valid attached peers.
- Receivers dedup duplicate route deliveries by message id and keep receipt generation idempotent.
- Existing two-person text/control behavior remains compatible.
- MQTT/Nostr/IPFS/QUIC providers remain signaling/rendezvous only; no provider application payload relay is added.

## Implementation Steps

1. Extend persisted text/control outbox records with optional route-peer send/receipt tracking while keeping legacy records default-compatible.
2. Populate group channel outbox records with admitted remote runtime peer ids derived from backend group membership, excluding pending/revoked/local members.
3. Update the Tauri text/control pump to fan out group frames over matching per-peer runtimes for the active text session, reporting route-specific pending/missing/send/receipt failures while still draining valid peers.
4. Keep DM/legacy pump semantics stable for route-unscoped records.
5. Make inbound envelope and receipt handling idempotent so duplicate route delivery does not create duplicate message or receipt rows.
6. Add focused Rust/Tauri backend tests for 3-profile group convergence, duplicate envelope dedup, missing/pending route failure, and existing two-person compatibility.

## Failure Modes And Safety

- A route peer is eligible only when it comes from admitted backend group membership; invite parsing alone is not used as delivery authority.
- Revoked, pending, local-loopback, duplicate, or missing peer ids do not create delivery success. The pump records explicit failures and leaves unresolved route peers pending for retry.
- Public providers are not used for application payload fallback. The pump only sends over attached direct/TURN WebRTC text/control runtimes.
- Group sender state reaches final peer receipt only after every expected route has produced a verified signed receipt.

## Verification

- Targeted desktop backend tests for 3-profile text fanout/convergence and dedup.
- Existing two-profile text/control pump test remains green.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local Rust/Tauri backend unit/harness evidence for three-profile group text convergence over test DataChannel runtimes. This is not split-machine or public-provider production evidence.

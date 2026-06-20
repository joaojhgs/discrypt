# P5-T07 presence heartbeat and expiry plan

Issue: PER-49 / P5-T07

## Requirements Summary

- Source task: Phase 5 governance, "Presence heartbeat and expiry."
- Acceptance criteria: owner/member see own and each other online when a route exists; status expires offline; no fake local-only online.
- Required evidence: two-profile or split-machine presence tests.
- Batch scope: implement backend-governed presence heartbeat and expiry only; do not implement right member panel, audit-log UI, voice, overlay, or later release-gate work.
- Product invariants: online presence requires backend/provider route evidence; MQTT/Nostr/IPFS/QUIC providers remain signaling/rendezvous only; protected group text/voice still requires authorized OpenMLS membership.
- Available context: PER-49 issue body, `.omc/plans/discrypt-plan.md` Phase 5 / AC-PRESENCE boundary, `docs/release/handoff-2026-06-10-current-state.md`, `docs/release/current-regressions.md`, `docs/release/release-gap-matrix-2026-06-15.md`, and adjacent Phase 5 plans P5-T05/P5-T06.
- Missing expected context: `.omx/plans/production-release-master-plan-2026-06-10.md`, `.omx/plans/admin-role-admission-plan-2026-06-04.md`, and `.omx/plans/peer-overlay-group-transport-plan-2026-06-05.md` are named by runtime context but are not present in this checkout.

## Code Anchors

- `apps/desktop/src-tauri/src/lib.rs`: `PublishGroupPresenceRequest`, `publish_group_presence`, `TextControlFrameView::GroupPresenceHeartbeat`, `apply_group_presence_heartbeat`, `group_with_effective_presence`, text/control runtime attachment and pump tests.
- `apps/ui/src/main.tsx`: active group presence heartbeat effect, `isPresenceOnline`, and member list rendering.
- `apps/ui/src/commands.ts`: `publishGroupPresence` fallback behavior and `normalizedGroupMembers`.

## Acceptance Criteria

- Publishing local online presence is rejected unless backend text/control state has a connected route and a matching attached runtime for the active group session.
- A published heartbeat queues only a signed backend text/control `GroupPresenceHeartbeat` frame; no provider application-message relay fallback is added.
- A second profile receiving the heartbeat over the backend text/control pump sees the sender online with `last_seen_at` and `presence_expires_at` source timestamps.
- When the sender stops heartbeating and the TTL is stale, backend state projects that member as offline, including on remote profiles.
- Browser/UI fallback code never fabricates an online member row or local online heartbeat when Tauri backend evidence is unavailable.
- Evidence is local backend/Tauri two-profile harness evidence unless an opt-in public/split-machine command is explicitly run and recorded.

## Implementation Steps

1. Add a backend helper that validates presence route evidence from the active text/control session and attached runtime before mutating a member to online.
2. Update `publish_group_presence` to fail closed before changing state when route evidence is missing, while preserving pending/revoked member rejection and queued governance heartbeat behavior.
3. Update the React heartbeat effect to start/attach/sync text runtime before publishing presence, and remove frontend fallback mutations that synthesize online state.
4. Add focused backend tests for no-route rejection, route-backed two-profile heartbeat propagation, TTL expiry to offline, and revocation/pending boundaries.
5. Run targeted desktop backend tests, UI typecheck if UI types change, formatting, and diff checks.

## Failure Modes And Safety

- Fake online state from local UI fallback: remove fallback mutation and keep backend command as the only truth source.
- Route loss while publishing: reject with a typed command error before changing status or queueing a heartbeat.
- Provider misuse: continue using the existing text/control DataChannel transport trait; do not publish application payloads through signaling providers.
- Expired heartbeats: project expired `online` members as `offline` without deleting timestamps, preserving audit/source evidence.
- Local harness limits: record local two-profile evidence honestly and do not claim production split-machine readiness unless separately proven.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t07 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml presence --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t07 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml g012_two_profile_group_text_delivery_bidirectional_persists --lib -- --test-threads=1`
- `npm --prefix apps/ui run typecheck`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t07 cargo fmt --check`
- `git diff --check`

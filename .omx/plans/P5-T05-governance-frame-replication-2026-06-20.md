# P5-T05 governance frame replication plan

Issue: PER-47 / P5-T05

## Requirements Summary

- Source task: Phase 5 governance, "Governance frame replication."
- Acceptance criterion: role changes, decisions, and revocations replicate over MLS-protected control frames with idempotent duplicate handling.
- Required evidence: offline/reconnect reconciliation test.
- Available project context: issue body, `.omc/plans/discrypt-plan.md` Phase 5 and D5, `docs/phase-5-governance-admission-recovery-abuse.md`, `docs/release/handoff-2026-06-10-current-state.md`, and adjacent `.omx/plans/P5-T01*`, `P5-T03*`, `P5-T04*`.
- Missing expected context: `.omx/plans/production-release-master-plan-2026-06-10.md`, `.omx/plans/admin-role-admission-plan-2026-06-04.md`, and `.omx/plans/peer-overlay-group-transport-plan-2026-06-05.md` are named by runtime context but are not present in this checkout.

## Code Anchors

- `apps/desktop/src-tauri/src/lib.rs`: `TextControlFrameView` governance variants, `queue_group_governance_frame`, `pump_text_control_transport_once`, `handle_text_control_frame`, `apply_group_member_role_changed`, and `apply_group_member_revoked`.
- `crates/mls-core/src/governance.rs`: signed, ordered governance primitives and canonical comparator.
- Existing tests in `apps/desktop/src-tauri/src/lib.rs`: `staff_promotion_governance_frame_converges_three_profiles`, `g005_revocation_commits_openmls_remove_member_and_rekeys_remaining_members`, and text/control pump tests.

## Acceptance Criteria

- Owner role-change frames queued in the governance outbox replicate through the backend text/control pump and converge on admitted peers.
- Owner/staff admission decision frames and revocation frames remain backend-authenticated control frames; no provider application-payload relay fallback is added.
- Duplicate/replayed governance frames are idempotent: repeated event ids do not duplicate governance log entries or downgrade already-applied policy.
- Offline/reconnect reconciliation is covered by a receiver missing the original pump, then later reconnecting and applying queued governance frames.
- Evidence is explicit local backend/Tauri harness evidence, not production public-provider or split-machine evidence.

## Implementation Steps

1. Inspect existing governance frame and outbox/pump behavior before changing code.
2. Add the smallest regression test that drives queued governance frames through `pump_text_control_transport_once` into an offline/reconnected receiver.
3. Cover role-change replication, admission decision/policy-frame application, revocation application, and duplicate replay idempotency in the same harness where possible.
4. Preserve current signaling/provider boundaries: the pump uses the backend text/control transport trait and does not add MQTT/Nostr/IPFS/QUIC provider payload relay behavior.
5. Run targeted desktop backend tests, MLS governance tests, format, and diff checks.

## Risks And Safety

- Risk: accepting stale duplicate role frames could downgrade policy. Mitigation: assert event-id idempotency and unchanged role/log state after replay.
- Risk: notice-only revocation could revoke the wrong peer. Mitigation: preserve existing non-target notice ignore behavior and test only validated/applicable revoke frame outcomes for reconciliation.
- Risk: local harness evidence could be overclaimed as production transport evidence. Mitigation: document this as local backend/Tauri evidence only.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t05 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml governance_frame_replication --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t05 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml staff_promotion_governance_frame_converges_three_profiles --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t05 cargo test -p discrypt-mls-core governance -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t05 cargo fmt --check`
- `git diff --check`

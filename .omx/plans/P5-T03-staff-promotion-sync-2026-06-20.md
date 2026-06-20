# P5-T03 Staff Promotion Sync Plan

Issue: PER-45 / P5-T03

## Requirements Summary

- Source task: Phase 5 governance, "Staff promotion sync."
- Acceptance criterion: owner promotes member to staff; all admitted profiles converge.
- Required evidence: three-profile test.
- Available project context: issue body, `.omc/plans/discrypt-plan.md` Phase 5 and D5 governance ordering, `docs/release/handoff-2026-06-10-current-state.md`, and `.omx/plans/P5-T02-authorization-matrix-2026-06-20.md`.
- Missing expected context: `.omx/plans/production-release-master-plan-2026-06-10.md` is named by the issue but is not present in this checkout.

## Code Anchors

- `apps/desktop/src-tauri/src/lib.rs`: `promote_group_member_to_staff`, `queue_group_governance_frame`, `handle_text_control_frame`, and `apply_group_member_role_changed`.
- `apps/ui/src/commands.ts`: fallback command parity for role changes.
- `crates/mls-core/src/governance.rs`: signed `SetRole` authority primitives where `Role::Admin` maps to app staff semantics.

## Acceptance Criteria

- Owner promotion queues a `GroupMemberRoleChanged` governance frame before local success is claimed.
- The owner roster and two independently persisted admitted profile rosters converge on the promoted member role.
- Applying the same role-change frame is idempotent and does not duplicate governance log entries.
- A promoted local target updates its legacy/current group role label only after backend governance frame application.
- The test evidence is backend/Tauri harness evidence, not production network delivery evidence.

## Implementation Steps

1. Keep promotion/demotion mutation transactional with governance frame queueing: prepare the frame, queue it successfully, then mutate the roster/log and emit success.
2. Add a three-profile backend test with owner, promoted member, and observer profile states sharing the admitted group roster.
3. Deliver the owner outbox `GroupMemberRoleChanged` frame into both non-owner profiles through `handle_text_control_frame` and assert all three profiles report the target as staff.
4. Assert repeated delivery is idempotent and returns the governance acknowledgement without duplicating logs.
5. Run targeted desktop backend tests, relevant MLS governance tests, format, and diff checks.

## Risks And Safety

- If the governance frame cannot serialize or queue, the command must fail closed without changing local role state.
- This task does not prove public provider transport delivery; it proves the backend control frame applies consistently once delivered over the authenticated text/control path.
- This task does not implement owner transfer, broader revocation completion, or new OpenMLS membership flows.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t03 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml staff_promotion --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t03 cargo test -p discrypt-mls-core governance_authorization_matrix -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t03 cargo fmt --check`
- `git diff --check`

## Evidence Boundary

This produces local backend harness evidence for governance-frame convergence across three persisted profiles. Production readiness still requires real authenticated transport delivery evidence and the broader release gate.

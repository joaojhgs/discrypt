# P5-T06 pending request reconciliation plan

Issue: PER-48 / P5-T06

## Requirements Summary

- Source task: Phase 5 governance, "Pending request reconciliation."
- Acceptance criteria: multiple staff see the same pending admission list; approval/refusal supersedes correctly; logs are retained.
- Required evidence: concurrent decision tests.
- Scope constraints from issue metadata: backend/Rust/Tauri governance-state only; do not implement presence heartbeat UI, right member panel, audit-log UI, voice, overlay, or later release-gate work.
- Product invariants: invite parsing is not membership; approved admission only counts for protected group access after authorized OpenMLS Welcome/add and persisted OpenMLS group state; governance state remains backend-governed; public rendezvous providers are signaling only.
- Available context: `.omc/plans/discrypt-plan.md` Phase 5 / AC-GOV / AC16, `docs/release/handoff-2026-06-10-current-state.md`, and adjacent plans `.omx/plans/P5-T01-canonical-governance-state-2026-06-19.md` and `.omx/plans/P5-T05-governance-frame-replication-2026-06-20.md`.
- Missing expected context: `.omx/plans/production-release-master-plan-2026-06-10.md`, `.omx/plans/admin-role-admission-plan-2026-06-04.md`, and `.omx/plans/peer-overlay-group-transport-plan-2026-06-05.md` are named by runtime context but are not present in this checkout.

## Code Anchors

- `apps/desktop/src-tauri/src/lib.rs`: `GroupAdmissionRequestView`, `GroupGovernanceLogEntryView`, `TextControlFrameView::GroupAdmissionDecision`, `mark_group_admission_decision`, `approve_group_admission_request`, `refuse_group_admission_request`, `handle_text_control_frame`, and existing governance/admission tests.
- `crates/mls-core/src/governance.rs`: signed, ordered governance primitives and canonical comparator; no direct changes expected for this task.

## Acceptance Criteria

- Multiple owner/staff replicas that receive the same admission key-package frame persist the same pending request id, signer, key package, and pending status.
- The first locally accepted approval/refusal decision is terminal for the request on that replica.
- Later conflicting decision frames are reconciled as superseded audit entries without mutating the terminal request status or duplicating accepted decision logs.
- Identical replay of an already accepted decision remains idempotent and does not duplicate governance log rows.
- Refusal never promotes a requester to group membership, and approval still does not let a joiner claim protected membership without the OpenMLS Welcome path.
- Evidence is local backend/Tauri harness evidence only, not production split-machine or public-provider proof.

## Implementation Steps

1. Inspect the current admission request and governance log reducer behavior.
2. Add a small helper for deterministic admission-decision event ids so accepted decision logs are unique per request/decision and superseded conflict logs are retained once per conflicting frame.
3. Update `mark_group_admission_decision` so accepted decisions remain idempotent while later conflicting decisions append a superseded audit entry and return success without changing the request.
4. Add regression tests in `apps/desktop/src-tauri/src/lib.rs` covering shared pending-list reconciliation across two staff replicas and concurrent conflicting approval/refusal decision order.
5. Run targeted desktop backend tests, MLS governance tests if touched behavior intersects governance primitives, formatting, and diff checks.

## Failure Modes And Safety

- Stale conflicting decisions must not overwrite the first accepted decision; mitigation: reducer checks terminal status before mutation and records only a superseded audit entry.
- Duplicate frames must not spam the governance log; mitigation: stable event ids for accepted and superseded decision rows.
- Approval must not fake protected membership; mitigation: keep OpenMLS Welcome generation and joiner handle application as the only path that persists OpenMLS membership state.
- Reconciliation evidence is a local harness result; release/PR notes must not claim production public-provider or split-machine readiness.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t06 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml pending_request_reconciliation --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t06 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml g007_manual_admission_approval_persists_openmls_join_without_auto_approving_old_requests --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t06 cargo test -p discrypt-mls-core governance -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t06 cargo fmt --check`
- `git diff --check`

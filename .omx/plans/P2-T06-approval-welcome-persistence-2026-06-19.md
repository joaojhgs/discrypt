# P2-T06 Approval Welcome Persistence

## Source And Scope

- Issue: PER-18 / P2-T06, "Owner/staff approval sends Welcome and persists it."
- Plan source: issue body Phase 2 acceptance criteria, `.omc/plans/discrypt-plan.md` AC3, `.omx/plans/P2-T04-pending-join-non-member-state-2026-06-17.md`, and `docs/release/handoff-2026-06-10-current-state.md`.
- Current release invariants: invite parsing is not membership; protected group text requires authorized MLS Welcome/add and persisted OpenMLS group state; owner/staff approval must fail closed if Welcome generation fails.
- Primary code paths: `approve_group_admission_request`, OpenMLS admission helpers in `apps/desktop/src-tauri/src/lib.rs`, and `crates/mls-core/src/openmls_engine.rs`.
- Scope boundary: prove manual approval Welcome persistence and protected text gating. Do not add automatic admission, refusal expiry policy, password/PAKE, transport relay behavior, voice, overlay, or UI redesign.

## Acceptance Criteria

- Pending invite joiner cannot send protected group text before approval and has no persisted OpenMLS group handle from invite parsing alone.
- Owner/staff approval generates a real OpenMLS Welcome from the pending key package before marking the request approved.
- The approval queues a Welcome frame for the joiner, and applying it persists the joiner's OpenMLS group handle and admitted member role.
- Reloading the joiner profile after Welcome keeps the OpenMLS handle loadable and protected group text send succeeds.
- `mls-core` unit coverage proves Welcome join state can be reopened from SQLite-backed OpenMLS storage and exporter secrets still converge.

## Implementation Steps

1. Strengthen the existing manual admission Tauri test in `apps/desktop/src-tauri/src/lib.rs` so it checks pre-approval send failure in the same approval path, then verifies post-Welcome reload and protected send success.
2. Strengthen the existing `openmls_join_from_welcome_validates_and_converges` unit test in `crates/mls-core/src/openmls_engine.rs` so Bob's joined group is loaded after engine reopen and exporter parity remains intact.
3. Run targeted Rust tests for the touched paths plus formatting/static diff checks.
4. Record evidence and any skipped checks in the Multica handoff and PR.

## Risks And Mitigations

- Risk: a test could pass by invite-created pending UI state rather than MLS evidence. Mitigation: assert pre-approval `admission_pending`, empty joiner OpenMLS handles, post-Welcome persisted handle, and post-reload send success.
- Risk: owner approval could mark a request approved before Welcome generation. Mitigation: keep the existing command ordering and test the queued `OpenMlsAdmissionWelcome` frame produced by approval.
- Risk: storage proof could be only in-memory. Mitigation: drop/reopen the OpenMLS engine before the final load/export parity assertions.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-mls-core openmls_join_from_welcome_validates_and_converges`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-desktop g007_manual_admission_approval_persists_openmls_join_without_auto_approving_old_requests`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-desktop g012_pending_invite_joiner_cannot_send_before_openmls_welcome`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

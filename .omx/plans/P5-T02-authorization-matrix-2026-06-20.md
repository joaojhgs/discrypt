# P5-T02 authorization matrix plan

## Source and scope

- Issue: PER-44 / P5-T02, "Authorization matrix."
- Available source context: issue body, `.omc/plans/discrypt-plan.md` Phase 5, `docs/release/handoff-2026-06-10-current-state.md`, and `.omx/plans/P5-T01-canonical-governance-state-2026-06-19.md`.
- Missing source context: `.omx/plans/production-release-master-plan-2026-06-10.md` is named by the issue but is not present in this checkout.
- Scope boundary: implement and verify the backend authorization matrix for approve/refuse/promote/revoke only. Do not implement broader governance replication, staff promotion sync, revoke/kick cryptographic completion, presence, member panel UI, voice, overlay, or release-gate work.

## Code anchors

- `crates/mls-core/src/governance.rs`: signed governance primitive authority checks already distinguish owner, admin/staff, and member capabilities.
- `apps/desktop/src-tauri/src/lib.rs`: Tauri command surface for `approve_group_admission_request`, `refuse_group_admission_request`, `promote_group_member_to_staff`, and `revoke_group_member_access`.
- `apps/desktop/src-tauri/src/lib.rs`: `local_actor_can_decide_group_admission` currently authorizes admission decisions and must rely on a current roster row, not a legacy group role label.

## Acceptance criteria

- Members cannot approve admission, refuse admission, promote members, or revoke members.
- Staff can approve/refuse pending admission requests and revoke members only.
- Staff cannot promote members, revoke staff, revoke owners, or revoke themselves.
- Owner can promote members/staff targets as existing command semantics allow and revoke staff/member targets.
- Unauthorized Tauri commands return explicit command errors and do not mutate group role state, admission request status, member revocation status, governance log, or governance outbox.
- Tests exercise backend-governed roster roles, not frontend or legacy group labels.

## Implementation steps

1. Tighten `local_actor_can_decide_group_admission` so missing, pending, or revoked local roster rows fail closed instead of falling back to `group.role`.
2. Add focused Tauri backend unit coverage for the command authorization matrix using real roster role rows:
   - member denial for approve/refuse/promote/revoke;
   - staff allow for approve/refuse/revoke member;
   - staff denial for promote/revoke staff-or-owner;
   - owner allow for promote/revoke staff/member.
3. Add regression coverage proving a legacy owner `group.role` label without a current backend-governed local member row cannot approve/refuse admission.
4. Preserve existing OpenMLS fail-closed revocation behavior and do not broaden crypto-removal claims.
5. Run targeted Rust tests and format checks; record local/harness evidence only.

## Risks and safety behavior

- Risk: a malformed or legacy persisted group may temporarily block admission decisions until the roster is repaired. Mitigation: fail closed with an explicit recovery hint instead of authorizing from a legacy label.
- Risk: revocation tests can accidentally assert OpenMLS production completion. Mitigation: keep tests focused on command authorization/mutation boundaries and existing fail-closed app-state behavior.
- Risk: role promotion tests could hide frontend labels. Mitigation: mutate only `GroupMemberView.role` rows in the persisted backend state.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t02 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml authorization_matrix --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t02 cargo test -p discrypt-mls-core governance_authorization_matrix -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t02 cargo fmt --check`

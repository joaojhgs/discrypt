# P5-T01 canonical governance state plan

## Source Context

- Multica issue: PER-43 / P5-T01.
- Product plan anchors: `.omc/plans/discrypt-plan.md` Phase 5, AC-GOV, AC16, R16.
- Current release handoff: `docs/release/handoff-2026-06-10-current-state.md` says manual admission and presence must come from backend policy data.
- Existing code anchors: `crates/mls-core/src/governance.rs` already covers signed epoch-bound governance ordering and authority; `apps/desktop/src-tauri/src/lib.rs` already serializes `GroupMemberView`, `GroupRolePolicyView`, `GroupAdmissionRequestView`, `GroupGovernanceLogEntryView`, and presence TTL fields; `crates/storage/src/appdb.rs` owns the durable SQL schema/migration contract.

## Scope

Add the smallest durable schema contract needed for canonical governance state:

- Persist owner/staff/member roster rows outside the legacy `groups.role` label.
- Persist group role/admission policy with policy epoch and actor.
- Persist admission requests and their policy/admission-mode snapshot.
- Persist append-only governance log summaries alongside signed governance events.
- Persist presence TTL as explicit member presence state.

This does not replace OpenMLS membership/admission checks and must not make invite parsing count as membership.

## Implementation Steps

1. Bump the app DB schema version and add v2 DDL in `crates/storage/src/appdb.rs`.
2. Add required manifest entries and column contracts for `group_role_policy`, `group_members`, `group_admission_requests`, `group_governance_log`, and `group_member_presence`.
3. Extend migration planning for 0->2, 1->2, 2->1, and 2->0 so upgrades and recovery rollbacks are testable.
4. Add schema/migration tests asserting the new governance tables and columns cover role policy, admission request, governance log, and presence TTL fields.
5. Run targeted storage tests and formatting.

## Failure Modes And Safety

- Old v1 stores must upgrade additively; no existing profile/group/message rows are dropped during 1->2.
- Recovery rollback can drop v2 governance tables before dropping v1 tables, but normal forward migration must preserve unreadable state and OpenMLS state boundaries.
- Presence remains TTL-backed data; `online` must still require backend heartbeat evidence, not stale serialized rows.
- Admission requests remain pre-membership state; protected group access still requires OpenMLS Welcome/add and persisted group state.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t01 cargo test -p discrypt-storage governance -- --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t01 cargo test -p discrypt-storage migration -- --nocapture`
- `git diff --check`


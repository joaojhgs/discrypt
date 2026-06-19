# P2-T08 Invite Failure Handling

## Source And Scope

- Issue: PER-20 / P2-T08, "Refuse/expired/revoked/max-use handling."
- Plan source: issue body Phase 2 acceptance criteria, `.omc/plans/discrypt-plan.md` AC3, `docs/release/handoff-2026-06-10-current-state.md`, `docs/release/current-regressions.md`, and adjacent Phase 2 plans for invite schema, pending non-member state, Welcome persistence, and automatic admission.
- Current release invariants: invite parsing is not membership; protected group text/voice requires authorized MLS Welcome/add and persisted OpenMLS group state; failed admission/invite attempts must not create joined, admitted, or connected UI/backend state.
- Primary code paths: `apps/desktop/src-tauri/src/lib.rs`, `apps/ui/src/commands.ts`, and `apps/ui/tests/e2e/stateful-ui.spec.ts`.
- Scope boundary: enforce refusal/expired/revoked/max-use failure behavior at the command/UI fallback boundary. Do not implement distributed cross-profile invite-consumption synchronization, new governance invite-revocation propagation, password/PAKE, or transport behavior.

## Acceptance Criteria

- Local revoked/refused invite rows fail with a typed command error before group focus, pending group creation, OpenMLS key-package queueing, or invite use increments.
- Local expired invite rows fail with a typed command error before group focus, pending group creation, OpenMLS key-package queueing, or invite use increments.
- Local max-used invite rows fail with a typed command error before group focus, pending group creation, OpenMLS key-package queueing, or invite use increments.
- Parsed expired invite links fail with a typed command error before any pending group/contact state is created.
- UI/Playwright coverage proves the visible failure copy and absence of optimistic pending group labels.

## Implementation Steps

1. Add shared invite-use validation helpers in `apps/desktop/src-tauri/src/lib.rs` for local rows and parsed metadata, mapping failure classes to stable command error codes.
2. Call validation from `join_group` and `accept_dm_invite` before any state mutation that could imply opened/joined/admitted state.
3. Preserve parsed descriptor revocation/consumed-use metadata so signed descriptors with revocation or exhausted use evidence are rejected before pending state.
4. Mirror the same validation behavior in the local-dev UI fallback in `apps/ui/src/commands.ts`.
5. Add focused Rust regression tests for revoked/refused, expired, max-used, and parsed expired invite handling.
6. Add Playwright coverage for visible revoked, max-used, and expired invite failures without pending group labels.

## Failure Modes And Safety Behavior

- Expired clocks: validation only rejects when RFC3339 expiry parses and is in the past; malformed legacy expiry remains parse-only compatibility rather than a production-ready claim.
- Local-only max-use: this task enforces persisted local invite rows and parsed consumed-use evidence. It does not claim distributed max-use synchronization across profiles.
- Revocation: local revoked rows and parsed descriptor revocation evidence fail closed. Future governance propagation can wire remote revocation events into the same row/descriptor fields.
- Rollback: validation is command-boundary only; removing the helpers restores prior behavior without data migration.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-desktop local_refused_expired_and_max_used_group_invites_fail_without_state_promotion expired_parsed_invite_fails_before_pending_group_state -- --test-threads=1`
- `npm --prefix apps/ui run typecheck`
- Targeted Playwright: `npm --prefix apps/ui run test:e2e -- --grep "expired revoked and max-used invites fail clearly"`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

## Evidence Boundary

This is local/backend command and local-dev UI harness evidence. It is not a production claim for distributed invite-use synchronization, remote governance revocation propagation, or full two-profile OpenMLS Welcome admission.

# P2-T07 Automatic Admission Mode

## Source And Scope

- Issue: PER-19 / P2-T07, "Automatic admission mode."
- Plan source: issue body Phase 2 acceptance criteria, `.omc/plans/discrypt-plan.md` AC3, `docs/release/handoff-2026-06-10-current-state.md`, and adjacent P2 plans for pending join and approval Welcome persistence.
- Current release invariants: invite parsing is not membership; protected group text/voice requires authorized MLS Welcome/add and persisted OpenMLS group state; provider signaling carries admission control frames only, not application relay payloads.
- Primary code path: `apps/desktop/src-tauri/src/lib.rs` admission mode handling, text/control admission frames, OpenMLS Welcome generation, and multi-profile tests.

## Acceptance Criteria

- New admission key-package requests received while `automatic_when_authorized_online` is active auto-approve only when an authorized local owner/staff roster row has a non-expired backend presence heartbeat.
- Automatic approval generates a real OpenMLS Welcome before marking the request approved.
- Requests received while no authorized owner/staff is online remain pending and do not claim admitted membership.
- Requests originally created under `manual_approval` remain pending after a later switch to automatic mode.
- Joiners still become admitted only after applying the authorized OpenMLS Welcome and persisting the OpenMLS group handle.

## Implementation Steps

1. Add a backend helper that proves the local admission authority is owner/staff and currently online via non-expired presence TTL.
2. Change automatic-mode admission handling so ineligible requests are persisted pending, while eligible requests persist an approved admission row only after Welcome generation succeeds.
3. Preserve manual-mode behavior and explicit manual approval command behavior.
4. Add/extend multi-profile Rust tests covering manual-to-auto switch, offline automatic pending behavior, online automatic Welcome generation, and post-Welcome OpenMLS persistence.

## Risks And Mitigations

- Risk: automatic mode could approve from stale roster labels. Mitigation: require a non-revoked/non-pending owner/staff member row for the local identity and a non-expired `presence_expires_at`.
- Risk: a request might be marked approved before OpenMLS evidence exists. Mitigation: generate `OpenMlsAdmissionWelcome` first, then mark the persisted request approved.
- Risk: old manual requests could be swept during mode switch. Mitigation: keep `set_group_admission_mode` policy-only and assert the old request remains pending.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-desktop g007_manual_admission_approval_persists_openmls_join_without_auto_approving_old_requests`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-desktop g007_automatic_admission_requires_authorized_owner_or_staff_online`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

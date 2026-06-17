# P2-T04 Pending Join Non-Member State

## Source And Scope

- Issue: PER-16 / P2-T04, "Implement pending join state as non-member."
- Plan source: issue body Phase 2 acceptance criteria, `.omc/plans/discrypt-plan.md` AC3, and `docs/release/handoff-2026-06-10-current-state.md`.
- Current release invariants: invite parsing is not membership; protected text/voice requires authorized MLS Welcome/add and persisted OpenMLS state; UI/backend must not claim joined/admitted from frontend-only or invite-only state.
- Primary code path: `apps/desktop/src-tauri/src/lib.rs` join/admission, protected text send, voice join, governance reload, and tests.

## Acceptance Criteria

- Manual-mode invite joiner persists an explicit pending state that survives reload and is not surfaced as an admitted member role.
- Pending joiner cannot send protected group text and cannot join protected voice/media.
- Pending join creates no OpenMLS group handle until an authorized Welcome/add is applied.
- Group naming for signed descriptors remains derived from signed group id/descriptor data, not unsigned `gname`.
- After owner/staff approval and Welcome application, the joiner can send protected text without "missing OpenMLS group state."

## Implementation Steps

1. Keep invite join as admission-request state only: preserve pending local roster row for UI compatibility, but set group compatibility role to `pending` and prevent governance reload from rewriting it to `member` before Welcome evidence.
2. Tighten backend truth gates: make role helpers and voice join reject pending local membership; keep protected text using the existing `admission_pending` and OpenMLS-handle checks.
3. On Welcome application, ensure local pending state becomes admitted member state and compatibility role becomes `member` only when OpenMLS join succeeds.
4. Add targeted Rust tests in `apps/desktop/src-tauri/src/lib.rs` covering pending reload/non-member role, signed descriptor naming, text rejection, voice rejection, no pre-Welcome OpenMLS handle, and post-Welcome protected text send.

## Risks And Mitigations

- Risk: existing UI/tests may still expect `group.role == "member"` after invite parsing. Mitigation: only change pending joiner compatibility role to `pending`; owner-created and Welcome-admitted groups remain owner/member.
- Risk: role migration helpers could re-admit pending groups on reload. Mitigation: explicitly skip pending local rows when deriving group role.
- Risk: voice join could create a joined state from a pending group. Mitigation: reject before creating `voice_session`.

## Verification

- Run focused desktop backend tests for pending invite/admission and OpenMLS send paths.
- Run `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`.
- Run `git diff --check`.
- If local Rust tooling is unavailable or too slow, document the exact blocker and rely on CI after PR creation.

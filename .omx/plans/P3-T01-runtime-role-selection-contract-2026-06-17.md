# P3-T01 runtime role selection contract - 2026-06-17

## Source and Scope

- Issue: PER-22 / P3-T01, Phase 3 provider-signaled WebRTC text/control reliability.
- Source context: `.omc/plans/discrypt-plan.md` Phase 3 text/control and handoff `docs/release/handoff-2026-06-10-current-state.md`.
- Missing in checkout: the named production master/admin/overlay plan files are not present; the issue body remains authoritative.
- Code paths: `apps/desktop/src-tauri/src/lib.rs` text/control runtime attach, group invite/admission state, and existing runtime peer tests.

## Invariants

- Signaling providers remain signaling-only for SDP/candidates; no application message relay fallback.
- Invite parsing is not protected membership, but runtime role selection must use persisted backend group/member state once the local group context exists.
- UI/runtime may not claim transport attachment unless the backend derives a live role and peer ids from current state.

## Acceptance Criteria

- Owner and staff group contexts derive `ProviderTextControlRuntimePeerRole::Offerer`.
- Member group contexts derive `ProviderTextControlRuntimePeerRole::Answerer`.
- Role derivation survives persisted state reload.
- Invite joiner derives answerer from current backend group membership/admission state, not stale `runtime_peers` labels.
- Stale `runtime_peers` rows cannot invert the selected role when backend membership says owner/staff/member.
- Missing migration-only roster defaults or missing signed group bootstrap peer IDs fail closed instead of using legacy `group.role` or persisted `runtime_peers`.

## Implementation Steps

1. Extract a small runtime role contract helper for DM and group attachments in `apps/desktop/src-tauri/src/lib.rs`.
2. Derive group offerer/answerer from a backend-governed `GroupRoleView` plus validated stable local/remote peer ids, not from stale persisted peer role strings.
3. Require signed bootstrap peer-id regeneration so local/remote peer ids come from invite/connectivity metadata.
4. Add targeted unit tests for owner/staff/member after reload and invite join, including stale `runtime_peers` mutation and fail-closed missing-state cases.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- Targeted Tauri backend tests:
  - `group_text_runtime_attachment_derives_owner_offer_role_after_reload`
  - `group_text_runtime_attachment_derives_staff_offer_role_after_reload`
  - `group_text_runtime_attachment_derives_member_answer_role_after_invite_join`
  - `group_text_runtime_attachment_fails_without_current_local_member_role`
  - `group_text_runtime_attachment_fails_without_signed_bootstrap_peer_ids`
  - existing runtime peer/attach regression tests where feasible

## Risks and Safety

- Failure mode: both peers choose the same WebRTC role. Mitigation: role is now selected from current backend role contract, and tests mutate stale runtime peers to prove they cannot invert the role.
- Failure mode: migration-only role defaults or invalid/stale peer ids. Mitigation: attachment construction requires backend-governed role evidence plus signed bootstrap peer-id regeneration, validates `SignalingPeerId`, and fails closed before runtime attach.
- Rollback: revert the helper/tests; no persisted schema changes.

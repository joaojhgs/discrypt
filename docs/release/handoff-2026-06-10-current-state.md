# Discrypt current state handoff - 2026-06-10

## Current verdict

Discrypt is not production-ready. Historical green ledgers remain useful
context, but the release reset requires fresh evidence after the known
regressions around invites, manual admission, presence, WebRTC ICE state, and
storage vault reinstall behavior.

The current known-bad regression ledger is
[`current-regressions.md`](current-regressions.md).

## Regression summary

The following user-reported scenarios are open regression targets:

- `REG-INVITE-BROKEN-GROUP` - invite broken group.
- `REG-MANUAL-ADMISSION-INVISIBLE` - manual admission invisible.
- `REG-PRESENCE-OFFLINE` - presence offline.
- `REG-WEBRTC-ICE-STATE-NEW` - WebRTC ICE state new.
- `REG-STORAGE-VAULT-REINSTALL-FAILURE` - storage vault reinstall failure.

## Claim boundary

This handoff is a current-state anchor, not production-ready evidence. It does
not claim that any listed scenario is fixed, and it does not replace the later
test tasks required to prove invite/admission, presence, transport route, or
storage recovery behavior.

Frontend and release copy must continue to preserve backend truth:

- Invite parsing is not group membership.
- Manual admission state must come from backend policy data.
- Online presence requires verified backend/provider evidence.
- WebRTC delivery/media state requires route evidence; signaling providers are
  not application relays.
- Existing unreadable storage must not be overwritten or silently reset.

## Verification expectation

Each regression in `current-regressions.md` names the later verification mapping
that must be implemented before release docs can promote the scenario out of
known-bad status. Documentation-only updates may cite static checks, but they
are not production behavior evidence.

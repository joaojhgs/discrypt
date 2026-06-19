# P4-T05 group actions relocation

Issue: PER-35 / P4-T05
Scope: relocate group actions in the existing Discord-like shell without changing backend membership, admission, invite, or transport semantics.

## Plan

1. UX flow: keep join/create group entry in the server rail `+` launcher; keep create invite and group configuration behind each real group rail item's context menu.
2. Component boundary: use the existing `ServerRail`, `GroupContextMenu`, `TopBar`, `LauncherPanel`, `GroupInvitePanel`, and `GroupConfigPanel` surfaces. Do not introduce new frontend-only state.
3. Backend truth source: continue calling existing Tauri/fallback commands (`createGroup`, `joinGroup`, `createInvite`, `setConnectivityPolicy`, `setGroupAdmissionMode`) and render only returned backend state.
4. Accessibility and focus: preserve pointer right-click plus keyboard context-menu access (`Shift+F10` / `ContextMenu`), menu focus, Escape close, and dialog labels.
5. Verification: strengthen the focused Playwright context-menu flow so it proves group invite/config actions live in the group context menu, join/create access starts from the rail launcher, and the header has no displaced group action clutter.

## Evidence Boundary

This is local frontend harness evidence only. It does not claim production invite admission, OpenMLS membership, verified presence, delivery, voice route, or storage recovery readiness.

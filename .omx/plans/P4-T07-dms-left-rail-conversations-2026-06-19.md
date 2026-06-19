# P4-T07 DMs as left-rail conversations

## Scope

PER-37 requires direct messages to live in the far-left conversation rail, not
inside the selected group's channel sidebar. Selecting a DM must open a
chat-only view without group channel navigation or member/presence controls.

## UX flow

- Group buttons remain in the server rail above the separator.
- DM buttons remain in the same rail below the separator and are treated as
  conversation targets.
- Selecting a group opens the normal group channel shell.
- Selecting a DM opens only the DM timeline and composer.
- On mobile, the existing bottom navigation remains the constrained navigation
  affordance; the DM view still hides group channel navigation.

## Component boundaries

- `ServerRail` owns group and DM conversation target buttons.
- `ChannelSidebar` remains group-only and is not rendered for `workflow === "dm"`.
- `DmPanel` owns the selected DM timeline and empty-start state.
- `TopBar` must not expose member-panel controls while a DM is active.

## Backend truth source

DMs, selected DM, messages, connectivity, and command outcomes continue to come
from the existing Tauri command-backed `AppState`. This task does not add local
optimistic delivery, online, admission, or transport claims.

## Accessibility and focus

- DM rail buttons keep descriptive `aria-label` text.
- The active rail target uses `aria-current="page"`.
- The member panel button is removed from DM mode because no verified group
  member panel applies to a private conversation.

## Verification

- `npm --prefix apps/ui run typecheck`
- `npm --prefix apps/ui run build`
- Targeted Playwright coverage in `apps/ui/tests/e2e/stateful-ui.spec.ts`
  proving DMs render from the rail, DM mode hides channel navigation/member
  controls, and desktop/mobile screenshots are captured.

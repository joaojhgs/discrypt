# P6-T03 - Voice Channel UX

Source: Multica PER-54 / Phase 6 two-person voice chat production path.

## Requirements Summary

Voice room clicks must drive the existing backend/Tauri `join_voice` path. The UI may show joined state, participant rows, mute/leave controls, mic gain, and app output only from `voice_session.joined` and the backend session channel id. Permission denied or missing device states must fail closed to idle with a precise command notification.

## UX Flow

1. The user clicks a voice room in the channel sidebar or mobile voice panel.
2. The frontend requests real microphone/device access, then calls `joinVoice` with the selected group/channel and device evidence.
3. Only a joined backend `voice_session` marks the matching voice room active and renders participants under that room.
4. The sidebar voice footer remains the control surface for connectivity/status copy, mic gain, app output, mute, and the leave icon.
5. Leaving voice calls `leaveVoice` and removes participants/leave controls from the sidebar.

## Component Boundaries

- `apps/ui/src/main.tsx`: derive displayed joined room from `voice_session.channel_id` when `voice_session.joined` is true, then pass that id to desktop/mobile voice navigation.
- `ChannelSidebar`: render participant list under the backend-joined room with an accessible participant-region label.
- `apps/ui/tests/e2e/stateful-ui.spec.ts`: cover room switching, participant placement, leave controls, and permission-denied fail-closed state.

## Backend Truth Source

The frontend uses `AppState.voice_session` from Tauri/fallback command state. It does not introduce local joined state. `voice_session.joined` gates active voice UI, and `voice_session.channel_id` chooses the room that can display participants.

## Accessibility And Focus

Voice room buttons keep `aria-current="page"` only for the joined room. The participant container has an `aria-label` of `<room name> participants`, and the leave icon retains `aria-label="Leave voice call"`.

## Acceptance Criteria

- Clicking a voice room joins only after backend `voice_session.joined` is true.
- Participants render under the backend joined voice room, not merely the last active voice context.
- The leave icon is visible only while joined and clears participant UI after leave.
- Sidebar exposes connectivity/status copy, mic gain, app output, and mute controls.
- Permission-denied joins remain idle and show no participant or leave control.

## Verification

- `npm --prefix apps/ui run typecheck`
- `npm --prefix apps/ui run build`
- `npm --prefix apps/ui exec playwright test apps/ui/tests/e2e/stateful-ui.spec.ts -g "voice channel"`

Evidence classification: frontend/UI harness evidence with backend-command state fixtures. It does not prove production two-machine media transport by itself.

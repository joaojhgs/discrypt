# P4-T08 Message Design Polish Plan

## Requirements Summary

- Keep the main chat surface Discord-like: message rows, not per-message bubbles.
- Show compact sent/delivered/failed-style status icons with tooltip details.
- Preserve backend truth: delivery state must come from `AppMessageView.state_key`, `state_label`, `state_detail`, and `peer_receipt`.
- Keep diagnostics detail optional and avoid permanent proof/debug clutter in the normal chat surface.

## Implementation Steps

1. Update the existing tooltip primitive so tooltip content is visible on hover and keyboard focus while remaining accessible.
2. Update `MessageRow` in `apps/ui/src/main.tsx` to use tooltip-backed status indicators and data attributes that make visual regression assertions precise.
3. Add targeted Playwright coverage in `apps/ui/tests/e2e/stateful-ui.spec.ts` for no bubble styling, compact status icons, tooltip details, and desktop/mobile screenshots.

## Backend Truth Source

No backend or Tauri command changes are needed. The UI will continue to render only the message state supplied by `AppMessageView`; local fallback messages remain "Sent locally" and do not claim remote delivery.

## Accessibility And Focus

The message status indicator will be keyboard-focusable with an `aria-label`, visible focus ring, and tooltip text available on hover/focus.

## Verification

- `npm --prefix apps/ui run typecheck`
- `npm --prefix apps/ui run build`
- Targeted Playwright coverage for message row visual/tooltip behavior, with desktop and mobile screenshots.

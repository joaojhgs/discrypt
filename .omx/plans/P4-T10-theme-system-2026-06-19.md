# P4-T10 Theme System Plan

Issue: PER-40 / P4-T10
Scope: make the existing dark shadcn-style theme system explicit, extensible, documented, and covered by focused frontend evidence.

## Requirements Summary

- Dark remains the default app appearance.
- Theme palettes are defined as a shared token contract, not one-off image-matching CSS.
- Primary shell and shadcn primitives continue to consume CSS variables.
- The settings appearance flow remains a modal/settings flow and does not add permanent chat clutter.
- No theme change may imply backend state such as joined, admitted, online, delivered, voice-active, or connected.

## UX Flow

1. Users keep changing appearance from the existing settings appearance card.
2. Selecting a theme saves preferences through the existing command-backed preferences path.
3. The selected theme applies to the root app shell through CSS variables so existing components update without per-screen rewrites.

## Component Boundaries

- `apps/ui/src/app-config.ts` owns the token contract, default theme id, theme definitions, and helper functions for theme style generation.
- `apps/ui/src/styles.css` owns the static boot/default dark theme variables before React state loads.
- `apps/ui/src/main.tsx` resolves the active theme and applies the generated variables to the app shell, first-run, and storage panels.
- `apps/ui/tests/e2e/stateful-ui.spec.ts` owns focused visual/token assertions and screenshot artifacts.
- `docs/release/p4-t10-theme-system-2026-06-19.md` records visual token evidence and the local/harness boundary.

## Backend Truth Source

Theme changes use existing app preferences returned through current command-backed app state and `savePreferences`. This task does not add backend commands or local-only claims about membership, delivery, presence, voice, transport, storage readiness, or admission.

## Accessibility And Focus

The existing settings modal and select remain the interaction surface. The theme selector keeps its accessible label, keyboard selection behavior, dialog focus management, and Escape/outside-close behavior inherited from existing primitives.

## Verification

- `npm --prefix apps/ui run typecheck`
- `npm --prefix apps/ui run build`
- Focused Playwright test proving default theme token values, shadcn primitive token consumption, settings-driven theme switching, and desktop/narrow screenshots.

## Evidence Boundary

This is frontend local harness evidence only. It proves token wiring and visual application in the UI harness, not production readiness for invite/admission, OpenMLS membership, verified presence, delivery, voice route, transport, or storage recovery behavior.

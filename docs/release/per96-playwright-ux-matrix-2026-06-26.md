# PER-96 Playwright UX Matrix Evidence

## Scope
- Issue: PER-96 / P12-T01 Playwright unit UX matrix.
- Evidence level: frontend Playwright local-dev harness evidence. This is not production split-machine, public-provider, installed-app, or Tauri WebDriver evidence.

## Changed Files
- `.omx/plans/P12-T01-playwright-ux-matrix-2026-06-26.md`
- `apps/ui/tests/e2e/ux-matrix.spec.ts`
- `apps/ui/tests/e2e/stateful-ui.spec.ts`
- `docs/release/per96-playwright-ux-matrix-2026-06-26.md`

## Matrix Coverage
- First-run local profile setup reaches the usable app shell.
- Config dialog changes theme/audio preferences and reload-persistence is asserted.
- Automatic admission is selected during group creation and verified through group configuration.
- Invite creation exposes signed metadata fields, admission copy, password state, and redacted TURN metadata.
- Text channel send remains local-command-backed with compact delivery status; no remote delivery is claimed.
- Manual admission review is driven from command-backed governance state; approve/refuse paths move requests into backend-rendered history and member rows.
- Member panel distinguishes backend route evidence from no route proof and uses TTL-backed offline state.
- Voice join, mute/leave, and microphone-denied failure paths stay in the sidebar and do not fake remote media.
- Diagnostics/manual runtime pairing controls remain hidden by default.

## Verification
- Passed: `npm --prefix apps/ui run typecheck`
- Passed: `PLAYWRIGHT_BROWSERS_PATH=/tmp/discrypt-playwright-browsers npx playwright test tests/e2e/ux-matrix.spec.ts tests/e2e/stateful-ui.spec.ts -g "PER-96|main chat layout" --workers=1` from `apps/ui`
- Passed: `PLAYWRIGHT_BROWSERS_PATH=/tmp/discrypt-playwright-browsers npm --prefix apps/ui run test:e2e`
  - Result: 39 passed, 1 skipped.
  - Existing skipped test: `tests/e2e/config-modal.spec.ts` diagnostics sheet support bundle copy requires explicit consent.

## Artifacts
- PER-96 matrix screenshot: `apps/ui/test-results/ux-matrix-PER-96-Playwrigh-f2d37-xt-voice-config-and-members-chromium/per96-ux-matrix-final.png`

## Known Limits
- The Playwright run uses `VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1`, so command state is persisted in the frontend local-dev fallback. It is suitable frontend UX harness evidence for this issue, but it does not prove real OpenMLS welcome exchange, split-machine transport, native audio devices, TURN routing, or installed Tauri package behavior.

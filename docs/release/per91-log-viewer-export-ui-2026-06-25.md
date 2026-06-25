# PER-91 Log Viewer Export UI Evidence - 2026-06-25

## Scope

PER-91 / P11-T02 adds the frontend support-bundle viewer and export surface for the existing backend `export_diagnostics_log()` command. It does not add new diagnostics backends, crash reporting, storage/MLS/ICE diagnostics, or installed-app release promotion.

## Implemented Behavior

- The Config modal now exposes a consent-gated support bundle panel under "Logs and export".
- The UI reads diagnostics only after the user enables consent, then loads the backend redacted support bundle through `export_diagnostics_log()`.
- The panel shows backend-derived bundle metadata, a scrollable raw redacted JSON preview, copy-to-clipboard, and JSON file export.
- The UI reports denied, loading, empty, command failure, clipboard-unavailable, and file-export-unavailable states without claiming diagnostic success.
- The diagnostics inspector sheet is a secondary support-bundle entry point when `VITE_DISCRYPT_SHOW_DIAGNOSTICS=1`; its `Copy logs` action now has its own explicit consent gate and reports clipboard-unavailable degraded state before claiming a copy.

## Evidence

Artifacts:
- `apps/ui/test-results/config-modal-configuration-3125d-ty-defaults-and-logs-export-chromium/configuration-modal-sections.png`

Commands run:
- `npm --prefix apps/ui --cache /tmp/discrypt-npm-cache ci`
- `npm --prefix apps/ui run typecheck`
- `npm --prefix apps/ui run build`
- `VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1 npm --prefix apps/ui run build`
- `PLAYWRIGHT_BROWSERS_PATH=/tmp/discrypt-playwright-browsers npx playwright test tests/e2e/config-modal.spec.ts --workers=1` from `apps/ui`
- `VITE_DISCRYPT_SHOW_DIAGNOSTICS=1 PLAYWRIGHT_BROWSERS_PATH=/tmp/discrypt-playwright-browsers npx playwright test tests/e2e/config-modal.spec.ts --grep "diagnostics sheet support bundle copy requires explicit consent" --workers=1` from `apps/ui`
- `PLAYWRIGHT_BROWSERS_PATH=/tmp/discrypt-playwright-browsers npx playwright test tests/e2e/production-smoke.spec.ts --workers=1` from `apps/ui`

## Evidence Boundary

This is local frontend and package-smoke evidence for the production shell UI. It proves the consent-gated viewer/export surface, frontend state handling, and integration with the existing backend command/fallback contract. It is not installed-app production support evidence and does not promote Discrypt to production-ready.

# UI honesty gates

G003/G006 lock the UI and command surface against over-claiming production capability while the app is still wiring real backend services.

## Good Taste UI honesty constraints

Good Taste here means restraint and truthfulness, not redesign. Future UI work must keep production labels disabled until backend state proves the capability. Do not say P2P, WebRTC, connected, relay active, TURN active, delivered, or encrypted as a product guarantee from static copy alone.

Allowed copy must be visibly scoped when it is local/harness/local-dev only, for example: `command-backed local state`, `socket delivery not claimed`, `network media route is not connected in this build`, or `encrypted envelope facade recorded by harness`. This makes the limitation legible instead of hiding it behind a polished badge.

## Static gates

- `apps/ui/scripts/honesty-gates.mjs` scans UI/Tauri user-facing source for unqualified capability claims and fails unless the line is explicitly local/harness-scoped or tied to backend state proof.
- `apps/ui/scripts/production-copy-gate.mjs` scans normal UI string and JSX copy for `test`, `honest proof`, `placeholder`, and `not implemented`, then runs the honesty and placeholder gates. Diagnostics and roadmap docs stay outside the normal UI scan and remain covered by their explicit release-review scripts. The Tauri IPC fallback error keeps the exact `local-dev/test harness` wording required by G009 as a diagnostic boundary exception.
- The same gate enumerates every Rust IPC command registered in the Tauri invoke handler and every strict TypeScript command client. A command path that returns local-only copy must not advertise itself as production-ready.
- CI runs the gate with the UI job, alongside the existing command-coverage check, so regressions fail before release packaging or demos.

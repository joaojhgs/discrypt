# P12-T02 Tauri WebDriver Integrated Two-Profile Plan

## Source And Scope
- Issue: PER-97 / P12-T02.
- Source plan context: Phase 12 full E2E harness expansion from the production release reset; current checkout contains the current-state handoff and G012 release evidence docs, while the named master plan file is not present.
- Primary path: `scripts/g012-tauri-webdriver-integrated.mjs`.
- Scope: harden the integrated two-profile Tauri WebDriver harness contract for setup, invite, owner/staff approval, text, voice, persistence, and degraded/unavailable-state evidence.
- Non-scope: PER-98 launcher policy, PER-99 split-machine hardening, public provider matrices, package tracks, release decision work, or backend behavior changes.

## Invariants
- Invite parsing is not membership; admission proof must come from backend approval plus persisted OpenMLS Welcome/add state.
- Providers remain signaling/rendezvous only and must not become application relay fallback evidence.
- UI/WebDriver observations cannot claim joined, admitted, delivered, or voice-active unless backend or transport/media evidence is recorded.
- Dry-run and launch-only checks are not production evidence.

## Implementation Steps
1. Add explicit PER-97 workflow and artifact-contract metadata to the G012 integrated WebDriver manifest and summary.
2. Add a static contract checker that validates the script tokens, release doc/matrix wiring, dry-run manifest, and no overclaiming.
3. Wire the checker into `apps/ui/package.json`.
4. Document PER-97 evidence level, command, artifacts, and truth boundaries under `docs/release`.

## Verification
- `npm --prefix apps/ui run test:p12-t02-tauri-webdriver-integrated`
- `node scripts/g012-tauri-webdriver-integrated.mjs --artifact-dir target/p12-t02-tauri-webdriver-integrated-contract`
- `node scripts/check-per59-release-smoke-proof.mjs`
- `node scripts/check-g012-native-voice-proof.mjs`
- `node scripts/check-release-verification-matrix.mjs`
- `git diff --check`

## Failure Modes
- Missing display/WebKitWebDriver/app binary should produce a failed-preflight manifest instead of a false pass.
- Native voice unavailable should remain explicit degraded evidence; synthetic fallback must not allow production or checkpoint claims.
- If real WebDriver evidence cannot run locally, handoff must label local evidence as harness contract/dry-run only.

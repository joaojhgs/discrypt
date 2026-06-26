# P12-T03 Tauri Two-Profile Launcher Boundary Plan

## Source And Scope
- Issue: PER-98 / P12-T03.
- Source context: Phase 12 full E2E harness expansion. The named 2026-06-10 master plan is not present in this checkout; the current-state handoff and PER-97 WebDriver plan/release docs are present and define the local boundary.
- Primary path: `scripts/g012-tauri-two-profile-e2e.mjs`.
- Adjacent contract: `scripts/g012-tauri-webdriver-integrated.mjs` is the PER-97 action-driving WebDriver harness for setup, invite, approval, text, voice, persistence, and degraded/unavailable evidence.
- Non-scope: split-machine hardening, public provider matrices, packaging, release decision work, backend admission/storage/transport behavior changes.

## Invariants
- Launching two Tauri profiles is launch-smoke evidence only.
- Production success cannot be claimed unless a separate action-driving WebDriver/native artifact proves the required setup/admission/text/voice/persistence actions.
- Invite parsing is not membership; no launcher artifact may imply OpenMLS admission, protected delivery, or voice-active state.
- Signaling providers remain rendezvous-only; the launcher must not introduce provider application relay behavior or claims.

## Implementation Steps
1. Add explicit evidence-boundary fields to the launcher manifest and summary for dry-run, failed preflight, launch smoke, delegated WebDriver, and action-driven evidence status.
2. Add `--delegate-webdriver` support that records and invokes the PER-97 integrated WebDriver command instead of treating launch smoke as completion.
3. Ensure failed preflight and dry-run write summary JSON, so QA can distinguish unavailable runner evidence from success.
4. Strengthen `scripts/check-g012-tauri-two-profile-e2e.mjs` to validate the manifest/summary boundary fields and reject production overclaims.
5. Document PER-98 evidence level under `docs/release`.

## Verification
- `npm --prefix apps/ui run test:g012-tauri-two-profile-e2e`
- `node scripts/g012-tauri-two-profile-e2e.mjs --artifact-dir target/p12-t03-tauri-two-profile-launcher-boundary`
- `git diff --check`

## Failure Modes
- Missing display, Cargo/Tauri CLI, or node modules must produce `failed-preflight` fields rather than launch success.
- Delegated WebDriver failure must be recorded as delegated harness failure, not hidden or reclassified as launch success.
- Plain launch-smoke summary must keep `production_claim_allowed: false`, `action_driven_evidence: false`, and `g012_checkpoint_eligible: false`.

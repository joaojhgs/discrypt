# P0-T04 Current Regressions Plan

## Requirements Summary

Source task: PER-6 / P0-T04, "Add a current known bad scenarios regression list from user reports."

Context consulted:

- Multica issue PER-6 description and routing comment.
- `.omc/plans/discrypt-plan.md` for original product intent.
- Existing release docs under `docs/release/`, especially `production-gap-matrix-2026-06-01.md`, `release-verification-matrix.md`, and `g011-production-readiness-matrix.md`.

Unavailable expected context:

- `.omx/plans/production-release-master-plan-2026-06-10.md` is not present in this checkout or local refs.
- `docs/release/handoff-2026-06-10-current-state.md` is not present in this checkout or local refs before this task.

## Acceptance Criteria

- `docs/release/current-regressions.md` exists.
- The regression list includes the reported bad scenarios: invite broken group, manual admission invisible, presence offline, WebRTC ICE state new, and storage vault reinstall failure.
- Each scenario records the user-visible symptom, truth invariant, current evidence boundary, and later verification mapping.
- The current handoff references the regression ledger without claiming that any scenario is fixed.

## Implementation Steps

1. Add a current regression ledger under `docs/release/current-regressions.md`.
2. Create/update `docs/release/handoff-2026-06-10-current-state.md` as a current-state anchor that links to the ledger and preserves the not-production-ready boundary.
3. Keep changes documentation-only; do not change frontend/backend behavior.
4. Run repository-local documentation/static checks that are practical for the changed files, plus `git diff --check`.

## Risks and Mitigations

- Risk: Turning user reports into production claims. Mitigation: label every row as known-bad/pending and explicitly map to future verification tasks.
- Risk: Losing the missing June 10 plan context. Mitigation: cite its absence and use the issue description as authoritative scope.
- Risk: Over-broadening into implementation. Mitigation: keep the edit to release docs and this plan artifact.

## Verification Steps

- `git diff --check`
- `npm --prefix apps/ui run test:release-verification-matrix`
- If package dependencies are unavailable, report the skipped check and keep the evidence boundary explicit.

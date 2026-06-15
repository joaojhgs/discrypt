# P0-T02 Release Gap Matrix Plan

## Requirements Summary

Source task: PER-4 / P0-T02, "Produce a current release-gap matrix that
supersedes stale complete ledgers."

Context consulted:

- Multica issue PER-4 description and metadata.
- `.omc/plans/discrypt-plan.md`, especially the OpenMLS admission, signaling-only
  provider, STUN -> peer overlay -> TURN, and honest metadata/deletion-copy
  constraints.
- `docs/release/handoff-2026-06-10-current-state.md`.
- `docs/release/current-regressions.md`.
- Existing release evidence docs under `docs/release/`, including
  `production-gap-matrix-2026-06-01.md`,
  `release-verification-matrix.md`, and
  `g011-production-readiness-matrix.md`.

Unavailable expected context:

- `.omx/plans/production-release-master-plan-2026-06-10.md` is not present in
  this checkout.

## Acceptance Criteria

- A current release-gap matrix labels every listed feature as exactly one of
  `verified`, `implemented-unverified`, `planned`, or `blocked`.
- The matrix explicitly supersedes older "complete" or green ledgers unless
  fresh evidence is named in the new row.
- Current known-bad regressions remain visible as blockers and cannot be hidden
  by local fallback, docs-only, or harness-only evidence.
- A repository script fails if the final report claims production-ready while
  blockers or other non-verified rows exist.
- No runtime UI, backend, MLS, storage, crypto, or transport behavior changes are
  included.

## Implementation Steps

1. Add `docs/release/release-gap-matrix-2026-06-15.md` with current feature rows,
   evidence boundaries, blockers, and a non-production-ready verdict.
2. Add a small static gate script that checks required sections, allowed status
   labels, stale-ledger supersession wording, known regression rows, and the
   production-ready contradiction rule.
3. Wire the script through `apps/ui/package.json` for repeatable verification.
4. Update the current handoff to point future agents at the new matrix.
5. Run the release-gap gate, release-verification gate, and whitespace/doc
   checks; record exact evidence.

## Risks and Mitigations

- Risk: A docs matrix is mistaken for production proof. Mitigation: mark the
  matrix as a truth reset only and preserve non-claims in every row.
- Risk: Stale green ledgers continue to steer later work. Mitigation: name the
  new matrix as the superseding release truth source and link it from the
  current handoff.
- Risk: The gate blocks legitimate "not production-ready" wording. Mitigation:
  only fail on explicit final-report readiness claims when non-verified rows or
  blocker text remains.
- Risk: Scope creep into Phase 1 implementation. Mitigation: restrict edits to
  release docs, static gate script, package script, and OMX artifacts.

## Verification Steps

- `npm --prefix apps/ui run test:release-gap-matrix`
- `npm --prefix apps/ui run test:release-verification-matrix`
- `git diff --check`

# P0-T01 Worktree Baseline Plan

## Requirements Summary

Source task: PER-3 / P0-T01, "Snapshot current dirty worktree and split changes into Lore-format commits or explicitly shelved patches by lane."

Context consulted:
- Multica issue PER-3 description and routing comment.
- `.omc/plans/discrypt-plan.md`, especially Phase 0 workspace/supply-chain context and product invariants.
- Available release docs under `docs/release/`.

Unavailable expected context:
- `.omx/plans/production-release-master-plan-2026-06-10.md` is not present in this checkout.
- `docs/release/handoff-2026-06-10-current-state.md` is not present in this checkout.

## Acceptance Criteria

- Fresh `git status --short --branch` records whether any production code or untracked files were present.
- Dirty files, if any, are grouped by lane and either committed with Lore-format messages or explicitly listed as shelved patches.
- No adjacent roadmap implementation is bundled into this baseline.
- Final evidence includes branch name, `git log --oneline`, changed/shelved files, and skipped checks with reasons.

## Implementation Steps

1. Create a dedicated task branch. Preferred `multica/P0-T01-worktree-baseline` is blocked by a flat `multica` ref path conflict in the shared repo, so use `multica-P0-T01-worktree-baseline`.
2. Inspect the clean/dirty state with `git status --short --branch` and `git log --oneline`.
3. If dirty production work exists, classify it by lane and create focused Lore-format commits or patch files.
4. If no pre-existing dirty state exists, preserve that fact as the baseline and commit only planning/checkpoint artifacts.
5. Open or update a PR and hand off to QA with explicit evidence.

## Risks and Mitigations

- Risk: Treating a clean fresh checkout as evidence that no dirty state ever existed. Mitigation: report the exact checkout path, branch, and command output scope.
- Risk: Missing the expected 2026-06-10 plan docs. Mitigation: cite their absence and use the issue description plus committed `.omc`/release docs only as context.
- Risk: Creating unrelated product changes. Mitigation: restrict changes to `.omx` planning/ultragoal artifacts unless dirty code is discovered.

## Verification Steps

- `git status --short --branch`
- `git log --oneline -10`
- `git diff --stat HEAD`
- Product tests are intentionally skipped unless product code changes are discovered.

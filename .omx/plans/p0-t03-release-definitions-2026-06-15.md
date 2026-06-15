# P0-T03 Release Definitions Plan

## Requirements Summary

Source task: PER-5 / P0-T03, "Freeze release definitions: production-ready, E2E-tested, split-machine, voice proof, overlay relay."

Context consulted:
- Multica issue PER-5 description and routing comment.
- `.omc/plans/discrypt-plan.md`, especially the locked OpenMLS, SFrame, STUN -> peer-relay overlay -> TURN, and honest metadata/deletion-copy constraints.
- `docs/release/release-verification-matrix.md`.
- `scripts/check-release-verification-matrix.mjs`.

Unavailable expected context:
- `.omx/plans/production-release-master-plan-2026-06-10.md` is not present in this checkout.
- `docs/release/handoff-2026-06-10-current-state.md` is not present in this checkout.

## Acceptance Criteria

- `docs/release/release-verification-matrix.md` defines the minimum evidence for production-ready, E2E-tested, split-machine, voice proof, and overlay relay.
- The definitions distinguish local smoke, Tauri WebDriver, split-machine, and public-provider proof without allowing a weaker row to imply a stronger one.
- A repository gate fails on forbidden absolute overclaim phrases.
- No product runtime, UI, MLS, storage, or transport behavior changes are included.

## Implementation Steps

1. Add a frozen release definitions section to `docs/release/release-verification-matrix.md`.
2. Extend `scripts/check-release-verification-matrix.mjs` to require the definition tokens and reject focused forbidden overclaim phrases.
3. Run `npm --prefix apps/ui run test:release-verification-matrix`.
4. Run an explicit `rg` grep gate for the forbidden phrase list and record the output.
5. Commit with Lore trailers, push the task branch, open a PR, and hand off to QA.

## Risks and Mitigations

- Risk: forbidding legitimate negative wording such as "not production-ready." Mitigation: gate only exact absolute/promotional phrases that lack evidence qualifiers.
- Risk: broadening into release implementation. Mitigation: restrict edits to docs, gate script, and OMX artifacts.
- Risk: stale missing plan docs. Mitigation: record their absence and rely on the issue text plus present `.omc`/release docs.

## Verification Steps

- `npm --prefix apps/ui run test:release-verification-matrix`
- `rg -n "<forbidden-overclaim-regex>" docs README.md apps/ui/src apps/desktop/src-tauri/src crates scripts`

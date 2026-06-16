# P1-T06 recovery roadmap stubs

Source issue: PER-12 / P1-T06, Phase 1 storage foundation. The named master
plan file `.omx/plans/production-release-master-plan-2026-06-10.md` is absent
in this checkout, so this plan uses the issue body, issue metadata,
`docs/release/handoff-2026-06-10-current-state.md`,
`docs/release/current-regressions.md`,
`docs/release/storage-security-roadmap.md`, adjacent Phase 1 storage plans, and
`.omc/plans/discrypt-plan.md`.

## Requirements summary

Add recovery roadmap stubs without implementing or claiming recovery. The
storage path must remain fail-closed: if keyring/vault unlock fails or existing
state is unreadable, Discrypt preserves the files, reports a recovery hint, and
does not create replacement state.

## Acceptance criteria

- `docs/release/storage-security-roadmap.md` describes the clear
  preserve-dont-overwrite path and separates future recovery/migration work
  from current product behavior.
- UI copy points users to the preservation/diagnostic roadmap without claiming
  password, keyring, vault, or profile restore exists.
- A static command gate fails if docs/UI copy drop the preserve-first language
  or introduce fake storage restore claims.

## Implementation steps

1. Update `docs/release/storage-security-roadmap.md` with a current behavior
   section: stop, preserve bytes, show typed error/recovery hint, collect
   diagnostics, and wait for a future explicit migration/recovery flow.
2. Tighten storage setup copy in `apps/ui/src/main.tsx` and fallback
   `apps/ui/src/commands.ts` to use "preserve, don't overwrite" language and
   avoid any storage restore wording.
3. Add `scripts/check-storage-recovery-roadmap-p1-t06.mjs` and expose it as
   `npm --prefix apps/ui run test:p1-t06-storage-recovery-roadmap`.

## Risks and mitigations

- Risk: Documentation sounds like implemented recovery. Mitigation: static gate
  rejects restore/recovered wording near storage password/keyring/vault copy
  unless it is explicitly future/no-claim language.
- Risk: Scope expands into real recovery. Mitigation: this task only edits
  docs/copy/static tests; storage implementation remains unchanged.

## Verification

- `node scripts/check-storage-recovery-roadmap-p1-t06.mjs`
- `npm --prefix apps/ui run test:p1-t06-storage-recovery-roadmap`
- `npm --prefix apps/ui run typecheck`

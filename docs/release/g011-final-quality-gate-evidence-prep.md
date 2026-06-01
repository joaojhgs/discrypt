# G011 final quality gate evidence prep

Generated: 2026-06-01
Scope: G011 production-ready evidence preparation only. This checklist does **not**
claim G012 two-installed-user Tauri text+voice E2E, and it does not replace the
leader-owned final all-green checkpoint after active worker lanes finish.

## Current task-state snapshot

| Task | Owner | Status | Evidence use |
| --- | --- | --- | --- |
| task-1 — production UX/docs/no-placeholder audit | worker-3 | completed | Integrated as commit `1148f3e`; UI copy and placeholder allowlist drift were repaired and verified. |
| task-2 — Linux packaging/build/dependency audit gates | worker-1 | in progress | Leader must wait for final packaging/build/dependency evidence before G011 checkpoint. |
| task-3 — backend integration/persistence/security/privacy gates | worker-2 | in progress | Leader must wait for backend/security/privacy closure before G011 checkpoint. |
| task-4 — production matrix/release docs reconciliation | worker-1 | completed | Matrix/docs reconciliation lane reported complete. |
| task-5 — integrated verification and duplicate/dead-code audit | worker-1 | completed | Initial integrated audit lane reported complete. |
| task-6 — G012-boundary and unsupported adapter fail-closed check | worker-2 | completed | Boundary lane reported complete; preserve G012 as a separate E2E goal. |
| task-7 — final quality gate evidence | worker-1 | completed | Prior evidence lane reported complete, but task-10 replaces stale task-7 evidence prep. |
| task-8 — worker-4 production matrix/release docs reconciliation | worker-4 | completed | Replacement docs/matrix reconciliation lane reported complete. |
| task-9 — replacement integrated verification/dead-code audit | worker-2 | pending | Leader must decide whether task-9 is still required before final all-green. |
| task-10 — replacement final quality gate evidence prep | worker-3 | in progress | This document and safe static-check evidence. |

## Safe static checks run in task-10

These checks were run from the worker-3 worktree on 2026-06-01 without duplicating
task 2, 3, or 8 work:

| Check | Result | Evidence |
| --- | --- | --- |
| `npm --prefix apps/ui run test:command-coverage` | PASS | `46 strict command clients mirror 44 Rust IPC commands`. |
| `npm --prefix apps/ui run test:honesty` | PASS | `22 source files scanned; 44 command paths enumerated`. |
| `npm --prefix apps/ui run test:no-placeholders-g127` | PASS | `74 production source files scanned`. |
| `npm --prefix apps/ui run test:placeholder-allowlist-g128` | PASS | `79 reviewed occurrence classes, 97 total occurrences`. |
| `npm --prefix apps/ui run test:release-no-fallback-g129` | PASS | Release plan rejects `harness`/`local-dev` and runs before packaging. |
| `npm --prefix apps/ui run test:ui-integration-g130` | PASS | Production commands are surfaced through shadcn-owned UI and Playwright coverage. |
| `npm --prefix apps/ui run test:release-verification-matrix` | PASS | `release verification matrix check passed`. |
| `npm --prefix apps/ui run typecheck` | PASS | `tsc --noEmit` exited zero. |
| `cargo fmt --all -- --check` | PASS | Rustfmt check exited zero. |
| `cargo check --workspace --all-targets` | PASS | Finished `dev` profile successfully. |

Rust workspace note: the team worktree needs the sibling path dependency
`../discrypt-signaling`. For task-10 verification, the worker recreated the
same non-git symlink used earlier:
`/home/developer/projects/discrypt/.omx/team/g011-production-ready-5f844bb2/worktrees/discrypt-signaling -> /home/developer/projects/discrypt-signaling`.

## Resume-team task-3 review notes

The crash-resume team `resume-existing-discr-94b2d1e8` task 3 reviewed the
remaining G011 evidence path without mutating `.omx/ultragoal` and without
claiming G012. The review found two concrete release-gate drifts to repair before
leader checkpointing:

- `npm --prefix apps/ui run test:g011-boundary` expected the production gap
  matrix to keep G011 visibly `Not done` until the integrated leader evidence is
  complete and to retain the exact warning that G012 is not complete until real
  Tauri/two-user artifacts exist.
- `npm --prefix apps/ui run test:placeholder-allowlist-g128` required the
  fail-closed `apps/ui/src/voice-media.ts` local-dev/test BroadcastChannel status
  string to be reviewed in the placeholder allowlist.

The same review also confirmed `npm --prefix apps/ui run test:repro-g126` is
not expected to pass before `release:linux` and `sbom:g124` generate package and
SBOM artifacts; the leader should treat missing artifact/SBOM hashes as an
ordering failure, not as G011 reproducibility evidence.

## Leader final-checkpoint checklist

Before checkpointing `G011-production-ready`, the leader should collect fresh
post-integration evidence for the current leader branch:

- [ ] Confirm tasks 2 and 3 are terminal, task 8 has landed in the
      integrated branch, and any required replacement task 9 is terminal or
      explicitly waived with owner evidence.
- [ ] Re-run the full final command set on the integrated branch, including:
  - [ ] `npm --prefix apps/ui run test:command-coverage`
  - [ ] `npm --prefix apps/ui run test:honesty`
  - [ ] `npm --prefix apps/ui run test:no-placeholders-g127`
  - [ ] `npm --prefix apps/ui run test:placeholder-allowlist-g128`
  - [ ] `npm --prefix apps/ui run test:g011-boundary`
  - [ ] `npm --prefix apps/ui run test:release-no-fallback-g129`
  - [ ] `npm --prefix apps/ui run test:ui-integration-g130`
  - [ ] `npm --prefix apps/ui run test:release-verification-matrix`
  - [ ] `npm --prefix apps/ui run typecheck`
  - [ ] `npm --prefix apps/ui run build`
  - [ ] `npm --prefix apps/ui run test:release-linux`
  - [ ] `npm --prefix apps/ui run sbom:g124` after Linux bundles exist
  - [ ] `npm --prefix apps/ui run repro:g126` after SBOMs exist
  - [ ] `npm --prefix apps/ui run test:repro-g126`
  - [ ] `cargo fmt --all -- --check`
  - [ ] `cargo check --workspace --all-targets`
  - [ ] `git diff --check`
- [ ] Add task-2 packaging/build/dependency-audit outputs after that lane lands;
      task-10 intentionally did not duplicate those audits.
- [ ] Add task-3 backend/persistence/security/privacy outputs after that lane
      lands; task-10 intentionally did not duplicate those gates.
- [ ] Reconcile task-8 release-doc updates after integration; task-10 did not
      edit the release matrix to avoid colliding with that lane.
- [ ] Verify all release copy states that G011 is production-ready only within
      the documented G011 gate boundary and keeps G012 two-installed-user
      text+voice E2E pending.

## Resume-team G011 closure evidence (leader, 2026-06-01T17:06Z)

Crash-resume team `resume-existing-discr-94b2d1e8` reached terminal phase
`complete` with 4/4 tasks completed. The leader reconciled worker outputs without
claiming G012 and fixed the final production-storage compile blocker on the
integrated branch. The task-1 worker worktree only retained a duplicate
`FileAppStore` cfg diff after the leader fix, so it was closed as a duplicate
conflict-repair lane rather than merged.

Integrated leader changes since this evidence prep started:

- Linux `production-storage` test builds now keep `FileAppStore` available under
  `#[cfg(test)]` while release Linux production-storage still uses the encrypted
  OS-keychain store.
- `TauriAppService::persist` was restored so persistence failure paths remain
  fail-closed instead of silently losing command errors.
- G011 release/package scripts now require the production feature set for Linux
  release dry-runs and record package/SBOM/reproducibility evidence ordering.
- G011 boundary, placeholder allowlist, production-status sync, package CI,
  SBOM, reproducibility, and release-governance gates are wired in the package
  scripts/docs.

Fresh leader verification on the integrated branch:

| Check | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | Rustfmt clean. |
| `cargo test -p discrypt-desktop --features tauri-runtime,production-network,production-media,production-storage --no-run` | PASS | Linux production-storage desktop test binaries compiled. |
| `cargo check --workspace --all-targets` | PASS | Workspace all-target check completed. |
| `npm --prefix apps/ui run typecheck` | PASS | `tsc --noEmit`. |
| `npm --prefix apps/ui run build` | PASS | Vite production build completed. |
| `npm --prefix apps/ui run test:command-coverage` | PASS | 44 Rust IPC commands mirrored by strict clients. |
| `npm --prefix apps/ui run test:honesty` | PASS | Static UI honesty gate passed. |
| `npm --prefix apps/ui run test:g011-boundary` | PASS | Re-run after matrix wording repair. |
| `npm --prefix apps/ui run test:g011-production-status-sync` | PASS | 13 duplicated production-status modules matched. |
| `npm --prefix apps/ui run test:release-linux` | PASS | Linux release dry-run requires production features. |
| `npm --prefix apps/ui run test:linux-runtime-docs` | PASS | Runtime dependency docs current. |
| `npm --prefix apps/ui run test:linux-package-smoke` | PASS | Linux package smoke dry-run passed. |
| `npm --prefix apps/ui run test:desktop-package-ci` | PASS | Desktop package CI contract passed. |
| `npm --prefix apps/ui run test:release-verification-matrix` | PASS | Release matrix passed. |
| `npm --prefix apps/ui run test:release-governance` | PASS | Release governance passed. |
| `npm --prefix apps/ui run test:security-privacy-g009` | PASS | Security/privacy no-shim gate passed. |
| `npm --prefix apps/ui run test:sbom-g124` | PASS | Rust, npm, and package SBOMs present. |
| `npm --prefix apps/ui run test:crypto-sensitive-g125` | PASS | Sensitive dependencies pinned by lockfiles. |
| `npm --prefix apps/ui run test:repro-g126` | PASS | Lockfiles, toolchains, package hashes, and SBOM hashes recorded. |
| `npm --prefix apps/ui run test:no-placeholders-g127` | PASS | 74 production source files scanned. |
| `npm --prefix apps/ui run test:placeholder-allowlist-g128` | PASS | 80 reviewed occurrence classes, 98 total occurrences. |
| `npm --prefix apps/ui run test:release-no-fallback-g129` | PASS | Release plan rejects harness/local-dev before packaging. |
| `npm --prefix apps/ui run test:ui-integration-g130` | PASS | UI integration gate passed. |
| `git diff --check` | PASS | No whitespace errors. |

Verification log: `target/g011-final-evidence/leader-g011-verify-20260601T170629Z.log`
contains the full command transcript; it includes the first `test:g011-boundary`
failure and the subsequent documented re-run after the production-gap matrix was
returned to the required `In progress / not closed` G011 boundary wording.

## G012 boundary reminder

G011 may claim production-readiness gates only after the leader's final evidence
is fresh and complete. G011 must not claim the G012 goal: two installed Tauri
users completing text and voice E2E. Current same-process, local-dev, browser,
public-provider, and command-layer harnesses remain evidence rows, not substitutes
for the separate G012 installed-app E2E proof.

# G011 production-readiness reconciliation matrix — 2026-06-01

## Scope and claim boundary

G011 is the final production-readiness gate before the separate G012 installed-app
E2E goal. This matrix reconciles the release documentation after G010 and the
current G011 worker lanes. It intentionally does **not** claim that two installed
Tauri users have completed text plus voice E2E; that remains G012.

A G011 completion claim is valid only when the rows below have fresh evidence on
the integrated leader branch and the final quality-gate evidence cites the exact
commands/artifacts retained for the release candidate.

## Required G011 rows

| Area | Required evidence before G011 can close | Current boundary |
| --- | --- | --- |
| Production UX, copy, and fallback honesty | Production UI/docs/no-placeholder/no-fallback/no-diagnostics gates pass, and unsupported paths are either disabled or explicitly non-default with honest recovery copy. | Must not treat browser fallback, harness controls, or diagnostics as production UX evidence. |
| Backend integration and persistence | Targeted `cargo`/`npm` command coverage proves backend command paths, two-profile persistence, signed receipt state, runtime pump boundaries, and privacy/security gates. | Same-process command and harness proofs support G011 only; they do not prove two installed GUI processes. |
| Linux packaging and dependency release gates | Linux package/build docs are current; release dry-run, package-smoke contracts, dependency/security audit gates, SBOM/reproducibility gates, and unsupported external-infra boundaries pass or are explicitly held. | External provider/TURN/package runners may be opt-in or runner-gated, but skips must be explicit and non-default. |
| Release verification matrix | `docs/release/release-verification-matrix.md` lists every required release gate and preserves local status boundaries for public-provider, package, privacy, and media rows. | Release rows must not promote G010 harnesses or local deterministic checks into G012 installed-app proof. |
| Final quality evidence | The leader-owned final G011 checkpoint cites the integrated branch diff, focused tests, cargo/npm checks, audit/doc gates, and code-review evidence. | Worker-local evidence is input to the leader gate; workers do not mutate `.omx/ultragoal` or close the Codex goal. |

## Explicit non-claims retained for G012

The following are intentionally out of scope for G011 and must remain visible in
release docs until G012 evidence exists:

- two separately installed Tauri app processes/devices completing group setup;
- text delivery both ways through the installed UI with retained logs/artifacts;
- real voice join, mute, speaking detection, per-peer volume, leave, and remote
audio/media cleanup across the installed apps;
- persistence reload of the installed-app E2E state after the text and voice run.

## Static reconciliation gates for this document

Worker-4 used the following repository-local static/doc gates to validate this
reconciliation slice:

- `npm --prefix apps/ui run test:release-verification-matrix`
- `npm --prefix apps/ui run test:release-governance`
- `npm --prefix apps/ui run test:linux-runtime-docs`
- `npm --prefix apps/ui run test:g010-adapter-public-matrix`
- `npm --prefix apps/ui run test:release-no-fallback-g129`

These gates validate release-matrix wiring, governance/secrets policy,
Linux-runtime documentation, G010 adapter/public matrix honesty, and release
no-fallback behavior. They do not replace the final integrated G011 quality gate
or the later G012 installed-app text-plus-voice E2E run.

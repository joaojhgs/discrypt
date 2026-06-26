# P12-T06 Public Nostr Split-Machine Matrix Plan

## Source And Scope
- Issue: PER-101 / P12-T06.
- Phase context: Phase 12 full E2E harness expansion. The named `production-release-master-plan-2026-06-10.md` is not present in this checkout; `docs/release/handoff-2026-06-10-current-state.md`, `docs/release/release-verification-matrix.md`, `docs/release/per99-g009-split-machine-app-flow-2026-06-26.md`, `.omx/plans/P3-T07-public-nostr-two-machine-text-e2e-2026-06-18.md`, and `.omc/plans/discrypt-plan.md` define the local evidence boundary.
- Primary paths: `apps/desktop/src-tauri/examples/g009_split_machine_app_flow.rs`, `docs/release/per101-public-nostr-split-machine-matrix-2026-06-26.md`, `scripts/check-p12-t06-public-nostr-split-machine-matrix.mjs`, and `docs/release/release-verification-matrix.md`.
- Scope: public Nostr app-flow matrix/reporting for local host plus SSH remote or an explicitly labeled substitute when no SSH target is configured.
- Non-scope: public MQTT matrix, three-member overlay relay, package install/reinstall, Phase 13 packaging, and production-ready release decisions.

## Invariants
- Invite parsing is not membership. Protected text evidence requires owner/staff approval plus persisted OpenMLS Welcome/add state.
- Nostr remains signaling/rendezvous only. Provider-visible material may include endpoint label, derived hashed rendezvous topic/tag, custom event kind, and sealed WebRTC negotiation envelopes; no application text/control/media payload may be relayed through Nostr.
- Delivery claims require direct WebRTC DataChannel or configured TURN-backed WebRTC route evidence. Missing route evidence must fail closed or be recorded as blocked.
- Presence claims require backend route-gated TTL evidence, not optimistic UI state.
- Voice evidence must be classified by what was actually observed and must not imply remote media transport unless remote route/media evidence exists.

## Implementation Steps
1. Add this plan as the OMC/OMX planning surface for PER-101 and steer Ultragoal with a G046 PER-101 story.
2. Add a release report that separates required split-machine promotion evidence from local substitute evidence and records the SSH blocker when no remote target is configured.
3. Add a deterministic static checker that verifies the PER-101 report, release matrix row, package script, and G009 artifact contract tokens needed for QA.
4. Run the G009 Nostr local-pair harness with `--features harness`, `--task-id PER-101`, and `--phase-task-id P12-T06` to produce owner/joiner/summary artifacts under `target/per101-public-nostr-split-machine-matrix/`.
5. Attempt or document the SSH/public Nostr promotion path. If no `DISCRYPT_G009_SSH_TARGET` or equivalent remote target exists, record that as the remaining blocker and avoid split-machine production claims.
6. Verify with formatting/static checks and targeted builds. Commit, push, open a draft PR, pin the PR metadata, and hand off to QA with one structured mention.

## Acceptance Criteria
- The PER-101 report names the branch, commit, evidence level, changed files, commands, artifacts, and production-vs-harness boundary.
- PER-101 artifacts identify `task_id=PER-101` and `phase_task_id=P12-T06`; the inherited G009 example may still default to PER-99/P12-T04 for prior release rows when no override is supplied.
- Local substitute artifacts show manual admission, OpenMLS admission after approval, protected text, route-gated presence, voice classification, and provider application relay disabled while using the Nostr adapter label.
- The SSH promotion section includes concrete owner/joiner commands and artifact expectations for local host plus SSH remote.
- The release matrix contains a P12-T06 row that cannot be confused with production-ready or package evidence.
- The static checker fails if the report overclaims production readiness or omits provider-signaling-only, OpenMLS/admission, route proof, provider-visible Nostr privacy, or artifact requirements.

## Verification
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --features harness --example g009_split_machine_app_flow`
- `XDG_DATA_HOME=/tmp/discrypt-per101-local-pair-xdg target/debug/examples/g009_split_machine_app_flow --role local-pair --artifact target/per101-public-nostr-split-machine-matrix/local-pair.json --adapter nostr --endpoint wss://nos.lol --admission-mode manual --timeout-secs 20 --task-id PER-101 --phase-task-id P12-T06`
- `npm --prefix apps/ui run test:p12-t06-public-nostr-split-machine-matrix`
- `git diff --check`

## Failure Modes
- SSH unavailable: publish local substitute evidence only, explicitly mark split-machine promotion blocked, and request QA/runner rerun on a configured remote host.
- Nostr endpoint unavailable during local/remote promotion: retain failure logs/artifacts and do not substitute provider application relay.
- OpenMLS admission missing: fail the local-pair artifact and keep invite parsing separate from membership.
- DataChannel route unavailable in default owner/joiner roles: fail closed with the current G009 route-precondition error rather than falling back to Nostr payload relay.

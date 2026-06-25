# P9-T03 Ordered MLS Delivery Layer Plan

## Requirements Summary
- Source task: PER-77 / P9-T03 from Phase 9 text/history semantics.
- Available source plan anchors: `.omc/plans/discrypt-plan.md` AC-MLS-FORK and D5; `.omc/specs/deep-interview-discrypt.md` v1.4 fork repair wording; `docs/release/handoff-2026-06-10-current-state.md` release-evidence boundary. The issue-referenced `.omx/plans/production-release-master-plan-2026-06-10.md` is not present in this checkout.
- Relevant code paths: `crates/mls-delivery/src/lib.rs` owns `EpochSummary`, `ForkStatus`, `ApplicationEvent`, `CommitEnvelope`, `DeliveryState`, and repair planning; `crates/mls-core/src/governance.rs` already defines the same epoch -> leaf -> signed content hash governance comparator.
- Invariants: ordered delivery is a local MLS service-layer model, not replacement cryptography; divergent MLS commits are never replayed; only still-valid application events may be re-proposed after repair; providers remain signaling/rendezvous only and are not part of this delivery proof.

## Acceptance Criteria
- Commits expose and use a total canonical comparator: epoch -> committer leaf -> signed/content hash.
- Application events continue to use the same epoch -> author leaf -> content hash ordering.
- Forked same-epoch histories are detected and deterministically recover to the comparator-maximal valid history.
- Recovery converges honest participants within the local AC-MLS-FORK harness without replaying divergent MLS commits.
- Downgrade, replay, skipped-epoch, stale-event, stale-repair, and explicit divergent-commit replay paths fail without mutating accepted state.

## Implementation Steps
1. Add `CommitOrderKey` and commit/history ordering helpers to `crates/mls-delivery/src/lib.rs`.
2. Use ordered commit bundles and repair winner selection based on comparator-maximal history instead of raw epoch-summary ordering.
3. Add focused Rust tests for commit ordering and an AC-MLS-FORK adversarial harness.
4. Run targeted MLS-delivery tests, formatting, clippy when feasible, and diff checks.

## Failure Modes And Safety
- Same-epoch tree/confirmation mismatch returns explicit fork evidence and does not silently accept the remote state.
- Skipped epochs remain rejected as downgrade/replay because the local model cannot prove ordered catch-up continuity.
- Repair rejects plans that replay divergent MLS commits or target an older epoch.
- Re-proposed app events are filtered to the repaired winner epoch and ordered before acceptance.

## Verification
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-mls-delivery ordered_mls_delivery -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-mls-delivery ac_mls_fork -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-mls-delivery -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-mls-delivery --lib -- -D warnings`
- `git diff --check`

## Evidence Classification
This is local Rust MLS-delivery model/unit evidence for ordering and fork recovery. It is not production split-machine delivery, full OpenMLS runtime integration, retention/shred, multi-device recovery, UI, packaging, or release-matrix evidence.

# P9-T02 History Sync Over Route Graph/Overlay Plan

## Requirements Summary
- Source task: PER-76 / P9-T02 from Phase 9 text/history semantics.
- Available source plan anchors: `.omc/plans/discrypt-plan.md` AC4/AC5/AC9 and local `.omx/plans/P9-T01-author-log-model-2026-06-25.md`; the issue-referenced `.omx/plans/production-release-master-plan-2026-06-10.md` is not present in this checkout.
- Relevant code paths: `crates/storage/src/lib.rs` owns append-only `AuthorLogEntry` and `LocalStore::merge_author_logs`; `crates/transport/src/peer_overlay.rs` owns direct/TURN/peer-overlay route evidence and provider-relay rejection.
- Invariants: only backend/OpenMLS-supplied current-epoch membership policy authorizes sync; provider application relay is never a history path; synced payloads remain ciphertext bytes; storage merge rejects forks and reused message ids.

## Acceptance Criteria
- An authorized returning/new member can receive ciphertext author-log history within retention/current-epoch policy and merge it into local storage.
- Offline/reconnect behavior is represented by a bounded pending queue that revalidates authorization and policy before applying queued history.
- Direct WebRTC, configured TURN-backed WebRTC, and peer-assisted overlay route selections are accepted as route evidence.
- Provider application relay attempts fail closed even if they appear only in route-attempt evidence.
- Unauthorized recipient, unauthorized author leaf, future epoch, retention overrun, empty ciphertext, and storage merge conflicts fail without mutating the recipient store.

## Implementation Steps
1. Add `crates/transport/src/history_sync.rs` with policy, item, plan, queue, and apply APIs that compose `PeerOverlayRouteSelection` with `discrypt_storage::LocalStore`.
2. Export the history-sync API from `crates/transport/src/lib.rs` and add the `discrypt-storage` crate dependency.
3. Add focused Rust tests covering direct route offline/reconnect, peer-assisted overlay evidence, provider relay rejection, policy rejection, and idempotent storage merge.
4. Run targeted transport/storage verification, formatting, and diff checks.

## Failure Modes And Safety
- Membership failures return `TransportError::InvalidConnectivityPolicy` before queueing or store mutation.
- Future-epoch entries are rejected because the local model cannot prove OpenMLS authorization for them.
- Retention is enforced against caller-supplied deterministic timestamps; this is policy evidence, not a full Phase 9 retention/shred implementation.
- Storage merge remains the last gate, so sequence forks and message-id reuse are rejected by the existing append-only model.

## Verification
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport history_sync -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage author_log -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

## Evidence Classification
This is local Rust transport+storage model/unit evidence for authorized ciphertext history sync and offline/reconnect queue behavior. It is not production split-machine delivery, ordered MLS delivery, full retention/shred, multi-device identity, backup/recovery, UI, packaging, or release-matrix evidence.

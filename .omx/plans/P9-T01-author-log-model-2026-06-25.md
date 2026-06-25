# P9-T01 Author Log Model Plan

## Requirements Summary
- Source task: PER-75 / P9-T01 from Phase 9 text/history semantics.
- Available source plan anchor: `.omc/plans/discrypt-plan.md` describes `storage/author_log.rs` as an append-only per-author log with multi-device merge, stable message ids, and ciphertext-only text/history storage. The issue-referenced `.omx/plans/production-release-master-plan-2026-06-10.md` is not present in this checkout.
- Relevant code paths: `crates/storage/src/lib.rs` owns `AuthorLogEntry`, `AuthorLogKey`, and `LocalStore`; `crates/core/src/services.rs` already exposes `MessageId` and text history page boundary types but has no author-log implementation.
- Invariants: no provider application relay fallback, no plaintext history persistence, no UI/backend truth shortcuts, and no silent overwrite of historical ciphertext.

## Acceptance Criteria
- A local author log accepts append-only entries keyed by author/device/sequence.
- Multi-device merge is deterministic and idempotent across own-device snapshots.
- Stable message ids can be derived from canonical entry fields and remain unchanged across device merges.
- Conflicting entries for the same author/device/sequence are rejected instead of overwriting existing history.
- Reuse of one message id for a different log position is rejected.
- Unit tests cover stable IDs, multi-device merge, idempotent duplicate merge, and conflict rejection.

## Implementation Steps
1. Extend `crates/storage/src/lib.rs` author-log types:
   - key by `author_leaf`, `device_id`, and `sequence`;
   - add stable message-id derivation helper;
   - add typed append/merge outcomes and errors.
2. Update `LocalStore` append and merge behavior:
   - insert new entries;
   - treat identical duplicates as idempotent;
   - reject forks/reused IDs without mutating existing entries.
3. Add focused unit coverage in `crates/storage/src/lib.rs`.
4. Run targeted storage tests, formatting, and diff checks.

## Risks And Mitigations
- Risk: existing harness code assumes `append_sent` is infallible. Mitigation: update direct call sites to handle the new result and keep behavior explicit.
- Risk: stable ID derivation accidentally includes arrival/order data. Mitigation: derive only from domain, author leaf, device id, sequence, epoch, and ciphertext hash.
- Risk: merge conflict handling overwrites prior data. Mitigation: check conflicts before insert and assert existing snapshots are unchanged in tests.

## Verification
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage author_log -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-storage local_store -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

## Evidence Classification
This is local Rust model/unit evidence for the author-log storage model. It is not production split-machine text/history, retention, shred, store-forward, or UI evidence.

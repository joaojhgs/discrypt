# Phase 3 text/history + MLS delivery review

G004 implements deterministic foundations for the approved Phase 3 scope without
claiming production OpenMLS networking. The new code models the service layer that
must surround MLS: ordering, Welcome/catch-up, fork detection, explicit repair,
per-author history convergence, and bounded recipient caches.

## Implementation map

- `crates/storage/src/lib.rs`
  - `AuthorLogEntry` and `AuthorLogKey` model authoritative per-author sent logs.
  - `LocalStore::merge_author_logs` deterministically merges multi-device author logs.
  - `BoundedRecipientCache` retains received ciphertext plus eligible key state under
    a fixed capacity.
- `crates/relay-overlay/src/gossip.rs`
  - `GossipItem` carries content-blind author-log metadata plus ciphertext hashes.
  - `GossipMesh` provides deterministic 12-16 member convergence tests.
- `crates/mls-delivery/src/lib.rs`
  - `ApplicationEvent` + `CanonicalEventKey` implement the plan comparator:
    epoch -> leaf index -> content hash.
  - `DeliveryState` accepts only forward commits and rejects downgrade/replay/forks.
  - `WelcomePackage` and `CatchUpBundle` cover expiring admission and ordered catch-up.
  - `ForkStatus`, `ForkEvidence`, and `RepairPlan` ensure divergence is detected and
    repaired by rejoin/reproposal; divergent MLS commits are explicitly not replayed.
- `harness/multinode/src/lib.rs`
  - `text_history_delivery_smoke` covers author-log merge, bounded recipient cache,
    16-peer gossip convergence, ordered delivery, Welcome/catch-up, same-epoch fork
    rejection, and repair convergence with equal confirmation tags.

## Acceptance coverage

- AC4: author-log authority and bounded recipient cache are covered by storage unit
  tests and the multinode smoke.
- AC5: 16-peer gossip convergence is covered by `GossipMesh` tests and the multinode
  smoke; delivery ordering is covered by `DeliveryState` and canonical event tests.
- AC9: Phase 2 store-forward remains ciphertext-only/TTL-bounded; G004 verifies that
  received history caches are bounded and do not replace author logs.
- AC-MLS-FORK: same-epoch divergence is detected, a forked commit is rejected, repair
  uses rejoin/reproposal, and all repaired summaries share the same confirmation tag.

## Production-hardening notes

- `DeliveryState` is a deterministic facade; replacing commit internals with real
  OpenMLS messages must preserve the same reject/repair semantics.
- `GossipMesh` is a harness flood model, not a production peer-selection algorithm.
  Production gossip should reuse Phase 2 ranking/topology while keeping ciphertext-only
  `GossipItem` boundaries.
- Repair re-proposes only application events already validated by the caller; Phase 5
  governance must revalidate authority under the repaired winner epoch before accepting
  governance events.

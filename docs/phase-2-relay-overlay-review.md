# Phase 2 relay overlay review

This review documents the current G003 relay-overlay slice against the approved
Phase 2 plan in `.omx/plans/discrypt-plan.md`. It is intentionally scoped to
repo-visible code and docs; `.omx/ultragoal` remains leader-owned and was not
mutated.

## Current implementation map

| Phase 2 concern | Current code | Review status |
| --- | --- | --- |
| Relay ranking | `crates/relay-overlay/src/lib.rs` ranks `RelayMetrics` by latency, stability, battery cost, and freeload penalty. | Foundation present. The score is deterministic and cheap, but still a heuristic without hysteresis, capacity, or topology state. |
| Hop limit | `crates/relay-overlay/src/lib.rs::hop_limit_ok` enforces `<= 3`. | Foundation present. Callers still need route construction tests proving every generated path respects the limit. |
| Ciphertext-only relay behavior | `crates/relay-overlay/src/integrity.rs` forwards opaque bytes and only inspects for plaintext in tests; `crates/media/src/sframe.rs` owns authentication and replay checks. | Good boundary. Relays remain content-blind and receiver-owned SFrame state rejects tamper/replay. |
| Media harness integration | `harness/multinode::media_security_smoke` protects a media frame, forwards relay-visible ciphertext, rejects replay, and rejects active tamper. | Deterministic smoke present. It re-validates Phase 1 media security through relay packet plumbing. |
| Failover/redelivery | No `failover.rs` or `redelivery.rs` module yet. | Pending. AC7 still needs route convergence, per-packet sequence/retransmit, duplicate/stale rejection at overlay level, and lossy-network tests. |
| Store-and-forward TTL/fanout | No `store_forward.rs` module yet. | Pending/future Phase 3-adjacent foundation. AC9 still needs ciphertext-only queues, membership gates, TTL expiry, fanout bounds, and retention-window interaction tests. |
| Topology construction | No `topology.rs` module yet. | Pending. AC6 still needs capacity-aware tree construction, hop-depth checks, churn damping, and instrumentation for p50/p95 hop latency. |

## Code-quality notes

- The relay crate currently keeps routing/ranking foundations small and
  transport-agnostic, which matches the plan's adaptive ALM boundary.
- `RelayPacket` is intentionally ciphertext-only; it should not grow plaintext,
  key, MLS, or media-decoding fields. Keep route metadata separate from protected
  media bytes.
- Ranking should remain deterministic for the harness. If randomized tie-breaks
  become necessary, inject deterministic seeds into tests rather than using
  process-global randomness.
- `contains_plaintext` is a smoke-test helper, not a proof of content security.
  The acceptance claim should continue to rely on AEAD authentication,
  per-sender/per-device binding, and receiver anti-replay.
- Store-and-forward must not override retention or crypto-shred semantics: queued
  ciphertext that outlives the author's current effective window should lock on
  delivery rather than decrypting from a stale cached key.
- `RelayMetrics` inputs should be range-checked before production routing uses
  them. NaN or infinite metric values must not degrade ranking into
  input-order-dependent ties.
- The current harness relays ciphertext byte slices, not whole protected-frame
  records. Phase 2 overlay tests should carry KID/counter/AAD metadata with the
  ciphertext so sender binding and replay windows are exercised end-to-end.

## Phase 2 acceptance gaps to close before claiming full G003

1. **Topology/ranking/failover:** add capacity-aware route construction, enforce
   tree depth `<= 3` for generated routes, damp re-parenting to avoid thrash, and
   prove relay kill convergence within the plan's `<= 3 s` target.
2. **Redelivery/replay:** add per-packet sequence state and retransmit decisions
   that can recover dropped packets without accepting duplicate or stale packets.
3. **Ciphertext-only integration:** carry complete protected media frames through
   relay routes, not just ciphertext byte slices, so KID/counter/AAD binding stays
   intact end-to-end.
4. **Store-forward foundations:** add content-blind queue records with TTL,
   fanout limits, membership eligibility, and explicit retention-window checks.
5. **Deterministic harness:** extend `harness/multinode` with fixed topology,
   relay-failure, replay/drop/tamper, and TTL-expiry scenarios that can run in CI
   without network timing flakiness.

## Verification commands

Run these before reporting Phase 2 relay changes:

```sh
cargo fmt --check
cargo check --workspace
cargo test -p discrypt-relay-overlay -p discrypt-multinode-harness
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

For this review pass, the targeted evidence is the relay-overlay crate tests plus
multinode harness media-security smoke. Full Phase 2 completion should also add
lossy/failover route tests once the missing modules exist.

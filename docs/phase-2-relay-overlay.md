# Phase 2 Relay Overlay Verification

Phase 2 keeps relays content-blind while adding deterministic overlay policy that
can be tested without UI or network nondeterminism.

Implemented slices:

- `crates/relay-overlay/src/ranking.rs` ranks relay candidates by latency,
  stability, energy cost, and freeload penalty with deterministic tie-breaking.
- `crates/relay-overlay/src/topology.rs` builds bounded-fanout topologies and
  selects routes that must stay within the ≤3 hop cap.
- `crates/relay-overlay/src/failover.rs` reroutes around a failed relay and
  records whether convergence satisfies the ≤3 second Phase 2 gate.
- `crates/relay-overlay/src/redelivery.rs` tracks packet ids, rejects duplicate
  or stale replay, and caps redelivery fanout.
- `crates/relay-overlay/src/store_forward.rs` stores only ciphertext envelopes
  with TTL and fanout limits for opportunistic delivery foundations.
- `crates/relay-overlay/src/integrity.rs` remains the content-blind byte boundary:
  relays can forward or tamper with bytes, but media authentication and replay
  checks stay receiver-owned.

The deterministic harness bridges the overlay to Phase 1 media frames in
`harness/multinode::relay_overlay_smoke`. It verifies:

1. route selection respects the ≤3 hop cap;
2. failover avoids the failed relay within the Phase 2 convergence budget;
3. redelivery bookkeeping rejects replay;
4. store-forward TTL prevents expired ciphertext delivery;
5. relays see ciphertext-only media bytes; and
6. active relay tampering is rejected by SFrame-like authentication.

Run the focused checks:

```bash
cargo test -p discrypt-relay-overlay
cargo test -p discrypt-multinode-harness
cargo run -p discrypt-multinode-harness --quiet
```

Full verification before checkpointing this phase should additionally include
workspace formatting, clippy, tests, and the UI type/build checks used by prior
goals.

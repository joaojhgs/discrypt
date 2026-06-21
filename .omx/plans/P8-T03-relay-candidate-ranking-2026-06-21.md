# P8-T03 - Relay Candidate Ranking

Issue: PER-68 / P8-T03

## Requirements Summary

Source context: Phase 8 relay overlay work follows PER-66 protocol skeleton and
PER-67 relay authorization in `crates/transport/src/peer_overlay.rs`. The issue
metadata narrows this task to deterministic relay candidate ranking only.

Acceptance:
- Rank only admitted current-epoch peers with explicit relay authority.
- Reject revoked, stale-epoch, non-member, missing-authority, unhealthy, and
  policy-disallowed candidates fail-closed.
- Ranking uses latency, stability, capacity, energy cost, and freeload penalty.
- Ties are deterministic.
- Providers remain signaling/rendezvous only; no forwarding runtime, overlay
  route selection, UI, voice fanout, or release-gate proof is added.

## Implementation Steps

1. Extend `crates/transport/src/peer_overlay.rs` with local candidate diagnostic,
   ranking policy, and ranked-candidate types.
2. Add `rank_relay_candidates(...)` that validates candidate peer refs against
   `PeerOverlayAdmittedSet`, requires `PeerOverlayRelayAuthoritySet` evidence,
   rejects provider application relay policy, rejects unhealthy or over-capacity
   candidates, then sorts by deterministic integer score and peer identity.
3. Export the ranking types/functions from `crates/transport/src/lib.rs`.
4. Update `docs/phase-8-relay-protocol-spec.md` to mark candidate ranking as
   local model evidence and keep runtime route selection out of scope.
5. Add focused Rust tests under the existing `peer_overlay` test module.

## Failure Modes And Safety

- Route graph observations and invite-derived peer refs are not ranking inputs
  unless backed by current admitted-set and relay-authority evidence.
- Invalid candidates fail the whole ranking call instead of being silently
  ignored, so stale/revoked peers cannot be hidden by a shorter candidate list.
- Ranking does not inspect payload bytes or provider adapter state.
- Provider application relay is explicitly rejected by policy validation.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local Rust model/unit evidence only. This is not
runtime forwarding, route selection, split-machine overlay delivery, or
production overlay route evidence.

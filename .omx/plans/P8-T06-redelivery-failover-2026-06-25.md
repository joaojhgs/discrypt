# P8-T06 - Redelivery Failover

Issue: PER-71 / P8-T06

## Requirements Summary

Source context: PER-71 follows the Phase 8 local overlay model in
`docs/phase-8-relay-protocol-spec.md` and `crates/transport/src/peer_overlay.rs`.
The checkout does not contain the named 2026-06-10 master plan file, so the
active constraints are the issue body/metadata, `.omc/plans/discrypt-plan.md`,
the existing Phase 7/8 plan artifacts, and the current local transport model.

Acceptance:
- Killed relay evidence triggers failover route selection within the caller's
  target window.
- Redelivery preserves the protected test frame's ack id, loop id, sequence,
  payload class, and ciphertext/commitment evidence so the frame is not
  permanently lost in the local harness model.
- Stale replay attempts are rejected before route selection or forwarding.
- Provider application relay remains forbidden for every redelivery path.
- Scope is local Rust transport model/unit evidence only; no UI, store-forward,
  voice fanout, live split-machine runtime, packaging, or release-gate proof.

## Implementation Steps

1. Extend `crates/transport/src/peer_overlay.rs` with redelivery/failover input,
   replay-attempt evidence, killed-relay evidence, selected redelivery route,
   and result evidence types.
2. Implement `plan_peer_overlay_redelivery_failover(...)` to validate the
   frame, admission, relay authority, ack-required redelivery policy, deadline,
   replay window, and killed-relay target window before selecting a replacement
   direct/TURN/peer-assisted route.
3. Exclude killed relays from ranked candidates and relay route evidence, then
   reuse existing route-selection and forwarding-plan validation for
   peer-assisted replacement routes.
4. Export the new API from `crates/transport/src/lib.rs`.
5. Update `docs/phase-8-relay-protocol-spec.md` with the local model evidence
   and production-proof caveat.
6. Add focused Rust tests for 3-member forced-relay failover, alternate relay
   reroute with forwarding evidence, stale replay rejection, and provider
   boundary preservation.

## Failure Modes And Safety

- Redelivery never uses MQTT, Nostr, IPFS PubSub, or Discrypt QUIC rendezvous
  as application relay fallback.
- A killed relay is not silently retried while marked failed; the planner
  removes failed relay ids before route selection.
- An already acknowledged or non-increasing attempt for the same ack/loop
  sequence is rejected as stale replay.
- Missing route evidence returns `NoViablePath` instead of claiming success.
- Direct/TURN fallback evidence is route-selection evidence only; peer-assisted
  replacement routes additionally build opaque forwarding hops without decrypt
  or key access.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local Rust transport model/unit evidence only. This is
not production split-machine overlay delivery or live relay-runtime proof.

# P8-T04 - Overlay Route Selection

Issue: PER-69 / P8-T04

## Requirements Summary

Source context: PER-69 follows PER-66 relay protocol spec, PER-67 relay
authorization, PER-68 relay candidate ranking, and the Phase 7 route graph
model. The checkout does not contain the named 2026-06-10 master plan files, so
the active constraints are the issue body/metadata, `.omc/plans/discrypt-plan.md`,
`docs/phase-8-relay-protocol-spec.md`, `docs/release/handoff-2026-06-10-current-state.md`,
and the local Phase 7/8 plan artifacts.

Acceptance:
- Direct WebRTC pair evidence is preferred when live.
- Explicit policy controls whether configured TURN is attempted before or after
  peer-assisted relay once direct fails.
- Peer-assisted relay is selected only from already ranked candidates that are
  admitted, current-epoch, explicitly authorized, provider-safe, and backed by
  two live non-provider route legs: source-to-relay and relay-to-destination.
- Stale, revoked, non-member, unhealthy, missing-authority, provider-relay, and
  missing-leg candidates fail closed.
- This task does not implement forwarding runtime, redelivery/failover, overlay
  UI truth claims, voice fanout, packaging, or release-gate proof.

## Implementation Steps

1. Add route-selection model types to `crates/transport/src/peer_overlay.rs`
   near the existing candidate ranking types.
2. Implement a planner that validates admitted source/destination/auth,
   preserves direct preference, honors configured-TURN vs relay ordering, and
   rejects provider application relay as route evidence.
3. Require selected relay evidence to use the top-ranked relay candidate and to
   include exactly the two live legs needed for source-to-relay and
   relay-to-destination forwarding.
4. Export the selector types from `crates/transport/src/lib.rs`.
5. Update `docs/phase-8-relay-protocol-spec.md` to mark route selection as
   local model evidence only.
6. Add focused Rust tests for direct preference, TURN/relay ordering, safe relay
   selection, missing-leg failure, and provider-relay rejection.

## Failure Modes And Safety

- Route selection never uses MQTT, Nostr, IPFS PubSub, or QUIC rendezvous as
  application relay evidence.
- Relay selection does not inspect payload bytes and does not expose any decrypt
  path.
- Missing or non-live route legs fail closed instead of silently selecting a
  weaker candidate.
- Existing `ConnectivityPlanner`, `RouteReport`, `RouteIntent`, and
  `TransportSession` direct/TURN behavior remain unchanged so no UI/session
  surface can claim overlay delivery from this local model alone.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local Rust model/unit evidence only. This is not
runtime forwarding, split-machine overlay delivery, voice fanout, or production
route evidence.

# P8-T02 - Relay Authorization

Issue: PER-67 / P8-T02

## Requirements Summary

Source context: PER-67 requires Phase 8 relay authorization after the PER-66
peer overlay protocol skeleton. The checkout still does not contain the named
master plan files from runtime context, so the active constraints are the issue
body/metadata, `.omc/plans/discrypt-plan.md`, `docs/phase-8-relay-protocol-spec.md`,
`docs/release/release-verification-matrix.md`, and the Phase 7 route graph
artifacts.

Acceptance:
- Only admitted current-epoch peers can be authorized as overlay relays.
- Revoked, stale-epoch, non-member, and route-graph-only peers fail closed.
- Relay authority is explicitly bound to current OpenMLS/backend state or to
  already-verified signed governance evidence.
- Providers remain signaling/rendezvous only.
- Do not implement candidate ranking, route selection, forwarding runtime,
  voice fanout, UI, or release gates.

## Implementation Steps

1. Extend `crates/transport/src/peer_overlay.rs` with an explicit relay
   authorization set/proof type. The type will validate group commitment,
   current epoch, confirmation-tag commitment, and each relay peer against
   `PeerOverlayAdmittedSet`.
2. Add frame-level validation that first performs the existing PER-66 frame
   checks, then requires every relay hop to have explicit authority for the same
   group/epoch/auth commitment.
3. Export the new authorization types from `crates/transport/src/lib.rs`.
4. Update `docs/phase-8-relay-protocol-spec.md` to mark relay authorization as
   implemented as a local model boundary while keeping runtime forwarding and
   route selection out of scope.
5. Add focused Rust tests for admitted current-epoch authorization, revoked
   relay rejection, stale epoch rejection, non-member/missing explicit authority
   rejection, and provider-as-signaling-only preservation.

## Failure Modes And Safety

- Invite parsing and stale route graph edges are not relay authority; callers
  must supply a backend/OpenMLS admitted set or already-verified signed
  governance grant.
- A relay authorization set cannot include revoked or non-admitted peers because
  every relay ref is validated against `PeerOverlayAdmittedSet`.
- A frame cannot pass relay-authorized validation when auth epoch, group
  commitment, confirmation commitment, route relay refs, or explicit relay
  authority disagree.
- Existing direct/TURN route behavior remains unchanged; this is a model/local
  authorization boundary only.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local Rust model/unit evidence only. This is not
runtime forwarding, candidate selection, split-machine overlay delivery, or
production route evidence.

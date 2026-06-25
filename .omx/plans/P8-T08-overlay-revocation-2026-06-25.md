# P8-T08 - Overlay Revocation

Issue: PER-73 / P8-T08

## Requirements Summary

Source context: PER-73 follows the local Phase 8 overlay model in
`docs/phase-8-relay-protocol-spec.md`, `crates/transport/src/peer_overlay.rs`,
and the prior P8 relay authorization, route selection, opaque forwarding, and
redelivery/failover plans. The named 2026-06-10 master plan and peer-overlay
handoff files are absent from this checkout; the active constraints are the
issue body/metadata, `.omc/plans/discrypt-plan.md`,
`docs/release/handoff-2026-06-10-current-state.md`, and local Phase 7/8 plan
artifacts.

Acceptance:
- A removed member cannot be source, destination, or relay for current-epoch
  peer-overlay frames after a revocation epoch transition.
- Stale pre-revocation overlay frames are rejected against post-revocation
  admitted state.
- Relay authority remains explicitly bound to the post-revocation epoch,
  group commitment, and confirmation-tag commitment.
- Relay forwarding remains opaque and provider application relay remains
  forbidden.
- Scope is local Rust transport model/unit evidence; no UI, store-forward,
  voice fanout, split-machine runtime, packaging, or release gate work.

## Implementation Steps

1. Add revocation evidence/state types to `crates/transport/src/peer_overlay.rs`
   that build post-revocation `PeerOverlayAdmittedSet` and
   `PeerOverlayRelayAuthoritySet` from current backend/OpenMLS/governance
   evidence.
2. Require revocation evidence to advance the epoch beyond every revoked ref and
   to exclude revoked member/device bindings from the remaining admitted set and
   relay-authority list.
3. Add a post-revocation validation method that rejects stale or revoked overlay
   frames before forwarding or route reuse.
4. Export the revocation API from `crates/transport/src/lib.rs`.
5. Update `docs/phase-8-relay-protocol-spec.md` with the implemented local
   revocation boundary and production-evidence caveat.
6. Add focused Rust tests for revoked send/receive/relay rejection, stale
   pre-revocation frame rejection, remaining-member forwarding, and malformed
   revocation evidence failure.

## Failure Modes And Safety

- A revoked member/device binding cannot re-enter the post-revocation admitted
  set under the same member/device identity.
- A stale frame fails because its route refs and auth epoch do not match the
  current admitted epoch.
- Relay authority cannot include revoked peers because authority construction
  validates against the post-revocation admitted set.
- Providers remain signaling/rendezvous only; the new path does not add any
  provider application relay fallback or decrypt capability.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-transport --lib -- -D warnings`
- `git diff --check`

Evidence classification: local Rust transport model/unit evidence only. This is
not production split-machine overlay delivery or live OpenMLS runtime proof.

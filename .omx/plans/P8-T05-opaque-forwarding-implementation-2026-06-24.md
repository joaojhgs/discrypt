# P8-T05 - Opaque Forwarding Implementation

Issue: PER-70 / P8-T05

## Requirements Summary

Source context: PER-70 requires Phase 8 peer-assisted opaque forwarding after
the PER-66 protocol skeleton, PER-67 relay authorization, PER-68 candidate
ranking, PER-69 route selection, and Phase 7 route graph work. The named
`.omx/plans/production-release-master-plan-2026-06-10.md` is not present in
this checkout; the active constraints are the issue body/metadata,
`.omc/plans/discrypt-plan.md`, `docs/release/handoff-2026-06-10-current-state.md`,
`docs/phase-8-relay-protocol-spec.md`, and local Phase 7/8 plan artifacts.

Acceptance:
- Relay forwards encrypted text/control and media frames without any decrypt
  path.
- Forwarding requires admitted current-epoch source/destination/relay refs and
  explicit relay authority for every relay hop.
- MQTT, Nostr, IPFS PubSub, and Discrypt QUIC rendezvous remain
  signaling/rendezvous only; provider application relay paths fail closed.
- Forbidden relay-visible plaintext/key markers fail before hop envelopes are
  emitted.
- This task does not implement split-machine runtime sockets, store-forward
  queues, redelivery convergence, revocation propagation, voice fanout, UI, or
  production release proof.

## Implementation Steps

1. Extend `crates/transport/src/peer_overlay.rs` with an opaque forwarding
   policy, forwarded-hop evidence, forwarding-plan evidence, and
   `build_peer_overlay_forwarding_plan`.
2. Validate frame schema, `PeerAssistedOverlay` carrier, admitted current epoch,
   explicit relay authority, supported payload class, TTL sufficiency, and
   forbidden relay-visible markers before emitting hop envelopes.
3. Export the forwarding API from `crates/transport/src/lib.rs`.
4. Update `docs/phase-8-relay-protocol-spec.md` to mark opaque forwarding as
   local model evidence while preserving production-proof caveats.
5. Add focused Rust tests for encrypted text/control forwarding, encrypted media
   forwarding, forbidden plaintext marker rejection, provider application relay
   rejection, and stale/unauthorized/TTL failure.

## Failure Modes And Safety

- Invite parsing, route graph membership, or provider signaling does not
  authorize forwarding; the caller must supply current backend/OpenMLS admitted
  state and explicit relay authority.
- Store-forward payloads are rejected until that later storage-specific relay
  queue exists.
- Forbidden plaintext marker checks are audit/test gates only; they do not
  decrypt. Upper layers still own MLS/SFrame authentication, replay protection,
  and decrypt.
- No existing direct/TURN WebRTC planner behavior is changed by this local
  forwarding model.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local Rust transport model/unit evidence only. This
is not split-machine overlay delivery, live revocation propagation, redelivery
convergence, voice fanout, UI truth, or production route evidence.

# P8-T01 - Relay Protocol Spec

Issue: PER-66 / P8-T01

## Requirements Summary

Source context: PER-66 requires the Phase 8 adaptive encrypted peer-assisted
relay protocol spec. The checkout does not contain
`.omx/plans/production-release-master-plan-2026-06-10.md`,
`.omx/plans/admin-role-admission-plan-2026-06-04.md`, or
`.omx/plans/peer-overlay-group-transport-plan-2026-06-05.md`; the issue body,
metadata, `docs/release/handoff-2026-06-10-current-state.md`,
`docs/release/release-verification-matrix.md`, `.omc/plans/discrypt-plan.md`,
Phase 7 route plans, and existing `docs/phase-2-relay-overlay*.md` are the
active constraints.

Acceptance:
- Define opaque relay frame format, source/relay/destination refs, epoch/auth,
  TTL, loop id, ack/redelivery, and revocation behavior.
- Keep public providers signaling/rendezvous only; no provider application
  relay fallback.
- Anchor the spec with minimal transport types and tests where useful.
- Do not implement relay authorization, candidate ranking, route selection,
  forwarding runtime, voice/media expansion, packaging, or release gates.

## Implementation Steps

1. Add `docs/phase-8-relay-protocol-spec.md` with the frame layout, actor refs,
   epoch/auth binding, TTL/loop behavior, ack/redelivery contract, revocation
   behavior, provider boundary, and explicit out-of-scope runtime work.
2. Add `crates/transport/src/peer_overlay.rs` as a data-only protocol contract
   with validation for current-epoch admitted peers, revoked peer rejection,
   opaque payloads, TTL, loop id/path uniqueness, and ack/redelivery bounds.
3. Export the module from `crates/transport/src/lib.rs` without wiring it into
   current route selection or runtime attach behavior.
4. Add focused unit tests proving provider relay is forbidden, revoked or
   stale-epoch peers cannot be named, relay-visible bytes stay opaque, duplicate
   loop paths are rejected, and ack/redelivery policies validate.

## Failure Modes And Safety

- Invite parsing is not membership. The frame validator requires an admitted
  current-epoch peer set supplied by higher-level OpenMLS/backend state.
- Revoked, stale-epoch, duplicate, source-as-relay, destination-as-relay, and
  local-loop route refs fail closed before any future relay runtime can use
  them.
- Relays receive only opaque protected payload bytes plus routing/auth metadata;
  no decrypt or key access API is exposed.
- Providers remain signaling-only. The protocol explicitly rejects
  provider-application-relay as a carrier and does not mutate existing Phase 7
  direct/TURN WebRTC route selection.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport peer_overlay -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: documentation and local Rust model/unit evidence only.
This is not runtime relay authorization, route selection, forwarding,
split-machine, voice/media, or production route evidence.

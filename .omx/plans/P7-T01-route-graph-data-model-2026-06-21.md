# P7-T01 - Route Graph Data Model

Issue: PER-60 / P7-T01

## Requirements Summary

Source context: PER-60 requires Phase 7 group route graph data modeling. The local checkout does not contain `.omx/plans/production-release-master-plan-2026-06-10.md`; the issue body, metadata, `docs/release/handoff-2026-06-10-current-state.md`, adjacent P3/P6 transport plans, and `.omc/plans/discrypt-plan.md` are the authority.

Acceptance: per group/channel, a local admitted member has route intents and edges for every admitted remote peer.

Scope:
- Add a transport-only route graph model under `crates/transport`.
- Keep provider adapters as signaling/rendezvous only.
- Do not implement runtime maps, per-peer attach, fanout, diagnostics export, overlay relay, voice, packaging, UI, or release-gate expansion.

## Implementation Steps

1. Add a pure `route_graph` module using committed `ConversationScope` values for group/channel scope and `SignalingPeerId` for admitted peer identities.
2. Model per-remote edges from one local admitted peer to each admitted remote peer.
3. Model route intents for direct WebRTC, configured TURN-backed WebRTC, pending, and unavailable states without provider app-relay alternatives.
4. Add deterministic validation for group/channel scoping, local-vs-remote identity, duplicate admitted peers, and route evidence consistency.
5. Export the model from `crates/transport/src/lib.rs`.
6. Add unit tests for 3, 8, and 16 member groups plus two-person JSON compatibility.

## Failure Modes And Safety

- Invite parsing is not represented as admission; callers must supply admitted peer ids only.
- Pending/unavailable intents are non-connected model states and cannot imply joined, connected, delivered, or voice-active UI truth.
- Direct and TURN intents must remain WebRTC route intents. TURN is accepted only as configured TURN-backed WebRTC carrying encrypted application traffic, never provider application relay.
- Providers remain absent from edge payload routing. Provider-visible signaling remains limited to existing sealed negotiation/rendezvous paths outside this model.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport route_graph -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local Rust model/unit evidence only. This is not runtime multi-peer attach, public-provider, split-machine, voice, or production route evidence.

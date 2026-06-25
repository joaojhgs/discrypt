# P10-T01 Adapter Registry/Factory Plan - 2026-06-25

## Source Scope

- Issue: PER-82 / P10-T01, Phase 10 adapter registry/factory.
- Plan context: original Discrypt plan locks signaling/rendezvous as content-blind infrastructure and delivery as WebRTC direct/TURN or peer overlay, never provider application relay.
- Current release handoff: Discrypt is not production-ready; this task can produce local Rust transport contract evidence only.
- Code anchors: `crates/transport/src/policy.rs`, `crates/transport/src/provider_adapters.rs`, runtime callers in `apps/desktop/src-tauri/src/lib.rs`.

## Acceptance Criteria

- Enabled adapter kinds instantiate through one registry-backed factory path.
- Disabled adapter kinds fail closed with typed `SignalingAdapterError`/policy errors.
- Unknown or mismatched policy/profile selection fails before any provider connection attempt.
- Dev/release required adapter kinds remain represented: MQTT, Nostr, IPFS/libp2p PubSub, and Discrypt QUIC rendezvous.
- Provider adapters remain signaling/rendezvous only; no application text/control/media relay fallback is added.

## Implementation Steps

1. Inspect existing registry/factory and runtime probe call sites.
2. Add the smallest missing validation surface so profile selection goes through the registry/factory and fails closed on unknown, disabled, or mismatched profiles.
3. Add targeted registry tests for required-kind completeness, factory/profile mismatch rejection, disabled-feature fail-closed behavior, and no provider application relay path.
4. Run transport formatting and targeted tests; run feature-enabled transport tests/clippy if feasible.
5. Document result as local/harness evidence in the Multica handoff and avoid claiming production readiness.

## Risks And Mitigations

- Risk: treating provider availability as delivery evidence. Mitigation: tests and comments keep registry/probe evidence limited to signaling, with WebRTC route evidence separate.
- Risk: broadening into individual adapter production hardening. Mitigation: no changes to public profile defaults, privacy capture, abuse/backoff, packaging, or release matrix beyond this registry contract.
- Risk: feature-gated builds diverge. Mitigation: run default and all-adapter-feature transport tests where feasible.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- Targeted `cargo test -p discrypt-transport ...provider...registry...`
- Feature-enabled targeted transport tests when feasible:
  `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter ...`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-transport --lib -- -D warnings` if feasible.

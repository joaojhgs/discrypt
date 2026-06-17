# P3-T05 Remove Provider App-Payload Relay Fallback

Issue: PER-26 / P3-T05

## Requirements Summary

- Source context: PER-26 issue metadata and `.omc/plans/discrypt-plan.md` require signaling providers to remain content-blind rendezvous/signaling only. The current 2026-06-10 handoff reiterates that WebRTC delivery/media state requires route evidence and signaling providers are not application relays.
- Scope: provider adapters, examples, public/local signaling harness copy, and release docs/scripts only. Do not implement new public MQTT/Nostr E2E, TURN proof, NAT-blocked harness, UI redesign, OpenMLS admission, or voice.
- Invariant: MQTT, Nostr, IPFS PubSub, and Discrypt QUIC rendezvous may carry opaque presence and sealed WebRTC negotiation payloads. They must not carry application text/control/media payloads as a fallback.

## Acceptance Criteria

- No test can pass by relaying application text/control/media payloads over MQTT/Nostr/IPFS/QUIC provider control paths.
- Provider control broadcast paths fail closed with a typed signaling-adapter error explaining that providers are signaling/rendezvous only.
- Split-machine app-flow example fails when direct text/control runtime attachment is unavailable instead of continuing through provider relay fallback copy.
- Static grep gate fails on provider-relay fallback copy, provider control relay tests, or provider control roundtrip fields.
- Existing WebRTC DataChannel proof paths remain available and continue to use provider signaling only for sealed offer/answer/ICE negotiation.

## Implementation Steps

1. Update `crates/transport/src/provider_adapters.rs` so provider `broadcast_control` implementations fail closed and provider roundtrip probes/tests no longer require control delivery.
2. Update `crates/transport/tests/public_signaling_e2e.rs` and related docs/scripts to rename public provider smoke tests from `presence_signal_and_control` to `presence_and_signal`.
3. Update `apps/desktop/src-tauri/examples/g009_split_machine_app_flow.rs` to surface runtime attach errors instead of logging fallback continuation.
4. Add `scripts/check-provider-no-app-relay-p3-t05.mjs` plus an `apps/ui/package.json` script wrapper.
5. Run formatting/static gates and targeted transport/desktop tests where available.

## Risks And Mitigations

- Risk: admission-helper control semantics may have depended on provider broadcast control. Mitigation: this task is limited to provider application-payload fallback removal; future admission work must use authorized MLS/OpenMLS state and explicit transport route evidence, not provider app relay.
- Risk: stale release docs still claim public provider control proof. Mitigation: static gate scans docs/scripts/tests for old test names and fallback copy.
- Risk: disabling provider control could mask WebRTC negotiation regressions. Mitigation: keep presence and sealed offer/answer/candidate signaling tests, and retain DataChannel proof tests as the actual text/control route evidence.

## Verification Strategy

- `node scripts/check-provider-no-app-relay-p3-t05.mjs`
- `npm --prefix apps/ui run test:p3-t05-provider-no-app-relay`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- Targeted Rust tests for provider fail-closed behavior, DataChannel proof paths, and desktop missing-runtime pump behavior.

## Evidence Boundary

This produces local/static/harness evidence that provider app-payload relay paths are disabled. It is not production public-provider text delivery evidence; production delivery still requires direct WebRTC P2P, configured TURN-backed WebRTC, or future encrypted peer overlay route evidence.

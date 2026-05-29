# G100 performance soak suite

This gate adds a deterministic Phase N soak harness for the high-load routing and media envelope before the final external multi-device release run.

## Covered cases

- **16 members**: the soak graph represents sixteen unique authenticated members.
- **8+ voice senders**: eight concurrent voice sender bindings protect and verify media frames through SFrame receiver state.
- **1-3 relay hops**: voice routes are constructed for one, two, and three overlay hops and remain inside the hop cap.
- **Packet loss**: deterministic packet loss drives bounded redelivery and rejects stale replay.
- **NAT switching**: direct STUN, relay-overlay, and TURN fallback planner legs are covered.
- **Android doze**: dozing relay posture is accepted as a metric but ranked behind powered relay capacity.
- **Restart/reconnect**: encrypted session route state restores after restart and reconnect recovers around a failed relay with media gap target met.

## Harness location

- `harness/multinode/src/lib.rs` exposes `PerformanceSoakSmoke` and `performance_soak_smoke`.
- The regression test is `performance_soak_smoke_covers_phase_n_load_and_reconnect_gates`.
- The gate uses `discrypt-media`, `discrypt-relay-overlay`, `discrypt-transport`, and `discrypt-storage` seams.

## Boundary

This is a deterministic soak envelope that exercises the Rust routing/media/storage boundaries quickly in CI. The final release E2E gate still owns wall-clock soak duration, physical-device Android doze behavior, and external network captures.

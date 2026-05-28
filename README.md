# discrypt

Implementation of the approved v1.4 plan in `.omx/plans/discrypt-plan.md`.

Phase 0 provides the workspace, crate boundaries, CI, UI shell, and deterministic identity/device-set/MLS/exporter primitives needed for later OpenMLS, SFrame, retention, overlay, and E2E harness phases.

Phase 1 adds the media-security boundary: Rust-owned SFrame-like AEAD media protection, sender/device-bound exporter context, transform bridge APIs with no raw JavaScript keys, relay opacity checks, anti-replay, tamper rejection, and Android native media fallback skeletons.

Phase 2 relay-overlay foundations now cover deterministic relay ranking, the `<= 3` hop guard, ciphertext-only relay packets, and multinode media-security smoke coverage. Full G003 acceptance still requires topology construction, failover/redelivery, store-forward TTL/fanout, and lossy deterministic overlay harnesses.

## Commands

```sh
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo audit
cargo deny check
cargo sbom --output-format spdx_json_2_3 > discrypt.spdx.json
cd apps/ui && npm ci && npm run typecheck && npm run build && npm audit --audit-level=moderate
```

## Security wording

- Content-private, not metadata-anonymous.
- Crypto-shred is cooperative and cannot erase screenshots, exported plaintext, modified clients, or offline own-device keys until reconnect.
- SFrame media requires app-level sender binding; SFrame alone is not treated as per-sender authentication.
- Web/React code passes encoded/protected frames only; raw media/content keys stay in Rust-owned transform bridges.

## Phase 2 relay-overlay review

- `crates/relay-overlay/src/lib.rs` provides deterministic ranking foundations and the `<= 3` hop-limit guard.
- `crates/relay-overlay/src/integrity.rs` models content-blind relay packets that forward opaque ciphertext bytes only.
- `harness/multinode::media_security_smoke` re-checks relay-visible media opacity, replay rejection, and active tamper rejection through relay packet plumbing.
- `docs/phase-2-relay-overlay-review.md` records the current G003 evidence matrix and the remaining acceptance gaps for topology, failover/redelivery, store-forward TTL/fanout, and deterministic lossy harnesses.

## Phase 1 media security slice

- `crates/media/src/sframe.rs` owns SFrame-like frame protection, per-sender/per-device KID binding, AEAD authentication, and receiver anti-replay windows.
- `crates/media/src/transform_bridge.rs` is the Insertable Streams boundary: JavaScript passes encoded bytes, KIDs, and counters only; raw media keys remain in Rust state.
- `crates/media/src/transport.rs` records the Android WebView Encoded Transform gate and the `webrtc-rs` native contingency skeleton.
- `harness/multinode::media_security_smoke` exercises passive relay opacity, replay rejection, and active tamper rejection.

See [`docs/phase-1-media-security-review.md`](docs/phase-1-media-security-review.md) for the G002 evidence matrix and production-hardening notes.

## Phase 2 relay overlay slice

- `crates/relay-overlay/src/ranking.rs` and `topology.rs` rank relay candidates deterministically and cap overlay routes at ≤3 hops.
- `crates/relay-overlay/src/failover.rs` excludes failed relays before recomputing a bounded route.
- `crates/relay-overlay/src/redelivery.rs` rejects duplicate/stale packet ids and provides deterministic retransmit fanout foundations.
- `crates/relay-overlay/src/store_forward.rs` queues ciphertext-only packets with TTL expiry, duplicate rejection, and bounded fanout.
- `harness/multinode::relay_overlay_smoke` exercises topology, failover, replay rejection, store-forward TTL/fanout, and protected media relay opacity in one deterministic scenario.

See [`docs/phase-2-relay-overlay-review.md`](docs/phase-2-relay-overlay-review.md) for the G003 evidence matrix and remaining production-hardening notes.

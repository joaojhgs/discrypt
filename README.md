# discrypt

Implementation of the approved v1.4 plan in `.omx/plans/discrypt-plan.md`.

Phase 0 provides the workspace, crate boundaries, CI, UI shell, and deterministic identity/device-set/MLS/exporter primitives needed for later OpenMLS, SFrame, retention, overlay, and E2E harness phases.

Phase 1 adds the media-security boundary: Rust-owned SFrame-like AEAD media protection, sender/device-bound exporter context, transform bridge APIs with no raw JavaScript keys, relay opacity checks, anti-replay, tamper rejection, and Android native media fallback skeletons.

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

## Phase 1 media security slice

- `crates/media/src/sframe.rs` owns SFrame-like frame protection, per-sender/per-device KID binding, AEAD authentication, and receiver anti-replay windows.
- `crates/media/src/transform_bridge.rs` is the Insertable Streams boundary: JavaScript passes encoded bytes, KIDs, and counters only; raw media keys remain in Rust state.
- `crates/media/src/transport.rs` records the Android WebView Encoded Transform gate and the `webrtc-rs` native contingency skeleton.
- `harness/multinode::media_security_smoke` exercises passive relay opacity, replay rejection, and active tamper rejection.

See [`docs/phase-1-media-security-review.md`](docs/phase-1-media-security-review.md) for the G002 evidence matrix and production-hardening notes.

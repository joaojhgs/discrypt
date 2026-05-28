# discrypt

Implementation of the approved v1.4 plan in `.omx/plans/discrypt-plan.md`.

Phase 0 provides the workspace, crate boundaries, CI, UI shell, and deterministic identity/device-set/MLS/exporter primitives needed for later OpenMLS, SFrame, retention, overlay, and E2E harness phases.

## Commands

```sh
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd apps/ui && npm install && npm run build
```

## Security wording

- Content-private, not metadata-anonymous.
- Crypto-shred is cooperative and cannot erase screenshots, exported plaintext, modified clients, or offline own-device keys until reconnect.
- SFrame media requires app-level sender binding; SFrame alone is not treated as per-sender authentication.

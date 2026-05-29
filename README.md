# discrypt

Implementation of the approved v1.4 plan in `.omx/plans/discrypt-plan.md`.

Phase 0 provides the workspace, crate boundaries, CI, UI shell, and deterministic identity/device-set/MLS/exporter primitives needed for later OpenMLS, SFrame, retention, overlay, and E2E harness phases.

Phase 1 adds the media-security boundary: Rust-owned SFrame-like AEAD media protection, sender/device-bound exporter context, transform bridge APIs with no raw JavaScript keys, relay opacity checks, anti-replay, tamper rejection, and Android native media fallback skeletons.

Phase 2 relay-overlay foundations cover deterministic relay ranking, the `<= 3` hop guard, failover, redelivery/replay rejection, ciphertext-only relay packets, store-forward TTL/fanout, and multinode media-security smoke coverage.

Phase 3 text/history delivery foundations cover per-author sent-log merge, bounded recipient caches, content-blind 16-peer gossip convergence, ordered delivery, expiring Welcome/catch-up, fork/downgrade/replay detection, and explicit repair by rejoin/reproposal without replaying divergent MLS commits.

Phase 4 retention/shred/live-key foundations cover required retention presets, shorten-retroactive/lengthen-future semantics, lock-not-vanish placeholders, cross-device tombstone sync, membership-gated/rate-limited live-key responses with decoys, secure-delete simulation, and account-continuity backups that exclude content keys.

Phase 5 governance/admission/recovery/abuse foundations cover signed epoch-bound governance ordering, authority checks, removed-admin race rejection, invite expiry/revoke/max-use, PAKE/helper-only password admission, account-continuity recovery trust material, and invite/spam/freeload abuse controls.

Phase 6 connectivity/signaling/push/metadata foundations cover content-blind in-memory rendezvous with zero linkage at rest, STUN→relay-overlay→TURN fallback with owner endpoint overrides, content-free Android FCM wake envelopes, and pcap-style metadata matrix assertions.

Phase 7 UX/E2E hardening adds the serializable command snapshot, native shell command facade, Discord-style React skeleton, final all-phase harness smoke, and honest deletion/metadata/security copy gates.

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



## Phase 7 UX/E2E hardening slice

- `crates/core/src/lib.rs` exposes the serializable app snapshot and safety-number verification command contracts for native UI wiring.
- `apps/desktop/src-tauri/src/lib.rs` provides the dependency-light Tauri command facade and command-health smoke.
- `apps/ui/src/commands.ts`, `main.tsx`, and `styles.css` build a Discord-style shell for friends, servers, channels, voice, invites, devices, retention, connectivity, and honest guarantees.
- `harness/multinode::ux_e2e_hardening_smoke` rechecks command surface readiness plus all previous phase smokes in one final deterministic E2E gate.

See [`docs/phase-7-ux-e2e-hardening.md`](docs/phase-7-ux-e2e-hardening.md) for the G008 evidence matrix and production-hardening notes.

## Phase 6 connectivity/signaling/push/metadata slice

- `external/signaling-repository/src/lib.rs` provides the content-blind rendezvous reference server, at-rest inspection records, metadata matrix, and pcap-style audit fixtures.
- `crates/transport/src/lib.rs` plans strict STUN→relay-overlay→TURN fallback and honors owner/group custom endpoints while marking overlay/TURN as ciphertext-only.
- `crates/push/src/lib.rs` builds content-free Android FCM wake envelopes with hashed tokens and auditable provider-visible bytes.
- `harness/multinode::connectivity_signaling_push_smoke` exercises AC13/AC15/AC18/AC-METADATA foundations end-to-end.

See [`docs/phase-6-connectivity-signaling-push-metadata.md`](docs/phase-6-connectivity-signaling-push-metadata.md) for the G007 evidence matrix and production-hardening notes.

G039 invite signaling metadata review is tracked in
[`docs/g039-invite-metadata-review.md`](docs/g039-invite-metadata-review.md).
It documents the current gap between existing admission/signaling foundations
and production invite descriptors carrying signed endpoint policy plus trust
metadata.

## Phase 5 governance/admission/recovery/abuse slice

- `crates/mls-core/src/governance.rs` models signed epoch-bound governance events, canonical ordering, role authority, and removed-admin race rejection.
- `crates/admission/src/lib.rs` enforces invite expiry/revoke/max-use and rejects offline-copyable password verifiers in favor of OPAQUE/PAKE or an online authorized helper plus final MLS Welcome/add.
- `crates/storage/src/lib.rs` models account-continuity recovery with explicit no-material failure and no archival content-key restoration.
- `crates/abuse/src/lib.rs` supplies deterministic invite/spam rate limits and relay freeload penalties.
- `harness/multinode::governance_admission_smoke` exercises AC-GOV/AC3/AC-RECOVERY/AC-ABUSE foundations.

See [`docs/phase-5-governance-admission-recovery-abuse.md`](docs/phase-5-governance-admission-recovery-abuse.md) for the G006 evidence matrix and production-hardening notes.

## Phase 4 retention/shred/live-key slice

- `crates/content-keys/src/lib.rs` models retention presets, transition semantics, tombstones, cross-device shred sync, and membership-gated live-key responses with rate limits and decoys.
- `crates/storage/src/lib.rs` models bounded caches, account-continuity backup without content keys, and secure-delete simulation for SQLite/WAL/key-store paths.
- `harness/multinode::retention_shred_smoke` exercises AC10/AC10b/AC11/AC-PRESENCE/AC-SHRED-PERSIST/AC-RECOVERY foundations in one deterministic scenario.

See [`docs/phase-4-retention-shred-recovery.md`](docs/phase-4-retention-shred-recovery.md) for the G005 evidence matrix and production-hardening notes.

## Phase 3 text/history + MLS delivery slice

- `crates/storage/src/lib.rs` models authoritative per-author logs, deterministic multi-device log merge, and bounded recipient caches for received ciphertext/key state.
- `crates/relay-overlay/src/gossip.rs` gossips content-blind author-log items and proves 16-peer convergence in deterministic harnesses.
- `crates/mls-delivery/src/lib.rs` implements canonical event ordering, expiring Welcome/catch-up bundles, fork/downgrade/replay detection, and explicit rejoin/reproposal repair plans that do not replay divergent MLS commits.
- `harness/multinode::text_history_delivery_smoke` exercises AC4/AC5/AC-MLS-FORK foundations end-to-end.

See [`docs/phase-3-text-history-delivery.md`](docs/phase-3-text-history-delivery.md) for the G004 evidence matrix and production-hardening notes.

## Phase 2 relay overlay slice

- `crates/relay-overlay/src/ranking.rs` and `topology.rs` rank relay candidates deterministically and cap overlay routes at ≤3 hops.
- `crates/relay-overlay/src/failover.rs` excludes failed relays before recomputing a bounded route.
- `crates/relay-overlay/src/redelivery.rs` rejects duplicate/stale packet ids and provides deterministic retransmit fanout foundations.
- `crates/relay-overlay/src/store_forward.rs` queues ciphertext-only packets with TTL expiry, duplicate rejection, and bounded fanout.
- `harness/multinode::relay_overlay_smoke` exercises topology, failover, replay rejection, store-forward TTL/fanout, and protected media relay opacity in one deterministic scenario.

See [`docs/phase-2-relay-overlay-review.md`](docs/phase-2-relay-overlay-review.md) for the G003 evidence matrix and remaining production-hardening notes.

# P9-T06 Multi-Device Identity Plan

Source: PER-80 / P9-T06 from Phase 9 text/history, retention, crypto-shred, and multi-device. The issue metadata names `.omx/plans/production-release-master-plan-2026-06-10.md`, but this checkout does not contain that master plan file. Scope is taken from PER-80, `.omc/plans/discrypt-plan.md` D6/AC2/AC11/AC17, existing P9-T04/P9-T05 plans, and the current `crates/mls-core` OpenMLS/device-set implementation.

## Scope

- Prove a second own device is added only after existing-device authorization, and that it joins a real OpenMLS group through its own key package and Welcome as a distinct leaf.
- Record backend-owned transparency evidence for device add/remove state; no frontend-only labels or invite-parse-only membership.
- Prove remove/rotation behavior advances the OpenMLS epoch, changes exporter material, and prevents the removed device from applying/decrypting future commits.
- Prove persisted device-set/OpenMLS state survives restart using the existing SQLite-backed OpenMLS provider and serialized device set model.
- Preserve PER-78/PER-79 retention/shred caveats: no backup/recovery expansion, no resurrection of shredded keys, and no "deleted everywhere" claim.

## Acceptance Criteria

- A two-device profile harness adds `alice:phone` from a signed pairing payload authorized by `alice:laptop`.
- `alice:phone` joins from a real OpenMLS Welcome, exports the same epoch secret as the existing device, and is represented as a separate member leaf from `alice:laptop`.
- Backend transparency events include a device-paired/add notice and a device-removed notice with epochs.
- Removing `alice:phone` produces an OpenMLS remove commit, advances the epoch, changes exporter material for remaining devices, and `alice:phone` cannot apply later commits.
- Device-set and OpenMLS group state can be reloaded from durable storage after restart.

## Implementation Steps

1. Add focused MLS-core harness coverage in `crates/mls-core/src/openmls_engine.rs` that composes `DeviceSet`, pairing payloads, joining device key packages, Welcome processing, persisted reload, and remove commit rejection.
2. Add any missing OpenMLS-engine inspection helper needed by the harness to prove separate device leaves from backend state.
3. Add release evidence under `docs/release/per80-multi-device-identity-2026-06-25.md` with exact scope, commands, artifacts, and production-readiness caveat.
4. Run targeted `discrypt-mls-core` tests, `cargo fmt --check`, `cargo clippy -p discrypt-mls-core --lib -- -D warnings` if feasible, and `git diff --check`.

## Risks and Mitigations

- OpenMLS member removal can leave the removed device unable to process future commits with an upstream error string rather than a Discrypt typed denial. The harness treats failure to apply post-removal commits as the current backend proof and documents it as local/harness evidence.
- This does not implement backup/recovery, UI device-management screens, adapter behavior, or release readiness. Evidence must state that explicitly.
- Cross-device crypto-shred remains cooperative and best-effort from PER-79; this task must not broaden it into an absolute deletion guarantee.

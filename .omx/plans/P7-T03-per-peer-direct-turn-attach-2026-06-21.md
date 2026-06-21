# P7-T03 - Per-Peer Direct/TURN Attach

Issue: PER-62 / P7-T03

## Requirements Summary

Source context: PER-62 is the Phase 7 follow-up to `.omx/plans/P7-T01-route-graph-data-model-2026-06-21.md` and `.omx/plans/P7-T02-runtime-map-2026-06-21.md`. PER-60 added admitted-peer route graph state and PER-61 added the per-peer Tauri runtime map. This task attaches one direct WebRTC or configured TURN-backed text/control runtime per admitted remote peer for an active group.

Acceptance:
- Active group text/control attach derives one runtime attachment for every admitted non-local, non-revoked remote member.
- Pending and attached runtime state remains keyed by `(text_session_id, remote_peer_id)`.
- Missing, pending-only, revoked, duplicate, or local-loopback peers fail closed.
- Existing DM/two-person attach behavior remains compatible.
- Providers remain signaling/rendezvous only; no message fanout, diagnostics export, overlay relay, voice expansion, packaging, or release-gate work is in scope.

## Implementation Steps

1. Add backend-only helpers in `apps/desktop/src-tauri/src/lib.rs` to derive stable per-member runtime peer ids from admitted group roster state and build per-peer group attachments.
2. Update `attach_text_control_transport_runtime` so `derive_from_state=true` schedules all active group remote peer attach jobs while preserving the existing single DM/legacy path.
3. Keep per-peer attach dedupe, stale completion checks, and runtime map insertion on existing `TextControlRuntimeMapKey::for_attachment`.
4. Add focused Tauri backend regression tests for 3-profile full mesh, independent per-peer pending state, and revoked/unadmitted fail-closed behavior.

## Failure Modes And Safety

- Invite parsing alone does not create runtime attachments; the helper uses admitted roster rows and rejects pending/revoked rows.
- Local peer ids are not accepted as remote edges.
- Duplicate remote runtime peer ids fail closed before any attach job is queued.
- Direct/TURN runtime startup errors remain per pending peer and remove only that peer key.
- The public providers still carry only sealed SDP/candidate/control signaling for WebRTC. No plaintext/ciphertext application message relay fallback is added.

## Verification

- Targeted desktop backend tests proving three admitted profiles produce two independent peer attach entries from one local profile.
- Targeted failure tests for revoked/unadmitted peers and duplicate runtime peer ids.
- Existing two-person/DM runtime-map tests remain green.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local Rust/Tauri backend unit/harness evidence for per-peer runtime attach state. This is not split-machine production route evidence.

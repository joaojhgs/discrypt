# P2-T05 Pre-Admission Request Route

## Source And Scope

- Issue: PER-17 / P2-T05, "Deliver pre-admission request to owner/staff."
- Phase source: Phase 2 of the production release reset; adjacent context from `.omc/plans/discrypt-plan.md` AC3 and `docs/release/handoff-2026-06-10-current-state.md`.
- Prior local context: `.omx/plans/P2-T04-pending-join-non-member-state-2026-06-17.md` established invite parsing as pending/non-member until an authorized OpenMLS Welcome is applied.
- Primary code path: `apps/desktop/src-tauri/src/lib.rs` `TextControlFrameView`, `join_group`, `queue_openmls_admission_key_package`, text/control pump handling, pending admission request persistence, and focused admission tests.

## Acceptance Criteria

- A joiner queues a provider text/control frame addressed to the invited group containing an OpenMLS key package and durable invite binding metadata.
- Owner/staff handling persists a pending admission request only when the request group id and invite binding match a known, non-revoked invite descriptor for that group.
- The pending request row stores the invite id/key and request metadata needed for owner/staff review; no group membership or admitted UI state is granted by receiving the request.
- Automatic admission may return a Welcome only after the same invite-bound key package passes validation and the owner has persisted OpenMLS group state.
- Provider routes remain signaling-only: they carry the pre-admission control frame and OpenMLS key package, not application text/media relay payloads.

## Implementation Steps

1. Extend admission request DTOs and `TextControlFrameView::OpenMlsAdmissionKeyPackage` with optional invite binding fields: invite id, invite key, room secret hash, descriptor schema version, and admission snapshot.
2. When `join_group` parses a production invite, persist the invite before queueing the key package so the outbox frame can bind to the stored signed descriptor metadata.
3. Validate inbound admission key-package frames on owner/staff state: require the invite id/key/hash to match a known group invite when provided, reject mismatched/revoked/cross-group bindings, and persist the invite id/key on the pending request.
4. Update focused Rust tests to cover two-profile delivery, invite binding persistence, rejected mismatched binding, and no pre-Welcome membership promotion.
5. Keep UI TypeScript DTOs in sync so native responses remain type-safe; avoid adding visible debug copy or frontend-only truth claims.

## Failure Modes And Safety Behavior

- Unknown or mismatched invite binding: reject the frame with `group.admission_request_rejected`; do not persist a request and do not create a Welcome.
- Revoked invite binding: reject the frame with the same fail-closed path; owners can recreate an invite.
- Missing OpenMLS owner state in automatic mode: surface the existing OpenMLS admission error and do not mark the joiner admitted.
- Missing invite metadata on legacy/test frames: preserve compatibility only for already-supported local harness frames, but production parsed invites include the binding fields.

## Verification

- Focused Rust backend tests for manual admission request delivery and invite-binding rejection.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`.
- Targeted desktop backend test command for the admission tests.
- `npm --prefix apps/ui run typecheck` if dependencies are present.
- `git diff --check`.
- Manual/two-profile evidence: current focused Rust harness exercises two persisted profiles and receiver-backed text/control transport; true split-machine manual admission remains a release evidence task if local runtime cannot run a multi-host Tauri session.

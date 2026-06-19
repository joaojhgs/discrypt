# P2-T09 Invite Password/PAKE Decision

## Source And Scope

- Issue: PER-21 / P2-T09, "Optional invite password/PAKE decision."
- Plan source: issue body Phase 2 acceptance criteria, `.omc/plans/discrypt-plan.md` AC3/D7/R18, `.omc/specs/deep-interview-discrypt.md` v1.4 admission hardening, and `docs/release/handoff-2026-06-10-current-state.md`.
- Current release invariants: invite parsing is not membership; password success is not membership; protected group text/voice still requires authorized MLS Welcome/add and persisted OpenMLS group state; signaling providers remain rendezvous-only.
- Primary code paths: `crates/admission/src/lib.rs`, `crates/admission/Cargo.toml`, `Cargo.toml`, and a release security note under `docs/release/`.
- Scope boundary: resolve the optional password/PAKE decision for the admission crate only. Do not implement broader governance propagation, automatic admission changes, transport behavior, UI redesign, or OpenMLS persistence changes.

## Decision

Ship the current online authorized admission-helper path, but make its password verifier memory-hard with Argon2id and keep OPAQUE/PAKE reserved for a later dependency/security review.

This satisfies the release requirement without embedding an offline-copyable verifier in invite descriptors. The helper keeps the password commitment private, applies per-subject attempt limits, emits uniform `PasswordRejected` errors for wrong-password and over-limit cases, and returns only a short-lived signed helper proof. Final admission still requires exact authorized MLS Welcome/add evidence.

## Acceptance Criteria

- `OnlineAdmissionHelper` stores an Argon2id password hash/PHC string, not a raw password or SHA-only verifier.
- Password attempts are verified through the helper and counted per subject; after `max_attempts`, even a correct password returns `InviteError::PasswordRejected`.
- Wrong-password and rate-limit failures are indistinguishable at the public error boundary.
- Invite descriptors reject offline verifier material through `InvitePasswordPolicy::validate` and `AdmissionController::validate_gate`.
- Final admission requires a valid helper proof plus matching `AuthorizedWelcome`/MLS Welcome payload before invite consumption.
- Release docs record the decision, threat model boundary, brute-force/rate-limit behavior, and OPAQUE/PAKE roadmap.

## Implementation Steps

1. Add `argon2` to workspace/admission dependencies and replace the helper's SHA-only password commitment with an Argon2id PHC password hash generated with random salt.
2. Keep `password_secret_commitment` only as a non-verifier policy commitment helper if needed by descriptor code/tests, and ensure docs/tests do not describe it as sufficient for password verification.
3. Strengthen admission tests for Argon2id hash shape, no raw password leakage in debug/serialized proof surfaces, uniform rejection, and brute-force/rate-limit behavior.
4. Add a release security note documenting the shipped memory-hard online-helper decision and the future OPAQUE/PAKE dependency-review path.
5. Run targeted admission tests, formatting, and static diff checks; note any unavailable broader checks in the handoff.

## Failure Modes And Safety Behavior

- Offline verifier leakage: descriptors and controller gates fail closed with `OfflineVerifierRejected`; no invite link carries verifier material.
- Helper compromise: a compromised helper can see verifier material and enforce/abuse admission attempts, but still cannot admit a joiner without signed `AuthorizedWelcome`/MLS Welcome authorization.
- Brute force: attempts are online and per-subject rate-limited; Argon2id increases verifier cost, and over-limit returns the same public error as a wrong password.
- Replay: helper proofs are subject-bound, helper-id-bound, signed, and expire before final admission.
- Rollback: the change is crate-local; reverting restores the previous helper verifier but should not change invite descriptor schema or OpenMLS admission requirements.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p2t09 cargo test -p discrypt-admission online_helper_flow_rate_limits_and_signs_expiring_proofs online_helper_uses_memory_hard_hash_without_password_leakage online_helper_failure_privacy_uses_uniform_rejection admission_rejects_offline_verifier_and_requires_welcome helper_admission_requires_matching_gate_and_welcome admission_password_decision_covers_adr_005`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

## Evidence Boundary

This is admission-crate security behavior and documentation evidence. It is not a production claim for a deployed admission-helper service, distributed invite-use synchronization, or full two-profile OpenMLS Welcome admission beyond the existing crate abstractions.

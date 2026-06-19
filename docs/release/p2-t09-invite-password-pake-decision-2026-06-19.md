# P2-T09 Invite Password/PAKE Decision

## Decision

Discrypt ships the optional invite password gate as an online authorized admission-helper flow backed by Argon2id password verification and per-subject attempt limits.

OPAQUE/PAKE remains reserved until a concrete dependency and protocol integration pass is reviewed. The current release path does not embed an offline-copyable verifier in invite descriptors.

## Security Boundary

- Invite descriptors may include a signed password policy, helper id, and rate-limit policy commitment, but no password verifier material.
- `OnlineAdmissionHelper` stores a private Argon2id PHC password hash and signs a short-lived `AuthorizedHelperProof` only after a matching password attempt.
- Wrong passwords and over-limit attempts both return `InviteError::PasswordRejected`, so callers cannot distinguish "wrong" from "rate limited."
- `PasswordGate::OfflineVerifier` and descriptor policies with `offline_verifier_allowed = true` fail closed with `InviteError::OfflineVerifierRejected`.
- A password/helper proof is not membership. Final protected group admission still requires a matching signed `AuthorizedWelcome` over the exact MLS Welcome/add payload, then persisted OpenMLS group state.

## Brute-Force And Rate-Limit Review

The helper increments attempts per joining subject before verification. Once a subject exceeds `max_attempts`, even the correct password is rejected with the same public error as a wrong password. Argon2id raises the cost of each online attempt at the helper boundary; real deployments still need operator-level throttling, abuse telemetry, and helper-key rotation procedures.

This is local admission-crate evidence only. It does not claim a deployed helper service, distributed invite-use synchronization, or full end-to-end production admission.

## Roadmap

- Review OPAQUE/PAKE dependencies and wire a concrete helper when dependency/security review selects one.
- Add deployment guidance for helper persistence, operator throttling, helper signing-key rotation, and abuse telemetry.
- Extend two-profile/Tauri admission evidence once a deployed helper surface exists.

## Verification Target

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p2t09 cargo test -p discrypt-admission online_helper_flow_rate_limits_and_signs_expiring_proofs online_helper_uses_memory_hard_hash_without_password_leakage online_helper_failure_privacy_uses_uniform_rejection admission_rejects_offline_verifier_and_requires_welcome helper_admission_requires_matching_gate_and_welcome admission_password_decision_covers_adr_005`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

# ADR-005: Password-gated admission helper

## Status

Accepted for the production E2E P2P overlay mesh launch gate.

## Context

Password-gated invite links must not become offline dictionary-attack material.
The original plan allows either a concrete OPAQUE/PAKE protocol or an online
authorized admission helper, but explicitly forbids offline-copyable verifiers
because they cannot enforce rate limits once copied from an invite, database, or
log. Final group admission still requires an authorized MLS add/commit or expiring
Welcome; a room secret and password success are not enough.

## Decision

The launch path is the online authorized helper design already modeled in
`crates/admission/src/lib.rs`:

- `PasswordGate::OnlineAuthorizedHelper { helper_id }` is the production selected
  password gate for v1 launch.
- `OnlineAdmissionHelper` holds a private `password_secret_commitment`, counts
  attempts per subject with `max_attempts`, and signs short-lived
  `AuthorizedHelperProof` values with Ed25519.
- Wrong password and over-limit outcomes both return `InviteError::PasswordRejected`
  so UI and network observers do not get a precise oracle.
- `AdmissionController::finalize_helper_admission` requires a valid helper proof
  and exact `AuthorizedWelcome` MLS Welcome/add authorization before consuming an
  invite.
- `PasswordGate::OfflineVerifier` is rejected with
  `InviteError::OfflineVerifierRejected` and must never be serialized into invite
  links or production storage.

OPAQUE/PAKE remains a reserved future path through `PasswordGate::OpaquePake`.
It may be enabled only after a dependency/security review selects a concrete
crate/protocol, adds transcript tests, and proves online or server-enforced rate
limits. Until then, the implementation may accept an already verified PAKE result
inside `AdmissionController::attempt_password`, but production invite UX must use
the online helper for password-gated rooms.

## Rate-limit proof

The proof is local and auditable:

1. `OnlineAdmissionHelper::authorize` increments `attempts_by_subject` before
   checking the supplied password attempt.
2. Attempts over `max_attempts` return `InviteError::PasswordRejected`, matching
   wrong-password failure.
3. Successful attempts return `AuthorizedHelperProof` with `helper_id`, `subject`,
   random `challenge_id`, `expires_at`, helper public key, and helper signature.
4. `AuthorizedHelperProof::verify` rejects mismatched helper id, mismatched
   subject, expired proof, malformed public key, malformed signature, and invalid
   signature.
5. `AdmissionController::validate_gate` rejects `PasswordGate::OfflineVerifier`.

## UX and command error states

Frontend and Tauri command surfaces should map these states without claiming the
link alone admits a member:

| State | User-facing meaning |
| --- | --- |
| `password_rejected` | Password attempt failed or rate limit reached; retry only through helper policy. |
| `helper_mismatch` | The proof was for a different helper or subject. |
| `helper_proof_expired` | The helper proof expired; request a fresh authorization. |
| `welcome_required` | Password/helper succeeded but MLS Welcome/add is still missing. |
| `welcome_invalid` | The Welcome/add authorization does not match this invite/payload. |
| `offline_verifier_rejected` | Offline verifier material is not accepted for production admission. |

Existing UI copy must keep the honest posture: password rooms use OPAQUE/PAKE or
an online authorized helper; no offline verifier; final admission still requires
authorized MLS Welcome/add.

## Verification

Required gates for this decision:

1. `cargo test -p discrypt-admission admission_password_decision_covers_adr_005 --quiet`
   proves the code-level ADR decision covers selected protocol, no-offline-verifier,
   rate-limit proof, final admission, and UX states.
2. `cargo test -p discrypt-admission online_helper_flow_rate_limits_and_signs_expiring_proofs --quiet`
   proves helper rate limiting, signatures, helper/subject matching, and expiry.
3. `cargo test -p discrypt-admission online_helper_failure_privacy_uses_uniform_rejection --quiet`
   proves wrong password and over-limit cases share the same public error.
4. `cargo test -p discrypt-admission helper_admission_requires_matching_gate_and_welcome --quiet`
   proves helper proof and Welcome/add are both required.
5. `cargo test -p discrypt-admission admission_rejects_offline_verifier_and_requires_welcome --quiet`
   proves offline verifiers are rejected and PAKE/helper success still needs Welcome.
6. `npm --prefix apps/ui run test:honesty` proves UI command/copy surfaces keep the
   no-offline-verifier and Welcome-required posture.

## Consequences

- Password-gated production invites are online-helper based until a separate
  OPAQUE/PAKE ADR selects and proves a concrete protocol.
- A leaked invite descriptor does not contain an offline verifier for attackers
  to brute-force.
- Helper availability is part of password-gated admission availability; users
  need clear retry/expired-proof states.
- Successful helper authorization is still not membership. MLS Welcome/add remains
  the final group admission boundary.

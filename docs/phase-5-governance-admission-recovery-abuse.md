# Phase 5 governance, admission, recovery, and abuse review

G006 implements deterministic v1 foundations for signed epoch-bound governance,
hardened admission, account-continuity recovery, and abuse/freeload controls.

## Implementation map

- `crates/mls-core/src/governance.rs`
  - deterministic signed `GovernanceEvent` facade;
  - canonical epoch -> committer leaf -> content-hash ordering;
  - `GovernanceState` authority checks for owner/admin/member;
  - out-of-epoch/invalid/unauthorized rejection;
  - same-epoch removed-admin protection by precomputing evicted committers before
    applying the ordered batch.
- `crates/admission/src/lib.rs`
  - invite expiry, revocation, and max-use controls;
  - `PasswordGate` distinguishes OPAQUE/PAKE and online-helper flows from rejected
    offline-copyable verifier flows;
  - final admission requires password success plus authorized MLS Welcome/add.
- `crates/storage/src/lib.rs`
  - account recovery requires existing device, recovery code, or sealed backup;
  - recovery restores account continuity only and never content keys.
- `crates/abuse/src/lib.rs`
  - fixed-window invite/spam controls;
  - relay contribution accounting produces freeload penalties for ranking.
- `harness/multinode/src/lib.rs`
  - `governance_admission_smoke` covers AC-GOV, AC3, AC-RECOVERY, AC-ABUSE, and
    removed-admin race behavior.

## Acceptance coverage

- AC-GOV/AC16: signed ordered governance events, authority rejection, out-of-epoch
  rejection, deterministic conflict ordering, and removed-admin same-epoch rejection.
- AC3: invite expiry/revoke/max-use, no offline password verifier claim, and final
  admission requiring authorized Welcome/add.
- AC-RECOVERY: no-material recovery fails; sealed backup restores membership/device
  continuity without archival content keys.
- AC-ABUSE: invite/spam rate limits and relay-freeloading penalties are deterministic.

## Production-hardening notes

- The governance signature is a deterministic placeholder. Production must replace it
  with MLS credential verification while preserving the same comparator and authority
  rules.
- `AdmissionController` treats password proof success as an external PAKE/helper result;
  production must wire OPAQUE/PAKE or an online authorized admission helper before any
  password-gated room claim.
- G039 invite signaling metadata is tracked separately in
  [`g039-invite-metadata-review.md`](g039-invite-metadata-review.md). The current
  admission descriptor covers opaque ids, room-secret commitments, expiry, max-use,
  issuer signatures, and revocation/use accounting; it does not yet claim signed
  endpoint policy or trust-fingerprint coverage.
- Abuse controls are local deterministic primitives; production Sybil resistance remains
  documented posture rather than a cryptographic guarantee.

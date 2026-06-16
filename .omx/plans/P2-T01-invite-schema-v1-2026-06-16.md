# P2-T01 - Canonical Invite Schema v1

## Source And Scope

- Source task: PER-13 / P2-T01 from Phase 2 of the production release plan.
- Current release context: `docs/release/handoff-2026-06-10-current-state.md` says invite parsing is not group membership and fresh evidence is required after invite/admission regressions.
- Product invariant: `.omc/plans/discrypt-plan.md` AC3 requires expiring, revocable, max-use invites plus final authorized MLS add/Welcome; link parsing alone is insufficient.
- Scope boundary: lock signed invite descriptor schema v1 only. Do not implement pending join, approval Welcome delivery, auto admission, or transport E2E.

## Code Paths

- Rust schema/signature authority: `crates/admission/src/lib.rs`.
- Rust tamper coverage: `crates/admission/tests/invite_metadata.rs`.
- Tauri DTO/projection compatibility: `apps/desktop/src-tauri/src/lib.rs`.
- TS DTO/fallback parser compatibility: `apps/ui/src/commands.ts`.

## Acceptance Criteria

- Signed descriptor v1 includes stable group/scope commitment, group commitment snapshot, signaling profiles, ICE profile, admission mode snapshot, expiry, max-use, revocation policy, and optional password policy.
- Tampering with group id/scope, signaling endpoint/profile adapter, ICE policy, expiry, max-use, revocation policy, admission snapshot, or password policy invalidates the descriptor signature.
- Descriptor parsing remains admission-only: it may create pending/request state, but must not claim joined/admitted/connected without MLS Welcome/add evidence.
- Password policy carries only helper/PAKE policy metadata; no offline verifier or password secret is serialized.

## Implementation Steps

1. Extend `crates/admission` descriptor structs with explicit v1 schema fields for admission snapshot, revocation policy, and optional password policy.
2. Include every new and existing policy field in canonical signing bytes, including provider allowlist/rotation fields already validated by `InviteSignalingProfile`.
3. Map the new descriptor fields through Tauri `InviteView` / `ConnectivityPolicyView` and TS command DTOs without broad UI behavior changes.
4. Add focused tamper tests for every required axis and serialization tests proving secrets/offline verifiers are absent.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-admission`
- Targeted desktop invite tests if Tauri DTO changes require it.
- `npm --prefix apps/ui run typecheck` if TS DTO changes affect compile surface.

## Risks And Mitigations

- Signature compatibility risk: new canonical fields intentionally lock v1. Existing descriptor generation in this branch will produce the new shape; legacy parser compatibility is kept in the Tauri/UI fallback paths where already present.
- Scope creep risk: no membership/Welcome delivery implementation in this task. Tests assert descriptor validation only.
- Secret leakage risk: serialize descriptors in tests and assert raw room secret/password/offline verifier strings are absent.

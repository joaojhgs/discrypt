# P5-T04 revoke/kick cryptographic completion plan

Issue: PER-46 / P5-T04

## Source and scope

- Source task: Phase 5 governance, "Revoke/kick cryptographic completion."
- Acceptance criterion: revocation issues an OpenMLS remove commit; revoked member loses send, decrypt, and relay authority.
- Required evidence: OpenMLS remove tests and revoked send rejection.
- Available project context: issue body, `.omc/plans/discrypt-plan.md` Phase 5, `docs/release/handoff-2026-06-10-current-state.md`, and adjacent `.omx/plans/P5-T01*`, `P5-T02*`, `P5-T03*`.
- Missing source context: `.omx/plans/production-release-master-plan-2026-06-10.md` is named by the issue but is not present in this checkout.

## Code anchors

- `crates/mls-core/src/openmls_engine.rs`: OpenMLS add/remove, external remove commit target validation, exporter state.
- `apps/desktop/src-tauri/src/lib.rs`: `revoke_group_member_access`, `GroupMemberRevoked` handling, OpenMLS group handle persistence, text send/receive exporter paths.

## Acceptance criteria

- Owner/staff revocation of an admitted member commits an OpenMLS remove-member epoch and queues metadata for remaining admitted members.
- Remaining members accept only a remove commit whose epoch, confirmation tag, and removed target match the expected member.
- Notice-only revoke frames do not revoke non-target members.
- The revoked target fails closed: app-state send is rejected and its local OpenMLS group handle is removed when it accepts its own revoke notice, so old exporter/decrypt access is unavailable even if the target missed intermediate epochs and cannot merge the remove commit.
- Local evidence is explicit harness evidence; it is not a production-network relay proof.

## Implementation steps

1. Inspect existing OpenMLS revoke and governance-frame tests to avoid duplicating covered paths.
2. Tighten target-side `GroupMemberRevoked` handling so accepted self-revocation removes local OpenMLS group state even if OpenMLS cannot merge the self-removal commit into a usable member group.
3. Add/adjust tests proving remaining members rekey, tampered commits fail closed, notice-only frames are ignored for non-targets, revoked send is rejected, and revoked target exporter/decrypt access is unavailable.
4. Keep provider/signaling semantics unchanged; no application payloads may be routed over signaling providers.
5. Run targeted Rust tests, format, and diff checks.

## Risks and safety

- Risk: deleting the target's local OpenMLS handle on an untrusted frame would cause denial of service. Mitigation: this follows the existing target-only notice acceptance path, affects only the local target identity, and is fail-closed; remaining non-target members still ignore notice-only frames and require carried commit bytes plus epoch and confirmation-tag validation before mutating state.
- Risk: app-state revocation without OpenMLS removal overclaims crypto completion. Mitigation: preserve explicit fail-closed status labels and only claim crypto completion for `openmls_remove_member_commit_applied`.
- Risk: old ciphertext can remain in local storage. Mitigation: this task removes future exporter/decrypt authority; historical cooperative deletion/shred remains governed by separate retention/shred work and must not be overclaimed.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t04 cargo test -p discrypt-mls-core openmls_remove -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t04 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml g005_revocation_commits_openmls_remove_member_and_rekeys_remaining_members --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t04 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml g005_presence_uses_ttl_and_revoked_local_member_cannot_send --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=/tmp/discrypt-target-p5t04 cargo fmt --check`
- `git diff --check`

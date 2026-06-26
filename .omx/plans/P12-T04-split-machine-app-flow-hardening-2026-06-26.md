# P12-T04 Split-Machine App-Flow Hardening Plan

## Source And Scope
- Issue: PER-99 / P12-T04.
- Source plan context: Phase 12 full E2E harness expansion from the production release reset. The named production master plan file is not present in this checkout; the current-state handoff and adjacent P12-T02/P12-T03 artifacts define the same evidence-boundary discipline.
- Primary path: `apps/desktop/src-tauri/examples/g009_split_machine_app_flow.rs`.
- Scope: harden the split-machine example artifact contract for owner, joiner, staff, revoked member, manual approval, protected text, presence, voice proof classification, and no provider application relay fallback.
- Non-scope: public MQTT/Nostr split-machine matrix, 3-member harnesses, package install/reinstall, Phase 13 packaging, or UI redesign.

## Invariants
- Invite parsing is not membership; protected group text and voice require an approved OpenMLS admission path and persisted group state.
- MQTT/Nostr/IPFS/QUIC providers remain signaling/rendezvous only; g009 must fail closed instead of using providers as message or media relays.
- Presence is backend TTL state gated by an attached text/control route.
- Voice evidence must distinguish local native/media session state from remote media transport proof. Local capture is harness evidence only unless remote audio frame/route evidence is present.
- Revoked members must fail closed for future protected text sends.

## Implementation Steps
1. Make g009 exercise manual approval by default, while retaining an explicit `--admission-mode automatic` option for compatibility.
2. Add owner-side approval evidence: pending request observation, approval command success, Welcome/decision pump, staff promotion, revoke, protected owner text, presence publication, and no-provider-relay fields.
3. Add joiner-side evidence: pre-approval pending state and send denial in manual mode, post-approval OpenMLS/admitted state, protected joiner text, received owner text, staff promotion observation, revoked state observation, revoked send denial, presence publication, and voice classification.
4. Add structured helper functions for role status, OpenMLS handle presence, voice proof, presence proof, and transport no-relay boundary evidence.
5. Verify with formatting, targeted example build/test, and retained local artifacts. Attempt SSH/split-machine execution if an SSH target is configured; otherwise record the exact blocker and classify evidence as local/harness only.

## Acceptance Criteria
- Artifact schema records PER-99, admission mode, evidence level, direct/TURN-only transport boundary, and provider application relay fallback as false.
- Owner artifact includes manual approval and governance evidence for owner, staff, and revoke flows.
- Joiner artifact includes pending-before-approval and fail-closed revoked send evidence.
- Presence evidence is tied to backend TTL state after route attach, not optimistic status.
- Voice evidence identifies whether it is local capture only or remote media proof.

## Verification
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml g007_manual_admission_approval_persists_openmls_join_without_auto_approving_old_requests --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --example g009_split_machine_app_flow`
- Local artifact command(s) under `target/per99-g009-split-machine-app-flow/`.
- SSH artifact command(s) when a target exists; otherwise document the missing target as a blocker and avoid production split-machine claims.

## Failure Modes
- Route attach unavailable: fail with an explicit no-provider-message-relay error and keep provider fallback false.
- Manual approval request not observed: fail instead of auto-claiming admission.
- Presence publish without route evidence: fail with backend command error.
- Voice remote media absent: record local capture boundary only, not remote audio proof.
- SSH unavailable: produce local/harness evidence only and leave production split-machine proof to QA or a configured runner.

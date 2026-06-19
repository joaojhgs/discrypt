# P3-T08 TURN-Needed Fail-Closed Path

Issue: PER-29 / P3-T08.

Source context:
- Issue metadata scopes this single-task batch to direct WebRTC impossible, no TURN configured, visible direct failed/TURN required, and no connected/delivered success.
- `docs/release/handoff-2026-06-10-current-state.md` says stale green ledgers do not prove WebRTC delivery state after the June reset.
- `docs/release/current-regressions.md` maps `REG-WEBRTC-ICE-STATE-NEW` to ICE new/checking/connected/failed state coverage and route details.
- `docs/release/release-gap-matrix-2026-06-15.md` keeps WebRTC route establishment blocked until direct, configured TURN, or approved relay route evidence exists.
- `.omc/plans/discrypt-plan.md` locks connectivity to STUN -> peer relay overlay -> TURN and keeps providers as signaling/rendezvous only.
- The named `.omx/plans/production-release-master-plan-2026-06-10.md` is absent in this checkout; the issue, metadata, current release docs, and adjacent P3 plans are the local authority.

Scope:
- Add a backend/transport-owned TURN-needed failure result for "direct failed and no configured TURN".
- Surface the failure through desktop diagnostics and command error/log export without marking text/control connected, delivered, admitted, or successful.
- Add deterministic NAT-blocked harness evidence and a release report.
- Do not implement configured TURN success, overlay relay, voice, Phase 4 UI redesign, or broader release gates.

Acceptance criteria:
- With direct path impossible and zero configured TURN servers, transport records failed route state and `turn_required=turn-required`.
- Diagnostics detail says direct WebRTC failed and TURN is required, with no provider application relay fallback.
- Text session remains non-connected and no protected text/control delivery receipt is created from the failure path.
- Error notification/log export includes a typed code and recovery hint.
- Evidence is clearly labeled local/harness evidence, not production-ready installed-app proof.

Implementation steps:
1. Extend `crates/transport/src/session.rs` with a typed helper for direct-failed/TURN-required failures.
2. Update `apps/desktop/src-tauri/src/lib.rs` route proof failure handling so no-direct/no-TURN probes leave a failed text-session snapshot plus command error detail.
3. Add a deterministic desktop test that simulates DataChannel open with no direct route and no TURN servers, then asserts failed state, diagnostics, no route proof, and log export content.
4. Add a release report under `docs/release/` and retain local harness output under `target/e2e/per-29-turn-needed-fail-closed/`.
5. Run targeted Rust formatting/tests and record any skipped broader checks honestly.

Failure modes and safety:
- DataChannel open without direct/TURN route is treated as untrusted and cannot create connected route proof.
- MQTT/Nostr/IPFS/QUIC provider payloads remain signaling-only; no application payload fallback is introduced.
- TURN readiness only counts when both peers report configured TURN evidence.
- If a broader live NAT harness is unavailable locally, retain deterministic local harness evidence and mark it as non-production.

Verification:
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport turn_needed_fail_closed`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml turn_needed_fail_closed --lib -- --test-threads=1`
- `git diff --check`

Stop condition:
- Branch `multica/P3-T08-turn-needed-fail-closed-path` contains code/docs/evidence updates.
- A PR is opened/updated and pinned in issue metadata if publishing succeeds.
- QA handoff comment includes exactly one `@discrypt-qa-tester` mention, branch/PR, changed files, exact verification, artifacts, known gaps, and QA focus.

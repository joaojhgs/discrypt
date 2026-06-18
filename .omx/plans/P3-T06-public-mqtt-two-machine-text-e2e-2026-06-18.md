# P3-T06 Public MQTT Two-Machine Text E2E

Issue: PER-27 / P3-T06.

Source context:
- `docs/release/handoff-2026-06-10-current-state.md` says Discrypt is not production-ready and requires fresh evidence for WebRTC delivery state; signaling providers must not be treated as application relays.
- `docs/release/release-verification-matrix.md` defines split-machine proof as two distinct machines/network hosts with retained local and remote artifacts, route evidence, provider-visible privacy scan results, and explicit non-claims.
- `.omc/plans/discrypt-plan.md` locks the transport model to STUN -> peer relay overlay -> TURN and keeps signaling/rendezvous separate from text/media delivery.
- The named `.omx/plans/production-release-master-plan-2026-06-10.md` is absent in this checkout; the issue body, metadata, current release handoff, release matrix, and adjacent P3 artifacts are the local authoritative scope.

Scope:
- Prove public MQTT provider-signaled WebRTC text/control between two role-split instances using `crates/transport/examples/split_machine_p2p.rs`.
- Preserve MQTT as signaling/rendezvous only: sealed SDP/candidate payloads may cross MQTT; application text/control and media-shaped frames must cross the WebRTC DataChannel.
- Produce fresh retained evidence under `target/e2e/per-27-public-mqtt-two-machine-text-e2e-*` plus a release report under `docs/release/`.
- Do not implement Nostr, TURN-required/fail-closed, overlay, Phase 4 UI, package, or broader release-gate work.

Acceptance criteria:
- Local offerer and SSH-remote or equivalent distinct-host answerer run with a shared room and public MQTT endpoint.
- Both role artifacts show `status=passed`, matching adapter/room/endpoint, WebRTC direct path ready, and DataChannel open.
- Offerer sends opaque text/control over the DataChannel and receives an opaque receipt; answerer records received frame count and opaque byte count.
- Evidence explicitly states that provider-visible MQTT material is limited to hashed/derived topic plus sealed negotiation and that no application payload/media relay fallback was used.
- If the run is transport-only, the release report must say it is not full OpenMLS admission/UI production evidence.

Implementation steps:
1. Update `crates/transport/examples/split_machine_p2p.rs` artifact fields so each role records a release-boundary summary, route assertions, and signaling-only provider assertions without logging raw SDP, ICE credentials, room secrets, text bodies, or media bytes.
2. Add or update a docs/release report for P3-T06 with commands, artifact paths, result fields, and non-claims.
3. Build/check the example with `mqtt-adapter`.
4. Run the local + remote/equivalent split-machine MQTT proof, retain local and copied remote artifacts under `target/e2e/`, and record hashes/paths in the report.
5. Run formatting and targeted checks; do not claim production-ready beyond this row.

Failure modes and safety:
- Public MQTT endpoint unavailable: capture failure JSON under the same artifact directory and mark the issue blocked with endpoint/network evidence.
- DataChannel not open or direct path not ready: fail the proof; do not substitute MQTT application relay.
- Remote SSH unavailable: use an equivalent distinct-host/container only if it satisfies the release matrix split-machine definition; otherwise report blocked.
- OpenMLS/admission not exercised by this transport example: report as an honest non-claim rather than fabricating admission evidence.

Verification:
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo check -p discrypt-transport --features mqtt-adapter --example split_machine_p2p`
- Public MQTT split-machine run, retaining both role artifacts under `target/e2e/*`.
- `git diff --check`

Stop condition:
- Branch `multica/P3-T06-public-mqtt-two-machine-text-e2e` contains code/docs/evidence updates.
- A PR is opened/updated and pinned in issue metadata if publishing succeeds.
- QA handoff comment includes exactly one `@discrypt-qa-tester` mention, branch/PR, changed files, exact verification, artifacts, known gaps, and QA focus.

# P3-T07 Public Nostr Two-Machine Text E2E

Issue: PER-28 / P3-T07.

Source context:
- `docs/release/handoff-2026-06-10-current-state.md` says Discrypt is not production-ready and requires fresh evidence for WebRTC delivery state; signaling providers must not be treated as application relays.
- `.omx/plans/P3-T06-public-mqtt-two-machine-text-e2e-2026-06-18.md` is the adjacent MQTT proof template and defines the same acceptance boundary for a public provider.
- `docs/release/p3-t06-public-mqtt-two-machine-text-e2e-2026-06-18.md` records the Docker namespace proof shape expected for the "same as MQTT with Nostr relay" criterion.
- `.omc/plans/discrypt-plan.md` locks the transport model to STUN -> peer relay overlay -> TURN and keeps signaling/rendezvous separate from text/media delivery.
- The named `.omx/plans/production-release-master-plan-2026-06-10.md` is absent in this checkout; the issue body, metadata, current release handoff, release matrix, and adjacent P3 artifacts are the local authoritative scope.

Scope:
- Prove public Nostr provider-signaled WebRTC text/control between two role-split instances using `crates/transport/examples/split_machine_p2p.rs`.
- Preserve Nostr as signaling/rendezvous only: sealed SDP/candidate payloads may cross Nostr; application text/control and media-shaped frames must cross the WebRTC DataChannel.
- Produce fresh retained evidence under `target/e2e/per-28-public-nostr-two-machine-text-e2e-*` plus a release report under `docs/release/`.
- Do not implement TURN-required/fail-closed, configured TURN proof, overlay, Phase 4 UI, voice, or broader release-gate work.

Acceptance criteria:
- Local offerer and SSH-remote or equivalent distinct-host answerer run with a shared room and public Nostr relay endpoint.
- Both role artifacts show `status=passed`, matching adapter/room/endpoint, WebRTC direct path ready, and DataChannel open.
- Offerer sends opaque text/control over the DataChannel and receives an opaque receipt; answerer records received frame count and opaque byte count.
- Evidence explicitly states that provider-visible Nostr material is limited to endpoint label, derived hashed rendezvous topic, and sealed negotiation envelopes, and that no application payload/media relay fallback was used.
- If the run is transport-only, the release report must say it is not full OpenMLS admission/UI production evidence.

Implementation steps:
1. Update `crates/transport/examples/split_machine_p2p.rs` artifact fields so Nostr runs record PER-28/P3-T07 release-boundary identity without logging raw SDP, ICE credentials, room secrets, text bodies, or media bytes.
2. Add a PR-only CI namespace proof for branch `multica/P3-T07-public-nostr-two-machine-text-e2e` using `--features nostr-adapter`, public relay `wss://nos.lol`, and distinct Docker network namespaces.
3. Add a docs/release report for P3-T07 with commands, artifact paths, result fields, and non-claims.
4. Build/check the example with `nostr-adapter`.
5. Run local role-split Nostr proof when reachable, then rely on PR namespace proof for merge-readiness split-host evidence; retain artifacts under `target/e2e/`.
6. Run formatting and targeted checks; do not claim production-ready beyond this row.

Failure modes and safety:
- Public Nostr endpoint unavailable or rejecting custom events: capture failure JSON/logs and mark blocked if no alternate public relay is available.
- DataChannel not open or direct path not ready: fail the proof; do not substitute Nostr application relay.
- Remote SSH unavailable: use Docker namespace isolation in CI as the equivalent distinct runtime evidence, matching the P3-T06 proof shape.
- OpenMLS/admission is not exercised by this transport example: report as an honest non-claim rather than fabricating admission evidence.

Verification:
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check` when available; otherwise use the cached Rust toolchain and record the deviation.
- `cargo check -p discrypt-transport --features nostr-adapter --example split_machine_p2p`
- Public Nostr split-machine role run, retaining both role artifacts under `target/e2e/*`.
- PR namespace proof job `PER-28 public Nostr namespace proof`.
- `git diff --check`

Stop condition:
- Branch `multica/P3-T07-public-nostr-two-machine-text-e2e` contains code/docs/evidence updates.
- A PR is opened/updated and pinned in issue metadata if publishing succeeds.
- QA handoff comment includes exactly one `@discrypt-qa-tester` mention, branch/PR, changed files, exact verification, artifacts, known gaps, and QA focus.

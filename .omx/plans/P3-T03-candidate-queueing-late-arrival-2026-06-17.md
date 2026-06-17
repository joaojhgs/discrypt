# P3-T03 Candidate Queueing And Late Arrival Handling

Issue: PER-24 / P3-T03.

Source context:
- `docs/release/handoff-2026-06-10-current-state.md` keeps WebRTC route evidence as a release blocker and confirms signaling providers are not application relays.
- `.omc/plans/discrypt-plan.md` keeps WebRTC as the text/control and voice transport boundary; provider adapters are rendezvous/signaling only.
- The issue acceptance criteria require candidates received before remote description to be queued and later applied, with unit/integration coverage for shuffled offer/answer/candidate order.

Scope:
- Touch transport negotiation only, primarily `crates/transport/src/webrtc_negotiation.rs`.
- Provider adapters may keep their existing local pending vectors, but candidate ordering must be safe at the `WebRtcNegotiator` boundary for every caller.
- Do not implement glare handling, reconnect, idempotent runtime attach, TURN proof, public two-machine evidence, UI changes, OpenMLS/admission work, overlay, or voice.

Plan:
1. Add a remote candidate queue to `WebRtcNegotiator` and route all remote candidate application through one helper.
2. When no remote SDP is installed, validate only redacted candidate metadata, record a queued diagnostic event, and retain the candidate without calling the WebRTC stack.
3. After `create_answer` applies a remote offer and after `accept_answer` applies a remote answer, drain the queue and apply each retained candidate exactly once.
4. Keep late candidates applying immediately after remote SDP exists, preserving direct path metrics and relay evidence only after successful WebRTC stack application.
5. Add shuffled-order regression coverage proving pre-description candidates are queued, flushed after description, late candidates apply directly, and diagnostics remain redacted.

Failure modes and safety:
- Malformed or stale candidates that fail WebRTC stack application must return `TransportError::Unavailable` with redacted diagnostics; no raw SDP, ICE credentials, TURN URL, or candidate line may enter the diagnostic timeline.
- Queued candidates must not mark a route ready or increment `remote_candidates_applied` until the WebRTC stack accepts them.
- Providers remain sealed offer/answer/candidate carriers only; no application plaintext or ciphertext frame fallback is added.

Verification:
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport webrtc_negotiation::tests::queues_candidates_until_remote_description_then_applies_late_candidates -- --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport webrtc_negotiation::tests::diagnostic_timeline_exports_redacted_counts_directions_and_failure_reason -- --nocapture`
- `git diff --check`

Stop condition:
- Commit and push branch `multica/P3-T03-candidate-queueing-late-arrival`, open a PR linked to PER-24, pin PR metadata, comment a QA handoff with exactly one `@discrypt-qa-tester` mention, and move the issue to `in_review`.

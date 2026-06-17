# P3-T02 WebRTC Diagnostic Timeline

Issue: PER-23 / P3-T02.

Source context:
- `docs/release/handoff-2026-06-10-current-state.md` says Discrypt is not production-ready and keeps `REG-WEBRTC-ICE-STATE-NEW` open until fresh backend transport evidence exists.
- `docs/release/current-regressions.md` maps `REG-WEBRTC-ICE-STATE-NEW` to transport tests for ICE states, fail-closed behavior, TURN skips, and route details.
- The issue acceptance criteria require redacted offer/answer/candidate counts, candidate directions, ICE gathering/state, DTLS, DataChannel open/close, failure reason, failing-run export, and a secret-scan/redaction gate.

Scope:
- Touch transport diagnostics only, primarily `crates/transport/src/webrtc_negotiation.rs` and provider probe evidence in `crates/transport/src/provider_adapters.rs`.
- Preserve the invariant that providers carry only presence and sealed WebRTC negotiation. Do not add provider application payload relay, reconnect/glare handling, TURN behavior changes, UI redesign, OpenMLS/admission work, overlay, or voice.

Plan:
1. Add a structured `WebRtcDiagnosticEvent` and `WebRtcDiagnosticTimeline` that records only redacted, ordered facts: event kind, peer role, direction, state, SDP/candidate counts, candidate type, relay flag, failure reason, and timestamp.
2. Record events at offer/answer creation/application, local/remote candidate gather/apply, ICE gathering/connection state changes, inferred DTLS state transitions from peer connection readiness, DataChannel attach/open/close/error/send/receive, timeout/failure points, and tear down.
3. Expose timeline snapshots through `WebRtcNegotiator`, direct/data transport metrics, and provider WebRTC probe evidence so Tauri diagnostics can serialize the timeline without raw SDP, ICE credentials, TURN URLs, or frame bytes.
4. Make timeout/failing probe paths include a compact redacted timeline export in the error string.
5. Add unit coverage for redaction, ordered counts/directions/states, failure export, and existing secret-scan expectations.

Verification:
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- Targeted transport tests for the new diagnostic timeline and existing WebRTC redaction/fail-closed tests.
- A repo secret scan over changed files for forbidden raw SDP/ICE/TURN tokens in serialized diagnostic output.

Stop condition:
- Commit and push branch `multica/P3-T02-webrtc-diagnostic-timeline`, open a PR linked to PER-23, pin PR metadata, comment a concise QA handoff with exactly one `@discrypt-qa-tester` mention, and move the issue to `in_review`.

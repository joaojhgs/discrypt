# P3-T04 Glare/Reconnect/Idempotent Attach

Issue: PER-25 / P3-T04.

Source context:
- `docs/release/handoff-2026-06-10-current-state.md` keeps WebRTC route evidence as a release blocker and confirms signaling providers are not application relays.
- `.omc/plans/discrypt-plan.md` keeps WebRTC as the text/control transport boundary for Phase 3 text/control work.
- The named 2026-06-10 production master plan is absent in this checkout; the issue body and adjacent P3 plans remain the authoritative local scope.

Scope:
- Touch transport/Tauri runtime attach only: `crates/transport/src/webrtc_negotiation.rs` and `apps/desktop/src-tauri/src/lib.rs`.
- Do not implement public MQTT/Nostr two-machine evidence, TURN proof, UI redesign, OpenMLS/admission changes, overlay, package work, or voice.
- Preserve provider adapters as sealed SDP/candidate rendezvous only; no application-message/media relay fallback.

Acceptance criteria:
- Duplicate DataChannel attach is idempotent and cannot inflate active channel count or create duplicate send paths.
- Duplicate attach requests for the same text session remain deduped.
- Rapid restart/reconnect cannot let a stale background attach job install a runtime for an old text session.
- Race/glare tests prove simultaneous or duplicate attach paths converge to one active runtime state without stale sessions.
- Diagnostics remain redacted and useful through dedupe/discard events.

Implementation steps:
1. Add DataChannelHub duplicate-channel detection by `(label, id)` before storing or spawning a receiver task.
2. Add redacted diagnostic events for duplicate channel attach attempts.
3. Add Tauri background attach completion validation that only installs a provider runtime if the pending job still matches the active text session, role, and peer ids.
4. Add targeted transport and Tauri tests for duplicate channel attach and rapid restart stale attach discard.
5. Run focused Rust/Tauri verification plus formatting and diff checks.

Failure modes and safety:
- If a background job completes after session stop/restart, discard it, clear only the matching pending row, and emit a command error/event instead of attaching stale runtime state.
- If a second channel with the same label/id arrives, ignore it and keep metrics at one attached channel.
- If a legitimate reconnect creates a different channel id, keep accepting it; this preserves reconnect while preventing duplicate attach of the same channel identity.

Verification:
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- Targeted transport duplicate attach/race test.
- Targeted Tauri attach/restart/idempotence tests.
- `git diff --check`

Stop condition:
- Commit and push branch `multica/P3-T04-glare-reconnect-idempotent-attach`, open a PR linked to PER-25, pin PR metadata, comment a QA handoff with exactly one `@discrypt-qa-tester` mention, and move the issue to `in_review`.

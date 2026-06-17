# P2-T03 - Remove Invalid Fallback Endpoint Leakage

## Source And Scope

- Source task: PER-15 / P2-T03 from Phase 2 invite foundation work.
- Current release context: `docs/release/handoff-2026-06-10-current-state.md` requires fresh evidence for invite/admission regressions and says invite parsing is not membership.
- Canonical schema context: `.omx/plans/P2-T01-invite-schema-v1-2026-06-16.md` locked signed invite descriptor v1 and maps descriptor metadata through `apps/desktop/src-tauri/src/lib.rs`.
- Product invariant: `.omc/plans/discrypt-plan.md` requires invite links to remain admission-only; final admission still requires authorized MLS add/Welcome.
- Scope boundary: remove `.invalid` fallback endpoint leakage from invite creation only. Do not implement pending join, owner/staff approval, automatic admission, transport E2E, or OpenMLS persistence.

## Code Paths

- Desktop invite creation and descriptor parsing: `apps/desktop/src-tauri/src/lib.rs`.
- Admission descriptor validation/default context: `crates/admission/src/lib.rs`.
- Durable execution checkpoint: `.omx/ultragoal/goals.json` and `.omx/ultragoal/ledger.jsonl`.

## Acceptance Criteria

- Group invite creation signs the selected group signaling adapter endpoint into the descriptor.
- Decoding generated Nostr and MQTT group invite links returns the same endpoint selected at group creation.
- Backend invite defaults must not fall back to a `.invalid` rendezvous endpoint except for explicitly supplied local/dev/test policy inputs.
- The change must preserve canonical invite descriptor schema v1 and must not claim joined, admitted, connected, delivered, or voice-active state.

## Implementation Steps

1. Inspect the desktop invite endpoint selection path and the signed descriptor parser.
2. Replace backend invite fallback endpoint selection so empty connectivity candidates use configured/default adapter endpoints instead of the admission crate's placeholder endpoint.
3. Add a targeted Rust regression test that creates Nostr and MQTT groups, creates invites, decodes the signed invite metadata, and asserts endpoint equality plus no `.invalid` leakage.
4. Run targeted desktop tests, formatting, and a static grep gate over invite endpoint fallback code.

## Risks And Mitigations

- Public endpoint defaults are not production readiness evidence. Keep the result scoped to descriptor correctness and local/backend harness evidence.
- Explicit `.invalid` endpoints can still appear in tests/docs for negative provider behavior. The new static gate should focus on the backend fallback path, not every historical test fixture.
- Provider endpoints remain signaling-only. No application message/media relay behavior is added.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-desktop invite_descriptor_decodes_selected_mqtt_and_nostr_group_endpoints -- --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-desktop group_invite_channel_message_flow -- --nocapture`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- Static grep gate on fallback/default invite endpoint code proving `default_signaling_endpoint` no longer uses `InviteSignalingMetadata::default_production().signaling_endpoint`.

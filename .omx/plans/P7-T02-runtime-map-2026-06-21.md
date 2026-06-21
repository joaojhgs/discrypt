# P7-T02 - Runtime Map

Issue: PER-61 / P7-T02

## Requirements Summary

Source context: PER-61 is the Phase 7 follow-up to `.omx/plans/P7-T01-route-graph-data-model-2026-06-21.md`. PER-60 added the data-only `GroupRouteGraph` and explicitly excluded runtime maps. This task adds the smallest Tauri backend runtime storage change for PER-61.

Acceptance:
- Replace the single `TextControlTransportRuntime` slot in `apps/desktop/src-tauri/src/lib.rs` with a per-peer runtime map.
- Preserve existing two-person text/control behavior and legacy test/runtime migration semantics.
- Key role-split runtimes by active text session and admitted remote runtime peer. Missing/stale/unmatched runtime state must fail closed.
- Keep MQTT/Nostr/IPFS/QUIC providers as signaling/rendezvous only. No application payload provider relay, group fanout, overlay relay, diagnostics export, voice expansion, or packaging work is in scope.

## Implementation Steps

1. Add an internal runtime-map key for `(text_session_id, remote_peer_id)` with a legacy session-only key for old two-person/test harness attachments.
2. Change `TauriAppService` runtime and pending attach storage from single `Option` fields to `BTreeMap` fields.
3. Update attach, pending, stale-completion, clear, status, presence-evidence, and pump selection helpers to use the map.
4. Keep the public `attach_text_control_transport_runtime` request stable. Derive/explicit role-split attaches use peer keys; legacy/no-peer attaches keep the compatibility key.
5. Add regression tests for two-person pump compatibility and peer-keyed attach dedupe/clearing.

## Failure Modes And Safety

- If no runtime exists for the active session, the pump reports `transport_runtime_missing` and does not send queued frames.
- If only stale runtimes exist, the pump reports session mismatch and does not send queued frames.
- If multiple peer runtimes exist but no frame-level peer selector exists yet, this task does not add fanout. The current pump remains compatible with the legacy/two-person runtime path.
- Runtime attach still derives production peer ids from backend state; explicit peer-id attach remains harness/test-only.

## Verification

- Targeted Tauri backend tests for text/control runtime map and old two-person flow.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

Evidence classification: local Rust/Tauri backend unit/harness evidence. This is not split-machine production route evidence.

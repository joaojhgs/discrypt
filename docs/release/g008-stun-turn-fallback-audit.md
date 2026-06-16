# G008 STUN/TURN/fallback hardening audit

Date: 2026-06-01
Base commit under audit: `31ee435`
Scope: direct/STUN success, no-TURN fail-closed behavior, credentialed TURN relay success, signaling-adapter fallback/outage behavior, retry/backoff, duplicate-session prevention, and honest UI states for TURN-required/provider-failed cases.

## Executive finding

G008 is **not production-closed**. The repository has strong deterministic and env-gated foundations, but the current evidence is split across local-process planners, loopback WebRTC tests, feature-gated provider probes, and cautious UI copy. The exact missing release gate is a single network/ICE matrix that proves the runtime behavior end-to-end and skips credentialed TURN only when credentials are absent.

## Existing implementation and evidence

| Area | Current evidence | Status |
| --- | --- | --- |
| Direct/STUN route selection | `crates/transport/src/webrtc_negotiation.rs` records `direct_path_ready` only after WebRTC connected/completed metrics; `crates/transport/src/session.rs` selects direct routes only from checking state; `crates/transport/src/lib.rs` planner preserves STUN → overlay → TURN order. | Partial: deterministic/loopback proof exists; live STUN/NAT proof is not consolidated into a release matrix. |
| No-TURN fail-closed | `WebRtcNegotiationConfig::validate` rejects `RelayOnly` without TURN; tests cover relay-only missing TURN and route selection without relay evidence; expired TURN credentials are rejected before offer generation. | Mostly implemented locally; planner-level placeholder TURN still means callers must enter through validated ICE/WebRTC config. |
| Credentialed TURN relay success | TURN credentials are validated/redacted and passed into `RTCIceServer`; TURN route promotion requires connected WebRTC plus relay candidate evidence. `public_mqtt_relay_only_turn_fallback_roundtrip_when_configured` is env-gated by `DISCRYPT_PUBLIC_TURN_E2E` and now requires endpoint, username, and credential before it can run. | Runtime path exists; real credentialed TURN evidence remains opt-in/missing unless envs are supplied. |
| Adapter fallback/outage behavior | `AdapterReadinessState`, `classify_provider_failure`, and `plan_signaling_adapter_fallback` model provider failures and selection; feature-gated adapters fail closed; degraded Nostr public-relay tests exist. | Planning/classification implemented; runtime outage/failover proof is not consolidated. |
| Retry/backoff | `ReconnectBackoffPolicy` and `schedule_reconnect` are deterministic and tested for delays/exhaustion. | State-machine proof only; no runtime scheduler/outage UI gate. |
| Duplicate sessions | Backend text transport `start_transport_session` reuses an active same-scope session id; UI voice signaling drops same-session/self-origin messages. | Partial; no explicit regression for duplicate start/join prevention across transport/voice/session surfaces. |
| Honest UI states | UI avoids unsupported production claims and diagnostics expose route, adapter, and DataChannel proof states. | Copy is generally honest; missing explicit TURN-required-unavailable and provider-failed/outage user-facing proof tests. |

## Exact missing G008 gates

1. **Network/ICE matrix gate**
   - Direct/STUN success over the real WebRTC path.
   - Relay-only/no-TURN fail-closed behavior.
   - Credentialed TURN relay-only success when `DISCRYPT_PUBLIC_TURN_*` credentials are present.
   - Honest skip when TURN credentials are absent; the skip must state that production TURN closure is not claimed.

2. **Adapter outage/fallback gate**
   - At least one configured provider outage/degraded adapter attempt.
   - Fallback selection evidence with ordered attempts and redacted failure classes.
   - No route/media/delivery claim when all configured adapters fail.

3. **Retry/backoff runtime gate**
   - Runtime caller exercises `schedule_reconnect` or an equivalent reconnect scheduler after a route/provider outage.
   - Evidence includes attempt count, bounded delays, terminal failure after exhaustion, and UI/status copy.

4. **Duplicate-session prevention gate**
   - Starting the same text/signaling/voice transport twice for the same active scope must return the existing active session or reject without replacing it.
   - Regression must verify no duplicate active session records, no duplicate runtime attachments, and no duplicate UI rows/events that imply two active sessions.

5. **UI failure-state gate**
   - Visible copy for `TURN required but unavailable` / relay-only without configured TURN.
   - Visible copy for provider failed/rate-limited/auth-required/outage states.
   - Playwright or command-state assertions that these states are distinguishable from generic `not-run` or local fallback mode.

## Recommended ownership split

- **Task 2 backend/runtime:** add the network/ICE matrix helper/gate, duplicate-session regression, and retry/backoff runtime proof. Reuse existing `WebRtcNegotiationConfig`, `ReconnectBackoffPolicy`, and `start_transport_session` surfaces rather than adding a parallel policy layer.
- **Task 3 frontend/UI:** add explicit TURN-required/provider-failed/fallback copy and UI tests. Preserve current no-fake-production-claims posture.
- **Task 4 scripts/e2e:** wire a G008 matrix script that runs deterministic gates by default, chains UI honesty and G132 provider-privacy checks, and only executes credentialed TURN when endpoint, username, and credential env vars exist.

## Task 4 release script/docs audit update

The G008 release row now lives in `docs/release/release-verification-matrix.md` and is guarded by `scripts/check-release-verification-matrix.mjs`. `scripts/check-g008-stun-turn-fallback.mjs` also checks that row before running its deterministic gates, so a future edit cannot leave G008 outside the release matrix while the dedicated npm alias stays green.

Credentialed TURN is still **not** claimed by default. The optional relay-only test requires `DISCRYPT_PUBLIC_TURN_E2E=1` plus `DISCRYPT_PUBLIC_TURN_ENDPOINT`, `DISCRYPT_PUBLIC_TURN_USERNAME`, and `DISCRYPT_PUBLIC_TURN_CREDENTIAL`; otherwise the release evidence must report the honest skip.

## Subagent findings integrated

- Backend probe confirmed direct/TURN promotion is metrics-gated, but live STUN/TURN proof is still fragmented and default planner TURN is placeholder-backed unless callers validate ICE policy first.
- Test probe confirmed existing G132/local gates and identified missing duplicate-session, UI failure-state, and end-to-end reconnect/outage coverage.
- UI probe confirmed copy is cautious, but TURN-required-unavailable and provider-failed states are not explicitly proven in user-visible flows.

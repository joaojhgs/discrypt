# Discrypt Production Gap Matrix — 2026-06-01

## Purpose

This file is the current-state contract for the restarted production ultragoal. It reconciles the original Discrypt plan, `.omc` continuation artifacts, the latest handoff (`docs/release/handoff-2026-06-01.md`), current source code, and the active `.omx/ultragoal/goals.json` plan. It is intentionally strict: fallback/browser tests, local-only state updates, mocked media, and manual diagnostic controls are not accepted as production evidence.

## Current verdict

Discrypt is **not production-ready yet**. The prior work improved UI navigation and added broad backend command coverage, but the app still lacks verified production-grade real-time delivery, real WebRTC voice audio, production signaling/ICE configuration flows, and a real two-user Tauri E2E harness. The current Playwright suite is useful regression coverage but it runs with `VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1`, so it does not prove Tauri IPC, actual provider signaling, remote WebRTC data delivery, or voice media.

## Evidence consulted

- `docs/release/handoff-2026-06-01.md` — previous agent handoff and explicit gaps.
- `docs/release/ui-production-gap-analysis.md` — previous UI production gap analysis.
- `.omc/plans/discrypt-plan.md` and `.omc/drafts/discrypt-plan.md` — OMC/original planning context.
- `.omx/ultragoal/goals.json` — active 12-goal production plan.
- `apps/ui/src/main.tsx`, `apps/ui/src/commands.ts`, `apps/ui/tests/e2e/*.spec.ts`, `apps/ui/playwright.config.ts` — frontend/runtime/test state.
- `apps/desktop/src-tauri/src/lib.rs` — Tauri command/backend state implementation.

## Do not treat these as production UX or production proof

| Artifact / behavior | Current role | Production decision |
|---|---|---|
| `.omc/**` session/checkpoint/project-memory files | Context and historical handoff material | Read-only reference only; do not execute stale OMC worker state as current truth. |
| `.omx/state/team/**` old team mailboxes/panes | Historical orchestration artifacts | Ignore unless a fresh team is launched for this run. |
| `VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1` browser fallback | Fast UI regression and offline Playwright test mode | Not proof of backend IPC, provider signaling, remote delivery, or media. |
| Inspector/debug rails, local dev banners, manual peer/role controls | Diagnostic escape hatch | Must remain hidden from the straight-line user experience unless explicitly opened. |
| Local voice/session state and fake/default participants | UI scaffolding | Not production voice; no fake participants in production mode. |
| 5-second event polling | Fallback keep-alive | Must not be the primary real-time update path. |

## Production gap matrix mapped to active ultragoals

| Goal | Status now | Missing to call it done | Required verification |
|---|---:|---|---|
| G001 Contract/current-state cleanup | In progress | Freeze this matrix, ignore stale artifacts, verify ultragoal/codex goal alignment. | `jq` over goals, fresh `get_goal`, checkpoint evidence. |
| G002 Production user-flow UI cleanup | Not done | Remove/hide all non-production/manual/harness UI from main flow; setup/recovery, create/join group, create/select channels, DM, text, and voice must be straight-line. | Browser UI tests plus manual/automated visual walkthrough without fallback-only controls. |
| G003 Real-time backend event bus | Not done | Backend must emit Tauri events for profile/group/channel/message/voice/provider changes; frontend must `listen()` and use polling only as slow fallback. | Unit/IPC tests plus two-instance latency evidence below polling interval. |
| G004 Persistence and invite policy correctness | Local command proof added; post-merge verification required | The desktop command layer now has a two-isolated-profile restart matrix covering identity separation, DM/group invites, memberships, channels, custom adapter/STUN/TURN policy propagation, message state, signed delivery receipts, voice device/mute/volume preferences, and UI preferences. Remaining scope is verification on the integrated leader branch; this does not claim live provider delivery, remote voice audio, credentialed TURN success, or two-installed-app E2E. | `cargo test -q -p discrypt-desktop g004_two_profile_restart_matrix_persists_invites_connectivity_receipts_voice_and_preferences -- --nocapture` plus the existing UI fallback reload suite. |
| G005 Signaling and ICE configuration UX | Partial/not production | Add app default + per-DM/per-group/per-channel adapter and ICE/STUN/TURN configuration UI; propagate signed invite config safely with redaction. | UI + backend tests for MQTT, Nostr, IPFS/libp2p, sibling rendezvous config and invalid config errors. |
| G006 Real text delivery through backend-owned runtimes | Not production-proven | No manual peer IDs/roles; DM and group text must auto-connect provider-signaled backend WebRTC runtimes and deliver encrypted messages/receipts across two users. | Two live profiles using public MQTT/Nostr and local/self-hosted IPFS/rendezvous harnesses where public infra is unavailable. |
| G007 Voice audio pipeline | Not done | Real microphone capture into WebRTC, remote playback, mute by track enablement, per-peer volume, speaking detection from real media, cleanup on leave; no fake members. | Two Tauri instances join voice; verify local capture, remote audio/credible loopback, speaking/mute/volume/leave artifacts. |
| G008 STUN/TURN/fallback hardening | Not done | Prove direct/STUN, fail-closed when TURN required but unavailable, credentialed TURN success when credentials supplied, outage fallback, retry/backoff, duplicate-session prevention. | Network/ICE matrix with env-gated TURN credentials and honest skip only when no credentials exist. |
| G009 Security/privacy/no-shim gates | Partial | No plaintext SDP/ICE credentials/TURN creds/room seeds/messages/audio/content keys in provider payloads/logs/persistence/debug screenshots; dependency/license/security audits current. | Secret scanners, log/payload tests, dependency audit, docs. |
| G010 Release harness and automation | Partial | Real Tauri two-user harness with isolated profiles, concurrent app instances, setup/group/invite/text/voice flows, logs/screenshots/artifacts. | Reproducible script command and archived artifacts. |
| G011 Production ready | Not done | All prior production gates clean; Linux packaging/build docs current; unsupported paths disabled or honest. | Final quality gate, code review, build/package verification. |
| G012 Two-user Tauri E2E text + voice | Not done | Run real Tauri app instances for two users in one group; text both ways; voice join/mute/speaking/volume/leave; persistence reload. | Final artifact bundle with logs/screenshots/test output. |

## Current known implementation facts

1. **Setup/profile flow exists** and persists user creation/recovery at the command layer, but must be revalidated in the new two-profile Tauri harness.
2. **Group/channel/invite commands exist**, including v1 invite URLs, but group/DM production invite flows need configuration correctness, UI integration, and restart validation.
3. **Text transport runtime exists** behind backend commands and prior tests, but the visible app still relies on fallback tests and auto-attach logic that must be proven with real provider signaling and two live profiles.
4. **Real-time UI updates are polling-first** according to the handoff and source references; Tauri push events are a required gap.
5. **Voice is not real remote audio yet**. The current implementation records local voice session state and local mic/RMS behavior, but does not prove audio track negotiation, remote playback, per-peer audio controls, or media cleanup.
6. **Signaling adapter support exists below the UI**, but production-level adapter and ICE/STUN/TURN configuration, signed invite propagation, redaction, and validation are not complete.
7. **Current E2E tests are not enough** because they run in local-dev fallback mode and can pass without real Tauri IPC or networked peer delivery.

## Fresh execution rules for the next team

- Work from `.omx/ultragoal/goals.json`; do not revive older `.omx/state/team/*` or `.omc` execution state.
- Workers may use `.omc` and release docs as evidence only.
- Each completed goal needs concrete code/test/docs evidence and leader-owned ultragoal checkpointing with a fresh Codex goal snapshot.
- The final two goals are intentionally strict: `G011-production-ready` and `G012-e2e-tested-running-using-tauri-with-two-users-in-one-g` are not complete until real Tauri and two-user artifacts exist.

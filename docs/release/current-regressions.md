# Discrypt current known bad scenarios

Date: 2026-06-15
Status: current regression ledger from user reports

## Purpose

This document records the current known-bad scenarios that must stay visible
while the release plan resets stale claims. It is not proof that the scenarios
are fixed. Each row is a regression target for later implementation and test
work, and every production claim must be backed by fresh evidence after the
fixing task runs.

## Regression ledger

| ID | User-reported scenario | Current known-bad symptom | Truth invariant | Later verification mapping |
| --- | --- | --- | --- | --- |
| REG-INVITE-BROKEN-GROUP | Invite broken group | A group invite can be parsed or displayed while the joining user still cannot reach a protected usable group chat. The UI must treat this as not joined until backend membership evidence exists. | Invite parsing is not group membership. Protected group text/voice requires an authorized MLS Welcome/add and persisted OpenMLS group state. | Add a two-profile invite/admission regression that proves invite creation, join request, MLS Welcome/add, persisted group membership, channel visibility, and restart survival. |
| REG-MANUAL-ADMISSION-INVISIBLE | Manual admission invisible | A manually admitted or pending user can be invisible or ambiguous in the UI, leaving the owner and requester unable to distinguish pending, approved, rejected, or failed admission state. | Manual admission state must come from backend policy data, not frontend optimism. Pending/approved/rejected status must be explicit and durable. | Add admission UI/backend tests that cover requester pending state, owner approval/rejection, rejected recovery copy, approved member visibility, and reload persistence. |
| REG-PRESENCE-OFFLINE | Presence offline | Members may appear offline or unknown even when a session is expected to be active, and the UI must not fill this gap with fake online indicators. | Presence/member lists must distinguish cached membership from verified online state; online status requires backend/provider evidence. | Add a presence regression that uses two live profiles or a Tauri-capable harness to prove online, offline, stale, and unknown states with timestamps/source evidence. |
| REG-WEBRTC-ICE-STATE-NEW | WebRTC ICE state new | Text/control or media setup can remain in a fresh/new ICE state without a usable direct, TURN, or approved relay route. The UI must report the unresolved route instead of claiming connected delivery. | Delivery and media route state must reflect backend transport/ICE evidence. Providers are signaling/rendezvous only and must not be treated as application relay. | Add transport tests that cover ICE new/checking/connected/failed states, relay-only fail-closed behavior, TURN credential skips, and user-facing route details. |
| REG-STORAGE-VAULT-REINSTALL-FAILURE | Storage vault reinstall failure | Reinstall or relaunch with existing encrypted state can fail vault/keyring recovery, risking an unreadable profile or confusing setup loop. The app must not silently reset or overwrite state. | Storage/keyring/vault setup is security-first: never overwrite unreadable state, never silently reset a profile, and make storage mode/password requirements clear. | Add a reinstall/recovery regression that uses an existing profile directory, missing/wrong credential paths, keyring unavailable mode, explicit recovery errors, and no-overwrite assertions. |

## Evidence boundary

These rows are user-report regression targets only. They do not prove current
code behavior, do not close any production gate, and do not replace later
Tauri/backend-backed tests. Until those tests exist and pass, release docs must
continue to treat these scenarios as open known-bad paths.

## Release handling rules

- Do not mark a user joined, admitted, online, connected, delivered, or
  persisted from frontend-only state.
- Do not hide a known-bad path behind fallback/local-dev success.
- Do not collapse pending, failed, and rejected states into generic empty UI.
- Do not use MQTT, Nostr, IPFS, or QUIC providers as application-message or
  media relays in docs, tests, or UI copy.
- Keep every future fix mapped back to one of the regression IDs above or add a
  new row before claiming the scenario has coverage.

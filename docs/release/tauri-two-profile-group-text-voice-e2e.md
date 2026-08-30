# Tauri Two-Profile Group Text and Voice E2E

## Scope
- Test: two native Tauri profiles completing user setup, group invite/admission, bidirectional text, and voice actions.
- Primary harness: `scripts/tauri-two-profile-group-text-voice-e2e.mjs`.
- Static contract check: `npm --prefix apps/ui run test:tauri-two-profile-group-text-voice-e2e-contract`.
- Real runner command: `node scripts/tauri-two-profile-group-text-voice-e2e.mjs --run --require-native-voice --artifact-dir target/tauri-two-profile-group-text-voice-e2e/<run-id>`.

## Evidence Contract
- The harness drives two real Tauri WebViews when `--run` is used with `tauri-driver`, `WebKitWebDriver`, `rumqttd` (or an explicitly supplied local MQTT endpoint), a built app binary, and `DISPLAY` or `WAYLAND_DISPLAY`.
- By default it starts an isolated MQTT v5 broker on `127.0.0.1` and records its config and log in the artifact directory. Override the port with `DISCRYPT_TAURI_TWO_PROFILE_E2E_MQTT_PORT`, or provide an already-managed loopback broker with `--mqtt-endpoint` / `DISCRYPT_TAURI_TWO_PROFILE_E2E_MQTT_ENDPOINT`.
- The summary artifact is `target/tauri-two-profile-group-text-voice-e2e/<run-id>/tauri-two-profile-group-text-voice-e2e-summary.json`.
- The artifact bundle must include setup, invite, owner/staff approval, text, voice, persistence, and degraded/unavailable-state evidence.
- Dry-run is contract/preflight evidence only; it does not prove setup, invite, approval, text, voice, persistence, or production readiness.

## Truth Boundaries
- Invite parsing is not membership; protected text and voice evidence require backend owner/staff approval plus persisted OpenMLS Welcome/add state.
- MQTT, Nostr, IPFS PubSub, and Discrypt QUIC remain signaling/rendezvous only and are not application relay evidence.
- Admission request/decision must cross the sealed configured MQTT broker control lane. After admission, bidirectional group text and voice signaling must cross provider-signaled direct WebRTC text/control runtimes. The strict harness has no manual WebDriver command-bridge fallback.
- Owner/staff admission approval is clicked in Alice's UI. Reading backend state afterward is evidence collection, not an action shortcut.
- Synthetic WebView voice fallback is diagnostic only. Strict E2E acceptance requires native Rust/generated-audio media evidence or a stronger external hardware loopback artifact.

## Local Verification
- Run the static contract with `npm --prefix apps/ui run test:tauri-two-profile-group-text-voice-e2e-contract`.
- Run full WebDriver evidence on a display and native WebDriver capable runner using the command above.

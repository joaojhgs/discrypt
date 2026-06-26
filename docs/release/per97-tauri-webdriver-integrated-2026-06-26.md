# PER-97 Tauri WebDriver Integrated Evidence

## Scope
- Issue: PER-97 / P12-T02 Tauri WebDriver integrated two-profile.
- Primary harness: `scripts/g012-tauri-webdriver-integrated.mjs`.
- Static contract check: `npm --prefix apps/ui run test:p12-t02-tauri-webdriver-integrated`.
- Real runner command: `node scripts/g012-tauri-webdriver-integrated.mjs --run --require-native-voice --artifact-dir target/g012-e2e/<run-id>`.

## Evidence Contract
- The harness drives two real Tauri WebViews when `--run` is used with `tauri-driver`, `WebKitWebDriver`, a built app binary, and `DISPLAY` or `WAYLAND_DISPLAY`.
- The summary artifact is `target/g012-e2e/<run-id>/tauri-webdriver-integrated-summary.json`.
- The artifact bundle must include setup, invite, owner/staff approval, text, voice, persistence, and degraded/unavailable-state evidence.
- Dry-run is contract/preflight evidence only; it does not prove setup, invite, approval, text, voice, persistence, or production readiness.

## Truth Boundaries
- Invite parsing is not membership; protected text and voice evidence require backend owner/staff approval plus persisted OpenMLS Welcome/add state.
- MQTT, Nostr, IPFS PubSub, and Discrypt QUIC remain signaling/rendezvous only and are not application relay evidence.
- The manual WebDriver command bridge is labeled non-provider-runtime evidence when used.
- Synthetic WebView voice fallback is diagnostic only. Production voice/checkpoint claims require native Rust/generated-audio media evidence or a stronger external hardware loopback artifact.

## Local Verification
- Pending in this branch: static contract check and dry-run manifest.
- Full WebDriver evidence must be produced on a display and native WebDriver capable runner.

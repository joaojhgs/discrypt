# PER-98 Tauri Two-Profile Launcher Boundary Evidence

## Scope
- Issue: PER-98 / P12-T03 Tauri two-profile launcher boundary.
- Primary harness: `scripts/g012-tauri-two-profile-e2e.mjs`.
- Static contract check: `npm --prefix apps/ui run test:g012-tauri-two-profile-e2e`.
- Local launcher command: `node scripts/g012-tauri-two-profile-e2e.mjs --run --artifact-dir target/g012-e2e/<run-id>`.
- Delegated action command: `node scripts/g012-tauri-two-profile-e2e.mjs --run --delegate-webdriver --artifact-dir target/g012-e2e/<run-id>`.

## Evidence Contract
- Plain launcher mode proves only that two isolated Tauri profiles were launched with distinct `DISCRYPT_APP_STATE_PATH` values.
- `launch-summary.json` and `tauri-two-profile-launch-manifest.json` include `evidence_boundary`, `evidence_mode`, `production_claim_allowed: false`, `action_driven_evidence: false`, and `g012_checkpoint_eligible: false` unless a delegated WebDriver summary is produced.
- Failed preflight and dry-run paths still write summary JSON so unavailable runner evidence cannot be mistaken for success.
- `--delegate-webdriver` records and invokes `scripts/g012-tauri-webdriver-integrated.mjs --run --require-native-voice`; action-driven evidence belongs to that WebDriver artifact, not to launch-smoke output.

## Truth Boundaries
- Launch smoke is not setup, invite, owner/staff approval, OpenMLS admission, protected text, native voice, persistence, split-machine transport, or production-ready evidence.
- Invite parsing is not membership; protected group text/voice claims require backend/OpenMLS evidence from the delegated WebDriver/native harness or stronger release artifact.
- MQTT, Nostr, IPFS PubSub, and Discrypt QUIC remain signaling/rendezvous only and are not application relay evidence.

## Local Verification
- `npm --prefix apps/ui run test:g012-tauri-two-profile-e2e`
- `node scripts/g012-tauri-two-profile-e2e.mjs --artifact-dir target/p12-t03-tauri-two-profile-launcher-boundary`
- `git diff --check`

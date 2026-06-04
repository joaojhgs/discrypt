# G010 release harness and automation

## Purpose

G010 provides a reproducible two-user harness contract for release candidates. It
keeps local fallback, native command-layer, and public-provider evidence separate
so the release notes do not turn local harness proof into fake production claims
or manual pairing claims.

## Commands

| Scope | Command | Evidence |
| --- | --- | --- |
| Local two-profile harness | `npm --prefix apps/ui run test:g010-release-harness` | Builds the UI with the local-dev fallback enabled, runs the two-profile Playwright setup/DM/group/invite/text/voice specs serially, runs desktop command-layer two-profile persistence/text/voice tests, runs the local adapter matrix, and runs the G009 privacy/no-shim gate. |
| Public adapter matrix | `npm --prefix apps/ui run test:g010-release-harness:public` | Runs the public/provider matrix wrapper. Missing external credentials are reported as skips, not failures or production claims. |
| Static contract gate | `npm --prefix apps/ui run test:g010-release-contract` | Verifies package scripts, launch/profile artifact scripts, adapter env-gate documentation, and artifact contract text remain present. CI and release-matrix wiring are owned by the release-matrix lane. |
| Tauri launch dry-run | `npm --prefix apps/ui run test:g010-tauri-launch-dry-run` | Writes the two-profile Tauri launch manifest without starting GUI processes, proving isolated profile paths and dev/build command construction are reproducible. |

The local harness writes `target/g010-release-harness/<run-id>/manifest.json`
with command status, log paths, and the isolated profile state paths. The Tauri
launch wrapper writes `target/g010-release-harness/<run-id>/tauri-launch-manifest.json`
with the exact concurrent launch commands and per-profile environment.

- `target/g010-release-harness/<run-id>/profiles/alice/app-state.discrypt-store`
- `target/g010-release-harness/<run-id>/profiles/bob/app-state.discrypt-store`
- `target/g010-release-harness/<run-id>/logs/*.log`
- `target/g010-release-harness/<run-id>/playwright/`
- `target/g010-release-harness/<run-id>/screenshots/`

The `scripts/g010-tauri-two-profile-launch.mjs` wrapper supports `--app-mode=dev`
and `--app-mode=build`. By default it is a dry-run manifest generator. A
GUI/WebDriver runner can pass `--run` to start one shared Vite dev server and two
concurrent Tauri instances with separate `DISCRYPT_APP_STATE_PATH` values. The
wrapper captures Vite, Alice, and Bob process logs under `logs/` and leaves UI
screenshots to the WebDriver layer under `screenshots/`. The default
`DISCRYPT_G010_TAURI_FEATURES` is `tauri-runtime,local-dev,production-media,mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter` because the desktop
state-path override is intentionally honored only in test/harness/local-dev
builds; override it only with a feature set that still includes `tauri-runtime`
and either `local-dev` or `harness`. This is profile-isolation harness evidence,
not production release packaging evidence. Headless local verification remains
based on the maintained native command-layer and Playwright profile-isolation
proofs.

## Covered user flow

The combined local command covers setup/recovery-adjacent first-run behavior,
DM creation, DM invite acceptance, group creation, group invite acceptance, text
channel send/receive state transitions, voice join/mute/volume/leave state, and
artifact/log capture. The browser specs validate user-facing navigation and media
controls; the Rust desktop tests validate native command persistence and isolated
profile state files.

## Public adapter env gates

No fake production claims: public-provider rows run only when their explicit
environment gates are set. Missing credentials/endpoints are honest skips.

| Adapter/proof | Required gate | Additional required env |
| --- | --- | --- |
| MQTT public signaling | `DISCRYPT_PUBLIC_MQTT_E2E=1` | Optional `DISCRYPT_PUBLIC_MQTT_ENDPOINT`; defaults to `mqtts://broker.emqx.io:8883`. Legacy `DISCRYPT_PUBLIC_SIGNALING_E2E=1` remains accepted for compatibility. |
| Nostr public signaling | `DISCRYPT_PUBLIC_NOSTR_E2E=1` | Optional `DISCRYPT_PUBLIC_NOSTR_ENDPOINT`; defaults to the test's public relay setting. |
| IPFS public topic peer | `DISCRYPT_PUBLIC_IPFS_E2E=1` | `DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS=<direct topic-peer multiaddr,...>`. |
| QUIC rendezvous | `DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E=1` | `DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=<https/wss endpoint>` and optional trust fingerprint. |
| TURN relay-only WebRTC | `DISCRYPT_PUBLIC_TURN_E2E=1` | `DISCRYPT_PUBLIC_TURN_ENDPOINT`, `DISCRYPT_PUBLIC_TURN_USERNAME`, and `DISCRYPT_PUBLIC_TURN_CREDENTIAL`. |

## No fake production claims

- Local Playwright runs use `VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1`; they are UI and
  profile-isolation regression evidence, not proof of production provider delivery.
- Same-process desktop tests are native command-layer evidence, not two installed
  GUI processes.
- Public MQTT/Nostr/IPFS/QUIC/TURN rows are opt-in and must cite the exact envs,
  logs, and manifest paths from the release run before any provider-specific
  production statement is made.

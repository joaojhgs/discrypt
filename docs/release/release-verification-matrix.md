# Release verification matrix

G094 release verification ties package install/launch, service deployment smoke,
and privacy-log checks into one auditable matrix. Each row must retain command
output with the release candidate commit and artifact hashes.

| Gate | Command | Required evidence | Local status boundary |
| --- | --- | --- | --- |
| Linux package build | `npm --prefix apps/ui run release:linux` | `.deb`, `.rpm`, and `.AppImage` paths plus package hashes. | Runs on Linux builder with Tauri build dependencies. |
| Linux clean install/launch | `npm --prefix apps/ui run smoke:linux-packages` | Clean Debian/Ubuntu container install, clean Fedora container install, AppImage launch under Xvfb/dbus. | Requires Docker and built package artifacts. |
| macOS/Windows package runners | `npm --prefix apps/ui run test:desktop-package-ci` plus `.github/workflows/package-desktop.yml` | Workflow-dispatch artifacts from macOS and Windows runners. | Local shell validates the runner contract only. |
| Android APK runner | `npm --prefix apps/ui run test:android-gate` plus `.github/workflows/android.yml` | Android emulator APK install, activity start, `RECORD_AUDIO` permission evidence, APK/logcat artifact. | Local shell validates the runner contract and Android media path tests. |
| Signaling/relay deployment smoke | `npm --prefix apps/ui run test:release-verification-matrix` | `/healthz`, `/metrics`, and server process startup without identity, message, media, key, or admin-token leakage. | Runs locally against loopback signaling server. |
| Update/rollback/privacy/secrets | `npm --prefix apps/ui run test:release-governance` | Policy and machine-readable secrets inventory validation. | Does not enable updater or crash upload. |
| Cross-lane installed-profile E2E support | `cargo test -q -p discrypt-desktop text_control_frame_roundtrip_persists_across_two_profile_state_files -- --nocapture`<br>`cargo test -q -p discrypt-desktop text_control_session_pump_uses_data_transport_trait_and_persists_receipt -- --nocapture`<br>`DISCRYPT_DESKTOP_PUBLIC_MQTT_RECEIPT_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 cargo test -q -p discrypt-desktop --features mqtt-adapter public_mqtt_two_profile_receipt_crosses_provider_webrtc_when_enabled -- --nocapture`<br>`DISCRYPT_DESKTOP_PUBLIC_NOSTR_RECEIPT_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol cargo test -q -p discrypt-desktop --features nostr-adapter public_nostr_two_profile_receipt_crosses_provider_webrtc_when_enabled -- --nocapture` | Two isolated local profile files, the reusable transport pump, and public MQTT/Nostr signed-receipt proofs are retained as release evidence. | Same-process Tauri harness proof; not yet two installed app processes. |
| STUN/TURN/provider privacy gate (G132) | `npm --prefix apps/ui run test:stun-turn-provider-privacy-g132`<br>`cargo test -p discrypt-multinode-harness connectivity_signaling_push_smoke_covers_phase6_gates --quiet`<br>`cargo test -p discrypt-transport valid_direct_overlay_and_turn_flows_select_expected_leg --quiet` | Harness proof, transport fallback contract, and provider-privacy evidence required. | Real-provider MQTT proof is opt-in via `DISCRYPT_PUBLIC_SIGNALING_E2E=1`.<br>Missing production adapters are explicitly reported as out-of-scope for this local gate. |
| G132 real-signaling matrix | `npm --prefix apps/ui run test:signaling-e2e-matrix-g132` | Verifies command matrix coverage and explicitly reports missing adapter gates (Nostr/IPFS/QUIC/MQTT when not enabled). | Keep optional/public checks opt-in to preserve deterministic CI.
| Provider-visible metadata capture (G133) | `npm --prefix apps/ui run test:provider-metadata-capture-g133` | Adapter-specific provider-visible capture and plaintext rejection across MQTT, Nostr, IPFS/libp2p, and QUIC boundary. | Repository-local deterministic capture; external host packet capture remains final release-run evidence. |
| Media-frame transport gate (WebRTC DataChannel) | `DISCRYPT_PUBLIC_MQTT_MEDIA_WEBRTC_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=<mqtts://...> cargo test -q -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_signals_real_webrtc_media_frame_roundtrip -- --nocapture` | Proves provider-signaled WebRTC DataChannel roundtrip for encrypted codec-shaped media payloads and receipt semantics over public network rendezvous before remote-audio claims. | Real media decode/playback assertions are intentionally deferred to future installed-app two-process audio transport proof. |

## Sensitive data exclusion

Release logs, crash previews, server stdout/stderr, health responses, metrics,
and uploaded artifacts must not contain:

- message body text, attachment bytes, media frames, SDP bodies, ICE passwords,
  STUN/TURN long-term secrets, MLS secrets, SFrame keys, recovery codes, invite
  secrets, room names, usernames, device names, profile display names, database
  rows, or raw environment variables;
- signing private keys, updater private keys, platform signing certificates,
  signaling admin audit tokens, TURN static auth secrets, crash collector upload
  tokens, or release environment dumps.

## Stop condition

A release candidate can leave Phase M only when the Linux package smoke is fresh,
runner-gated platform rows have retained artifacts or are explicitly held from
promotion, the loopback signaling smoke passes, and privacy/secrets checks pass.

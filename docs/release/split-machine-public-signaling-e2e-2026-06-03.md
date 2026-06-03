# Split-machine public signaling E2E evidence — 2026-06-03

## Scope

This records the first true split-machine Discrypt transport proof requested by the user: one peer ran on the local workstation and the other peer ran on `skyron@skyron-server.example.com` over SSH, in an isolated remote folder. The peers negotiated over public signaling and exchanged opaque application frames over the resulting WebRTC DataChannel.

This is not a physical microphone capture proof. For voice, the proof sends an opaque media-frame ciphertext over the same provider-negotiated WebRTC DataChannel. Native microphone/audio device UX remains a separate UI/runtime validation area.

## Harness added

- `crates/transport/examples/split_machine_p2p.rs`
  - `--role answerer|offerer`
  - `--adapter mqtt|nostr`
  - shared `--room`
  - per-run JSON artifacts
  - sends one opaque text/control frame and one opaque media-frame ciphertext from offerer to answerer
  - answerer returns opaque receipt frames for both payloads

## Runtime fix added

- `crates/transport/src/provider_adapters.rs`
  - Installs a deterministic rustls crypto provider before Nostr relay clients start.
  - This fixed a real split-machine blocker where Nostr panicked with: `Could not automatically determine the process-level CryptoProvider`.
- `crates/transport/Cargo.toml`
  - Makes `rustls` a direct optional dependency of `nostr-adapter` so the Nostr adapter can install the provider without relying on transitive dependency paths.

## Machines and staging

- Local peer: `/home/developer/projects/discrypt`
- Remote peer: `skyron@skyron-server.example.com`
- Remote staged repo: `/home/skyron/projects/discrypt-split-machine-20260603T140822Z/discrypt`
- Remote sibling dependency staged because the workspace references it: `/home/skyron/projects/discrypt-split-machine-20260603T140822Z/discrypt-signaling`
- Remote runtime: Docker `rust:latest`, mounted over the staged folder

## MQTT proof

- Public signaling endpoint: `mqtts://broker.emqx.io:8883`
- Room: `discrypt-split-mqtt-20260603T135445Z-bin`
- Local artifact: `target/split-machine-p2p/20260603T135445Z/mqtt-bin/split-machine-mqtt-offerer.json`
- Remote artifact copied locally: `target/split-machine-p2p/20260603T135445Z/mqtt-bin/remote-split-machine-mqtt-answerer.json`

Result:

- Offerer status: `passed`
- Answerer status: `passed`
- WebRTC direct path: `true`
- DataChannel open: `true`
- Text receipt prefix check: `true`
- Media-frame receipt prefix check: `true`
- Remote received frame count: `2`

## Nostr proof

- Public signaling endpoint: `wss://nos.lol`
- Room: `discrypt-split-nostr-20260603T140822Z-fixed`
- Local artifact: `target/split-machine-p2p/20260603T140822Z/nostr-fixed/split-machine-nostr-offerer.json`
- Remote artifact copied locally: `target/split-machine-p2p/20260603T140822Z/nostr-fixed/remote-split-machine-nostr-answerer.json`

Result:

- Offerer status: `passed`
- Answerer status: `passed`
- WebRTC direct path: `true`
- DataChannel open: `true`
- Text receipt prefix check: `true`
- Media-frame receipt prefix check: `true`
- Remote received frame count: `2`

## Validation commands

```bash
cargo fmt --all -- --check
cargo check -p discrypt-transport --features mqtt-adapter,nostr-adapter --example split_machine_p2p
```

Remote build command:

```bash
docker run --rm \
  -v /home/skyron/projects/discrypt-split-machine-20260603T140822Z:/work \
  -w /work/discrypt \
  -e CARGO_TARGET_DIR=/work/discrypt/target-split-machine \
  rust:latest \
  bash -c 'cargo build -p discrypt-transport --features mqtt-adapter,nostr-adapter --example split_machine_p2p'
```

## Remaining boundary

The verified boundary is real inter-machine, inter-network public signaling plus WebRTC DataChannel delivery for text/control and media-frame ciphertext. It does not prove:

- physical microphone capture on both hosts,
- audible speaker playback,
- Tauri permission UX across both hosts,
- full installed desktop UI flow across both hosts.

Those require a GUI/audio-device E2E pass with Tauri windows on both machines and microphone device selection/permission exercised through the product UI.

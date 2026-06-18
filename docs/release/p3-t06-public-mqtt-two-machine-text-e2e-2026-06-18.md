# P3-T06 Public MQTT Text/Control E2E Evidence - 2026-06-18

Issue: PER-27 / P3-T06.

## Result

Status: partial evidence, not full split-machine closure.

The public MQTT role-split transport proof passed on this branch using two
separate local processes and the public MQTT endpoint
`mqtts://broker.emqx.io:8883`. Both roles opened a real provider-signaled WebRTC
DataChannel, the offerer sent opaque text/control and media-shaped frames over
the DataChannel, and the answerer returned opaque receipts over the same
DataChannel.

This is useful transport evidence, but it is not a full release-matrix
split-machine proof because the available runtime could not authenticate to the
SSH remote and could not access the Docker daemon to create an isolated
container-host substitute.

## Fresh Artifacts

Artifact directory:

`target/e2e/per-27-public-mqtt-two-machine-text-e2e-20260618T034157Z`

Files:

- `split-machine-mqtt-offerer.json`
  - SHA-256: `bb7e899dba6c854e26cb1c2b8a50cfe69fd8e644d627e170e819063049431746`
  - `status`: `passed`
  - `direct_path_ready`: `true`
  - `data_channel_open`: `true`
  - `bidirectional_text_control`: `true`
  - `provider_application_relay_used`: `false`
- `split-machine-mqtt-answerer.json`
  - SHA-256: `faa0d6bc858be12bcd8f2fd239125f2bb34abcd38fdb4759f4256c03f899dd3a`
  - `status`: `passed`
  - `direct_path_ready`: `true`
  - `data_channel_open`: `true`
  - `received_frame_count`: `2`
  - `provider_application_relay_used`: `false`

The artifacts include `release_boundary` and `provider_boundary` objects. These
state that MQTT is signaling/rendezvous only, application payload relay is not
allowed or used, provider-visible material is limited to endpoint label, derived
hashed rendezvous topic, and sealed WebRTC negotiation envelopes, and message
plaintext, opaque app frame bytes, receipts, MLS secrets, and SFrame/content
keys are not provider-visible.

## Commands Run

Formatting:

```bash
PATH=/tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin:$PATH \
  /tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/cargo fmt --check
```

Result: passed. The requested `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check` form
could not run because this runtime has no `cargo` or `rustup` on PATH; the
available cached toolchain was used directly.

Example build check:

```bash
CARGO_HOME=/tmp/discrypt-cargo-home \
CARGO_TARGET_DIR=target/per-27-cargo-check \
PATH=/tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin:$PATH \
  /tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/cargo check \
    -p discrypt-transport \
    --features mqtt-adapter \
    --example split_machine_p2p
```

Result: passed.

Answerer process:

```bash
CARGO_HOME=/tmp/discrypt-cargo-home \
CARGO_TARGET_DIR=target/per-27-cargo-check \
PATH=/tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin:$PATH \
  /tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/cargo run -q \
    -p discrypt-transport \
    --features mqtt-adapter \
    --example split_machine_p2p -- \
    --adapter mqtt \
    --role answerer \
    --room discrypt-per-27-mqtt-20260618T034157Z \
    --endpoint mqtts://broker.emqx.io:8883 \
    --artifact-dir target/e2e/per-27-public-mqtt-two-machine-text-e2e-20260618T034157Z \
    --answerer-hold-secs 120 \
    --receipt-timeout-secs 45
```

Offerer process:

```bash
CARGO_HOME=/tmp/discrypt-cargo-home \
CARGO_TARGET_DIR=target/per-27-cargo-check \
PATH=/tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin:$PATH \
  /tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/cargo run -q \
    -p discrypt-transport \
    --features mqtt-adapter \
    --example split_machine_p2p -- \
    --adapter mqtt \
    --role offerer \
    --room discrypt-per-27-mqtt-20260618T034157Z \
    --endpoint mqtts://broker.emqx.io:8883 \
    --artifact-dir target/e2e/per-27-public-mqtt-two-machine-text-e2e-20260618T034157Z \
    --receipt-timeout-secs 45
```

Result: both role processes passed and wrote artifacts.

## Blocked Split-Machine Attempts

SSH remote attempt:

```bash
ssh -o BatchMode=yes -o ConnectTimeout=8 skyron@skyron-server.example.com 'printf reachable'
```

Result: failed with host-key verification. A non-mutating retry using
`ssh-keyscan` and a temporary `UserKnownHostsFile` reached authentication but
failed with `Permission denied (publickey,password)`.

Docker isolated-host attempt:

```bash
docker ps --format '{{.ID}} {{.Image}}'
```

Result: failed with `permission denied while trying to connect to the docker API
at unix:///var/run/docker.sock`, including after the sandbox escalation request.

## Non-Claims

- This evidence is not production-ready evidence for the full app.
- This evidence is not a two-installed-Tauri-GUI proof.
- This evidence is not physical microphone/speaker audio proof.
- This evidence does not prove OpenMLS invite/admission. The transport example
  derives a scoped DM-style rendezvous and proves the WebRTC text/control route;
  a separate app-flow artifact is still required for create/join/admit semantics.
- This evidence does not prove TURN relay-only behavior or future peer overlay
  behavior.

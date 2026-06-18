# P3-T06 Public MQTT Text/Control E2E Evidence - 2026-06-18

Issue: PER-27 / P3-T06.

## Result

Status: PR namespace proof passed.

GitHub Actions PR run
`https://github.com/joaojhgs/discrypt/actions/runs/27789088226` passed the
`PER-27 public MQTT namespace proof` job on branch
`multica/P3-T06-public-mqtt-two-machine-text-e2e`, head
`3fc5cbe320c4ba1f1721e4f505e7f0f2d1521296`.

The proof ran the public MQTT split-machine transport example in two distinct
Docker network namespaces on a user-defined bridge network. MQTT remained the
public signaling/rendezvous provider at `mqtts://broker.emqx.io:8883`; WebRTC
ICE used host-only candidates bound to each container's real bridge IP, then
opened a DataChannel directly between the two namespaces. The offerer sent
opaque text/control and media-shaped frames over the DataChannel, and the
answerer returned opaque receipts over the same DataChannel.

## Fresh Artifacts

PR artifact name:

`per27-public-mqtt-namespace-27789088226-1`

Downloaded validation directory:

`/tmp/per27-gha-namespace-pass/per27-public-mqtt-namespace-27789088226-1`

Files:

- `split-machine-mqtt-offerer.json`
  - SHA-256: `c10ef714591514f372c7acc34ca152f89176a73acfd8ebec0dfa6a950167a378`
  - `status`: `passed`
  - `room`: `discrypt-per-27-mqtt-gha-27789088226-1`
  - `endpoint`: `mqtts://broker.emqx.io:8883`
  - `direct_path_ready`: `true`
  - `data_channel_open`: `true`
  - `p2p_datachannel_open`: `true`
  - `bidirectional_text_control`: `true`
  - `provider_application_relay_used`: `false`
  - provider boundary `application_payload_relay_used`: `false`
- `split-machine-mqtt-answerer.json`
  - SHA-256: `3262ab35fd23f8c68a46d17fed0f334e7226249cbe518a06eff1a66d29835238`
  - `status`: `passed`
  - `room`: `discrypt-per-27-mqtt-gha-27789088226-1`
  - `endpoint`: `mqtts://broker.emqx.io:8883`
  - `direct_path_ready`: `true`
  - `data_channel_open`: `true`
  - `p2p_datachannel_open`: `true`
  - `received_frame_count`: `2`
  - `provider_application_relay_used`: `false`
  - provider boundary `application_payload_relay_used`: `false`
- `answerer-docker-namespace.txt`
  - SHA-256: `ef9612b1e18d9c25f231a1d449d50d92119a72d9e0c4aeffd1e39f638a5770ad`
  - `container_ip`: `172.18.0.2`
  - `webrtc_udp_addrs`: `172.18.0.2:0`
  - namespace: `net:[4026532274]`
- `offerer-docker-namespace.txt`
  - SHA-256: `0e9a1833952a3d5e413e7358055c9d8398d3a7f3ef8070ca22e0471ad11a0245`
  - `container_ip`: `172.18.0.3`
  - `webrtc_udp_addrs`: `172.18.0.3:0`
  - namespace: `net:[4026532339]`
- `runner-build-namespace.txt`
  - SHA-256: `9818befabb16ed4f08d345cdc1430a42a88d11ae8fbf2a9a594fff61c40579b9`
  - runner namespace: `net:[4026531833]`

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

PR namespace verification:

```bash
gh run view 27789088226 --repo joaojhgs/discrypt --json status,conclusion,jobs,url,headSha,attempt
gh run download 27789088226 --repo joaojhgs/discrypt -D /tmp/per27-gha-namespace-pass
jq '{status,adapter,role,room,endpoint,evidence:{direct_path_ready:.evidence.direct_path_ready,data_channel_open:.evidence.data_channel_open,p2p_datachannel_open:.evidence.p2p_datachannel_open,bidirectional_text_control:.evidence.bidirectional_text_control,provider_application_relay_used:.evidence.provider_application_relay_used,received_frame_count:.evidence.received_frame_count,provider_boundary:.provider_boundary.application_payload_relay_used}}' \
  /tmp/per27-gha-namespace-pass/per27-public-mqtt-namespace-27789088226-1/split-machine-mqtt-offerer.json \
  /tmp/per27-gha-namespace-pass/per27-public-mqtt-namespace-27789088226-1/split-machine-mqtt-answerer.json
sha256sum \
  /tmp/per27-gha-namespace-pass/per27-public-mqtt-namespace-27789088226-1/split-machine-mqtt-offerer.json \
  /tmp/per27-gha-namespace-pass/per27-public-mqtt-namespace-27789088226-1/split-machine-mqtt-answerer.json \
  /tmp/per27-gha-namespace-pass/per27-public-mqtt-namespace-27789088226-1/*namespace.txt
```

Result: PR proof job passed. Both JSON artifacts reported `status: passed`,
`direct_path_ready: true`, `data_channel_open: true`, and
`provider_application_relay_used: false`; the offerer reported
`bidirectional_text_control: true`, and the answerer reported
`received_frame_count: 2`.

## Historical Same-Host Artifact

Before the PR namespace proof existed, a same-host local role-split transport
proof passed under:

`target/e2e/per-27-public-mqtt-two-machine-text-e2e-20260618T034157Z`

This older evidence is retained only as supporting context. It is not the
merge-readiness proof because it used two local processes in the same host
namespace.

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

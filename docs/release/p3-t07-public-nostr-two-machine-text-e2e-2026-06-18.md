# P3-T07 Public Nostr Text/Control E2E Evidence - 2026-06-18

Issue: PER-28 / P3-T07.

## Result

Status: local same-host role-split Nostr proof passed; PR namespace proof is
prepared and must be used as the split-runtime merge-readiness artifact after
the branch is pushed.

This proof uses `crates/transport/examples/split_machine_p2p.rs` with
`--adapter nostr`. Nostr remains the public signaling/rendezvous provider. The
example records sealed WebRTC negotiation boundary evidence while sending opaque
text/control and media-shaped frames only over the WebRTC DataChannel.

## Fresh Artifacts

Local artifact directory:

`target/e2e/per-28-public-nostr-two-machine-text-e2e-20260618T2342Z`

Expected PR artifact name:

`per28-public-nostr-namespace-<run_id>-<run_attempt>`

Expected files:

- `split-machine-nostr-offerer.json`
  - SHA-256: `0d95dc09b2022c583ca62166b55a6ba1da828be967448a053e82e466a4aad5eb`
  - `status`: `passed`
  - `adapter`: `nostr`
  - `room`: `discrypt-per-28-nostr-local-20260618T2342Z`
  - `endpoint`: `wss://nos.lol`
  - `release_boundary.issue`: `PER-28 / P3-T07`
  - `evidence.direct_path_ready`: `true`
  - `evidence.data_channel_open`: `true`
  - `evidence.p2p_datachannel_open`: `true`
  - `evidence.bidirectional_text_control`: `true`
  - `evidence.provider_application_relay_used`: `false`
  - `provider_boundary.application_payload_relay_used`: `false`
- `split-machine-nostr-answerer.json`
  - SHA-256: `3bc600346130a9055ddaf41833d5951b4908b7163ed54b7fd4a2d8a7ac5807fa`
  - `status`: `passed`
  - `adapter`: `nostr`
  - `room`: `discrypt-per-28-nostr-local-20260618T2342Z`
  - `endpoint`: `wss://nos.lol`
  - `release_boundary.issue`: `PER-28 / P3-T07`
  - `evidence.direct_path_ready`: `true`
  - `evidence.data_channel_open`: `true`
  - `evidence.p2p_datachannel_open`: `true`
  - `evidence.received_frame_count`: `2`
  - `evidence.received_opaque_bytes`: `412`
  - `evidence.provider_application_relay_used`: `false`
  - `provider_boundary.application_payload_relay_used`: `false`
- `answerer-docker-namespace.txt`
- `offerer-docker-namespace.txt`
- `runner-build-namespace.txt`

The artifacts include `release_boundary` and `provider_boundary` objects. These
state that Nostr is signaling/rendezvous only, application payload relay is not
allowed or used, provider-visible material is limited to endpoint label, derived
hashed rendezvous topic, and sealed WebRTC negotiation envelopes, and message
plaintext, opaque app frame bytes, receipts, MLS secrets, and SFrame/content
keys are not provider-visible.

## Commands Run

Planned formatting:

```bash
RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check
```

Result: not available in this runtime because `cargo`/`rustup` are not on PATH.
Equivalent cached-toolchain command passed:

```bash
PATH=/tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin:$PATH \
  /tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/cargo-fmt -- --check
```

Planned example build check:

```bash
cargo check -p discrypt-transport --features nostr-adapter --example split_machine_p2p
```

Result: passed with the cached toolchain:

```bash
CARGO_HOME=/tmp/discrypt-cargo-home \
CARGO_TARGET_DIR=target/per-28-cargo-check \
PATH=/tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin:$PATH \
  /tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/cargo check \
    -p discrypt-transport \
    --features nostr-adapter \
    --example split_machine_p2p
```

Planned answerer process:

```bash
CARGO_HOME=/tmp/discrypt-cargo-home \
CARGO_TARGET_DIR=target/per-28-cargo-check \
PATH=/tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin:$PATH \
  /tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/cargo run -q \
    -p discrypt-transport \
    --features nostr-adapter \
    --example split_machine_p2p -- \
    --adapter nostr \
    --role answerer \
    --room discrypt-per-28-nostr-local-20260618T2342Z \
    --endpoint wss://nos.lol \
    --artifact-dir target/e2e/per-28-public-nostr-two-machine-text-e2e-20260618T2342Z \
    --answerer-hold-secs 45 \
    --receipt-timeout-secs 45
```

Result: passed and wrote `split-machine-nostr-answerer.json`.

Planned offerer process:

```bash
CARGO_HOME=/tmp/discrypt-cargo-home \
CARGO_TARGET_DIR=target/per-28-cargo-check \
PATH=/tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin:$PATH \
  /tmp/discrypt-rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/cargo run -q \
    -p discrypt-transport \
    --features nostr-adapter \
    --example split_machine_p2p -- \
    --adapter nostr \
    --role offerer \
    --room discrypt-per-28-nostr-local-20260618T2342Z \
    --endpoint wss://nos.lol \
    --artifact-dir target/e2e/per-28-public-nostr-two-machine-text-e2e-20260618T2342Z \
    --receipt-timeout-secs 45
```

Result: passed and wrote `split-machine-nostr-offerer.json`.

Artifact validation:

```bash
jq '{status,adapter,role,room,endpoint,release_issue:.release_boundary.issue,evidence:{direct_path_ready:.evidence.direct_path_ready,data_channel_open:.evidence.data_channel_open,p2p_datachannel_open:.evidence.p2p_datachannel_open,bidirectional_text_control:.evidence.bidirectional_text_control,provider_application_relay_used:.evidence.provider_application_relay_used,received_frame_count:.evidence.received_frame_count,received_opaque_bytes:.evidence.received_opaque_bytes},provider_boundary:{application_payload_relay_used:.provider_boundary.application_payload_relay_used,provider_role:.provider_boundary.provider_role}}' \
  target/e2e/per-28-public-nostr-two-machine-text-e2e-20260618T2342Z/split-machine-nostr-offerer.json \
  target/e2e/per-28-public-nostr-two-machine-text-e2e-20260618T2342Z/split-machine-nostr-answerer.json
sha256sum \
  target/e2e/per-28-public-nostr-two-machine-text-e2e-20260618T2342Z/split-machine-nostr-offerer.json \
  target/e2e/per-28-public-nostr-two-machine-text-e2e-20260618T2342Z/split-machine-nostr-answerer.json
```

Result: passed; both artifacts reported `status: passed`, `direct_path_ready:
true`, `data_channel_open: true`, `p2p_datachannel_open: true`, and
`provider_application_relay_used: false`. The offerer reported
`bidirectional_text_control: true`; the answerer reported
`received_frame_count: 2` and `received_opaque_bytes: 412`.

Planned PR namespace verification:

```bash
gh run view <run_id> --repo joaojhgs/discrypt --json status,conclusion,jobs,url,headSha,attempt
gh run download <run_id> --repo joaojhgs/discrypt -D /tmp/per28-gha-namespace-latest
jq '{status,adapter,role,room,endpoint,evidence:{direct_path_ready:.evidence.direct_path_ready,data_channel_open:.evidence.data_channel_open,p2p_datachannel_open:.evidence.p2p_datachannel_open,bidirectional_text_control:.evidence.bidirectional_text_control,provider_application_relay_used:.evidence.provider_application_relay_used,received_frame_count:.evidence.received_frame_count,provider_boundary:.provider_boundary.application_payload_relay_used}}' \
  /tmp/per28-gha-namespace-latest/per28-public-nostr-namespace-<run_id>-<attempt>/split-machine-nostr-offerer.json \
  /tmp/per28-gha-namespace-latest/per28-public-nostr-namespace-<run_id>-<attempt>/split-machine-nostr-answerer.json
sha256sum \
  /tmp/per28-gha-namespace-latest/per28-public-nostr-namespace-<run_id>-<attempt>/split-machine-nostr-offerer.json \
  /tmp/per28-gha-namespace-latest/per28-public-nostr-namespace-<run_id>-<attempt>/split-machine-nostr-answerer.json \
  /tmp/per28-gha-namespace-latest/per28-public-nostr-namespace-<run_id>-<attempt>/*namespace.txt
```

Result: pending until the PR branch is pushed and GitHub Actions runs
`PER-28 public Nostr namespace proof`.

## Historical Same-Host Artifact

The local role-split transport proof passed under:

`target/e2e/per-28-public-nostr-two-machine-text-e2e-20260618T2342Z`

This evidence is retained only as supporting context. It is not the
merge-readiness proof because it used two local processes in the same host
namespace. The PR-only Docker namespace proof in `.github/workflows/ci.yml`
exists to produce the required isolated-runtime artifact bundle.

## Non-Claims

- This evidence is not production-ready evidence for the full app.
- This evidence is not a two-installed-Tauri-GUI proof.
- This evidence is not physical microphone/speaker audio proof.
- This evidence does not prove OpenMLS invite/admission. The transport example
  derives a scoped DM-style rendezvous and proves the WebRTC text/control route;
  a separate app-flow artifact is still required for create/join/admit semantics.
- This evidence does not prove TURN relay-only behavior or future peer overlay
  behavior.

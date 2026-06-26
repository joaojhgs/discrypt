# PER-100 Public MQTT Split-Machine Matrix

## Scope
- Issue: PER-100 / P12-T05.
- Branch: `multica/P12-T05-public-mqtt-split-machine-matrix`.
- Primary example: `apps/desktop/src-tauri/examples/g009_split_machine_app_flow.rs`.
- Evidence level: local MQTT-labeled app-flow harness evidence in this runtime, plus a ready-to-run local+SSH promotion matrix. This is not production-ready evidence.

## Matrix Result

| Row | Status | Evidence | Boundary |
| --- | --- | --- | --- |
| Local MQTT app-flow substitute | Passed in this runtime | `target/per100-public-mqtt-split-machine-matrix/local-pair.json`, `local-pair-owner.json`, and `local-pair-joiner.json` | Harness-only two isolated app-state files; not split-machine proof |
| Local host + SSH remote public MQTT promotion | Blocked until an SSH target is configured | Requires copied owner/joiner artifacts from distinct hosts | Split-machine evidence only after both peers run on distinct hosts with retained route/provider artifacts |
| Provider-visible MQTT boundary | Covered by G009 artifact fields and prior PER-27 transport proof shape | `provider_relay_boundary.provider_application_relay_used=false`; MQTT endpoint label only | MQTT is signaling/rendezvous only, not application relay |

## Fresh Local Artifacts

Command result: passed.

Artifacts:
- `target/per100-public-mqtt-split-machine-matrix/local-pair.json`
  - SHA-256: `4024d6569259cbecff52011d0f21d191e5a4e2707a4b2bbd4e895a3071648d53`
  - `status`: `passed`
  - `task_id`: `PER-100`
  - `phase_task_id`: `P12-T05`
  - `adapter`: `mqtt`
  - `endpoint`: `mqtts://broker.emqx.io:8883`
  - `provider_application_relay_used`: `false`
- `target/per100-public-mqtt-split-machine-matrix/local-pair-owner.json`
  - SHA-256: `fcbc5c04f3f84322823a5e25077b070c86f4a75e051abca3840a5169ca01386e`
  - `task_id`: `PER-100`
  - `phase_task_id`: `P12-T05`
  - `manual_admission.approved`: `true`
  - `protected_text.owner_to_joiner.response_kind`: `receipt`
  - `protected_text.joiner_to_owner.response_kind`: `receipt`
  - `presence.backend_route_gated_ttl.frame_kind`: `group_presence_heartbeat`
  - `provider_relay_boundary.provider_application_relay_used`: `false`
  - `voice.proof_level`: `local_native_capture_boundary`
- `target/per100-public-mqtt-split-machine-matrix/local-pair-joiner.json`
  - SHA-256: `f948d9e0fa742faf782a071ed892a438eacf81ed025ea56e726333d4adecf7a2`
  - `task_id`: `PER-100`
  - `phase_task_id`: `P12-T05`
  - `manual_admission.pre_approval_send_error`: `admission_pending`
  - `manual_admission.welcome_delivery.frame_kind`: `open_mls_admission_welcome`
  - `protected_text.received_owner_text`: `true`
  - `staff_role_seen`: `true`
  - `revoked_role_seen`: `true`
  - `revoked_send_error`: `openmls_group_state_missing`
  - `provider_relay_boundary.provider_application_relay_used`: `false`
  - `voice.proof_level`: `local_native_capture_boundary`

Expected negative probe logs were emitted during the run:
- `admission_pending` before the OpenMLS Welcome was received.
- `openmls_group_state_missing` after revoke removed the joiner's OpenMLS send state.

## Required Local Substitute Command

```bash
RUSTUP_TOOLCHAIN=1.89.0 cargo build \
  --manifest-path apps/desktop/src-tauri/Cargo.toml \
  --features harness \
  --example g009_split_machine_app_flow

XDG_DATA_HOME=/tmp/discrypt-per100-local-pair-xdg \
  target/debug/examples/g009_split_machine_app_flow \
  --role local-pair \
  --artifact target/per100-public-mqtt-split-machine-matrix/local-pair.json \
  --adapter mqtt \
  --endpoint mqtts://broker.emqx.io:8883 \
  --admission-mode manual \
  --timeout-secs 20 \
  --task-id PER-100 \
  --phase-task-id P12-T05
```

Expected artifacts:
- `target/per100-public-mqtt-split-machine-matrix/local-pair.json`
- `target/per100-public-mqtt-split-machine-matrix/local-pair-owner.json`
- `target/per100-public-mqtt-split-machine-matrix/local-pair-joiner.json`

Expected fields:
- `task_id: PER-100`
- `phase_task_id: P12-T05`
- `provider_application_relay_used: false`
- `provider_relay_boundary.provider_application_relay_used: false`
- manual approval before admitted protected text
- OpenMLS handle/member role evidence after Welcome/add
- route-gated presence TTL
- protected text/control evidence
- voice proof classification that does not imply remote media unless remote media was observed

## SSH Promotion Commands

Set:

```bash
export DISCRYPT_G009_SSH_TARGET=<user@host>
export DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883
export DISCRYPT_PER100_RUN_ID=per100-$(date -u +%Y%m%dT%H%M%SZ)
export DISCRYPT_PER100_ARTIFACT_DIR=target/per100-public-mqtt-split-machine-matrix/${DISCRYPT_PER100_RUN_ID}
```

Build locally and on the remote host from the same commit:

```bash
RUSTUP_TOOLCHAIN=1.89.0 cargo build \
  --manifest-path apps/desktop/src-tauri/Cargo.toml \
  --features mqtt-adapter \
  --example g009_split_machine_app_flow

ssh "$DISCRYPT_G009_SSH_TARGET" \
  'cd /path/to/discrypt && RUSTUP_TOOLCHAIN=1.89.0 cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --features mqtt-adapter --example g009_split_machine_app_flow'
```

Prepare the owner locally:

```bash
XDG_DATA_HOME=/tmp/discrypt-per100-owner-xdg \
  target/debug/examples/g009_split_machine_app_flow \
  --role prepare-owner \
  --artifact "${DISCRYPT_PER100_ARTIFACT_DIR}/prepare-owner.json" \
  --adapter mqtt \
  --endpoint "$DISCRYPT_PUBLIC_MQTT_ENDPOINT" \
  --admission-mode manual \
  --task-id PER-100 \
  --phase-task-id P12-T05
```

Run joiner remotely and owner locally with isolated state paths, then copy the remote artifact back:

```bash
ssh "$DISCRYPT_G009_SSH_TARGET" \
  "cd /path/to/discrypt && XDG_DATA_HOME=/tmp/discrypt-per100-joiner-xdg target/debug/examples/g009_split_machine_app_flow --role joiner --artifact ${DISCRYPT_PER100_ARTIFACT_DIR}/joiner.json --adapter mqtt --endpoint ${DISCRYPT_PUBLIC_MQTT_ENDPOINT} --admission-mode manual --timeout-secs 120 --task-id PER-100 --phase-task-id P12-T05"

XDG_DATA_HOME=/tmp/discrypt-per100-owner-xdg \
  target/debug/examples/g009_split_machine_app_flow \
  --role owner \
  --artifact "${DISCRYPT_PER100_ARTIFACT_DIR}/owner.json" \
  --adapter mqtt \
  --endpoint "$DISCRYPT_PUBLIC_MQTT_ENDPOINT" \
  --admission-mode manual \
  --timeout-secs 120 \
  --task-id PER-100 \
  --phase-task-id P12-T05

scp "$DISCRYPT_G009_SSH_TARGET:/path/to/discrypt/${DISCRYPT_PER100_ARTIFACT_DIR}/joiner.json" \
  "${DISCRYPT_PER100_ARTIFACT_DIR}/remote-joiner.json"
```

Promotion requires both artifacts to show:
- `task_id=PER-100` and `phase_task_id=P12-T05`
- matching branch/commit and MQTT endpoint label
- authorized OpenMLS admission before protected text
- direct or configured TURN-backed WebRTC route evidence for text/control
- route-gated presence TTL where presence is claimed
- `provider_application_relay_used=false`
- no raw message body, media, MLS secret, SDP body, ICE password, TURN secret, or invite secret in logs/artifacts

## Current SSH Status

This runtime has no `DISCRYPT_G009_SSH_TARGET`, `DISCRYPT_PER100_SSH_TARGET`, or equivalent remote host variable configured. The local substitute row can be generated here, but the local+SSH row remains blocked until QA or a configured runner supplies a reachable remote checkout at the same commit. PER-100 is therefore asking the architect to accept local substitute evidence only unless a configured split-machine runner is supplied.

## Non-Claims
- This report does not claim production readiness for Discrypt.
- Local-pair artifacts are not split-machine proof.
- MQTT is not an application relay, media relay, or overlay relay.
- The local substitute row does not prove installed package behavior, package reinstall behavior, public MQTT service reliability, configured TURN closure, or remote voice media playback.

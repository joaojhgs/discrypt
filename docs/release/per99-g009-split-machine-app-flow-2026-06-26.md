# PER-99 G009 Split-Machine App-Flow Evidence

## Scope
- Issue: PER-99 / P12-T04.
- Primary example: `apps/desktop/src-tauri/examples/g009_split_machine_app_flow.rs`.
- Branch: `multica/P12-T04-split-machine-app-flow-hardening`.
- Evidence level: local harness/build evidence plus local prepare-owner and two-profile local-pair artifacts. This is not production split-machine proof.

## Hardened Contract
- G009 artifacts now use schema `discrypt.g009.split_machine_app_flow.v2`.
- Manual approval is the default admission mode; `--admission-mode automatic` remains available for compatibility.
- Owner evidence records manual pending-request approval, Welcome/decision pump, protected owner text, staff promotion, revoke, presence, voice proof classification, and no provider application relay fallback.
- Joiner evidence records pending-before-approval state, pre-approval send denial, post-approval role state, protected joiner text, received owner text, staff promotion, revoked state, revoked send denial, presence, voice proof classification, and no provider application relay fallback.
- Voice proof is classified as `remote_media_transport`, `local_native_capture_boundary`, `voice_session_without_media_capture`, or `no_voice_session` so local capture cannot be mistaken for remote media proof.
- Provider application relay fallback is explicitly recorded as disabled; allowed delivery remains direct WebRTC DataChannel or configured TURN-backed WebRTC DataChannel.
- The default `owner` and `joiner` roles remain live-provider/split-machine roles. They require a configured provider/WebRTC runtime and a peer online in the same scope; default builds do not synthesize runtime success.
- `--role local-pair` is a harness-only fallback for runtimes without SSH/provider setup. It drives two isolated app-state files through backend text/control frame handlers and writes separate owner/joiner artifacts without claiming public provider readiness.

## Local Verification
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check` passed.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml g007_manual_admission_approval_persists_openmls_join_without_auto_approving_old_requests --lib -- --test-threads=1` passed.
- `RUSTUP_TOOLCHAIN=1.89.0 cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --example g009_split_machine_app_flow` passed.
- Harness local-pair artifact command:
  - Build: `RUSTUP_TOOLCHAIN=1.89.0 cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --features harness --example g009_split_machine_app_flow`
  - Run: `XDG_DATA_HOME=/tmp/discrypt-per99-local-pair-xdg target/debug/examples/g009_split_machine_app_flow --role local-pair --artifact target/per99-g009-split-machine-app-flow/local-pair.json --adapter nostr --endpoint wss://nos.lol --admission-mode manual --timeout-secs 20`
  - Summary artifact: `target/per99-g009-split-machine-app-flow/local-pair.json`
  - Owner artifact: `target/per99-g009-split-machine-app-flow/local-pair-owner.json`
  - Joiner artifact: `target/per99-g009-split-machine-app-flow/local-pair-joiner.json`
  - Expected negative probe logs: `admission_pending` before Welcome and `openmls_group_state_missing` after revoke/OpenMLS removal.
- Local prepare-owner artifact:
  - Command: `XDG_DATA_HOME=/tmp/discrypt-per99-g009-xdg target/debug/examples/g009_split_machine_app_flow --role prepare-owner --artifact target/per99-g009-split-machine-app-flow/prepare-owner.json --adapter nostr --endpoint wss://nos.lol --admission-mode manual`
  - Artifact: `target/per99-g009-split-machine-app-flow/prepare-owner.json`
  - State sidecars: `/tmp/discrypt-per99-g009-xdg/discrypt/app-state.discrypt-store` and `/tmp/discrypt-per99-g009-xdg/discrypt/app-state.discrypt-store.openmls.sqlite`

## Split-Machine / SSH Status
- No remote SSH target or `DISCRYPT_G009_*` split-machine host variables were configured in this runtime.
- A first local run with only `DISCRYPT_APP_STATE_PATH` failed with `state_save_failed` because the production-storage example correctly ignored that local-dev override outside harness/test gates and attempted to use an unwritable production path.
- The successful local artifact used `XDG_DATA_HOME=/tmp/discrypt-per99-g009-xdg` to keep the production-domain app store writable inside this container.
- A freshly prepared owner state cannot complete `--role owner` alone in a default build, because backend text/control runtime derivation intentionally requires either a live provider/WebRTC peer route or an admitted remote peer. Use `--role local-pair` with `--features harness` for isolated local proof, or run `owner` and `joiner` on configured split-machine peers with provider features and endpoints available.
- QA must run the owner/joiner roles on two configured machines to promote this from local-pair harness evidence to split-machine evidence.

## Non-Claims
- This evidence does not prove public Nostr/MQTT provider reliability, remote voice media, installed package behavior, package reinstall behavior, or production readiness.
- The prepare-owner artifact contains a live local invite generated for the isolated local run; do not treat it as reusable release material.

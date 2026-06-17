# Public signaling production status

_Last updated: 2026-05-30T20:45Z_

## Checkpoint 2026-05-30T20:45Z — OMC session evidence sweep

### What passed in this session

**Tauri dev build fix:**
- `tauri.conf.json` `build.features` now includes `tauri-runtime`; `cargo tauri dev` DevCommand is `cargo run --no-default-features --features tauri-runtime`
- `cargo check -p discrypt-desktop --features tauri-runtime` — passed
- `cargo fmt --all --check` — passed

**Playwright two-profile UI E2E (local-dev browser, no real transport):**
- `cd apps/ui && VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1 npx playwright test tests/e2e/two-profile-flow.spec.ts --workers=1` — 2/2 passed
- Covers: account creation, DM local send/receive, DM invite create/accept, reciprocal runtime peers, group create/join, group channel local text, voice join/mute/leave/speaking indicators

**Voice session tests:**
- `cargo test -q -p discrypt-desktop voice_join_mute_volume_leave_flow_does_not_clear_state` — passed
- `cargo test -q -p discrypt-core voice_session_state_persists_across_restart` — passed

**Relay overlay tests:**
- `cargo test -q -p discrypt-relay-overlay` — 34/34 passed (gossip, store-forward, integrity, topology, capability, ranking, failover, manager, redelivery)

**Public provider DM receipt (env-gated, requires real providers):**
- `DISCRYPT_DESKTOP_PUBLIC_MQTT_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 timeout 240s cargo test -q -p discrypt-desktop --features mqtt-adapter public_mqtt_live_runtime_pair_pump_persists_peer_receipt_when_enabled` — passed
- `DISCRYPT_DESKTOP_PUBLIC_NOSTR_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol timeout 240s cargo test -q -p discrypt-desktop --features nostr-adapter public_nostr_live_runtime_pair_pump_persists_peer_receipt_when_enabled` — passed

**Public provider group receipt (env-gated, requires real providers):**
- `DISCRYPT_DESKTOP_PUBLIC_MQTT_GROUP_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 timeout 240s cargo test -q -p discrypt-desktop --features mqtt-adapter public_mqtt_group_live_runtime_pair_pump_persists_peer_receipt_when_enabled` — passed (7.85s)
- `DISCRYPT_DESKTOP_PUBLIC_NOSTR_GROUP_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol timeout 240s cargo test -q -p discrypt-desktop --features nostr-adapter public_nostr_group_live_runtime_pair_pump_persists_peer_receipt_when_enabled` — passed (11.02s)

**Public transport role-split text runtime (env-gated):**
- `DISCRYPT_PUBLIC_MQTT_ROLE_SPLIT_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 timeout 240s cargo test -q -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_role_split_text_runtime_roundtrip` — passed

### Still not production-complete in this session

The following require external infrastructure not available in this environment:

- **Real hardware audio pipeline**: microphone capture → Opus encode → WebRTC media transport → Opus decode → speaker playback — no audio hardware available for E2E proof
- **Credentialed TURN relay-only**: TURN server credentials not configured; fail-closed gate exists and is tested
- **IPFS public rendezvous**: requires externally reachable Discrypt topic-peer `/p2p/<peer-id>` multiaddr; direct-topic-peer local roundtrip is proven
- **Deployed QUIC rendezvous**: requires deployed HTTPS/WSS sibling rendezvous endpoint; local loopback binary roundtrip is proven
- **Full installed Tauri two-window UI E2E**: requires two real GUI processes with display; backend same-process proofs are the current evidence

---

## Executive status

Discrypt is **not production-complete** for the full serverless P2P encrypted app target yet. This update has real MQTT, Nostr, and IPFS/libp2p signaling paths at the Rust transport boundary, but it is still not a complete installed-app proof:

- **MQTT public signaling: implemented behind `mqtt-adapter` and latest reruns passed against a real public broker after the adapter began waiting for broker subscription acknowledgements before publishing.**
- **Nostr signaling: real relay client is wired behind `nostr-adapter` and verified against a public relay.**
- **IPFS/libp2p PubSub signaling: real rust-libp2p gossipsub client is wired behind `ipfs-pubsub-adapter` and verified with a local two-node transport roundtrip; default public bootstrap is now disabled while the libp2p/Hickory DNS stack remains audit-blocked, so production IPFS profiles must use explicit direct topic-peer `/ip4` or `/ip6` multiaddrs with `/p2p/<peer-id>` until DNS/topic-peer discovery is remediated and real public peers are supplied.**
- **Separate Rust rendezvous service adapter: `discrypt-quic-rendezvous-adapter` now wires the content-blind sibling `discrypt-signaling` service API over validated HTTPS/WSS-or-loopback HTTP endpoints, enforces signed endpoint fingerprints from policy/invites for production/self-hosted endpoints, validates `/healthz` status/service/public-base/protocol/max-body/rate-limit metadata plus signed service identity, accepted ALPN, expiry, rotation, and endpoint allowlist commitments before production/self-hosted connects, and proves local roundtrip when the sibling binary is available; native `quic://` transport remains explicitly reserved until a native QUIC client is audited.**
- **Provider-signaled WebRTC data-channel proof: MQTT and Nostr are now green in live public-provider tests when using public STUN and a real network UDP bind; relay-only TURN is now wired as an opt-in public release gate that requires real TURN credentials and reports relay candidate evidence before claiming fallback.**
- **Full app-level two-Tauri-instance DM/group text + voice E2E over those adapters: not done.** Current proof reaches the Rust transport signaling adapter layer, sealed WebRTC offer/answer exchange, a real WebRTC DataChannel frame over public MQTT/Nostr rendezvous, an opt-in Tauri `send_message(..., transport_proof=true)` path that sends an opaque message-derived frame through public MQTT and Nostr WebRTC diagnostics, a Tauri command path that verifies signed peer delivery receipts against stored encrypted message envelopes, and env-gated same-process two-profile MQTT and Nostr proofs that carry Alice's serialized text/control envelope frame over the provider-signaled DataChannel, invoke Bob's receiver frame handler **only after the answerer receives that frame over the DataChannel** to verify/persist the envelope and generate the signed receipt frame, return that receipt frame, and only then let Alice apply `peer_receipt`; a local two-state-file regression now reloads Bob and Alice from disk after the frame handling to prove those receipt state transitions persist, and the reusable app-service transport-pump drives the `TextControlDataTransport` send/recv trait against Bob's receiver-backed state to persist Bob's envelope receipt and Alice's final peer receipt through the same outbox handoff path. IPFS remains narrowed to an explicit direct-topic-peer gate with a typed blocker when DNS/bootstrap or bare dialable endpoints are attempted. Voice UI speaking state can now be updated from real local microphone level evidence collected by the UI and persisted through the Tauri backend, but that remains local VAD/UX evidence only. It is still not the complete persistent two-installed-profile peer receipt transport or voice/media-plane proof.

## What was implemented now

### MQTT real adapter

Files:

- `crates/transport/src/provider_adapters.rs`
- `crates/transport/src/lib.rs`
- `crates/transport/Cargo.toml`
- `crates/transport/tests/public_signaling_e2e.rs`

Behavior:

- Adds `MqttProviderAdapter` behind Cargo feature `mqtt-adapter`.
- Uses `rumqttc` with TLS-capable MQTT URLs.
- Accepts validated `SignalingAdapterProfile` values with `SignalingAdapterKind::Mqtt`.
- Joins a pre-derived `RendezvousCapability` and creates provider-visible topics under:
  - `discrypt/v1/rendezvous/{hashed-topic}/presence`
  - `discrypt/v1/rendezvous/{hashed-topic}/signal/{peer-id}`
  - `discrypt/v1/rendezvous/{hashed-topic}/control`
- Publishes only sealed/opaque payload envelopes:
  - encrypted presence bytes
  - `SealedWebRtcNegotiationPayload` for offer/answer/candidate signaling
  - opaque room control bytes
- Keeps the public broker away from raw SDP, ICE credentials, display names, group names, invite secrets, message plaintext, and audio plaintext.
- Marks MQTT boundary readiness as `implementation_available` only when compiled with `mqtt-adapter`.
- Leaves the generic `FeatureGatedProviderAdapter` fail-closed; production code should instantiate `MqttProviderAdapter` for MQTT.
- **UI state integration:** command state now surfaces transport/join/voice status cards from command state and keeps route/media claims policy-only when proof is absent.

### Nostr, IPFS, and separate rendezvous service adapters

Files:

- `docs/adapters/nostr-adapter-readiness.md`
- `docs/adapters/ipfs-pubsub-adapter-readiness.md`
- `docs/adapters/quic-rendezvous-adapter-readiness.md`

Commands:

```bash
cargo test -q -p discrypt-transport --features nostr-adapter \
  nostr_adapter_feature_is_selectable_with_real_relay_client
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_adapter_feature_is_selectable_with_real_libp2p_client

cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  ipfs_pubsub_local_two_peer_presence_and_signal_roundtrip -- --nocapture
cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter \
  quic_rendezvous_feature_gate_is_selectable_but_rejects_reserved_native_quic_scheme

cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter \
  discrypt_rendezvous_sibling_service_roundtrip_when_binary_is_available -- --nocapture
```

Result: Nostr is selectable when feature-gated and backed by `nostr-sdk`; IPFS/libp2p is selectable when feature-gated and backed by rust-libp2p gossipsub; the separate Discrypt rendezvous adapter is selectable when feature-gated and uses the sibling service HTTP API for sealed presence/signal roundtrips. The adapter still rejects native `quic://` endpoints because the sibling service ADR reserves them until a native QUIC client is implemented and audited.

- Nostr profile handling now preserves every configured relay endpoint when joining a room and publishes/subscribes against the configured relay set instead of silently collapsing a profile to the first relay. The latest single-relay public WebRTC smoke still passes against `wss://nos.lol`; a degraded multi-relay public soak now proves fallback behavior with one intentionally invalid relay, and blocked-relay auth evidence now maps to typed `provider_auth_required`; reproducible public rate-limit evidence remains opportunistic.


### Tauri runtime adapter probe

Files:

- `crates/transport/src/provider_adapters.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/ui/src/commands.ts`
- `apps/ui/src/main.tsx`

Behavior:

- Adds `probe_provider_adapter_roundtrip(...)`, a reusable transport-layer probe that connects two local peers through the selected real provider adapter and verifies opaque presence plus sealed WebRTC-negotiation payload delivery only. Provider control/app-payload relay is disabled; text/control delivery evidence must come from an open WebRTC DataChannel route.
- Extends `start_signaling_session` with `adapter_probe=true` and optional `adapter_kind` so the Tauri backend can run the selected DM/group/invite signaling profile instead of only showing static readiness.
- Persists structured `adapter_probe_status`, `adapter_probe_detail`, and redacted probe evidence into transport diagnostics.
- Adds a UI "Probe adapter" action in the transport status panel.
- Adds a UI "Probe data channel" action and `start_signaling_session(..., data_channel_probe=true)`
  command path that reuses the Rust transport WebRTC probe through Tauri diagnostics. The transport status strip now exposes an explicit "Start text proof" action wired to `start_text_session(..., data_channel_probe=true)`, which can bind the same provider-signaled WebRTC DataChannel proof to the backend text session route state when both peers prove either an open direct STUN path or a configured TURN relay path with relay-candidate evidence. Tauri diagnostics now surface TURN readiness, configured TURN counts, and local/remote relay candidate counters; credentialed TURN can be supplied to local probes through the same `DISCRYPT_PUBLIC_TURN_*` environment variables used by the transport release gate. This proves provider-signaled WebRTC text/control transport for the selected per-scope policy, while still keeping installed-app remote persistence, background receiver-loop, and voice/media claims separate.
- Adds an opt-in message composer switch and Tauri `send_message(..., transport_proof=true)` path. When enabled, the backend derives an opaque ciphertext-labeled frame from the message command, sends it over the selected provider-signaled WebRTC DataChannel diagnostic, and marks the message `transport_probe_verified` only if that frame crosses the DataChannel. This still does **not** claim signed peer receipt, remote persistence, or voice/media delivery.
- Adds env-gated Tauri two-profile receipt proofs for MQTT and Nostr where Alice's serialized text/control envelope frame crosses the provider-signaled WebRTC DataChannel to the Bob-side transport peer, a transport answerer callback invokes Bob's `handle_text_control_frame` path only after DataChannel receipt to verify and persist the envelope, Bob then returns a signed receipt frame, that receipt returns over the same DataChannel as an opaque control frame, and Alice only applies `peer_receipt` through her receipt-frame handler after signature verification. The reusable app-service transport-pump also exercises the `TextControlDataTransport` send/recv trait with Bob's receiver handler behind the transport boundary, records DataChannel-style metrics, marks Alice's outbox handoff, and persists the returned receipt. These remain same-process harnesses, not persistent installed-app sessions.
- Uses public Nostr (`wss://relay.damus.io`) first and public MQTT (`mqtts://broker.emqx.io:8883`) second as zero-config default endpoint candidates when no `DISCRYPT_DEFAULT_*`/`VITE_DISCRYPT_DEFAULT_*` override is supplied; both the native backend and browser fallback omit IPFS and QUIC from generated default connectivity profiles unless explicit endpoints are configured because no production default pubsub rendezvous mesh or self-hosted endpoint has been accepted yet.
- Keeps route/media claims separate: a successful adapter probe proves provider rendezvous only; it does not mark ICE, data-channel, or voice media as connected.
- Adds a test-only Tauri app-service loader with explicit state-file override so two isolated local profiles can be exercised in one test process without the global command singleton collapsing them into one state file. This is harness groundwork for real two-profile E2E, not a production delivery claim.
- Adds UI controls for DM contact invite creation and acceptance in the invite panel, alongside group invite creation/joining. The local-dev two-browser Playwright flow now drives setup, local DM persistence, DM invite create/accept, group invite create/join, group text sends on both profiles, and voice join/mute/slider/leave controls while asserting no fabricated Bob/relay members appear. Voice join now samples the real browser microphone stream before stopping the permission probe and sends RMS/peak audio-level evidence to a Tauri `update_voice_activity` command, so the local speaking indicator is backend state derived from real capture levels rather than a mock participant flag. This is still a local UI/state-flow proof in the web fallback harness, not a real two-installed-Tauri-instance, provider-delivered remote-message, or transported remote-audio proof.

Verification:

```bash
cargo test -q -p discrypt-transport provider_adapter_roundtrip_probe_quic_fails_closed
cargo test -q -p discrypt-desktop signaling_adapter_probe_surfaces_runtime_blocker_without_route_claim
npm --prefix apps/ui run typecheck
cargo check -q -p discrypt-desktop --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter
cargo test -q -p discrypt-desktop probe -- --nocapture
DISCRYPT_DESKTOP_PUBLIC_MQTT_WEBRTC_E2E=1 \
DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 \
  cargo test -q -p discrypt-desktop --features mqtt-adapter \
  public_mqtt_data_channel_probe_reaches_tauri_diagnostics_when_enabled -- --nocapture
DISCRYPT_DESKTOP_PUBLIC_MQTT_MESSAGE_E2E=1 \
DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 \
  cargo test -q -p discrypt-desktop --features mqtt-adapter \
  public_mqtt_message_send_proves_provider_webrtc_transport_when_enabled -- --nocapture
DISCRYPT_DESKTOP_PUBLIC_NOSTR_MESSAGE_E2E=1 \
DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol \
  cargo test -q -p discrypt-desktop --features nostr-adapter \
  public_nostr_message_send_proves_provider_webrtc_transport_when_enabled -- --nocapture
DISCRYPT_DESKTOP_PUBLIC_MQTT_RECEIPT_E2E=1 \
DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 \
cargo test -q -p discrypt-desktop --features mqtt-adapter \
  public_mqtt_two_profile_receipt_crosses_provider_webrtc_when_enabled -- --nocapture
DISCRYPT_DESKTOP_PUBLIC_NOSTR_RECEIPT_E2E=1 \
DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol \
cargo test -q -p discrypt-desktop --features nostr-adapter \
  public_nostr_two_profile_receipt_crosses_provider_webrtc_when_enabled -- --nocapture
DISCRYPT_DESKTOP_PUBLIC_MQTT_TEXT_SESSION_E2E=1 \
DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 \
cargo test -q -p discrypt-desktop --features mqtt-adapter \
  public_mqtt_text_session_probe_marks_text_route_when_enabled -- --nocapture
DISCRYPT_DESKTOP_PUBLIC_NOSTR_TEXT_SESSION_E2E=1 \
DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol \
  cargo test -q -p discrypt-desktop --features nostr-adapter \
  public_nostr_text_session_probe_marks_text_route_when_enabled -- --nocapture
npm --prefix apps/ui run typecheck
```

### Public real-network tests

File:

- `crates/transport/tests/public_signaling_e2e.rs`

MQTT command:

```bash
DISCRYPT_PUBLIC_SIGNALING_E2E=1 \
  cargo test -q -p discrypt-transport --features mqtt-adapter \
  public_mqtt_two_peer_presence_and_signal_roundtrip -- --nocapture
```

MQTT status:

- Latest reruns passed against default public broker `mqtts://broker.emqx.io:8883` after the adapter started waiting for all broker `SUBACK`s before treating a joined room as ready.
- Prior failures on `broker.emqx.io` timed out waiting for peer delivery and are treated as a subscription-readiness race that this fix targets; `test.mosquitto.org` still has TLS certificate incompatibility and `broker.hivemq.com` still hit network timeout in this environment.

Nostr command:

```bash
DISCRYPT_PUBLIC_NOSTR_E2E=1 \
DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://relay.damus.io \
  cargo test -q -p discrypt-transport --features nostr-adapter \
  public_nostr_two_peer_presence_and_signal_roundtrip -- --nocapture
```

Nostr status:

- Latest rerun passed against `wss://relay.damus.io`.
- The test creates two independent transport sessions (`alice-device` and `bob-device`) on the same hashed DM rendezvous topic.
- It verifies opaque provider roundtrip only:
  1. Alice publishes sealed presence and Bob receives it.
  2. Alice sends a sealed WebRTC offer envelope to Bob and Bob receives it.
  3. Bob broadcasts sealed control and Alice receives it.

These are real public signaling proofs at the provider adapter boundary.

Public provider-signaled WebRTC data-channel commands:

```bash
DISCRYPT_PUBLIC_MQTT_WEBRTC_E2E=1 \
DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 \
  cargo test -q -p discrypt-transport --features mqtt-adapter \
  --test public_webrtc_datachannel_e2e \
  public_mqtt_signals_real_webrtc_datachannel_roundtrip -- --nocapture

DISCRYPT_PUBLIC_NOSTR_WEBRTC_E2E=1 \
DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol \
  cargo test -q -p discrypt-transport --features nostr-adapter \
  --test public_webrtc_datachannel_e2e \
  public_nostr_signals_real_webrtc_datachannel_roundtrip -- --nocapture

DISCRYPT_PUBLIC_MQTT_MEDIA_WEBRTC_E2E=1 \
DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 \
  cargo test -q -p discrypt-transport --features mqtt-adapter \
  --test public_webrtc_datachannel_e2e \
  public_mqtt_signals_real_webrtc_media_frame_roundtrip -- --nocapture

DISCRYPT_PUBLIC_NOSTR_MEDIA_WEBRTC_E2E=1 \
DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol \
  cargo test -q -p discrypt-transport --features nostr-adapter \
  --test public_webrtc_datachannel_e2e \
  public_nostr_signals_real_webrtc_media_frame_roundtrip -- --nocapture

DISCRYPT_PUBLIC_IPFS_WEBRTC_E2E=1 \
DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS=<direct-topic-peer-multiaddr,...> \
  cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  --test public_webrtc_datachannel_e2e \
  public_ipfs_signals_real_webrtc_datachannel_roundtrip -- --nocapture

DISCRYPT_PUBLIC_IPFS_MEDIA_WEBRTC_E2E=1 \
DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS=<direct-topic-peer-multiaddr,...> \
  cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter \
  --test public_webrtc_datachannel_e2e \
  public_ipfs_signals_real_webrtc_media_frame_roundtrip -- --nocapture

DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_WEBRTC_E2E=1 \
DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=https://... \
  cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter \
  --test public_webrtc_datachannel_e2e \
  public_quic_rendezvous_signals_real_webrtc_datachannel_roundtrip -- --nocapture

DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_MEDIA_WEBRTC_E2E=1 \
DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=https://... \
  cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter \
  --test public_webrtc_datachannel_e2e \
  public_quic_rendezvous_signals_real_webrtc_media_frame_roundtrip -- --nocapture
```

Public provider-signaled WebRTC data-channel status:

- Latest MQTT run passed against `mqtts://broker.emqx.io:8883`.
- Latest Nostr run passed against `wss://nos.lol`.
- `wss://relay.damus.io` was not counted as a failure of the WebRTC path in the latest rerun because it rejected the smoke with Nostr relay rate limiting (`rate-limited: you are noting too much`).
- The test uses `stun:stun.l.google.com:19302`, waits for completed local ICE SDP where possible, exchanges sealed offer/answer through the selected public provider, opens the WebRTC DataChannel, sends an opaque text/control frame, and returns an opaque receipt/control frame over the same channel.
- The previous `set remote answer failed: Disconnected(WriteNotify)` failure was traced to exercising public STUN while binding WebRTC UDP to `127.0.0.1:0`; the public-provider test now binds `0.0.0.0:0` so STUN and host candidate gathering can use the actual network interface.
- The same transport proof is exposed to Tauri as an explicit `data_channel_probe` diagnostic. It is not run automatically because public providers can rate-limit and the probe is network-dependent.
- The media payload transport gates `public_mqtt_signals_real_webrtc_media_frame_roundtrip`, `public_nostr_signals_real_webrtc_media_frame_roundtrip`, `public_ipfs_signals_real_webrtc_media_frame_roundtrip`, and `public_quic_rendezvous_signals_real_webrtc_media_frame_roundtrip` now exercise provider-signaled DataChannel routing with codec-like encrypted media payloads and return receipts as dedicated, opt-in production gates on the transport/media boundary. Latest Nostr media-frame gate passed against `wss://nos.lol`; IPFS and separate Discrypt rendezvous gates compile and skip cleanly until explicit direct topic peers or a staged rendezvous endpoint are supplied. This is not yet decoded remote audio transport; it proves encrypted-media-shaped payload carriage only.
- The message composer can now opt into the same backend proof per send. Latest live Tauri MQTT message-proof run passed with `DISCRYPT_DESKTOP_PUBLIC_MQTT_MESSAGE_E2E=1` against `mqtts://broker.emqx.io:8883`; latest live Tauri Nostr message-proof run passed with `DISCRYPT_DESKTOP_PUBLIC_NOSTR_MESSAGE_E2E=1` against `wss://nos.lol`. Both set `transport_probe_verified` and record frame plus return-receipt SHA-256 diagnostics. This is a command/backend bidirectional transport proof, not a signed remote peer receipt.
- Latest live Tauri MQTT receipt proof passed with `DISCRYPT_DESKTOP_PUBLIC_MQTT_RECEIPT_E2E=1` against `mqtts://broker.emqx.io:8883`; latest live Tauri Nostr receipt proof passed with `DISCRYPT_DESKTOP_PUBLIC_NOSTR_RECEIPT_E2E=1` against `wss://nos.lol`. Latest live runtime-pair pump proofs were also rerun successfully on 2026-05-30 with `DISCRYPT_DESKTOP_PUBLIC_MQTT_RUNTIME_PAIR_E2E=1` against `mqtts://broker.emqx.io:8883` and `DISCRYPT_DESKTOP_PUBLIC_NOSTR_RUNTIME_PAIR_E2E=1` against `wss://nos.lol`. In both, the response frame is generated by Bob's `handle_text_control_frame` receiver path from a transport answerer callback that runs only after the answerer receives Alice's opaque frame over the DataChannel, then applied by Alice's receipt-frame handler. Latest text-session route proof passed for both MQTT (`mqtts://broker.emqx.io:8883`) and Nostr (`wss://nos.lol`), marking the backend text session route as direct only after the provider-signaled DataChannel proof succeeds. The Tauri route binder now also records a TURN route when the probe reports TURN fallback readiness on both peers and exposes the configured TURN/relay-candidate counters in backend/UI diagnostics; the real credentialed TURN E2E run remains the open production evidence gate. It verifies both DataChannel directions and signed receipt semantics. A local transport-pump regression proves the reusable app-service pump can drive a pending signed frame through the `TextControlDataTransport` trait into Bob's receiver handler and back to Alice's receipt handler with durable outbox/receipt state; role-split attach now provides a backend-owned pending/attached runtime lifecycle, but two installed GUI processes are still unproven.


### Signed text delivery receipt boundary

Files:

- `crates/mls-delivery/src/lib.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/ui/src/commands.ts`

Behavior:

- `send_message` now stores a signed `TextMessageEnvelope` record for the opaque encrypted text/control frame that would be delivered to a peer.
- The Tauri command `receive_text_delivery_envelope` accepts a peer `TextMessageEnvelope`, verifies the sender signature and DM/group/channel delivery binding, persists a received-envelope timeline row, and returns a locally signed `TextDeliveryReceipt` plus recipient verifying key for transport back to the sender. Tampered envelopes are rejected without receipt generation.
- The Tauri command `handle_text_control_frame` accepts a typed text/control frame from a future DataChannel/session loop. Envelope frames are verified through `receive_text_delivery_envelope` and return a receipt response frame; receipt frames are verified through `apply_text_delivery_receipt` and update the sender timeline. A two-state-file regression now proves Bob's received-envelope/receipt row and Alice's final `peer_receipt` survive disk reload after those frame handlers run. The backend now has a reusable one-shot `TextControlDataTransport` pump that lists pending outbox frames, sends them over the app-facing transport trait, records the hash-guarded send handoff, receives response frames, verifies/applies signed receipts, emits metrics, and persists the final receipt. The pump is deliberately fail-closed when no runtime is attached, when no text session is active, or when an attached runtime's session id does not match the active text session; these cases surface typed command errors instead of silently marking delivery. This is the command/runtime boundary the persistent installed-app receiver loop should call. Native Tauri builds now start a backend-owned periodic pump loop instead of relying on a React foreground timer, but that loop still remains idle until a real matching `TextControlDataTransport` runtime is attached; two-process UI orchestration remains open.
- The Tauri command `apply_text_delivery_receipt` accepts a `TextDeliveryReceipt`, verifies it with `discrypt-mls-delivery` against the stored envelope, message id, group/DM/channel delivery group id, recipient verifying key, and envelope ciphertext hash, then marks the message as `peer_receipt` only after verification succeeds.
- Tampered receipts are rejected with `receipt_verification_failed` and do not upgrade the message state.
- The UI command surface has typed envelope/receipt/text-control-frame/receipt-view models plus native-only `receiveTextDeliveryEnvelope(...)`, `handleTextControlFrame(...)`, and `applyTextDeliveryReceipt(...)` bindings; browser fallback stays honest and reports that signed envelope/receipt verification requires the Rust/Tauri backend.
- This is the signed state-transition boundary needed for remote delivery honesty. A two-profile backend test proves a distinct Bob profile identity can verify Alice's envelope, persist a received-envelope row, sign a receipt, and return it for Alice to verify; env-gated public MQTT and Nostr DataChannel tests carry Alice's serialized text/control envelope frame to Bob's transport answerer, invoke Bob's receiver-frame handler after DataChannel receipt, and return Bob's signed receipt frame before Alice's receipt-frame handler applies `peer_receipt`. This is **not yet** a full production peer-delivery flow because the persistent installed-app session still has to own a live receiver event loop across process boundaries and transport the generated receipt back automatically in two installed apps.

Verification:

```bash
cargo test -q -p discrypt-desktop signed_text_delivery_receipt_updates_message_state -- --nocapture
cargo test -q -p discrypt-desktop tampered_text_delivery_receipt_is_rejected -- --nocapture
cargo test -q -p discrypt-desktop two_profile_receiver_identity_can_sign_delivery_receipt -- --nocapture
cargo test -q -p discrypt-desktop receiver_command_accepts_verified_envelope_and_returns_signed_receipt -- --nocapture
cargo test -q -p discrypt-desktop receiver_command_rejects_tampered_envelope_without_receipt -- --nocapture
cargo test -q -p discrypt-desktop text_control_frame_handler_bridges_envelope_to_receipt -- --nocapture
cargo test -q -p discrypt-desktop text_control_frame_roundtrip_persists_across_two_profile_state_files -- --nocapture
cargo test -q -p discrypt-desktop text_control_session_pump_uses_data_transport_trait_and_persists_receipt -- --nocapture
DISCRYPT_DESKTOP_PUBLIC_MQTT_RECEIPT_E2E=1 \
DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 \
cargo test -q -p discrypt-desktop --features mqtt-adapter \
  public_mqtt_two_profile_receipt_crosses_provider_webrtc_when_enabled -- --nocapture
DISCRYPT_DESKTOP_PUBLIC_NOSTR_RECEIPT_E2E=1 \
DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol \
cargo test -q -p discrypt-desktop --features nostr-adapter \
  public_nostr_two_profile_receipt_crosses_provider_webrtc_when_enabled -- --nocapture
cargo check -q -p discrypt-desktop --features mqtt-adapter,nostr-adapter
npm --prefix apps/ui run typecheck
npm --prefix apps/ui run test:command-coverage
```

## What remains open before production

### P0: adapter support gaps

- [x] Lock Nostr feature-gate readiness and document production requirements.
- [x] Implement real Nostr adapter boundary behind `nostr-adapter`:
  - connects to configured `wss://` relays,
  - signs Nostr events with scoped relay identities,
  - uses hashed/random rendezvous tags only,
  - receives/filters by rendezvous topic.
- [ ] Complete Nostr production hardening:
  - map relay failures/rate limits/auth requirements to typed `SignalingHealthState`; conservative failure-class parsing and structured `NOTICE`/`CLOSED`/negative `OK` relay-message extraction now map common rate-limit/auth/message-size/trust strings to typed health states, and Nostr all-relay publish/subscribe failures include the redacted failure class; public auth/block rejection evidence passed on 2026-05-30 against `wss://nostr.oxtr.dev` with `failure_class=provider_auth_required`, while reproducible public rate-limit evidence remains opportunistic,
  - public multi-relay fallback soak passed on 2026-05-30 with `wss://nos.lol,wss://relay.damus.io,wss://discrypt-degraded-relay.invalid`, proving sealed presence/signal delivery survives one degraded configured relay,
  - provider-visible capture scans are covered by G133; external host packet capture remains a separate release-run artifact.
- [x] Lock IPFS/libp2p feature-gate/fail-closed readiness and document production requirements.
- [x] Implement real IPFS/libp2p PubSub adapter with rust-libp2p gossipsub, derived topics, opaque envelopes, unsubscribe, duplicate suppression, and local two-node transport E2E.
- [ ] Complete IPFS/libp2p production hardening:
  - public/default bootstrap peer policy and local resource limits are now configured and tested: default public bootstrap is intentionally empty while the libp2p/Hickory DNS stack remains audit-blocked; dialable bootstrap endpoints must be explicit `/ip4` or `/ip6` topic-peer multiaddrs with `/p2p/<peer-id>`, remain capped at 16 with duplicate rejection, 64 KiB envelope/transmit limit, bounded command queue, strict gossipsub validation, flood-publish disabled, and bounded mesh/history/duplicate-cache settings,
  - typed IPFS health covers `topic_mesh_unavailable`, unreachable `bootstrap_connect`, duplicate-envelope storms, and libp2p listener runtime errors as `provider_unhealthy` plus oversized envelopes as `provider_message_too_large`, while invalid/duplicate/overflow profile rejection remains policy-level,
  - provider-visible metadata capture is covered by G133 for the IPFS/libp2p boundary; external host packet capture remains a release-run artifact,
  - remaining hardening gaps are public topic-peer discovery/rendezvous and public-swarm E2E,
  - [x] define the current safe “public IPFS” profile as no built-in DNS bootstrap/default Kubo dependency; production/self-hosted IPFS profiles must provide explicit direct `/ip4` or `/ip6` TCP multiaddrs with `/p2p/<peer-id>` for reachable Discrypt topic peers,
  - [x] expose deterministic direct topic-peer multiaddrs from the rust-libp2p adapter and prove a self-hosted/direct `/p2p/<peer-id>` topic-peer roundtrip for presence and sealed WebRTC signaling without relying on generic IPFS bootstrap peers,
  - [x] enforce the direct topic-peer policy in the adapter bootstrap validator: DNS bootstrap and dialable non-`/p2p` endpoints are rejected before connection, while explicit direct topic-peer multiaddrs are accepted and local loopback listeners can still bind ephemeral `/tcp/0` ports,
  - add public topic-peer discovery/rendezvous instead of relying on generic IPFS bootstrap peers as topic mesh members,
  - run public-swarm E2E with configured direct bootstrap/rendezvous multiaddrs.
- [x] Lock separate Rust QUIC rendezvous feature-gate/fail-closed readiness and document production requirements.
- [ ] Harden separate Rust rendezvous service adapter:
  - [x] use the sibling signaling service as an explicit/self-hosted adapter over the content-blind `/v1/signals/*` API,
  - [x] reject native `quic://` endpoints honestly until native QUIC support is audited,
  - [x] enforce the signed endpoint trust fingerprint from policy/invite before production/self-hosted service use,
  - [x] validate `/healthz` status, service label, production/self-hosted `public_base_url`, protocol/schema version, max-body bounds, and rate-limit metadata before exposing a connected session,
  - [x] require production/self-hosted `/healthz` identity metadata: signed service identity fingerprint, accepted ALPN, future service expiry, rotation policy, and endpoint allowlist commitment must match the signed endpoint trust before connect succeeds,
  - [ ] validate that health identity against an external TLS certificate/public-key pin plus provider-visible capture before production release,
  - [ ] add staged/deployed-service E2E plus provider-visible capture scans.

### P0: app integration gaps

- [x] Add an adapter registry/factory used by Tauri/backend runtime, not only transport tests. Tauri provider diagnostics now enter `probe_provider_adapter_roundtrip` / `probe_provider_webrtc_datachannel_request_response_roundtrip`, which validate the runtime profile and dispatch through `SignalingAdapterFactory::for_kind(...)` before selecting a real MQTT/Nostr/IPFS implementation or a fail-closed QUIC/feature boundary.
- [x] Make per-DM/per-group/per-channel connectivity policy select from configured real adapter profiles and exclude unconfigured IPFS/QUIC placeholder endpoints from default app/invite profiles.
- [x] Carry selected adapter state into UI status honestly: backend `transport_status` now includes an `adapter` row with the selected provider plus readiness/fallback attempts, and transport diagnostics continue to expose selected provider, readiness, fallback state, and failure class for UI rendering without claiming a route/media connection.
- [ ] Run two actual app profiles/instances through:
  - setup/recovery,
  - DM invite generation/acceptance,
  - group invite generation/join,
  - text channel send/receive,
  - voice negotiation/join/leave/mute/speaker controls,
  - adapter fallback.
  - Current local-dev evidence: Playwright drives two isolated browser profiles through setup, local DM send/reload isolation, DM invite create/accept, group invite create/join, group text send on both profiles, and voice join/mute/speaker-slider/leave controls. This keeps the production gate open because it is not two installed Tauri app processes/devices and does not prove provider-delivered peer messages or real audio media.

### P0: WebRTC/media/data-plane gaps

- [x] Use the signaling adapters to exchange real WebRTC offer/answer/candidate payloads generated by the Rust transport harness over public MQTT and Nostr rendezvous.
- [x] Establish data channel for opaque text/control delivery across two independent Rust transport peers over public MQTT and Nostr rendezvous.
- [x] Expose a UI/Tauri opt-in message-send transport proof that sends an opaque message-derived frame through the provider-signaled WebRTC DataChannel diagnostic.
- [x] Add a same-process Tauri service harness that can load and persist two isolated app profiles from distinct state files, removing the prior global-state-only blocker for two-profile command E2E tests.
- [ ] Expose a production-safe long-lived text/control runtime attachment path (not a test shim).
  - `attach_text_control_transport_runtime` now starts role-split offerer/answerer runtimes when role/local/remote peer ids are supplied; legacy no-role probe-resume remains fail-closed with `transport_runtime_not_supported`.
  - Current probe-only helpers (`run_provider_webrtc_data_channel_probe` / `..._request_response_probe`) terminate their transport sessions after verification and do not leave a reusable `WebRtcNegotiator` or equivalent runtime object in app-service state.
- [ ] Establish persistent send/receive over the same data-channel path across two real Tauri app profiles/devices from UI-driven DM/group state, with signed peer receipts. The signed receipt verification/apply boundary, receiver-side envelope acceptance/receipt-generation command, text-control-frame handler, durable outbound text-control outbox, and reusable backend `TextControlDataTransport` pump are implemented and tested; the pump lists pending frames, sends over the transport trait, records frame-hash-guarded send handoff, receives response frames, applies signed receipts, emits metrics, persists `receipted` state, and now rejects missing-runtime, missing-session, and session-id-mismatch cases with typed command errors. Env-gated same-process two-profile MQTT and Nostr proofs now consume the persisted outbox frame, send that serialized frame through a real provider-signaled DataChannel, invoke Bob's receiver handler after answerer DataChannel delivery, return a signed receipt frame over the same DataChannel, mark the outbox handoff `sent`, and then transition to `receipted`; `start_text_session(..., data_channel_probe=true)` marks the backend text session route as direct only after the provider-signaled DataChannel proof succeeds. A local `TextControlDataTransport` pump proof now covers send/recv trait invocation, receiver handler invocation, DataChannel-style metrics, frame-hash-guarded send handoff, durable receipt persistence, and fail-closed ownership preconditions. Native builds now keep text/control pumping on the Tauri backend rather than a React foreground timer, and the pump remains fail-closed/status-visible until a matching live runtime exists; role-split provider WebRTC runtime attachment and background pending status are implemented; installed-device/two-process app proof remains open.
- [ ] Establish audio media path and prove speaking/mute/volume UI state reflects real media state. Partial local-only progress: the UI now measures a real local microphone buffer, the backend app service updates the local participant speaking flag from RMS/peak evidence while respecting self-mute, and a core reload regression now persists that speaking state across restart; remote transported audio, per-peer playback volume, and encrypted media-frame E2E remain open.
- [x] Prove public STUN participates in provider-signaled WebRTC data-channel setup in the live same-host Rust transport harness with real network UDP bind.
- [ ] Prove STUN works across distinct machines and normal NAT scenarios.
- [ ] Prove hard NAT fails honestly without TURN and succeeds with a configured TURN service. Relay-only WebRTC policy now rejects missing TURN deterministically, TURN route selection stays fail-closed until relay candidate evidence exists, `DISCRYPT_PUBLIC_TURN_E2E=1` can run a public MQTT-signaled DataChannel with `WebRtcIceTransportPolicy::RelayOnly` plus real TURN credentials, and Tauri/backend diagnostics can now surface and bind TURN relay proof when the transport probe reports relay-candidate evidence; a real credentialed TURN run across a constrained network is still required before closing this gate.

### P0: security/release gaps

- [ ] Dependency/security audit for `rumqttc` and any Nostr/IPFS/libp2p dependencies. Latest `cargo audit` is documented in `docs/release/dependency-security-audit.md` and remains release-blocking: the libp2p 0.56, rumqttc-next, direct-IPFS, and MLS/libcrux slices removed the old `ring 0.16`, `rustls-webpki 0.101.7`, MQTT `rustls-webpki 0.102.8`, and `libcrux-chacha20poly1305 0.0.7` findings; a follow-up local libp2p metadata patch removed the optional DNS/mDNS lockfile edges, so latest `cargo audit` exits zero with 0 vulnerabilities. Release remains blocked by unmaintained/unsound warning triage plus the app/device/media E2E gaps below.
- [x] Public provider allowlist/versioning and rotation policy. Signed invite/app signaling profiles now carry `provider_policy_version`, endpoint allowlist commitments, and provider rotation policy text; Tauri validates endpoint commitments before converting profiles into transport probes, and invite bootstrap validation rejects empty/malformed provider-policy metadata.
- [x] Connect STUN/TURN fallback and provider-privacy proof into a dedicated release gate (G132)
  for deterministic harness evidence and optional public-provider MQTT validation. `npm --prefix apps/ui run test:stun-turn-provider-privacy-g132` passed and runs both local cargo gates by default; real distinct-machine STUN/TURN release evidence remains tracked separately above.
- [x] Provider-visible metadata capture/PCAP tests for MQTT, Nostr, IPFS, and QUIC. `npm --prefix apps/ui run test:provider-metadata-capture-g133` now runs deterministic provider-visible conformance capture plus plaintext-rejection tests across all four adapter boundaries; external host packet captures remain a release-run artifact, not a local CI claim.
- [x] Abuse/rate-limit handling against public relays/brokers. Existing G119/G120 abuse gates passed, and provider failure classification maps public relay/broker rate-limit/auth/message-size/trust failures into typed health/readiness states; public multi-relay soak evidence remains tracked under Nostr/IPFS hardening rather than this local handling gate.
- [ ] Full release matrix across Linux desktop package and Android once mobile exists.

#### G132 status

- Added local deterministic proof for STUN→overlay→TURN behavior and provider-privacy hygiene in:
  - `harness/multinode/src/lib.rs`
  - `crates/transport/tests/connectivity_flows.rs`
- Added release gate script:
  - `npm --prefix apps/ui run test:stun-turn-provider-privacy-g132`
- Public-provider smoke remains optional to keep default CI deterministic:
  - set `DISCRYPT_PUBLIC_SIGNALING_E2E=1` for MQTT reruns and `DISCRYPT_PUBLIC_NOSTR_E2E=1` for Nostr reruns; latest MQTT and Nostr public adapter-boundary proofs are green against their configured default public providers.

### G132 production evidence matrix

#### Two-profile signaling verification matrix (required + planned)

| Gate slice | Command | Evidence target |
| --- | --- | --- |
| STUN overlay ordering and TURN fallback determinism | `cargo test -p discrypt-multinode-harness connectivity_signaling_push_smoke_covers_phase6_gates --quiet` | `ConnectivitySignalingPushSmoke` flags: `fallback_chain_covered`, `owner_overrides_used`, `metadata_matrix_validated`, `relays_ciphertext_only`, `ac_metadata_matrix_validated` |
| Transport policy/ciphertext-only routing | `cargo test -p discrypt-transport valid_direct_overlay_and_turn_flows_select_expected_leg --quiet` | Test-asserted route ordering and relay leg ciphertext-only constraints |
| Optional public MQTT proof (provider-visible real smoke) | `DISCRYPT_PUBLIC_SIGNALING_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=<mqtts://...> cargo test -q -p discrypt-transport --features mqtt-adapter public_mqtt_two_peer_presence_and_signal_roundtrip -- --nocapture` | Latest reruns passed against `mqtts://broker.emqx.io:8883` after broker `SUBACK` readiness was enforced; `test.mosquitto.org` certificate incompatibility and `broker.hivemq.com` network timeout remain provider-specific caveats. |
| Nostr public-provider proof | `DISCRYPT_PUBLIC_NOSTR_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://relay.damus.io cargo test -p discrypt-transport --features nostr-adapter public_nostr_two_peer_presence_and_signal_roundtrip -- --nocapture`; `DISCRYPT_PUBLIC_NOSTR_MULTI_RELAY_E2E=1 cargo test -q -p discrypt-transport --features nostr-adapter public_nostr_multi_relay_degraded_fallback_soak -- --nocapture`; `DISCRYPT_PUBLIC_NOSTR_REJECTION_E2E=1 cargo test -q -p discrypt-transport --features nostr-adapter public_nostr_blocked_relay_maps_to_auth_required -- --nocapture` | Latest single-relay rerun passed against `wss://relay.damus.io`; degraded multi-relay fallback passed on 2026-05-30 with `wss://nos.lol,wss://relay.damus.io,wss://discrypt-degraded-relay.invalid`; blocked relay rejection passed against `wss://nostr.oxtr.dev` with typed `provider_auth_required` and no payload leakage |
| Optional public provider-signaled WebRTC data-channel proof | `DISCRYPT_PUBLIC_MQTT_WEBRTC_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 cargo test -q -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_signals_real_webrtc_datachannel_roundtrip -- --nocapture` and `DISCRYPT_PUBLIC_NOSTR_WEBRTC_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol cargo test -q -p discrypt-transport --features nostr-adapter --test public_webrtc_datachannel_e2e public_nostr_signals_real_webrtc_datachannel_roundtrip -- --nocapture` | Latest MQTT and Nostr runs passed. They use `stun:stun.l.google.com:19302`, bind WebRTC UDP to `0.0.0.0:0`, exchange sealed offer/answer through the provider, open a WebRTC DataChannel, and deliver an opaque text/control frame. Damus was rate-limited in one rerun, so `nos.lol` is the latest green public Nostr relay evidence. |
| Optional public TURN relay-only WebRTC proof | `DISCRYPT_PUBLIC_TURN_E2E=1 DISCRYPT_PUBLIC_TURN_ENDPOINT=<turns://...> DISCRYPT_PUBLIC_TURN_USERNAME=<user> DISCRYPT_PUBLIC_TURN_CREDENTIAL=<secret> cargo test -q -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_relay_only_turn_fallback_roundtrip_when_configured -- --nocapture` | Executable opt-in release gate. Local deterministic coverage rejects relay-only WebRTC without configured TURN. A real credentialed TURN run is still missing before hard-NAT/TURN production closure. |
| IPFS local libp2p proof | `cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_local_two_peer_presence_and_signal_roundtrip -- --nocapture`; `cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_resource_policy_is_bounded_and_default_bootstrap_is_parseable -- --nocapture`; `cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_bootstrap_policy_rejects_duplicates_and_overflow -- --nocapture`; `cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_unreachable_bootstrap_maps_to_typed_health -- --nocapture`; `cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_oversized_envelope_maps_to_typed_health -- --nocapture`; `cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_insufficient_peers_reports_actionable_topic_mesh_error -- --nocapture`; `cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_duplicate_storm_maps_to_typed_health -- --nocapture`; `cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_swarm_runtime_errors_map_to_typed_health -- --nocapture` | Passed locally with two rust-libp2p gossipsub nodes over loopback; opaque presence/signal only; bootstrap/resource policy is bounded and parse-tested with empty public defaults plus explicit direct endpoint validation; unreachable bootstrap, topic mesh, duplicate storms, libp2p listener runtime errors, and oversize failures map to typed health |
| IPFS public-provider proof | `DISCRYPT_PUBLIC_IPFS_E2E=1 DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS=<direct-multiaddr,...> cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter public_ipfs_two_peer_signaling_smoke -- --nocapture`; `DISCRYPT_PUBLIC_IPFS_WEBRTC_E2E=1 DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS=<direct-multiaddr,...> cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter --test public_webrtc_datachannel_e2e public_ipfs_signals_real_webrtc_datachannel_roundtrip -- --nocapture`; `DISCRYPT_PUBLIC_IPFS_MEDIA_WEBRTC_E2E=1 DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS=<direct-multiaddr,...> cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter --test public_webrtc_datachannel_e2e public_ipfs_signals_real_webrtc_media_frame_roundtrip -- --nocapture` | Public signaling/DataChannel/media-frame gates are executable and fail closed unless explicit direct topic-peer `/p2p/<peer-id>` multiaddrs are supplied. Missing real public/direct topic-peer run in this environment. The previous `/dnsaddr/bootstrap.libp2p.io/...` approach is no longer a production default because DNS bootstrap is audit-blocked and generic bootstrap-only peers did not provide a topic mesh. |
| QUIC public-provider / separate rendezvous service proof | `cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter discrypt_rendezvous_sibling_service_roundtrip_when_binary_is_available -- --nocapture`; `DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E=1 DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=https://... cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter public_quic_two_peer_signaling_smoke -- --nocapture`; `DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_WEBRTC_E2E=1 DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=https://... cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter --test public_webrtc_datachannel_e2e public_quic_rendezvous_signals_real_webrtc_datachannel_roundtrip -- --nocapture`; `DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_MEDIA_WEBRTC_E2E=1 DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=https://... cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter --test public_webrtc_datachannel_e2e public_quic_rendezvous_signals_real_webrtc_media_frame_roundtrip -- --nocapture`; `cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter quic_rendezvous_feature_gate_is_selectable_but_rejects_reserved_native_quic_scheme -- --nocapture`; `cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter quic_rendezvous_rejects_https_endpoint_without_signed_trust_fingerprint -- --nocapture`; `cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter quic_rendezvous_rejects_mismatched_signed_trust_fingerprint -- --nocapture`; `cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter quic_rendezvous_health_requires_matching_public_base_for_production -- --nocapture`; `cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter quic_rendezvous_health_accepts_signed_identity_and_rotation_metadata -- --nocapture`; `cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter quic_rendezvous_health_requires_production_protocol_metadata -- --nocapture` | Local sibling binary roundtrip passed when `../discrypt-signaling/target/debug/discrypt-signaling-server` is available; native `quic://` endpoint use is still rejected as reserved; production/self-hosted HTTPS/WSS endpoints must carry the signed endpoint fingerprint and `/healthz` must advertise matching public-base/protocol/max-body/rate-limit metadata plus service identity, accepted ALPN, future expiry, rotation policy, and endpoint allowlist commitment before connect succeeds. Deployed signaling, WebRTC DataChannel, and media-frame gates are executable but remain opt-in and unproven until a staged HTTPS/WSS endpoint is supplied. External TLS certificate/public-key pinning and capture-scan evidence are still missing. |

- Real producer/adapter route proofs still missing in this release gate: live IPFS public-bootstrap/topic-discovery proof, staged/deployed Discrypt rendezvous service proof with TLS certificate/public-key pinning plus capture evidence, and end-to-end mobile/installed-app transport smoke (tracked separately).
- Missing adapter check status is intentionally exposed as blockers instead of fake green signals in this phase.

## How to rerun the current real MQTT proof

Default public broker:

```bash
DISCRYPT_PUBLIC_SIGNALING_E2E=1 \
  cargo test -q -p discrypt-transport --features mqtt-adapter \
  public_mqtt_two_peer_presence_and_signal_roundtrip -- --nocapture
```

Custom public broker:

```bash
DISCRYPT_PUBLIC_SIGNALING_E2E=1 \
DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 \
  cargo test -q -p discrypt-transport --features mqtt-adapter \
  public_mqtt_two_peer_presence_and_signal_roundtrip -- --nocapture
```

The test is intentionally environment-gated so normal unit tests do not depend on public network availability.

### 2026-05-30 live Tauri runtime-pair public rerun evidence

- [x] Reran the desktop public MQTT live runtime-pair pump proof against `mqtts://broker.emqx.io:8883`; it passed and persisted Alice's peer receipt plus Bob's received-envelope/receipt state.
- [x] Reran the desktop public Nostr live runtime-pair pump proof against `wss://nos.lol`; it passed and persisted Alice's peer receipt plus Bob's received-envelope/receipt state.
- [ ] These tests still use the backend live runtime-pair harness rather than two separately installed GUI processes; they are stronger than a probe, but not the final installed-app UI E2E gate.

Verification rerun:

```bash
DISCRYPT_DESKTOP_PUBLIC_MQTT_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 timeout 240s cargo test -q -p discrypt-desktop --features mqtt-adapter public_mqtt_live_runtime_pair_pump_persists_peer_receipt_when_enabled -- --nocapture
DISCRYPT_DESKTOP_PUBLIC_NOSTR_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol timeout 240s cargo test -q -p discrypt-desktop --features nostr-adapter public_nostr_live_runtime_pair_pump_persists_peer_receipt_when_enabled -- --nocapture
```

### 2026-05-30 role-split runtime progress

- [x] Added transport-level role-split provider WebRTC text/control runtime APIs:
  - `start_provider_webrtc_text_control_offer_runtime(...)`
  - `start_provider_webrtc_text_control_answer_runtime_with_answerer(...)`
  - `ProviderTextControlRuntime`, `ProviderTextControlRuntimePeerEvidence`, and `ProviderTextControlRuntimePeerRole`
- [x] Proved the new runtime shape with two separately started local provider peers over the signaling adapter boundary. The answerer starts first, waits in the rendezvous scope, receives the sealed offer, answers, opens a real WebRTC DataChannel, receives an opaque frame, and returns an opaque receipt to the offerer.
- [x] Kept provider-visible material opaque in the role-split test by scanning the local conformance bus for forbidden plaintext/SDP markers.
- [x] Wire the role-split runtime APIs into `attach_text_control_transport_runtime` and app state. The Tauri command now accepts role/local-peer/remote-peer configuration and stores an owned `ProviderTextControlRuntime` handle for the active scope. Legacy no-role clients still use the fail-closed persisted probe resume path.
- [x] Expose a UI role-split attach control surface that derives default peer ids from active DM/group/invite state and can call backend answerer/offerer runtime attach. This is an explicit manual runtime attach bridge, not final automatic invite/member orchestration.

Verification added/run:

```bash
cargo fmt --all --check
cargo check -q -p discrypt-transport
cargo check -q -p discrypt-transport --features mqtt-adapter,nostr-adapter
cargo check -q -p discrypt-transport --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter
cargo test -q -p discrypt-transport live_provider_text_control_role_split_runtimes_connect_two_peers -- --nocapture
cargo test -q -p discrypt-transport live_provider_text_control_runtime_pair_carries_multiple_opaque_frames -- --nocapture
```

### 2026-05-30 Tauri role-split attach surface progress

- [x] Extended the Tauri/backend `attach_text_control_transport_runtime` command with optional role-split fields:
  - `runtime_role`: `offerer` or `answerer`
  - `local_peer_id`
  - `remote_peer_id`
- [x] Preserved backward compatibility: if no role is supplied, the command still uses the legacy fail-closed persisted-probe resume path and does not pretend stale probe SDP is a live runtime.
- [x] When role and peer ids are supplied, the backend now builds active-scope signaling profile/scope/ICE material, starts the corresponding one-peer provider runtime, stores the owned runtime handle plus executor in app-service state, and exposes role/peer evidence in the text/control runtime status row.
- [x] Updated the frontend command type and command-coverage gate to include the new attach fields.
- [x] UI now derives default runtime peer ids from active DM/group/invite state and lets the operator set reciprocal local/remote peer ids before starting answerer or offerer.
- [x] Backend role-split attach now starts on a background thread and surfaces an `attaching` text/control runtime status row, so answerer/offerer startup no longer blocks the command/UI while it waits for the reciprocal peer.
- [x] DM runtime peer defaults now use signed DM invite bootstrap commitments: the inviter side defaults to the inviter identity commitment and the accepted side defaults to the reply rendezvous commitment.
- [ ] Group/member runtime peer identity exchange still needs signed member-device metadata; manual reciprocal peer id entry remains the operator bridge outside DM bootstrap flows.
- [ ] Two installed app instances have not yet been run through the role-split attach command over public providers.

Verification added/run:

```bash
cargo fmt --all --check
cargo check -q -p discrypt-desktop
cargo check -q -p discrypt-desktop --features mqtt-adapter,nostr-adapter
cargo test -q -p discrypt-desktop attach_text_control_transport_runtime -- --nocapture
npm --prefix apps/ui run test:command-coverage
npm --prefix apps/ui run typecheck
```

### 2026-05-30 UI role-split runtime attach bridge

- [x] Added manual UI controls to the transport status strip for `Local runtime peer`, `Remote runtime peer`, `Listen as answerer`, and `Connect as offerer`.
- [x] Defaults are deterministically derived from active profile plus active DM/group/channel/invite state so the control is scoped to the current conversation context instead of global hard-coded peer ids.
- [x] The UI calls the native `attach_text_control_transport_runtime` command with `runtime_role`, `local_peer_id`, and `remote_peer_id`; browser fallback remains honest through the existing command layer.
- [x] Backend attach returns an honest `attaching` state immediately and completes/fails asynchronously through app events, avoiding a blocked UI while a peer is offline.
- [x] DM defaults use signed invite bootstrap commitments for reciprocal inviter/reply peer roles where the app can infer the accepted-invite side.
- [ ] This is not yet the final production UX. Group runtime attach still needs signed member-device metadata and a two-installed-app UI E2E over public providers.

Verification added/run:

```bash
npm --prefix apps/ui run typecheck
npm --prefix apps/ui run test:command-coverage
npm --prefix apps/ui run build
cargo fmt --all --check
cargo check -q -p discrypt-desktop
cargo check -q -p discrypt-desktop --features mqtt-adapter,nostr-adapter
cargo test -q -p discrypt-desktop attach_text_control_transport_runtime -- --nocapture
```

### 2026-05-30 public role-split text runtime evidence

- [x] Added executable public-provider role-split text/control runtime gates for all required signaling adapter families:
  - MQTT: `public_mqtt_role_split_text_runtime_roundtrip`
  - Nostr: `public_nostr_role_split_text_runtime_roundtrip`
  - IPFS/libp2p: `public_ipfs_role_split_text_runtime_roundtrip`
  - separate Discrypt rendezvous: `public_quic_rendezvous_role_split_text_runtime_roundtrip`
- [x] Ran the real public MQTT role-split runtime gate against `mqtts://broker.emqx.io:8883`; two separately-started offerer/answerer peers negotiated through the public broker, opened a WebRTC DataChannel with public STUN, sent an opaque text/control frame, and received an opaque receipt.
- [x] Ran the real public Nostr role-split runtime gate against `wss://nos.lol`; two separately-started offerer/answerer peers negotiated through the public relay, opened a WebRTC DataChannel with public STUN, sent an opaque text/control frame, and received an opaque receipt.
- [ ] IPFS/libp2p and separate Discrypt rendezvous role-split gates are executable but remain unproven against public/deployed infrastructure in this environment because they require explicit direct topic-peer multiaddrs and a deployed HTTPS/WSS rendezvous endpoint respectively.

Verification added/run:

```bash
cargo fmt --all --check
cargo check -q -p discrypt-transport --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter
cargo test -q -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_role_split_text_runtime_roundtrip -- --nocapture
DISCRYPT_PUBLIC_MQTT_ROLE_SPLIT_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 timeout 240s cargo test -q -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_role_split_text_runtime_roundtrip -- --nocapture
DISCRYPT_PUBLIC_NOSTR_ROLE_SPLIT_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol timeout 240s cargo test -q -p discrypt-transport --features nostr-adapter --test public_webrtc_datachannel_e2e public_nostr_role_split_text_runtime_roundtrip -- --nocapture
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter --test public_webrtc_datachannel_e2e public_ipfs_role_split_text_runtime_roundtrip -- --nocapture
cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter --test public_webrtc_datachannel_e2e public_quic_rendezvous_role_split_text_runtime_roundtrip -- --nocapture
```

## 2026-05-30 update: invite-shared role-split runtime material

Role-split text/control runtime attach now derives its WebRTC rendezvous bootstrap
and entropy from signed connectivity metadata (`connectivity_schema_version`,
invite kind, scope commitment, selected profile id, and DM/group bootstrap
commitments) instead of the local profile identity seed. This is required for two
separately installed app profiles to compute the same provider rendezvous topic
after one user creates a DM invite and the other accepts it.

The active-DM connectivity resolver also prefers the matching signed DM invite
snapshot for that DM before falling back to the local-only DM default, so the
inviter and invitee attach against the same DM scope when a contact invite is
present.

Verification added:

- `live_role_split_runtime_material_is_invite_shared_not_profile_local` proves
  Alice and Bob have distinct local profile identities but resolve identical
  role-split runtime profile, scope, ICE config, bootstrap secret, and entropy
  after create/accept DM invite.

Verification run:

- `cargo fmt --all --check`
- `cargo check -q -p discrypt-desktop`
- `cargo test -q -p discrypt-desktop attach_text_control_transport_runtime -- --nocapture`
- `cargo test -q -p discrypt-desktop live_role_split_runtime_material_is_invite_shared_not_profile_local -- --nocapture`

Remaining gap: this removes a real two-profile attach precondition, but does not
by itself prove two installed GUI processes completed a public-provider attach
and sent/received text from the UI.

## 2026-05-30 update: desktop runtime-pair proof now uses role-split constructors

The desktop persisted-state public runtime-pair proof no longer uses the
single-call in-process pair constructor. Its harness now starts the receiver with
`start_provider_webrtc_text_control_answer_runtime_with_answerer`, starts the
sender with `start_provider_webrtc_text_control_offer_runtime`, waits for both
roles to attach, pumps the sender's persisted outbox over the offerer runtime,
and verifies the receiver-generated app receipt.

Public evidence rerun:

- MQTT: `DISCRYPT_DESKTOP_PUBLIC_MQTT_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 timeout 240s cargo test -q -p discrypt-desktop --features mqtt-adapter public_mqtt_live_runtime_pair_pump_persists_peer_receipt_when_enabled -- --nocapture` passed.
- Nostr: `DISCRYPT_DESKTOP_PUBLIC_NOSTR_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol timeout 240s cargo test -q -p discrypt-desktop --features nostr-adapter public_nostr_live_runtime_pair_pump_persists_peer_receipt_when_enabled -- --nocapture` passed.

Remaining gap: this is now a stronger two-role persisted-state backend proof, but
still not a two-window installed GUI Playwright/Tauri UI flow.

## 2026-05-30 update: group invite-derived runtime peer defaults

The React runtime attach controls now derive group owner/member peer defaults
from signed group bootstrap commitments instead of falling through to local UI
hash seeds. For an owner/admin profile, the default local peer is derived from
`group_identity_commitment` and the remote member peer is derived from the signed
role-admission/channel-policy commitments. For a joined member profile, the
mapping is reversed. Users can still override the fields manually, but the normal
group path no longer starts from ad-hoc profile-local defaults when signed group
metadata is present.

Verification run:

- `npm --prefix apps/ui run typecheck`
- `npm --prefix apps/ui run test:command-coverage`
- `npm --prefix apps/ui run build`

Remaining gap: this improves UI/runtime attach defaults for group flows, but does
not yet provide a full signed member-device directory or two-window group text
and voice E2E proof.

## 2026-05-30 update: backend group runtime peers are persisted and returned

Group creation, invite join, and account-recovery room hydration now persist a
backend-owned `runtime_peers` list on each `GroupView`. The peer ids are derived
from signed group bootstrap commitments with the same domain separation used by
the UI/runtime attach path:

- owner peer: signed `group_identity_commitment`
- member peer: signed `role_admission_policy_commitment` + `channel_policy_commitment`
- `is_local` marks owner-local on created groups and member-local on joined or
  recovered groups
- `source=signed_group_bootstrap_v1` documents that these are not ad-hoc UI-only
  hashes

The React runtime attach controls now prefer backend-returned group runtime peers
before falling back to local bootstrap derivation, so the normal group attach path
is driven by persisted Tauri state.

Verification added:

- `group_invite_join_persists_signed_runtime_peers` proves Alice-created owner
  peers and Bob-joined member peers are reciprocal, derived from the signed group
  invite bootstrap, and survive reload.

Verification run:

- `cargo fmt --all --check`
- `cargo check -q -p discrypt-desktop`
- `cargo test -q -p discrypt-desktop group_invite_join_persists_signed_runtime_peers -- --nocapture`
- `npm --prefix apps/ui run typecheck`
- `npm --prefix apps/ui run test:command-coverage`
- `npm --prefix apps/ui run build`

Remaining gap: this closes the UI-only group peer defaulting gap for two-sided
owner/member invite bootstrap, but it is not a full multi-member signed device
directory, not dynamic peer selection for large groups, and not yet a two-window
installed GUI proof of group text or voice over public providers.

## 2026-05-30 update: UI fallback invites carry bootstrap commitments and E2E asserts reciprocal peers

The browser/local-dev fallback invite format now carries the same bootstrap
commitments needed by two independent UI profiles to derive reciprocal runtime
peers from an invite:

- group invites include `group_identity`, `role_policy`, and `channel_policy`
  commitment query fields
- DM invites include `dm_inviter`, `dm_contact`, and `dm_reply` commitment query
  fields
- parsing preserves these commitments instead of rebuilding bootstrap metadata
  from invite-key-local fallbacks

The two-profile Playwright flow now asserts that after Alice creates a group
invite and Bob joins it, the displayed local/remote runtime peer ids are
reciprocal (`Alice.local == Bob.remote` and `Alice.remote == Bob.local`) before
sending group messages. This does not claim remote delivery; it verifies the UI
state needed before role-split attach is no longer divergent.

Verification run:

- `npm --prefix apps/ui run typecheck`
- `VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1 npm --prefix apps/ui run build`
- `cd apps/ui && CI=1 VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1 npx playwright test tests/e2e/two-profile-flow.spec.ts --workers=1`
- `npm --prefix apps/ui run test:command-coverage`

Remaining gap: this is browser fallback E2E for invite/bootstrap state and UI
reciprocal runtime peer fields, not an installed Tauri two-process public-provider
DataChannel receipt proof.

## 2026-05-30 update: backend DM runtime peers are persisted and E2E-asserted

Direct-message state now mirrors the group runtime-peer contract. `DirectConversationView`
returns a backend-owned `runtime_peers` list derived from signed DM bootstrap
commitments:

- inviter peer: signed `inviter_identity_commitment`
- reply peer: signed `reply_rendezvous_commitment`
- `is_local` marks inviter-local on locally started/seeded DMs and reply-local
  on accepted DM contact invites
- `source=signed_dm_bootstrap_v1` records the evidence boundary

The React runtime attach controls now prefer backend-returned DM runtime peers
before falling back to bootstrap derivation. The two-profile Playwright flow now
checks reciprocal DM runtime peer ids after Alice creates a DM invite and Bob
accepts it, then checks reciprocal group runtime peer ids after group invite
join.

Verification added:

- `dm_invite_accept_persists_signed_runtime_peers` proves Alice inviter and Bob
  reply DM peers are reciprocal, derived from signed DM invite bootstrap, and
  survive reload.

Verification run:

- `cargo fmt --all --check`
- `cargo check -q -p discrypt-desktop`
- `cargo test -q -p discrypt-desktop dm_invite_accept_persists_signed_runtime_peers -- --nocapture`
- `npm --prefix apps/ui run typecheck`
- `VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1 npm --prefix apps/ui run build`
- `cd apps/ui && CI=1 VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1 npx playwright test tests/e2e/two-profile-flow.spec.ts --workers=1`
- `npm --prefix apps/ui run test:command-coverage`

Remaining gap: this closes backend/UI reciprocal peer derivation for basic DM
invite attach setup, but it is still not an installed Tauri two-process public
provider receipt proof and not real media/voice E2E.

## 2026-05-30 update: live runtime-pair proofs now use signed invite peer rows

The backend live runtime-pair pump no longer synthesizes offerer/answerer peer ids
from local profile ids for the public MQTT/Nostr receipt proof path. It now:

- derives the shared runtime bootstrap/entropy from the signed invite connectivity
  metadata through the same role-split material path used by runtime attach
- reads local/remote peer ids from persisted DM/group `runtime_peers`
- fails closed when sender and receiver states do not contain reciprocal signed
  bootstrap peer rows
- updates the public MQTT/Nostr runtime-pair tests so Bob accepts Alice's real DM
  invite before the pump attempts a peer receipt

Verification added:

- `live_runtime_peer_ids_are_signed_invite_reciprocals` proves Alice/Bob DM invite
  states produce reciprocal signed runtime peer ids.
- Public MQTT/Nostr runtime-pair tests compile and skip cleanly unless their
  public-provider E2E env flags are explicitly enabled; their setup now uses the
  real DM invite/accept flow.

Verification run:

- `cargo fmt --all --check`
- `cargo check -q -p discrypt-desktop`
- `cargo test -q -p discrypt-desktop live_runtime_peer_ids_are_signed_invite_reciprocals -- --nocapture`
- `cargo test -q -p discrypt-desktop live_role_split_runtime_material_is_invite_shared_not_profile_local -- --nocapture`
- `cargo test -q -p discrypt-desktop --features mqtt-adapter public_mqtt_live_runtime_pair_pump_persists_peer_receipt_when_enabled -- --nocapture` (skipped because public E2E env flag was not set)
- `cargo test -q -p discrypt-desktop --features nostr-adapter public_nostr_live_runtime_pair_pump_persists_peer_receipt_when_enabled -- --nocapture` (skipped because public E2E env flag was not set)

Remaining gap: this strengthens the real public-provider test harness and removes
profile-local peer-id synthesis from that path, but the long-running public MQTT
and Nostr receipt proof was not re-run in this slice because the explicit public
E2E env flags were not set. IPFS direct topic-peer, deployed QUIC rendezvous,
credentialed TURN relay-only, and real voice/audio capture/playback E2E remain
open.

## 2026-05-30 update: hardened public MQTT/Nostr receipt proofs passed

After the runtime-pair hardening, the env-enabled public-provider receipt proofs
were run against public endpoints with the real DM invite/accept setup:

- MQTT: Alice creates a DM invite, Bob accepts it, both sides use reciprocal
  signed `runtime_peers`, and the text/control envelope plus signed receipt cross
  a provider-negotiated WebRTC DataChannel via `mqtts://broker.emqx.io:8883`.
- Nostr: the same two-profile DM invite/accept receipt flow passes through
  `wss://nos.lol`.

Verification run:

- `DISCRYPT_DESKTOP_PUBLIC_MQTT_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 timeout 240s cargo test -q -p discrypt-desktop --features mqtt-adapter public_mqtt_live_runtime_pair_pump_persists_peer_receipt_when_enabled -- --nocapture` — passed
- `DISCRYPT_DESKTOP_PUBLIC_NOSTR_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol timeout 240s cargo test -q -p discrypt-desktop --features nostr-adapter public_nostr_live_runtime_pair_pump_persists_peer_receipt_when_enabled -- --nocapture` — passed

Remaining gap: MQTT and Nostr DM text/control receipt proof now has public-provider
evidence through signed invite state, but IPFS direct topic-peer, deployed QUIC
rendezvous, credentialed TURN relay-only, group public-provider receipt, installed
GUI two-window E2E, and real voice/audio capture/playback E2E remain open.

## 2026-05-30 update: public MQTT/Nostr group channel receipt proofs passed

The backend now has opt-in public-provider group-channel receipt gates mirroring
the DM receipt proof. Each test creates a group, issues a signed group invite,
has Bob join through that invite, sends a channel message from Alice, and pumps
the persisted text/control outbox through the provider-signaled WebRTC runtime
using reciprocal group `runtime_peers` from signed bootstrap metadata.

Verification run:

- `DISCRYPT_DESKTOP_PUBLIC_MQTT_GROUP_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 timeout 240s cargo test -q -p discrypt-desktop --features mqtt-adapter public_mqtt_group_live_runtime_pair_pump_persists_peer_receipt_when_enabled -- --nocapture` — passed
- `DISCRYPT_DESKTOP_PUBLIC_NOSTR_GROUP_RUNTIME_PAIR_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol timeout 240s cargo test -q -p discrypt-desktop --features nostr-adapter public_nostr_group_live_runtime_pair_pump_persists_peer_receipt_when_enabled -- --nocapture` — passed

Remaining gap: MQTT/Nostr now have public-provider DM and group text/control
receipt evidence through Tauri backend state, but IPFS direct topic-peer, deployed
QUIC rendezvous, credentialed TURN relay-only, installed GUI two-window E2E, and
real voice/audio capture/playback E2E remain open.

## 2026-05-30 update: IPFS direct topic-peer WebRTC text/control proof passed

The IPFS/libp2p adapter now has a self-hosted/direct topic-peer WebRTC
text/control gate. The test starts a rust-libp2p gossipsub topic-peer listener
on loopback, extracts its explicit `/p2p/<peer-id>` multiaddr, configures the
runtime profile with that direct topic-peer endpoint, negotiates a WebRTC
DataChannel through the IPFS provider adapter, sends an opaque ciphertext-shaped
text/control frame, and receives a receipt frame back over the same negotiated
DataChannel.

Verification run:

- `cargo fmt --all --check` — passed
- `cargo check -q -p discrypt-transport --features ipfs-pubsub-adapter` — passed
- `timeout 180s cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_direct_topic_peer_webrtc_text_control_roundtrip -- --nocapture` — passed
- `timeout 180s cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_direct_topic_peer_multiaddr_roundtrip -- --nocapture` — passed

Remaining gap: this closes the local/self-hosted direct IPFS topic-peer WebRTC
text/control proof, but it is not a public Internet IPFS topic-peer run, not a
long-lived role-split Tauri text runtime, not installed GUI two-window E2E, and
not real voice/audio capture/playback. Public IPFS still requires an explicit
reachable Discrypt topic-peer multiaddr until public discovery/rendezvous is
implemented and audited.

## 2026-05-30 update: IPFS direct topic-peer runtime-pair text/control proof passed

The local/self-hosted IPFS direct topic-peer path now also exercises the
transport runtime-pair primitive instead of only the one-shot WebRTC diagnostic
probe. The runtime-pair constructor was hardened to exchange complete SDP
offer/answer descriptions before applying them, which makes the direct-topic-peer
path tolerant of non-trickle provider latency. The new regression starts an
explicit `/p2p/<peer-id>` topic-peer, attaches an IPFS provider-signaled
offerer/answerer runtime pair, sends an opaque text/control frame through the
returned app-facing `TextControlDataTransport`, and receives the answerer receipt
frame back over that same runtime.

Verification run:

- `cargo fmt --all --check` — passed
- `cargo check -q -p discrypt-transport --features ipfs-pubsub-adapter` — passed
- `timeout 180s cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_direct_topic_peer_runtime_pair_text_control_roundtrip -- --nocapture` — passed
- `timeout 180s cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_direct_topic_peer_webrtc_text_control_roundtrip -- --nocapture` — passed
- `timeout 180s cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub_direct_topic_peer_multiaddr_roundtrip -- --nocapture` — passed

Remaining gap: this closes the local/self-hosted IPFS direct-topic-peer
runtime-pair text/control proof. It is still not a public Internet IPFS
topic-peer proof, not a two-installed-Tauri-process runtime ownership proof, not
deployed QUIC rendezvous, not credentialed TURN relay-only, and not real
voice/audio capture/playback E2E.

## 2026-05-30 update: sibling Discrypt rendezvous runtime-pair proof passed

The separate sibling `discrypt-signaling` service path now has a local
runtime-pair text/control proof in addition to the existing presence/signal
roundtrip. When `../discrypt-signaling/target/debug/discrypt-signaling-server`
is built, the test starts that external binary on a random loopback port,
validates health, opens a `discrypt_quic_rendezvous` adapter profile against the
service API, negotiates a real WebRTC DataChannel, sends an opaque text/control
frame through `TextControlDataTransport`, and receives an opaque receipt frame
from the answerer runtime.

Verification run:

- `cargo fmt --all --check` — passed
- `cargo check -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter` — passed
- `timeout 180s cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter discrypt_rendezvous_sibling_service_runtime_pair_text_control_when_binary_is_available -- --nocapture` — passed
- `timeout 180s cargo test -q -p discrypt-transport --features discrypt-quic-rendezvous-adapter discrypt_rendezvous_sibling_service_roundtrip_when_binary_is_available -- --nocapture` — passed

Remaining gap: this is still loopback/local sibling-service evidence, not a
staged/deployed HTTPS/WSS rendezvous service proof with external TLS
certificate/public-key pinning and capture evidence. Native `quic://` transport
remains reserved; this adapter is still the sibling service API rendezvous path
for sealed signaling, not a replacement for WebRTC data/audio.

## 2026-05-30 update: public MQTT/Nostr role-split text runtime proofs passed

The public MQTT and Nostr adapters now have env-enabled role-split runtime
evidence at the transport boundary. In these gates the answerer runtime starts
first, the offerer runtime starts separately with reciprocal peer ids, the
selected public provider carries only sealed WebRTC negotiation payloads, a real
DataChannel opens, the offerer sends an opaque text/control frame through its
`TextControlDataTransport`, and the answerer returns an opaque receipt over the
same runtime.

Verification run:

- `DISCRYPT_PUBLIC_MQTT_ROLE_SPLIT_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 timeout 240s cargo test -q -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_role_split_text_runtime_roundtrip -- --nocapture` — passed
- `DISCRYPT_PUBLIC_NOSTR_ROLE_SPLIT_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol timeout 240s cargo test -q -p discrypt-transport --features nostr-adapter --test public_webrtc_datachannel_e2e public_nostr_role_split_text_runtime_roundtrip -- --nocapture` — passed

Remaining gap: this is transport-boundary role-split runtime evidence, not a
two-installed-Tauri-process GUI test and not remote voice/audio. IPFS public
topic-peer, deployed Discrypt rendezvous, credentialed TURN relay-only, installed
GUI two-window E2E, and real voice/audio capture/playback E2E remain open.

## 2026-05-30 update: public MQTT/Nostr encrypted media-frame gates passed

The public MQTT and Nostr adapters were also re-run through their encrypted
media-shaped WebRTC DataChannel gates. These tests prove provider-signaled
DataChannel delivery for codec-like opaque media frames plus receipts over public
rendezvous providers. They do **not** prove microphone capture, decoded remote
audio playback, jitter buffering, echo cancellation, or end-user voice-call UX.

Verification run:

- `DISCRYPT_PUBLIC_MQTT_MEDIA_WEBRTC_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 timeout 240s cargo test -q -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_signals_real_webrtc_media_frame_roundtrip -- --nocapture` — passed
- `DISCRYPT_PUBLIC_NOSTR_MEDIA_WEBRTC_E2E=1 DISCRYPT_PUBLIC_NOSTR_ENDPOINT=wss://nos.lol timeout 240s cargo test -q -p discrypt-transport --features nostr-adapter --test public_webrtc_datachannel_e2e public_nostr_signals_real_webrtc_media_frame_roundtrip -- --nocapture` — passed

Remaining gap: this is still media-frame transport evidence only. Real voice/audio
capture/playback E2E, TURN relay-only, installed GUI two-window E2E, public IPFS
topic-peer, and deployed Discrypt rendezvous remain open.

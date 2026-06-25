# Phase 8 Relay Protocol Spec

PER-66 defines the protocol contract for Discrypt's adaptive encrypted
peer-assisted relay route. The local transport model now covers the protocol
schema, relay authorization, candidate ranking, route selection, and opaque
forwarding envelope construction. It still does not open network sockets, claim
split-machine production delivery, or expose any decrypt/key path.

## Scope

The peer overlay carries protected application envelopes between already
admitted group peers when direct WebRTC cannot reach every admitted recipient
and configured TURN is unavailable, undesirable, or reserved as the final
fallback. The route remains peer-assisted and E2EE:

- MQTT, Nostr, IPFS PubSub, and Discrypt QUIC rendezvous providers remain
  signaling/rendezvous only.
- Providers may carry presence and sealed WebRTC negotiation/control material
  defined by existing transport adapters, but must not carry overlay
  application frames as a relay fallback.
- Relay peers forward opaque ciphertext envelopes and route metadata only.
  They never receive plaintext, content keys, MLS exporter material, SFrame
  keys, or a decrypt capability. `build_peer_overlay_forwarding_plan` emits
  per-hop forwarding envelopes from already protected text/control or media
  frames after admission, epoch, authority, provider-boundary, TTL, and
  relay-visible plaintext-marker checks pass.
- Relay eligibility is bounded to peers that the backend/OpenMLS state proves
  admitted in the current group epoch. Invite parsing, pending admission, and
  stale route graph state are not relay authority.

## Frame Format

The transport type skeleton is `PeerOverlayFrame` in
`crates/transport/src/peer_overlay.rs`. Its wire schema is intentionally
versioned and opaque:

| Field | Purpose |
| --- | --- |
| `schema_version` | Currently `1`; future incompatible changes must bump it. |
| `route` | Source, destination, relay path, TTL, and loop id. |
| `auth` | OpenMLS group/epoch binding, sender leaf, confirmation-tag commitment, and frame authentication tag. |
| `delivery` | Ack id, ack requirement, redelivery deadline, attempt cap, and relay fanout cap. |
| `payload` | Payload kind plus protected opaque bytes. |

`payload.opaque_ciphertext` is not decoded by transport. Upper layers own
content encryption, SFrame/media authentication, text/control authentication,
anti-replay, and OpenMLS exporter-derived keys.

## Peer Refs

Every frame names three classes of refs:

- `source`: admitted current-epoch peer/device that created the protected
  envelope.
- `relay_path`: one or more admitted current-epoch peer/devices allowed to
  forward the opaque frame in order.
- `destination`: admitted current-epoch peer/device that can authenticate and
  decrypt the protected payload.

Each `PeerOverlayPeerRef` includes:

- `peer_id`: transport-level peer id.
- `member_id`: backend/OpenMLS-governed member id.
- `device_id`: backend/OpenMLS-governed device id.
- `epoch`: group epoch for which this peer is admitted.

Validation fails closed if any ref is not in the supplied admitted peer set,
has a stale epoch, is revoked, lacks explicit relay authority when acting as a
relay hop, or appears in a structurally unsafe position: source equals
destination, a relay equals source/destination, or a relay appears twice in the
loop path.

## Epoch And Auth Binding

`PeerOverlayAuth` binds a frame to:

- a group id commitment,
- the current OpenMLS epoch,
- the source sender leaf index,
- the current confirmation-tag commitment, and
- a frame auth tag produced by the protected-envelope layer.

The transport skeleton validates that commitments and auth tag are present and
that the frame epoch matches the current admitted-peer set. It does not verify
OpenMLS signatures or decrypt payloads. Future runtime work must verify those
bindings against persisted OpenMLS group state before enqueueing or forwarding.

## TTL And Loop ID

`PeerOverlayTtl` carries:

- `remaining_hops`, bounded by `PEER_OVERLAY_MAX_RELAY_HOPS` (`3`),
- `expires_at_ms`, a wall-clock expiry timestamp owned by the sender/runtime,
- `created_at_ms`, used only to reject inverted expiry windows.

`PeerOverlayLoopId` is a 128-bit opaque id generated per logical send attempt.
Receivers and relays use it with the route path and ack id to suppress loops and
duplicates. The skeleton rejects an all-zero loop id and duplicate relay refs;
future forwarding runtime must also keep a bounded seen-loop cache.

## Ack And Redelivery

`PeerOverlayDelivery` carries an `ack_id`, an ack mode, and a redelivery policy:

- `AckRequired`: destination must authenticate and return a receipt/ack over an
  already authorized direct/TURN/overlay control path.
- `BestEffort`: no destination ack required; suitable only for future
  low-value control hints, not protected group text delivery.

`PeerOverlayRedelivery` bounds retries:

- `max_attempts` is normalized by callers and must be non-zero.
- `max_relay_fanout` must be non-zero and cannot exceed the hop bound.
- `deadline_ms` must not exceed frame TTL expiry.

Missing acks do not authorize provider fallback. Redelivery may retry eligible
admitted relays only after route selection and relay authorization are
implemented in a later task.

## Revocation Behavior

Revocation is fail-closed:

- A revoked source cannot create an accepted frame.
- A revoked destination cannot be named for delivery.
- A revoked relay cannot be named as a forwarder.
- A stale route graph edge or invite-derived peer ref cannot authorize relay
  forwarding without a current backend/OpenMLS relay authority set or an
  already-verified signed governance relay grant.
- Frames from prior epochs are rejected unless a future task adds an explicit,
  audited catch-up mode tied to OpenMLS state. This spec does not define such a
  catch-up mode.

Route cleanup from Phase 7 remains authoritative for direct/TURN runtime state.
This spec adds no alternate delivery success state for revoked or removed peers.

## Provider Boundary

`PeerOverlayCarrier::ProviderApplicationRelay` exists only as a forbidden enum
variant so tests and future callers can fail closed explicitly. Valid future
carriers are:

- `DirectWebRtcDataChannel`
- `ConfiguredTurnBackedWebRtc`
- `PeerAssistedOverlay`

Provider signaling remains limited to rendezvous, presence, and sealed WebRTC
negotiation/control. It is never overlay application relay evidence.

## Opaque Forwarding

PER-70 adds the local opaque forwarding boundary in
`crates/transport/src/peer_overlay.rs`:

- `PeerOverlayForwardingPolicy` requires `PeerAssistedOverlay` and can carry
  test/audit-only forbidden relay-visible markers.
- `build_peer_overlay_forwarding_plan` first validates the frame and explicit
  current-epoch relay authority, then emits ordered `PeerOverlayForwardedHop`
  envelopes for source-to-relay and relay-to-destination legs.
- Each hop carries peer refs, ack id, loop id, payload kind, protected sequence,
  remaining relay-hop count, and `PeerOverlayOpaquePayload::relay_visible_bytes`.
- The function supports encrypted text/control and encrypted media payload
  classes only. Store-forward queues remain out of scope for a later storage
  task.
- Provider application relay carriers, stale/revoked/non-authorized relay refs,
  insufficient TTL, unsupported payload classes, and configured forbidden
  relay-visible markers fail closed before any hop is emitted.
- The returned evidence records `provider_application_relay_used = false` and
  `decrypt_path_exposed = false`; this is a local model assertion, not a
  production network proof.

## Out Of Scope

The local Phase 8 model still does not implement:

- store-forward queues,
- voice/media expansion,
- UI route claims,
- split-machine or release-gate proof.

PER-67 adds local Rust model/unit evidence for explicit relay authorization.
PER-68 adds local candidate ranking that first requires current admitted-peer
state and explicit relay authority, then ranks content-blind diagnostics by
latency, health stability, spare capacity, energy cost, and freeload penalty
with deterministic peer-identity ties. PER-69 adds local route-selection model
evidence: direct WebRTC is preferred; configured TURN can be ordered before or
after peer-assisted relay by explicit policy; and relay selection requires the
top ranked authorized current-epoch relay plus two live non-provider route legs
for source-to-relay and relay-to-destination. PER-70 adds local opaque
forwarding envelopes for encrypted text/control and media frames. Production
evidence will require later runtime tasks that open real peer routes, exercise
split-machine delivery, prove fail-closed revocation under live group state,
and preserve no-provider application relay under live adapters.

# Phase 6 connectivity, signaling, push, and metadata review

G007 implements deterministic foundations for the approved Phase 6 scope: a
content-blind signaling reference, strict STUN -> peer-overlay -> TURN fallback,
content-free Android FCM wake envelopes, and pcap-style metadata matrix fixtures.

## Implementation map

- `external/signaling-repository/src/lib.rs`
  - `ReferenceSignalingServer` stores only opaque rendezvous blobs in memory.
  - request endpoint/IP metadata is transient and deliberately not included in
    at-rest records.
  - `MetadataMatrix` encodes the approved §0 infrastructure matrix.
  - `AuditFixture` models pcap-style observations for AC15/AC18/AC-METADATA.
- `crates/transport/src/lib.rs`
  - `ConnectivityPlanner` tries fallback legs in strict STUN -> relay-overlay ->
    TURN order under deterministic NAT scenarios.
  - `EndpointOverrides` allow owner/group STUN and TURN endpoints.
  - overlay and TURN legs are marked ciphertext-only and never content-carrying.
- `crates/push/src/lib.rs`
  - `AndroidWakeService` creates FCM Android wake envelopes with hashed tokens,
    coarse wake reason, nonce, and no room/user/message/plaintext fields.
  - provider-visible bytes are auditable for forbidden content/identity tokens.
- `harness/multinode/src/lib.rs`
  - `connectivity_signaling_push_smoke` covers content-blind signaling, all
    fallback legs, endpoint overrides, content-free Android wake, pcap no-content
    checks, and the metadata matrix.

## Acceptance coverage

- AC13: STUN succeeds first, overlay activates when STUN is blocked, and TURN
  activates when both STUN and overlay are blocked; owner STUN/TURN overrides are
  honored.
- AC15: FCM Android wake is content-free and exposes no room/user/message token in
  the provider-visible audit bytes.
- AC18: signaling inspection shows no persisted identity <-> room <-> topology
  linkage; rendezvous data is opaque and expired/fetched blobs are removed.
- AC-METADATA: fixture rows match the approved matrix: signaling/STUN/push expose
  no content, TURN/peer/volunteer relays expose ciphertext only, and no fixture row
  persists durable linkage.

## Production-hardening notes

- The signaling reference is an in-memory deterministic model. Production should
  preserve the same at-rest shape while adding authentication, expiry cleanup, and
  abuse throttles around publish/fetch.
- The fallback planner is policy, not socket code. Native QUIC/ICE/TURN plumbing
  must feed real reachability results into the same ordered policy and keep TURN
  strictly ciphertext-only.
- The pcap fixture is a release-gate oracle for forbidden bytes and metadata-row
  shape; it is not a traffic-analysis anonymity claim. The product claim remains
  metadata-minimizing, not metadata-anonymous.

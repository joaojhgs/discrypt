# ADR-007: Pcap and forbidden-byte tooling

## Status

Accepted for the production E2E P2P overlay mesh launch gate.

## Context

Discrypt's production plan requires network-observation evidence that signaling,
STUN/TURN, overlay relay, store-forward relay, push, text, and voice paths do not
expose identity, room, topology, message plaintext, media plaintext, content keys,
MLS epoch secrets, SFrame keys, or WebRTC negotiation material beyond the approved
metadata matrix. The launch hint asks for capture tooling, redaction rules,
forbidden token generation, CI artifact storage, and pass/fail thresholds.

## Decision

The launch gate uses the deterministic repository-local G096 pcap acceptance
suite as the CI capture gate and reserves external host packet captures for the
final release-run evidence package:

- `AuditFixture` and `PcapEvent` in `../discrypt-signaling/src/lib.rs` are the shared
  pcap-style row model for provider-visible bytes.
- `MetadataMatrix::approved_v1` is the exact expected exposure matrix for
  Signaling, STUN, TURN, Push FCM, PeerRelay, and VolunteerStorageRelay rows.
- `../discrypt-signaling/tests/process_webrtc_transport_paths.rs` drives direct,
  overlay, and TURN modes through separate peer processes and records
  ciphertext-only pcap-style rows.
- `../discrypt-signaling/tests/process_signal_exchange.rs` starts a real local
  signaling server process and client processes, then calls the admin at-rest
  audit endpoint.
- `harness/multinode/src/lib.rs::pcap_acceptance_matrix_smoke` combines AC1,
  AC8, AC15, AC18, AC-METADATA, and forbidden-token scanner coverage.
- `pcap_forbidden_byte_tooling_decision()` is the code-level launch decision and
  `covers_adr_007()` is the executable coverage assertion for this ADR.

This ADR does not claim that external libpcap/tcpdump captures have already run
on multiple physical hosts. Those captures are required release-run artifacts and
must be stored only after redaction.

## Capture tooling

The accepted capture tooling stack is:

1. Deterministic pcap-style rows from `AuditFixture`/`PcapEvent` for all CI gates.
2. Separate-process WebRTC-path tests for direct, overlay, and TURN transport.
3. Separate-process signaling server/client tests for publish/take/admin-audit.
4. Android wake provider-visible-byte checks through `discrypt-push`.
5. A later release-run external capture layer using libpcap/tcpdump or OS-native
   capture tooling, with tool version, command line, interface, duration, and
   redaction evidence recorded in the release dashboard.

## Forbidden-token generation

The scanner must generate and scan sentinel classes for:

- identity alias and device identity material;
- friend code and safety number;
- room/group name and topology link labels;
- message plaintext and push/message identifiers;
- media sample and encoded media sentinel text;
- content key material;
- MLS epoch secret material;
- SFrame key material;
- WebRTC SDP/candidate strings such as `v=0`, `candidate:`, `ice-ufrag`,
  `ice-pwd`, `fingerprint`, loopback/private endpoints, and username fragments.

`contains_any_token` is positive-tested with forbidden message and MLS epoch
secret samples and negative-tested with sealed protected ciphertext so ciphertext
visibility is not treated as a plaintext leak by itself.

## Redaction rules

Audit and CI output must be safe to upload:

- never echo forbidden bytes, raw opaque payloads, identities, room names,
  topology labels, raw SDP, raw candidates, media samples, or packet bytes;
- return counts and booleans only for admin/audit APIs, including `zero_linkage`,
  `at_rest_records`, and `forbidden_tokens_scanned`;
- store ciphertext samples only when forbidden-byte scanning has passed and the
  row's `ContentExposure` is `CiphertextOnly`;
- scrub external packet captures before retention; raw packet captures are forbidden in normal CI artifacts.

## CI artifact storage

CI may retain only redacted artifacts:

- pcap matrix rows with `InfrastructureComponent`, `ContentExposure`, endpoint
  and timing booleans, and `persists_linkage`;
- command names and exit codes;
- tool/package versions;
- forbidden-token class names and counts, never values;
- audit booleans/counts;
- reviewer sign-off or release-dashboard links.

CI artifacts must not contain raw packet captures, raw opaque payloads, raw SDP,
raw candidates, room names, identity values, message plaintext, media plaintext,
or key material. External libpcap/tcpdump files are release-run artifacts and
must be redacted before storage.

## Pass/fail thresholds

A pcap/forbidden-byte gate passes only when all thresholds hold:

1. zero forbidden-byte matches across all provider-visible bytes.
2. Every pcap-style row matches `MetadataMatrix::approved_v1` exactly.
3. Signaling at-rest audit reports expected `at_rest_records` and
   `forbidden_tokens_scanned` counts without echoing requested tokens.
4. AC1 has explicit safety-number verification and no directory/account provider.
5. AC8 relay/TURN/media rows are `CiphertextOnly` and do not persist linkage.
6. AC15 push wake bytes are content-free.
7. AC18 signaling at-rest records have zero identity-room-topology linkage and
   expired/taken records are pruned.
8. Release claims fail if required capture artifacts, redaction metadata, package
   versions, or reviewer sign-off are missing.

## Verification

Required gates for this decision:

1. `cargo test --manifest-path ../discrypt-signaling/Cargo.toml -p discrypt-signaling pcap_forbidden_byte_tooling_decision_covers_adr_007 --quiet`
   proves the code-level launch decision covers capture tooling, redaction,
   forbidden-token generation, artifact storage, and thresholds.
2. `cargo test -p discrypt-multinode-harness pcap_acceptance_matrix_covers_ac1_ac8_ac15_ac18_and_metadata --quiet`
   proves the acceptance matrix and forbidden scanner are wired.
3. `cargo test --manifest-path ../discrypt-signaling/Cargo.toml -p discrypt-signaling --test process_webrtc_transport_paths --quiet`
   proves direct, overlay, and TURN process paths produce ciphertext-only rows.
4. `cargo test --manifest-path ../discrypt-signaling/Cargo.toml -p discrypt-signaling --test process_signal_exchange --quiet`
   proves local server/client exchange and redacted admin at-rest audit.
5. `cargo test -p discrypt-push android_wake_envelope_is_content_free --quiet`
   proves Android wake provider bytes are content-free.
6. `npm --prefix apps/ui run test:pcap-suite-g096` proves the documented suite
   and executable gates stay aligned.

## Consequences

- CI has deterministic leakage gates before release-run external captures.
- Release dashboards can store enough evidence for reviewers without retaining
  sensitive capture bytes.
- Any future external capture tooling must preserve the redaction and threshold
  contract here before it can support a production release claim.

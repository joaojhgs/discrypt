# G096 pcap acceptance suite

G096 adds a deterministic pcap acceptance suite for the release-gated network-observation claims in the original Discrypt plan. The suite is intentionally explicit about its current evidence level: it verifies provider-visible bytes, process-harness captures, and pcap-style audit rows in this repository; external host packet captures remain a later release-run artifact.

## Acceptance criteria covered

| Acceptance criterion | Suite evidence | Pass condition |
| --- | --- | --- |
| AC1 — identity/DM + verify | `pcap_acceptance_matrix_smoke` verifies command-backed safety-number comparison and records only signaling/STUN observations for DM setup. | Safety number is explicitly verified, no directory/account component is present in the fixture, and forbidden identity/message/key tokens do not egress. |
| AC8 — relays cannot decrypt | `voice_media_e2e_smoke`, `text_history_delivery_smoke`, and `connectivity_signaling_push_smoke` feed relay/TURN observations into the pcap-style matrix. | Peer relay and TURN rows are `CiphertextOnly`; forbidden PCM, Opus, text plaintext, SFrame key, MLS epoch secret, and content-key tokens are absent. |
| AC15 — Android wake | `connectivity_signaling_push_smoke` builds a `discrypt-push` FCM Android wake envelope and scans provider-visible bytes. | Push bytes contain hashed/coarse wake state only and no room, sender, message body, identity, or content token. |
| AC18 — signaling zero metadata at rest | Signaling process tests and `connectivity_signaling_push_smoke` audit opaque rendezvous records. | At-rest audit reports zero identity-room-topology linkage and expired/taken records are not retained. |
| AC-METADATA | `AuditFixture` rows are validated against `MetadataMatrix::approved_v1`. | Signaling/STUN/push expose no content; TURN/peer/volunteer relay rows expose ciphertext only; no row persists identity-room-topology linkage. |

## Test entry points

- `cargo test -p discrypt-multinode-harness pcap_acceptance_matrix_covers_ac1_ac8_ac15_ac18_and_metadata --quiet`
- `cargo test -p external-signaling --test process_webrtc_transport_paths --quiet`
- `cargo test -p external-signaling --test process_signal_exchange --quiet`
- `cargo test -p discrypt-push android_wake_envelope_is_content_free --quiet`
- `npm --prefix apps/ui run test:pcap-suite-g096`

## Forbidden-byte sentinel classes

The G096 suite scans identity, friend-code/safety-number, message plaintext, media sample, content-key, MLS epoch secret, SFrame key, room, topology, and push/message sentinel classes. The scanner is positive-tested so a known forbidden token is detected and a sealed ciphertext sample is not rejected merely for being network-visible.

## Current limitation

This is a repository-local deterministic pcap suite. It does not claim external libpcap/tcpdump capture from multiple physical hosts. The later full production E2E gate must store the external capture artifact, command output, package versions, and reviewer sign-off in the release dashboard.

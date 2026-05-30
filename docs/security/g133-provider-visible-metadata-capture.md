# G133 provider-visible metadata capture gate

G133 adds the adapter-specific provider-visible metadata capture gate for MQTT,
Nostr, IPFS/libp2p PubSub, and the separate Rust QUIC rendezvous boundary. It is
an extension of the repository-local G096 pcap suite: it scans bytes that would be
visible to each signaling provider/relay and verifies that display names, group
names, topology labels, plaintext messages, raw SDP, ICE credentials, TURN
secrets, and media markers do not appear.

## Scope

Covered adapters:

- MQTT (`mqtt`)
- Nostr (`nostr`)
- IPFS/libp2p PubSub (`ipfs_pubsub`)
- Discrypt Rust QUIC rendezvous boundary (`discrypt_quic_rendezvous`)

The local conformance adapter captures provider-visible publishes for all four
adapter kinds using the same signaling trait surface as production adapters. The
QUIC adapter remains fail-closed for real networking, but its provider-visible
boundary is still included in the conformance capture so future client wiring
inherits the no-plaintext contract.

## Required executable evidence

- `cargo test -q -p discrypt-transport local_conformance_adapters_deliver_opaque_dm_payloads_without_plaintext_leaks -- --nocapture`
- `cargo test -q -p discrypt-transport local_conformance_adapter_rejects_plaintext_sdp_and_ice_markers -- --nocapture`
- `npm --prefix apps/ui run test:provider-metadata-capture-g133`

## Pass condition

The gate passes only when:

1. all four required adapter kinds are represented in the conformance capture;
2. provider-visible material contains only derived rendezvous topics, peer ids,
   payload kinds, and opaque/ciphertext payload bytes;
3. raw display names, group/channel labels, raw WebRTC SDP, `ice-ufrag`,
   `ice-pwd`, candidates, TURN secrets, message plaintext, and media markers are
   absent;
4. the plaintext-rejection test rejects raw SDP/ICE/control samples before they
   reach provider-visible state; and
5. release documentation keeps external libpcap/tcpdump capture as a separate
   release-run artifact rather than claiming it has already passed.

## Current limitation

This is deterministic repository-local provider-visible capture. It does not
replace the later external host packet-capture artifact for the full production
release run, and it does not prove IPFS public-swarm availability or the future
real QUIC sibling-service client.

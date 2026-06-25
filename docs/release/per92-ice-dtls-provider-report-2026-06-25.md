# PER-92 ICE/DTLS/Provider Report

PER-92 adds a redacted transport diagnostic report for provider-signaled WebRTC setup. The report classifies synthetic failure evidence as:

- `provider_missing`
- `offer_missing`
- `answer_missing`
- `candidate_missing`
- `ice_failed`
- `dtls_failed`
- `data_channel_failed`
- `turn_required`

The report is diagnostic-only. It does not mark a session connected, delivered, admitted, voice-active, or production-ready.

## Redaction and Provider Boundary

The report records booleans and state labels from existing redacted timelines and probe evidence. It does not serialize raw SDP, ICE credentials, TURN endpoint URLs, TURN credentials, frame bytes, message bodies, media, or key material.

`provider_application_relay_used` remains `false`. MQTT, Nostr, IPFS PubSub, and Discrypt QUIC rendezvous providers remain signaling/rendezvous only.

## Evidence Level

This is local synthetic Rust/backend diagnostics evidence. It is not split-machine public-provider proof, installed-app support workflow evidence, or production-ready WebRTC route evidence.

## Verification

Planned local verification:

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport ice_dtls_provider_report`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml transport_diagnostics --lib -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `git diff --check`

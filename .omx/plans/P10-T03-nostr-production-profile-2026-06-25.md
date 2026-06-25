# P10-T03 Nostr Production Profile Plan

Source issue: PER-84 / P10-T03, Phase 10 signaling adapters, public profiles, and abuse/privacy.

Present plan anchors:
- `docs/release/handoff-2026-06-10-current-state.md`: Discrypt is not production-ready; provider state must remain backend-proven and signaling-only.
- `.omc/plans/discrypt-plan.md`: signaling/rendezvous is content-blind, metadata-minimizing, and must not carry app messages or media.
- The issue body supplies the missing Phase 10 task text because `.omx/plans/production-release-master-plan-2026-06-10.md` is not present in this checkout.

## Requirements

- Public/custom Nostr relay lists must validate and work through the existing adapter profile/factory contract.
- Nostr provider-visible data must stay limited to endpoint metadata, derived hashed topic/tag, and opaque sealed WebRTC/presence envelopes.
- Nostr must fail closed for malformed relay profiles, oversized provider envelopes, provider errors/rate limits/auth failures, and attempted application/control relay.
- Verification must include local harness tests and a public Nostr relay path when network access allows.

## Code Paths

- `crates/transport/src/provider_adapters.rs`: `NostrProviderAdapter`, `NostrProviderRoom`, Nostr event encoding, provider failure mapping, adapter tests.
- `apps/desktop/src-tauri/src/lib.rs`: `SignalingProfileView`, default Nostr profile generation, signed endpoint allowlist/backoff policy validation, desktop adapter tests.
- `crates/transport/examples/split_machine_p2p.rs`: public Nostr split-machine evidence path and provider-visible boundary copy.
- `docs/release/`: fresh PER-84 evidence note.

## Implementation Steps

1. Harden Nostr relay profile validation in transport so production endpoints require `wss://`, local-dev endpoints require loopback `ws://`, duplicate relays are removed deterministically, and empty/malformed relay sets fail before connecting.
2. Add Nostr provider envelope size validation before publishing, using each configured endpoint's `max_message_bytes` with a fail-closed typed `provider_message_too_large` error.
3. Add tests for custom/public relay lists, invalid relay schemes/security, duplicate relay dedupe, privacy-safe event tags/content, oversized payload failure, and disabled application relay.
4. Update desktop default/custom profile tests so the generated Nostr production profile carries a public relay list, signed allowlist commitments for every relay, backoff/message caps, and validates through `transport_profile_from_view`.
5. Write a concise release evidence artifact with commands, local/public evidence level, and skipped checks if any.

## Failure Modes And Safety

- Invalid public relay (`ws://` non-loopback, `http://`, `nostr://`, empty host) fails as `InvalidConnectivityPolicy` before any network connection.
- Oversized Nostr envelope fails as `SignalingAdapter` with `failure_class=provider_message_too_large` and no payload echo.
- Provider relay notices/closed/failed OK messages map to typed readiness and surface a recovery class without raw payloads.
- `broadcast_control`/`take_control_payloads` continue to reject provider application relay attempts.
- Rollback is low risk: changes are scoped to validation and tests; no storage or MLS state migration.

## Acceptance Criteria

- A Nostr profile with multiple public/custom relays preserves all validated relays and signs allowlist commitments for each.
- A Nostr local relay profile accepts loopback `ws://127.0.0.1...`; a public `ws://` relay fails closed.
- Provider-visible Nostr events use only the custom event kind and `d` tag with a hashed topic, and content remains base64-encoded opaque envelope data without raw SDP, ICE credentials, names, or plaintext markers.
- Oversized Nostr provider envelopes fail closed with typed `provider_message_too_large` diagnostics.
- Application relay paths over Nostr remain disabled.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features nostr-adapter nostr_`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --features nostr-adapter default_profiles_carry_provider_allowlist_and_rotation_policy default_profiles_omit_unconfigured_ipfs_quic_placeholder_endpoints -- --test-threads=1`
- Public relay check where network access permits: targeted `discrypt-transport` public/local Nostr test or split-machine example against `DISCRYPT_PUBLIC_NOSTR_ENDPOINT`.

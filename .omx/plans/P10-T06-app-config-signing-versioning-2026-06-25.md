# P10-T06 App Config Signing/Versioning Plan - 2026-06-25

## Source Scope

- Issue: PER-87 / P10-T06, Phase 10 signaling adapters, public profiles, and abuse/privacy.
- Master-plan context from issue body: public defaults must be signed/versioned and endpoint allowlists must be updateable without weakening signaling-only provider boundaries.
- Current handoff: `docs/release/handoff-2026-06-10-current-state.md` says production readiness requires fresh evidence and UI/backend state must not overclaim route, membership, or transport truth.
- Current code anchors: `apps/desktop/src-tauri/src/lib.rs` owns `SignalingProfileView`, default app connectivity, endpoint allowlist commitments, and conversion through `transport_profile_from_view`; `crates/admission/src/lib.rs` already signs invite bootstrap profile allowlist/version fields.

## Acceptance Criteria

- App public signaling defaults are loaded only from a verified signed app-config envelope with schema and config versions.
- Signed app-config updates can replace endpoint allowlists, but stale, unsigned, downgraded, malformed, wrong-key, or tampered updates fail closed.
- Generated `SignalingProfileView` values still carry endpoint allowlist commitments and provider rotation policy before reaching transport.
- Provider adapters remain signaling/rendezvous only; no app text/control/media relay path is added.
- Regression tests cover signature verification, endpoint tampering, downgrade/staleness, missing config, and signed allowlist updates.

## Implementation Steps

1. Add a small signed app-config envelope in `apps/desktop/src-tauri/src/lib.rs` for provider endpoint defaults, using the existing Ed25519 dependency and deterministic canonical bytes.
2. Change `default_adapter_endpoints` / `default_signaling_profiles` to read from verified built-in defaults or an optional signed JSON update, rather than trusting raw endpoint env updates.
3. Preserve the existing `validate_provider_policy` allowlist-commitment gate so signed config verification and transport-profile validation both have to pass.
4. Add targeted desktop tests for valid built-in defaults, signed update acceptance, tampered endpoint/signature/key rejection, stale/downgraded rejection, and no unconfigured IPFS/QUIC placeholders.
5. Update release evidence docs for PER-87 with commands and scope boundaries.

## Failure Modes And Safety

- Missing or malformed signed update: fail closed to the verified built-in defaults for app defaults; malformed explicit signed-update parsing returns a typed error in tests and logs no provider secrets.
- Tampered endpoint or commitment: signature verification or `transport_profile_from_view` rejects before any provider connection.
- Downgraded/stale config: rejected before endpoints are surfaced.
- Provider relay invariant: config only selects signaling endpoints; data-plane delivery remains WebRTC/TURN/overlay-gated elsewhere.

## Verification Strategy

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- Targeted desktop signature/config tests:
  `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml app_config_ -- --test-threads=1`
- Existing default-profile regression tests:
  `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml default_profiles_ -- --test-threads=1`
- `git diff --check`

This is local/backend config evidence. It does not claim full production readiness, public-provider soak, packaging, OpenMLS admission, voice, overlay, or provider-visible packet-capture closure.

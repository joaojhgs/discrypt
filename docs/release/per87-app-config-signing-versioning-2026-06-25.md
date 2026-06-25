# PER-87 App Config Signing/Versioning Evidence - 2026-06-25

## Scope

PER-87/P10-T06 adds a signed, versioned app-config boundary for public
signaling defaults and endpoint allowlist updates.

This evidence covers the desktop backend config/default-profile layer only. It
does not claim installed-app, split-machine, public-provider soak, packaging,
OpenMLS admission, voice/media, overlay relay, or provider-visible packet
capture readiness.

## Implemented Behavior

- Built-in public app signaling defaults are represented as a versioned
  `SignedAppConfigEnvelope`.
- The envelope is verified with the embedded Ed25519 public key before default
  provider endpoints are surfaced.
- Explicit signed updates use `DISCRYPT_SIGNED_APP_CONFIG_JSON`; malformed,
  empty, stale, downgraded, wrong-key, or tampered updates fail closed instead
  of silently falling back to public defaults.
- Generated `SignalingProfileView` values still carry
  `provider_policy_version`, endpoint allowlist commitments, and provider
  rotation policy before conversion through `transport_profile_from_view`.
- Raw unsigned `DISCRYPT_DEFAULT_*` endpoint env vars no longer enable public
  app default provider profiles.
- MQTT, Nostr, IPFS/libp2p, and Discrypt QUIC remain signaling/rendezvous
  profile selectors only; no provider application-message or media relay path
  was added.

## Verification

Commands run locally:

- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml app_config_ -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml default_profiles_ -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml unsigned_ipfs_default_env_cannot_enable_app_default_profile -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --lib -- -D warnings`
- `git diff --check`

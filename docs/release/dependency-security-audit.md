# Dependency security audit

Date: 2026-05-30

Latest command run:

```bash
cargo audit --json > /tmp/discrypt-cargo-audit-libp2p-patched.json
```

Result: **vulnerability-clean but not release-complete**. After the libp2p 0.56, MQTT client, direct-IPFS, MLS/libcrux, and libp2p umbrella metadata remediation slices, `cargo audit` exits zero with **0 vulnerability hits**. G122 now enforces a zero-vulnerability policy and an exact warning watchlist in `docs/security/g122-rust-advisory-waivers.md`; the remaining production release work is replacing or target-scoping those unmaintained/unsound warnings plus closing the broader app/device/media E2E gaps.

## What improved in these slices

- Upgraded the optional IPFS/libp2p adapter stack from `libp2p 0.54.1` to `0.56.0`.
- Removed the old `ring 0.16.20` finding from the resolved lockfile.
- Removed the old `rustls-webpki 0.101.7` finding from the resolved lockfile.
- Moved libp2p TLS dependencies onto newer `libp2p-tls 0.6.2` / `rcgen 0.13.2` era packages.
- Kept the IPFS adapter compiling and passing its targeted conformance tests after the upstream gossipsub API changed `PublishError::InsufficientPeers` to `PublishError::NoPeersSubscribedToTopic` and `unsubscribe` now returns `bool`.
- Migrated the MQTT adapter from `rumqttc 0.25.1` to the `rumqttc-next 0.33.2` facade, removing the vulnerable `rustls-webpki 0.102.8` MQTT TLS edge.
- Preserved public MQTT signaling and public MQTT-signaled WebRTC DataChannel proof by adapting the MQTT v5 client API and explicit TLS transport configuration.
- Raised workspace Rust MSRV to `1.89` because `rumqttc-next 0.33.2` declares `rust-version = 1.89`.
- Removed the production IPFS adapter's active `libp2p-dns` path by dropping the `dns` feature and disabling `/dnsaddr` default bootstrap while the Hickory DNS stack remains audit-blocked.
- Patched the MLS HPKE libcrux edge with a local vendored `hpke-rs-libcrux 0.6.1+discrypt.1` metadata-only source patch that keeps upstream Rust source unchanged and updates `libcrux-aead`/`libcrux-chacha20poly1305` to `0.0.8`, removing `RUSTSEC-2026-0124` from the resolved audit result.
- Patched the libp2p umbrella crate with a local vendored `libp2p 0.56.0+discrypt.1` metadata-only source patch that removes unused optional `libp2p-dns`/`libp2p-mdns` lockfile edges while Discrypt production IPFS remains direct-address-only, removing both Hickory advisories from the lockfile.

## Remaining vulnerability blockers

None in the current `cargo audit` vulnerability list. Release is still blocked by warning triage and by the broader production E2E gaps tracked in `docs/release/public-signaling-production-status.md`.

## Remediated vulnerability blockers

| Advisory | Prior edge | Remediation | Verification |
| --- | --- | --- | --- |
| RUSTSEC-2026-0124 | `openmls_rust_crypto 0.5.1` -> `hpke-rs 0.6.1` -> `hpke-rs-libcrux 0.6.1` -> `libcrux-aead 0.0.7` -> `libcrux-chacha20poly1305 0.0.7` | Added `[patch.crates-io] hpke-rs-libcrux = { path = "third_party/hpke-rs-libcrux-0.6.1-discrypt" }`; vendored crate only changes dependency metadata to `libcrux-aead 0.0.8` / `libcrux-traits 0.0.7` and records `0.6.1+discrypt.1`. | `cargo test -q -p discrypt-mls-core`; latest `cargo audit` no longer reports `libcrux-chacha20poly1305`. |
| RUSTSEC-2026-0119 / RUSTSEC-2026-0118 | `libp2p 0.56.0` umbrella optional package metadata locked `libp2p-dns`/`libp2p-mdns`, which locked `hickory-proto 0.25.2`, even though Discrypt no longer enabled DNS at runtime | Added `[patch.crates-io] libp2p = { path = "third_party/libp2p-0.56.0-discrypt" }`; vendored crate only changes dependency metadata to make `dns`/`mdns` empty, remove those features from `full`, remove tokio forwarding, and remove the optional DNS/mDNS dependency tables. | `cargo check -q -p discrypt-transport --features ipfs-pubsub-adapter`; `cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub -- --nocapture`; `cargo audit` exits zero with 0 vulnerabilities. |


## Previous attempted remediation

```bash
cargo update -p hickory-proto --precise 0.26.1
```

Cargo rejected that direct update because `libp2p-mdns 0.48.0` required `hickory-proto ^0.25.2`. Removing the production `libp2p/dns` feature proved the active Discrypt IPFS graph did not use Hickory; the follow-up local libp2p metadata patch removed the unused optional DNS/mDNS lockfile edges so `cargo audit` no longer reports Hickory.

## Warning blockers

`cargo audit` also reports unmaintained/unsound packages, including GTK3-era Tauri stack packages (`atk`, `gdk`, `gtk`, `gtk-sys`, etc.), `instant`, `paste`, `proc-macro-error`, `unic-*`, and `glib`. G122 now requires every current warning ID to appear in the warning watchlist with owner, expiry, disposition, and upgrade path. Production packaging still needs replacement evidence or target-scoped acceptance for the retained warning set.

## Required completion steps

- Replace the local `libp2p` metadata patch with direct subcrate dependencies or an upstream libp2p release that no longer locks vulnerable DNS/mDNS packages; keep `dns`/`mdns` disabled in Discrypt production until that migration is audited.
- Keep IPFS public defaults disabled until DNS/topic-peer discovery is remediated; only explicit direct-address profiles should be accepted in production builds while Hickory remains unresolved.
- Replace the local `hpke-rs-libcrux` patch with an upstream patched release as soon as one exists and passes the same MLS verification gates.
- Replace or target-scope every retained unmaintained/unsound warning before a production release candidate.
- Re-run `cargo audit`; keep the production dependency/security gate open until vulnerability output remains zero and the warning watchlist is current, owner-assigned, and release-reviewed.

## Verification evidence for the dependency remediation slices

```bash
cargo check -q -p discrypt-transport --features ipfs-pubsub-adapter
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub -- --nocapture
cargo check -q -p discrypt-transport --features ipfs-pubsub-adapter,mqtt-adapter,nostr-adapter
cargo check -q -p discrypt-desktop --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter
cargo test -q -p discrypt-mls-core
DISCRYPT_PUBLIC_SIGNALING_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 cargo test -q -p discrypt-transport --features mqtt-adapter --test public_signaling_e2e public_mqtt_two_peer_presence_signal_and_control_roundtrip -- --nocapture
DISCRYPT_PUBLIC_MQTT_WEBRTC_E2E=1 DISCRYPT_PUBLIC_MQTT_ENDPOINT=mqtts://broker.emqx.io:8883 cargo test -q -p discrypt-transport --features mqtt-adapter --test public_webrtc_datachannel_e2e public_mqtt_signals_real_webrtc_datachannel_roundtrip -- --nocapture
cargo tree --workspace --all-features --target all -i hickory-proto@0.25.2
cargo audit --json > /tmp/discrypt-cargo-audit-libp2p-patched.json
```

Latest audit result after these slices: **0 vulnerability hits remain**. `npm --prefix apps/ui run test:cargo-audit-g122` now passes only when strict `cargo audit` exits zero and the 16 unmaintained warnings plus 1 unsound warning exactly match the documented watchlist.

## G009 audit coupling

G009 security/privacy/no-shim closure relies on the same supply-chain gate family:
`cargo audit` for Rust advisories, `cargo deny` for license/advisory/bans/source
policy, `npm audit` for UI package advisories, and SBOM generation for release
inventory. Privacy gates must not introduce new dependencies unless the audit,
license, and security review docs are updated in the same change.

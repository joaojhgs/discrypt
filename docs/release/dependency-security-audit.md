# Dependency security audit

Date: 2026-05-30

Latest command run:

```bash
cargo audit --json > /tmp/discrypt-cargo-audit-current.json
```

Result: **not release-clean**. After the libp2p 0.56, MQTT client, direct-IPFS, and MLS/libcrux remediation slices, `cargo audit` still exits non-zero with **2 vulnerability hits**, plus unmaintained and unsound warnings.

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

## Remaining vulnerability blockers

| Advisory | Package/version in lockfile | Patched version / status | Current dependency edge observed | Production impact |
| --- | --- | --- | --- | --- |
| RUSTSEC-2026-0119 | `hickory-proto 0.25.2` | `>=0.26.1` | Still present in `Cargo.lock` through optional `libp2p 0.56.0` package edges (`libp2p-dns`/`libp2p-mdns`), even though the production `ipfs-pubsub-adapter` feature no longer enables `libp2p/dns`. `cargo tree --workspace --all-features --target all -i hickory-proto@0.25.2` prints nothing after the direct-address change. | Still blocks a cargo-audit-clean production release because `cargo audit` scans the lockfile, not only the active feature graph. IPFS remains non-default and requires explicit `/ip4` or `/ip6` multiaddrs until the umbrella libp2p lock edge is removed/upgraded or a security-reviewed exception is accepted. |
| RUSTSEC-2026-0118 | `hickory-proto 0.25.2` | `>=0.26.1` | Same optional libp2p DNS/mDNS lockfile edge as above | Same release blocker as above. Runtime DNS bootstrap is disabled, but the lockfile advisory remains unresolved. |

## Remediated vulnerability blockers

| Advisory | Prior edge | Remediation | Verification |
| --- | --- | --- | --- |
| RUSTSEC-2026-0124 | `openmls_rust_crypto 0.5.1` -> `hpke-rs 0.6.1` -> `hpke-rs-libcrux 0.6.1` -> `libcrux-aead 0.0.7` -> `libcrux-chacha20poly1305 0.0.7` | Added `[patch.crates-io] hpke-rs-libcrux = { path = "third_party/hpke-rs-libcrux-0.6.1-discrypt" }`; vendored crate only changes dependency metadata to `libcrux-aead 0.0.8` / `libcrux-traits 0.0.7` and records `0.6.1+discrypt.1`. | `cargo test -q -p discrypt-mls-core`; latest `cargo audit` no longer reports `libcrux-chacha20poly1305`. |

## Previous attempted remediation

```bash
cargo update -p hickory-proto --precise 0.26.1
```

Cargo rejects that direct update because `libp2p-mdns 0.48.0` still requires `hickory-proto ^0.25.2`. Removing the production `libp2p/dns` feature proves the active Discrypt IPFS graph does not use Hickory, but `cargo audit` remains non-zero while the umbrella `libp2p` package keeps optional vulnerable DNS/mDNS packages in `Cargo.lock`.

## Warning blockers

`cargo audit` also reports unmaintained/unsound packages, including GTK3-era Tauri stack packages (`atk`, `gdk`, `gtk`, `gtk-sys`, etc.), `instant`, `paste`, `proc-macro-error`, `rustls-pemfile`, `unic-*`, `glib`, and `lru`. These must be triaged before production packaging; if they are build-only or platform-only, the release matrix needs explicit target-scoped acceptance or replacement evidence.

## Required completion steps

- Finish the IPFS/libp2p dependency remediation by replacing the umbrella `libp2p` dependency with direct subcrates that do not lock `libp2p-dns`/`libp2p-mdns`, upgrading to a libp2p release that resolves `hickory-proto >=0.26.1`, or documenting a formal cargo-audit exception after security review.
- Keep IPFS public defaults disabled until DNS/topic-peer discovery is remediated; only explicit direct-address profiles should be accepted in production builds while Hickory remains unresolved.
- Replace the local `hpke-rs-libcrux` patch with an upstream patched release as soon as one exists and passes the same MLS verification gates.
- Triage every remaining unmaintained/unsound warning into replaced, target-scoped accepted, or release-blocking.
- Re-run `cargo audit`; only mark the production dependency/security gate complete when it exits zero or when every remaining advisory has a documented, target-scoped, security-reviewed exception.

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
cargo audit --json > /tmp/discrypt-cargo-audit-current.json
```

Latest audit result after these slices: **2 vulnerability hits remain** (`hickory-proto 0.25.2` x2). `libcrux-chacha20poly1305 0.0.7` no longer appears in the resolved vulnerability list.

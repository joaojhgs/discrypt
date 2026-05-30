# Dependency security audit

Date: 2026-05-30

Latest command run:

```bash
cargo audit --json > /tmp/discrypt-cargo-audit-libp2p56.json
```

Result: **not release-clean**. After the libp2p 0.56 remediation slice, `cargo audit` still exits non-zero with **7 vulnerability hits**, plus unmaintained and unsound warnings.

## What improved in this slice

- Upgraded the optional IPFS/libp2p adapter stack from `libp2p 0.54.1` to `0.56.0`.
- Removed the old `ring 0.16.20` finding from the resolved lockfile.
- Removed the old `rustls-webpki 0.101.7` finding from the resolved lockfile.
- Moved libp2p TLS dependencies onto newer `libp2p-tls 0.6.2` / `rcgen 0.13.2` era packages.
- Kept the IPFS adapter compiling and passing its targeted conformance tests after the upstream gossipsub API changed `PublishError::InsufficientPeers` to `PublishError::NoPeersSubscribedToTopic` and `unsubscribe` now returns `bool`.

## Remaining vulnerability blockers

| Advisory | Package/version in lockfile | Patched version / status | Current dependency edge observed | Production impact |
| --- | --- | --- | --- | --- |
| RUSTSEC-2026-0119 | `hickory-proto 0.25.2` | `>=0.26.1` | `libp2p 0.56.0` -> `libp2p-dns 0.44.0` -> `hickory-resolver 0.25.2` | Still blocks production release of the IPFS/libp2p public adapter when the DNS feature is enabled. Next fix is to upgrade libp2p again when it resolves `hickory-proto >=0.26.1`, remove the `dns` feature from production builds, or provide an audited direct-address-only IPFS profile. |
| RUSTSEC-2026-0118 | `hickory-proto 0.25.2` | `>=0.26.1` | Same libp2p DNS edge as above | Same release blocker as above; the current upgrade improved but did not fully remediate Hickory. |
| RUSTSEC-2026-0124 | `libcrux-chacha20poly1305 0.0.7` | `>=0.0.8` | `openmls_rust_crypto 0.5.1` -> `hpke-rs 0.6.1` -> `hpke-rs-libcrux`/`libcrux-aead 0.0.7` in the MLS dependency graph | Blocks production release of MLS-backed encrypted messaging until OpenMLS/HPKE/libcrux dependencies publish a compatible patched graph or the crypto provider is replaced with an audited alternative. |
| RUSTSEC-2026-0098 / 0099 / 0104 / 0049 | `rustls-webpki 0.102.8` | `>=0.103.13` for this advisory set | `rumqttc 0.25.1` TLS stack | Blocks release of TLS-backed public MQTT until `rumqttc` upgrades, the MQTT adapter is migrated to a different MQTT/TLS client, or a target-scoped security exception is formally accepted. |

## Previous attempted remediation

```bash
cargo update -p hickory-proto --precise 0.26.1
```

Cargo rejected that direct update before the libp2p upgrade because `libp2p-mdns 0.46.0` required `hickory-proto ^0.24.1`. A broad `cargo update` alone did not reduce the audit finding count. The later explicit `libp2p 0.56.0` upgrade reduced the finding count but still leaves Hickory at `0.25.2`, below the patched range.

## Warning blockers

`cargo audit` also reports unmaintained/unsound packages, including GTK3-era Tauri stack packages (`atk`, `gdk`, `gtk`, `gtk-sys`, etc.), `instant`, `paste`, `proc-macro-error`, `rustls-pemfile`, `unic-*`, `glib`, and `lru`. These must be triaged before production packaging; if they are build-only or platform-only, the release matrix needs explicit target-scoped acceptance or replacement evidence.

## Required completion steps

- Finish the IPFS/libp2p dependency remediation by either upgrading to a libp2p graph with `hickory-proto >=0.26.1`, removing `dns` from production IPFS features, or proving a direct-address-only adapter profile that cannot exercise vulnerable DNS paths.
- Upgrade/replace the MQTT TLS stack so the production MQTT adapter no longer pulls vulnerable `rustls-webpki 0.102.8`.
- Upgrade/replace the OpenMLS/HPKE/libcrux crypto edge so the production MLS graph no longer pulls `libcrux-chacha20poly1305 0.0.7`.
- Triage every remaining unmaintained/unsound warning into replaced, target-scoped accepted, or release-blocking.
- Re-run `cargo audit`; only mark the production dependency/security gate complete when it exits zero or when every remaining advisory has a documented, target-scoped, security-reviewed exception.

## Verification evidence for the libp2p 0.56 remediation slice

```bash
cargo check -q -p discrypt-transport --features ipfs-pubsub-adapter
cargo test -q -p discrypt-transport --features ipfs-pubsub-adapter ipfs_pubsub -- --nocapture
cargo check -q -p discrypt-transport --features ipfs-pubsub-adapter,mqtt-adapter,nostr-adapter
cargo check -q -p discrypt-desktop --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter
cargo audit --json > /tmp/discrypt-cargo-audit-libp2p56.json
```

Latest audit result after this slice: **7 vulnerability hits remain**.

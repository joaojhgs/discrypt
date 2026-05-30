# Dependency security audit

Date: 2026-05-30

Command run:

```bash
cargo audit --json > /tmp/discrypt-cargo-audit-after.json
```

Result: **not release-clean**. The RustSec database was current to `2026-05-29T20:55:26+02:00`; `cargo audit` exited non-zero with **10 vulnerability hits**, plus unmaintained and unsound warnings.

## Vulnerability blockers

| Advisory | Package/version in lockfile | Patched version | Current dependency edge observed in `Cargo.lock` | Production impact |
| --- | --- | --- | --- | --- |
| RUSTSEC-2026-0119 | `hickory-proto 0.24.4` | `>=0.26.1` | `libp2p-mdns 0.46.0`, `hickory-resolver 0.24.4` | Blocks release of IPFS/libp2p public adapter until libp2p/DNS stack is upgraded or mdns/DNS path is removed from production builds. |
| RUSTSEC-2026-0124 | `libcrux-chacha20poly1305 0.0.7` | `>=0.0.8` | `libcrux-aead 0.0.7` | Blocks release if this crypto path remains in the production dependency graph. |
| RUSTSEC-2025-0009 | `ring 0.16.20` | `>=0.17.12` | `rcgen 0.11.3` | Blocks release for any target still resolving the old rcgen/ring stack. |
| RUSTSEC-2026-0098 / 0099 / 0104 / 0049 | `rustls-webpki 0.101.7`, `0.102.8` | `>=0.103.13` for this advisory set | `libp2p-tls 0.5.0`, `rumqttc 0.25.1`; `rustls 0.23.40` already resolves `rustls-webpki 0.103.13` | Blocks release of TLS-backed public MQTT/libp2p paths until dependency graph is upgraded or replaced. |

Attempted remediation:

```bash
cargo update -p hickory-proto --precise 0.26.1
```

Cargo rejected that update because `libp2p-mdns 0.46.0` requires `hickory-proto ^0.24.1`. A full compatible remediation likely requires a `libp2p`/`libp2p-mdns` upgrade, removing mDNS/DNS from production adapter features, or replacing the affected provider stack. A broad `cargo update` did not reduce the audit finding count, so the lockfile was not kept as an audit fix.

## Warning blockers

`cargo audit` also reports unmaintained/unsound packages, including GTK3-era Tauri stack packages (`atk`, `gdk`, `gtk`, `gtk-sys`, etc.), `instant`, `paste`, `proc-macro-error`, `rustls-pemfile`, `unic-*`, `glib`, and `lru`. These must be triaged before production packaging; if they are build-only or platform-only, the release matrix needs explicit target-scoped acceptance or replacement evidence.

## Required completion steps

- Upgrade or replace the libp2p stack so IPFS/libp2p public adapter builds without `hickory-proto 0.24.4` and old `rustls-webpki`.
- Upgrade/replace the MQTT TLS stack or configure it so `rumqttc` no longer pulls vulnerable `rustls-webpki 0.102.8` for production builds.
- Remove/upgrade the old `rcgen 0.11.3` edge that keeps `ring 0.16.20` in the lockfile.
- Remove/upgrade the `libcrux-aead 0.0.7` edge that keeps `libcrux-chacha20poly1305 0.0.7` in the lockfile.
- Re-run `cargo audit`; only mark the production dependency/security gate complete when it exits zero or when every remaining advisory has a documented, target-scoped, security-reviewed exception.

# G122 Rust advisory scan and warning watchlist

## Scope

G122 gates the Rust advisory surface for the production E2E P2P overlay mesh.
The required scan is `cargo audit` over the committed `Cargo.lock`, enforced by
`scripts/check-cargo-audit-g122.mjs`.

## Current result

`cargo audit` currently reports **0 vulnerabilities** on this lockfile. The G122
gate no longer permits vulnerability waivers: any future vulnerability ID fails
CI until the dependency graph is fixed or the release is explicitly held.

The remaining `cargo audit` output is a documented warning watchlist: 18
unmaintained warnings and 3 unsound warnings. These warnings are not vulnerability
waivers and do not prove production readiness by themselves. They remain visible
so release engineering can replace or target-scope the underlying GTK3/Tauri and
parser/OpenMLS proof-tooling transitive dependencies before a production release
candidate.

## Removed vulnerability waiver

The former non-release waiver for `RUSTSEC-2026-0124` /
`libcrux-chacha20poly1305` is closed. The current lockfile resolves the patched
`libcrux-chacha20poly1305 >= 0.0.8` path and `cargo audit --json` reports no
vulnerability entry for that advisory. The G122 script now runs strict
`cargo audit` without `--ignore`; if `RUSTSEC-2026-0124` or any other
vulnerability reappears, the gate fails.

## Informational warning watchlist

The following `cargo audit` warnings are tracked but are not vulnerability
waivers. Every warning currently emitted by `cargo audit --json` must be listed
here, and every listed warning must still be present until it is intentionally
removed from the dependency graph and this document is updated.

| Advisory | Package | Kind | Owner | Expiry | Upgrade path / release disposition |
| --- | --- | --- | --- | --- | --- |
| RUSTSEC-2024-0413 | `atk` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration; release must target-scope Linux GTK3 exposure. |
| RUSTSEC-2024-0416 | `atk-sys` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration; release must target-scope Linux GTK3 exposure. |
| RUSTSEC-2024-0412 | `gdk` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration; release must target-scope Linux GTK3 exposure. |
| RUSTSEC-2024-0418 | `gdk-sys` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration; release must target-scope Linux GTK3 exposure. |
| RUSTSEC-2024-0411 | `gdkwayland-sys` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration; release must target-scope Linux GTK3 exposure. |
| RUSTSEC-2024-0415 | `gtk` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration; release must target-scope Linux GTK3 exposure. |
| RUSTSEC-2024-0420 | `gtk-sys` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration; release must target-scope Linux GTK3 exposure. |
| RUSTSEC-2024-0419 | `gtk3-macros` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration; release must target-scope Linux GTK3 exposure. |
| RUSTSEC-2024-0384 | `instant` | unmaintained | desktop/runtime dependency owner | 2026-07-31 | Replace via upstream dependency updates; target-scope if retained only through platform/runtime transitive edges. |
| RUSTSEC-2024-0436 | `paste` | unmaintained | Rust dependency owner | 2026-07-31 | Replace through upstream dependency updates or remove direct/transitive edge. |
| RUSTSEC-2024-0370 | `proc-macro-error` | unmaintained | desktop runtime owner | 2026-07-31 | Transitive macro stack upgrade through GTK/glib dependencies. |
| RUSTSEC-2026-0173 | `proc-macro-error2` | unmaintained | MLS/OpenMLS dependency owner | 2026-07-31 | Transitive through `openmls_rust_crypto` -> `hpke-rs` -> `libcrux` hax macro tooling; no safe upgrade is currently available, so `deny.toml` carries a temporary explicit advisory ignore until upstream migrates away from `proc-macro-error2` or the MLS crypto stack is replaced. |
| RUSTSEC-2025-0081 | `unic-char-property` | unmaintained | desktop runtime owner | 2026-07-31 | Transitive `tauri-utils`/`urlpattern` replacement or upstream fix. |
| RUSTSEC-2025-0075 | `unic-char-range` | unmaintained | desktop runtime owner | 2026-07-31 | Transitive `tauri-utils`/`urlpattern` replacement or upstream fix. |
| RUSTSEC-2025-0080 | `unic-common` | unmaintained | desktop runtime owner | 2026-07-31 | Transitive `tauri-utils`/`urlpattern` replacement or upstream fix. |
| RUSTSEC-2025-0100 | `unic-ucd-ident` | unmaintained | desktop runtime owner | 2026-07-31 | Transitive `tauri-utils`/`urlpattern` replacement or upstream fix. |
| RUSTSEC-2025-0098 | `unic-ucd-version` | unmaintained | desktop runtime owner | 2026-07-31 | Transitive `tauri-utils`/`urlpattern` replacement or upstream fix. |
| RUSTSEC-2026-0243 | `nostr-relay-pool` | unmaintained | transport dependency owner | 2026-10-31 | Migrate `nostr-sdk` to the maintained 0.45 client stack after adapter compatibility verification; the patched 0.44.3 relay pool remains required by the current 0.44 SDK. |
| RUSTSEC-2026-0221 | `event-listener` | unsound | transport dependency owner | 2026-10-31 | Avoid non-`Send` listener tags and upgrade through the async dependency graph when a patched release is available. |
| RUSTSEC-2024-0429 | `glib` | unsound | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade; avoid direct use of `glib::VariantStrIter`; release must target-scope Linux GTK3 exposure. |
| RUSTSEC-2026-0253 | `lru` | unsound | transport dependency owner | 2026-10-31 | Avoid panic-catching around `LruCache::pop`; upgrade the Nostr dependency graph when a patched release is available. |

## Release rule

A production release candidate must have `cargo audit` pass with zero
vulnerabilities and must carry a current, owner-assigned warning watchlist. New
vulnerability IDs are release blockers until fixed. New warning IDs are release
blockers until they are either removed or added here with owner, expiry,
disposition, and upgrade path.

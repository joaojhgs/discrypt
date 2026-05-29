# G122 Rust advisory scan and non-release waivers

## Scope

G122 gates the Rust advisory surface for the production E2E P2P overlay mesh.
The required scan is `cargo audit` over the committed `Cargo.lock`, with any
exception documented here and enforced by `scripts/check-cargo-audit-g122.mjs`.

## Current result

`cargo audit` reports one vulnerability and fifteen allowed informational
warnings on this lockfile. The vulnerability is not accepted for production use
as a reachable runtime dependency; it is waived only because the affected crate
is lockfile-only for this workspace's active feature graph and `cargo tree
--workspace --target all --locked` does not contain the affected package. If that
package becomes reachable, the G122 gate fails.

## Vulnerability waiver: RUSTSEC-2026-0124

- Advisory: RUSTSEC-2026-0124
- Package: `libcrux-chacha20poly1305` 0.0.7
- Title: Potential panic on overlong ciphertext buffer
- Severity: high
- Patched version: `libcrux-chacha20poly1305 >=0.0.8`
- Owner: supply-chain release owner
- Release disposition: non-release waiver only; release remains blocked if the
  affected package appears in the active workspace dependency graph.
- Reason: `cargo audit` flags the optional libcrux HPKE backend package retained
  in `Cargo.lock`, but the workspace uses the RustCrypto HPKE path through
  `openmls_rust_crypto`; `cargo tree --workspace --target all --locked -i
  libcrux-chacha20poly1305` prints no active dependency tree. A direct
  `cargo update -p libcrux-chacha20poly1305 --precise 0.0.8` is not compatible
  because `libcrux-aead 0.0.7` requires exactly `libcrux-chacha20poly1305
  0.0.7`.
- Mitigation: do not enable the `hpke-rs/libcrux` backend for production builds;
  keep the active OpenMLS provider on RustCrypto until upstream HPKE/libcrux
  versions can resolve to `libcrux-chacha20poly1305 >=0.0.8`.
- Upgrade path: monitor `hpke-rs-libcrux` and `libcrux-aead` releases; remove
  this waiver when the OpenMLS/HPKE dependency graph resolves to the patched
  libcrux package or when the stale optional package is removed from the lockfile
  without breaking reproducible builds.
- Expiry: 2026-07-31 or before any production release candidate, whichever comes
  first.
- Enforcement: `npm --prefix apps/ui run test:cargo-audit-g122` must fail if
  `RUSTSEC-2026-0124` disappears from this file, if another vulnerability
  appears, or if `libcrux-chacha20poly1305` appears in the active cargo tree.

## Informational warning watchlist

The following `cargo audit` warnings are tracked but are not vulnerability
waivers. They remain visible so release engineering can replace the underlying
Tauri/Linux GTK3 and parser transitive dependencies when upstream routes exist.

| Advisory | Package | Kind | Owner | Expiry | Upgrade path |
| --- | --- | --- | --- | --- | --- |
| RUSTSEC-2024-0413 | `atk` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration |
| RUSTSEC-2024-0416 | `atk-sys` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration |
| RUSTSEC-2024-0412 | `gdk` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration |
| RUSTSEC-2024-0418 | `gdk-sys` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration |
| RUSTSEC-2024-0411 | `gdkwayland-sys` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration |
| RUSTSEC-2024-0415 | `gtk` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration |
| RUSTSEC-2024-0420 | `gtk-sys` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration |
| RUSTSEC-2024-0419 | `gtk3-macros` | unmaintained | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade or platform webview migration |
| RUSTSEC-2024-0370 | `proc-macro-error` | unmaintained | desktop runtime owner | 2026-07-31 | transitive macro stack upgrade through GTK/glib dependencies |
| RUSTSEC-2025-0081 | `unic-char-property` | unmaintained | desktop runtime owner | 2026-07-31 | transitive `tauri-utils`/`urlpattern` replacement or upstream fix |
| RUSTSEC-2025-0075 | `unic-char-range` | unmaintained | desktop runtime owner | 2026-07-31 | transitive `tauri-utils`/`urlpattern` replacement or upstream fix |
| RUSTSEC-2025-0080 | `unic-common` | unmaintained | desktop runtime owner | 2026-07-31 | transitive `tauri-utils`/`urlpattern` replacement or upstream fix |
| RUSTSEC-2025-0100 | `unic-ucd-ident` | unmaintained | desktop runtime owner | 2026-07-31 | transitive `tauri-utils`/`urlpattern` replacement or upstream fix |
| RUSTSEC-2025-0098 | `unic-ucd-version` | unmaintained | desktop runtime owner | 2026-07-31 | transitive `tauri-utils`/`urlpattern` replacement or upstream fix |
| RUSTSEC-2024-0429 | `glib` | unsound | desktop runtime owner | 2026-07-31 | Tauri/wry/GTK stack upgrade; avoid direct use of `glib::VariantStrIter` |

## Release rule

A production release candidate must either have `cargo audit` pass without
vulnerabilities or carry only this enforced non-release waiver while the affected
package remains absent from the active cargo tree. New vulnerability IDs are
release blockers until fixed or documented here with owner, reason, expiry,
mitigation, and upgrade path.

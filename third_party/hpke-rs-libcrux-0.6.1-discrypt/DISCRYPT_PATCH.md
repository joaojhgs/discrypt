# Discrypt local patch

This directory vendors `hpke-rs-libcrux` from crates.io release `0.6.1` and applies the smallest local dependency patch needed to keep the OpenMLS 0.8 provider on the current, vulnerability-free libcrux graph.

Patch scope:

- crate version suffix: `0.6.1+discrypt.2`
- `libcrux-aead`: `0.0.7` -> `0.0.9`
- `libcrux-ecdh`: `0.0.6` -> `0.0.8`
- `libcrux-hkdf`: `0.0.6` -> `0.0.8`
- `libcrux-kem`: `0.0.7` -> `0.0.9`
- `libcrux-traits`: `0.0.6` -> `0.0.8`

Rust source changes are limited to compatibility with the updated libcrux APIs.

Release rule: replace this local patch with an upstream `hpke-rs-libcrux` release as soon as one depends on the patched libcrux AEAD/chacha graph and passes the same MLS verification gates.

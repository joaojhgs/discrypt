# Discrypt local patch

This directory vendors `hpke-rs-libcrux` from crates.io release `0.6.1` and applies the smallest local metadata/dependency patch needed while upstream has no patched release that removes the `libcrux-chacha20poly1305 0.0.7` audit finding.

Patch scope:

- crate version suffix: `0.6.1+discrypt.1`
- `libcrux-aead`: `0.0.7` -> `0.0.8`
- `libcrux-traits`: kept on `0.0.7` to match the patched libcrux AEAD graph

No Rust source files were changed from the crates.io package in this local patch.

Release rule: replace this local patch with an upstream `hpke-rs-libcrux` release as soon as one depends on the patched libcrux AEAD/chacha graph and passes the same MLS verification gates.

# Discrypt local patch

This directory vendors `hpke-rs` from crates.io release `0.6.1` and applies the smallest dependency-only patch needed to keep OpenMLS 0.8 on a vulnerability-free libcrux SHA-3 release without raising Discrypt's Rust 1.89 MSRV.

Patch scope:

- crate version suffix: `0.6.1+discrypt.1`
- `libcrux-sha3`: `0.0.8` -> `0.0.10`

No Rust source files are changed from the crates.io package.

Release rule: replace this local patch with a compatible upstream OpenMLS/HPKE release once it supports the same MSRV and passes the MLS persistence and interoperability gates.

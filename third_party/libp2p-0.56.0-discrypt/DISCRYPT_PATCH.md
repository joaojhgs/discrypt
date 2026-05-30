# Discrypt local patch

This directory vendors `libp2p` from crates.io release `0.56.0` and applies the smallest local metadata patch needed while the upstream umbrella crate still locks optional DNS/mDNS crates that depend on vulnerable `hickory-proto 0.25.2`.

Patch scope:

- crate version suffix: `0.56.0+discrypt.1`
- `dns` feature is declared as empty instead of depending on `libp2p-dns`
- `mdns` feature is declared as empty instead of depending on `libp2p-mdns`
- `full` no longer enables `dns` or `mdns`
- `tokio` no longer forwards into optional DNS/mDNS dependencies
- normalized manifest no longer has `libp2p-dns` or `libp2p-mdns` dependency tables

No Rust source files were changed from the crates.io package in this local patch.

Release rule: replace this local patch with upstream libp2p direct-subcrate usage or a release whose optional DNS/mDNS dependencies no longer lock vulnerable Hickory packages. Do not re-enable DNS or mDNS in Discrypt production builds until `cargo audit` and runtime DNS-path evidence are clean.

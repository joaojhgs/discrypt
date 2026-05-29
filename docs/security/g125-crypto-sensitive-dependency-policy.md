# G125 Crypto-sensitive dependency policy

G125 proves that crypto-sensitive dependencies are pinned or vendored according
to ADR-008. Discrypt uses a lockfile-first supply-chain model: `Cargo.lock` pins
Rust crates, `apps/ui/package-lock.json` pins UI/build packages, and vendoring is not the default unless a release isolation requirement explicitly adds vendor
hashes and source URLs to the SBOM evidence.

## Direct sensitive crate inventory

The machine-readable inventory is
`docs/security/g125-crypto-sensitive-dependencies.json`. It records every direct
workspace dependency used for MLS, HPKE/AEAD, signatures, hashing, randomness,
secret zeroization, OpenMLS storage, and voice codec boundaries with:

- manifest version constraint;
- exact locked version from `cargo metadata --locked` / `Cargo.lock`;
- registry source;
- pin source;
- whether the dependency is vendored.

## Transitive watchlist

The inventory also records sensitive transitive packages matching OpenMLS, HPKE,
AEAD, WebRTC/DTLS/SRTP-adjacent, TLS, random-number, signature, hash, and
zeroization names. This watchlist is not a hand-written allowlist; it is derived
from `cargo metadata --locked` and must change whenever the lockfile changes.

## UI package lock

The current UI dependency graph does not provide application cryptography, but
its package manager state is still release-sensitive because it builds and ships
the frontend. The G125 gate requires `apps/ui/package-lock.json` to have package
versions and integrity hashes for all npm package entries.

## Verification

Run:

```sh
npm --prefix apps/ui run test:crypto-sensitive-g125
```

The check fails when a direct crypto-sensitive dependency is missing from the
workspace manifest, uses a wildcard or git source, resolves to multiple direct
versions, is absent from `Cargo.lock`, when the generated inventory is stale, or
when the npm lockfile has package entries without versions/integrity hashes.

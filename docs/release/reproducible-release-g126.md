# G126 reproducible release build evidence

G126 records the exact inputs required to reproduce a release build from
lockfiles and documented toolchain versions.

## Pinned inputs

- Rust toolchain: `rust-toolchain.toml` pins Rust `1.96.0` with `rustfmt` and `clippy`.
- Node toolchain: `.node-version` pins Node `22.22.0`; npm is recorded from that runtime.
- Rust dependencies: `Cargo.lock` SHA-256 is recorded.
- UI dependencies: `apps/ui/package-lock.json` SHA-256 is recorded and installs with `npm ci`.
- Tauri CLI: release tooling uses `@tauri-apps/cli@2.11.2`.
- Determinism seed: `SOURCE_DATE_EPOCH` defaults to the git commit timestamp and is recorded.

## Release evidence command

```sh
npm --prefix apps/ui run repro:g126
```

This writes `target/release/reproducibility-g126.json` with git commit,
lockfile hashes, Rust/Node/npm/Tauri/cargo-audit/cargo-deny/cargo-sbom versions,
release features, package artifact hashes, and SBOM hashes.

## Prerequisite order

`npm --prefix apps/ui run test:repro-g126` is an evidence-completeness gate,
not a standalone dry-run. It is expected to fail with missing package artifact or
SBOM hashes until the release lane has first produced Linux bundles and SBOMs.
For G011 release evidence, run the gates in this order on the integrated leader
branch:

1. `npm --prefix apps/ui run test:release-linux` to verify the dry-run plan.
2. `npm --prefix apps/ui run release:linux` to build `.deb`, `.rpm`, and
   `.AppImage` artifacts.
3. `npm --prefix apps/ui run sbom:g124` to generate packaged-artifact SBOMs
   from those bundles.
4. `npm --prefix apps/ui run repro:g126` to record the artifact/SBOM hashes.
5. `npm --prefix apps/ui run test:repro-g126` to prove the recorded
   reproducibility evidence is complete.

## Rebuild recipe

1. Check out the recorded git commit.
2. Install the pinned Rust and Node toolchains from `rust-toolchain.toml` and `.node-version`.
3. Run `npm --prefix apps/ui ci` and verify the recorded package-lock hash.
4. Run `cargo metadata --locked --format-version 1` and verify the recorded `Cargo.lock` hash.
5. Set `SOURCE_DATE_EPOCH` to the recorded commit timestamp.
6. Run `npm --prefix apps/ui run release:linux` on the documented Linux baseline.
7. Compare `.deb`, `.rpm`, `.AppImage`, and SBOM hashes with `target/release/reproducibility-g126.json`.

Local developer builds are not claimed byte-for-byte reproducible unless all
recorded toolchain versions, package artifacts, SBOMs, and the Linux build
baseline match this evidence.

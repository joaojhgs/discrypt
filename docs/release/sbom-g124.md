# G124 SBOM generation

G124 requires generated SBOM evidence for the Rust workspace, UI npm graph, and
packaged desktop artifacts.

## Commands

Plain npm token for gate: `npm sbom --sbom-format spdx`.


- `cargo sbom --output-format spdx_json_2_3` generates `target/sbom/discrypt-rust.spdx.json`.
- `npm --prefix apps/ui sbom --sbom-format spdx --sbom-type application` (equivalent to `npm sbom --sbom-format spdx` in `apps/ui`) generates `target/sbom/discrypt-ui-npm.spdx.json`.
- `node scripts/generate-sbom-g124.mjs --out-dir target/sbom --require-packaged-artifacts` hashes package outputs under `target/release/bundle` and generates `target/sbom/discrypt-packaged-artifacts.spdx.json` plus `target/sbom/discrypt-sbom-index.json`.

## Packaged artifact rule

Release builds run `scripts/generate-sbom-g124.mjs` after Tauri creates `.deb`,
`.rpm`, and `.AppImage` artifacts. The packaged-artifact SBOM records each file
path, SHA-256 checksum, SPDX package metadata, and package URL reference. If no
package artifact exists, the release SBOM command fails instead of producing an
empty package SBOM.

## CI and local validation

- `npm --prefix apps/ui run test:sbom-g124` generates all three SBOM documents in a temporary directory and verifies they are populated SPDX 2.3 documents.
- The supply-chain CI job uploads `target/sbom` as the `discrypt-sbom` artifact.
- The Linux release dry-run check verifies the release plan includes SBOM generation after packaging.

## Retention

For a release candidate, archive `target/sbom/discrypt-rust.spdx.json`,
`target/sbom/discrypt-ui-npm.spdx.json`,
`target/sbom/discrypt-packaged-artifacts.spdx.json`, and
`target/sbom/discrypt-sbom-index.json` alongside package hashes and lockfile
hashes.

# ADR-008: Supply chain, SBOM, licenses, and reproducibility

## Status

Accepted for the production E2E P2P overlay mesh launch gate.

## Context

Discrypt uses Rust crates, npm packages, Tauri desktop packaging, cryptographic
libraries, WebRTC/OpenMLS dependencies, and Linux/macOS/Windows/Android release
artifacts. Production releases need explicit supply-chain policy for
`cargo-deny`, `cargo audit`, `npm audit`, SBOM generation, pinned or vendored
crypto-sensitive dependencies, license acceptance, and reproducible build
assumptions.

## Decision

The launch policy is lockfile-first, CI-enforced, and release-dashboard backed:

- Rust dependencies are locked by `Cargo.lock`; npm dependencies are locked by
  `apps/ui/package-lock.json`; CI must use those lockfiles.
- Rust advisories are scanned with `cargo audit` and `cargo deny check` in the
  `supply-chain` job.
- npm advisories are scanned with `npm audit --audit-level=high --omit=dev` after
  `npm ci` in the `supply-chain` job, and G123 additionally gates the full UI
  graph with `npm audit --audit-level=high`.
- SBOMs are generated for Rust with `cargo sbom --output-format spdx_json_2_3`,
  for npm with `npm sbom --sbom-format spdx`, and for packaged artifacts with
  `scripts/generate-sbom-g124.mjs`; CI uploads them as the `discrypt-sbom` artifact.
- License policy lives in `deny.toml`; allowed licenses are MIT, Apache-2.0,
  BSD-2-Clause, BSD-3-Clause, Unicode-3.0, AGPL-3.0-or-later, MPL-2.0, and ISC.
- Source policy denies unknown registries and unknown git sources.
- Ban policy denies wildcard dependency versions and warns on duplicate versions
  until later reproducibility goals reduce duplicates where feasible.

The current ADR locks the policy and CI wiring. It does not waive advisory debt:
production release remains blocked until the later advisory/reproducibility gates
resolve or explicitly document every exception with owner, reason, expiry, and
upgrade path.

## Crypto-sensitive dependency policy

Crypto-sensitive dependencies include OpenMLS/provider crates, HPKE, AEAD/SFrame,
WebRTC/DTLS/SRTP/ICE, keychain, random-number, and signature dependencies.
Policy:

1. No wildcard versions for direct dependencies.
2. Lockfile changes touching crypto-sensitive crates require review evidence in
   the release dashboard.
3. Vulnerability advisories in crypto-sensitive transitive dependencies block
   release unless a documented exception names the advisory, exposure path,
   mitigation, owner, expiry, and replacement plan.
   `deny.toml` may carry only those explicit temporary advisory exceptions, and
   each exception must also appear in `docs/security/g122-rust-advisory-waivers.md`.
4. Vendoring is not the default. If network-isolated or long-term support builds
   require vendoring, vendor hashes and source URLs must be stored with the SBOM
   and package hashes.
5. `cargo update`/`npm update` must not be run opportunistically in release
   branches; dependency updates are intentional PRs with audit evidence.

## License policy

Allowed licenses are listed in `deny.toml`. MPL-2.0 and ISC are accepted because
current OpenMLS/HPKE and transitive parser/runtime dependencies use them, and the
AGPL-licensed application can distribute those dependencies while preserving
source obligations. New copyleft or source-available licenses require an explicit
ADR or legal review before entering the production dependency graph.

## Reproducible build assumptions

A reproducible release claim requires:

- exact git commit;
- `Cargo.lock` hash;
- `apps/ui/package-lock.json` hash;
- Rust toolchain version;
- Node/npm versions;
- Tauri CLI version;
- OS image/container digest for Linux packaging;
- package artifact hashes;
- generated SBOM path and hash;
- signing identity and release channel.

Local developer builds are not claimed reproducible. Release reproducibility is a
separate gate that must rebuild from lockfiles and compare artifact hashes or
explain platform-specific nondeterminism.

## CI artifact storage

CI may retain the generated SPDX SBOM, package hashes, lockfile hashes, command
names, tool versions, and advisory summaries. CI must not retain secrets, signing
keys, local keychain entries, raw packet captures, unredacted crash dumps, or
user profile state.

## Verification

Release build can be reproduced from lockfiles and documented toolchain versions.


Required gates for this decision:

0. `npm --prefix apps/ui run test:cargo-deny-g121` runs the full
   `cargo deny check --hide-inclusion-graph` policy and fails on advisory,
   license, wildcard, unknown-registry, or unknown-git violations.
1. `npm --prefix apps/ui run test:cargo-audit-g122` runs strict `cargo audit`,
   permits no vulnerability waivers, and fails if the current warning watchlist
   diverges from the documented owner/expiry/disposition table.
2. `npm --prefix apps/ui run test:npm-audit-g123` runs production and full UI
   `npm audit --audit-level=high` checks and permits only a documented non-release waiver.
3. `npm --prefix apps/ui run test:sbom-g124` runs `generate-sbom-g124` and
   verifies SBOM generated for Rust, npm, and packaged artifacts, including a
   manifest with lockfile hashes, SBOM hashes, Linux bundle targets, and package
   artifact hashes when packages have been built.
4. `npm --prefix apps/ui run test:crypto-sensitive-g125` proves
   Crypto-sensitive dependencies are pinned or vendored according to ADR-008 by
   validating direct dependency specs, lockfile checksums, and vendoring policy.
5. `npm --prefix apps/ui run test:repro-g126` proves Release build can be
   reproduced from lockfiles and documented toolchain versions by checking
   lockfile resolution, release dry-run, SBOM generation, and concrete tool
   version commands.
6. `npm --prefix apps/ui run test:adr-008-supply-chain` proves ADR/CI/config
   wiring for cargo-audit, cargo-deny, npm audit, SBOM generation, license policy,
   source policy, lockfiles, and reproducibility assumptions.
7. `cargo metadata --locked --format-version 1 --no-deps` proves Rust metadata is
   resolvable from `Cargo.lock` without lockfile mutation.
8. `cargo deny check licenses --hide-inclusion-graph` proves the accepted license
   set matches the current dependency graph.
9. `cargo deny check bans sources --hide-inclusion-graph` proves wildcard/source
   policy is configured, with duplicate versions still warnings.
10. `npm --prefix apps/ui run test:no-placeholders-g127` proves CI scans
    production-gated modules for blocking placeholder markers and inventories
    shim/emulation/facade/skeleton/fixture/local-only wording for G128 review.
11. `npm --prefix apps/ui run test:placeholder-allowlist-g128` proves every
    remaining review-pattern occurrence has an explicit path and release-review
    rationale.
12. `npm --prefix apps/ui run test:release-no-fallback-g129` proves Linux
    release builds exclude `harness`/`local-dev`, strip local fallback env flags,
    and do not render fallback-only UI copy.
13. Full `cargo audit` and advisory-deny clean runs remain release-blocking gates
    handled by the later advisory/reproducibility stories.

## Consequences

- Supply-chain policy is explicit before final release-hardening stories.
- CI now includes Rust advisory/config/SBOM checks and npm high-severity audit.
- Existing advisory debt is visible and release-blocking rather than silently
  waived by this ADR.
- Later G121-G126 stories must either make the advisory/SBOM/reproducibility gates
  green or record narrowly scoped, expiring exceptions before release.

# G131 final E2E verification

G131 is the final aggregate release gate for the Discrypt production E2E plan. It
requires the maintained gates to be wired and then run as a single release-ready
verification set.

## E2E determinism boundary

`npm --prefix apps/ui run test:e2e` runs Playwright with one worker. The local-dev
fallback harness intentionally models one local desktop profile/store; parallel
workers race the same preview/local state and can drop the preview server mid-suite,
which hides product regressions behind infrastructure flake. Production network,
relay, media, storage, package, and abuse gates remain covered by the command
matrix below.

Required command evidence:

```sh
cargo fmt --all --check
npm --prefix apps/ui run test:final-e2e-g131
npm --prefix apps/ui run test:release-two-profile-harness-g010
npm --prefix apps/ui run release:two-profile-harness-g010:dry-run
npm --prefix apps/ui run test:g011-boundary
npm --prefix apps/ui run test:e2e
npm --prefix apps/ui run test:ui-integration-g130
npm --prefix apps/ui run test:release-no-fallback-g129
npm --prefix apps/ui run test:placeholder-allowlist-g128
npm --prefix apps/ui run test:no-placeholders-g127
npm --prefix apps/ui run test:release-linux
npm --prefix apps/ui run test:linux-package-smoke
npm --prefix apps/ui run test:desktop-package-ci
npm --prefix apps/ui run test:android-gate
npm --prefix apps/ui run test:release-governance
npm --prefix apps/ui run test:release-verification-matrix
npm --prefix apps/ui run test:pcap-suite-g096
npm --prefix apps/ui run test:malicious-relay-g097
npm --prefix apps/ui run test:malicious-member-g098
npm --prefix apps/ui run test:retention-shred-g099
npm --prefix apps/ui run test:performance-soak-g100
npm --prefix apps/ui run test:security-privacy-g009
npm --prefix apps/ui run test:presence-g115
npm --prefix apps/ui run test:abuse-g120
npm --prefix apps/ui run test:cargo-deny-g121
npm --prefix apps/ui run test:cargo-audit-g122
npm --prefix apps/ui run test:npm-audit-g123
npm --prefix apps/ui run test:sbom-g124
npm --prefix apps/ui run test:crypto-sensitive-g125
npm --prefix apps/ui run test:repro-g126
npm --prefix apps/ui run test:honesty
npm --prefix apps/ui run test:command-coverage
npm --prefix apps/ui run build
cargo check --workspace --quiet
cargo test --workspace --quiet
cargo clippy --workspace --all-targets --quiet -- -D warnings
git diff --check
```

The readiness script does not replace the command evidence above. It prevents a
final checkpoint when the release/UI/package/adversarial/supply-chain gates or
required artifacts are not wired into the repository.


## Evidence from this G131 run

The final local verification run on 2026-05-30 passed the complete command set
above. The browser/UI portion reported **9 Chromium tests passed** across:

- first-run setup without blank screen;
- account recovery without content-key recovery claims;
- direct-message send through command clients;
- group creation, invite metadata, join/open, text-channel creation, and message
  send;
- voice join, speaking state, self mute, speaker-volume control, and leave;
- absence of fake relay/member rows;
- small-window navigation;
- local-dev persistence after reload; and
- transport-status honesty before invite metadata exists.
- two independent browser profiles using isolated local stores for DM send
  attempts, invite join/open, and voice/media attempts without fabricated
  remote members or relay rows.
- the G010 two-profile release harness dry-run now records the reproducible local/public adapter matrix, isolated `DISCRYPT_APP_STATE_PATH` profile plan, and credential-gated public-provider skip policy before release checkpointing.

The multi-process/multi-host coverage is represented by maintained Rust/process
harness gates rather than by the browser-only UI test: `test:pcap-suite-g096`,
`test:malicious-relay-g097`, `test:malicious-member-g098`,
`test:performance-soak-g100`, `test:release-verification-matrix`, and the full
workspace `cargo test`/`cargo clippy --workspace --all-targets --quiet -- -D warnings`
run. Package/install coverage is represented by the Linux release dry run, Linux
package smoke dry run, desktop package CI check, Android gate check, SBOM gate,
and reproducibility gate.

`apps/ui/test-results/.last-run.json` must remain `status: passed` with zero
failed tests when this story is checkpointed.

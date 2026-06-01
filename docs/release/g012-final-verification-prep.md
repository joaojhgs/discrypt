# G012 final verification prep

This prep note is for the final worker lane only. It does not mutate
`.omx/ultragoal`, does not checkpoint the Codex goal, and does not by itself
claim G012 completion.

## Current stop condition

G012 can close only after the functional lanes produce a fresh two-installed-user
Tauri artifact bundle showing:

- two isolated Tauri profiles/users in one group;
- realtime group text both ways with delivery/receipt evidence;
- persistence/reload evidence for the two profiles;
- voice join, speaking indication, mute/unmute, per-peer volume, and leave
  cleanup using platform-credible capture/playback evidence; and
- final quality gates, code review, and architect/reviewer evidence on the
  integrated branch.

## Required G131 command evidence

The canonical command list is defined in `scripts/check-final-e2e-g131.mjs` and
mirrored in `docs/release/g131-final-e2e-verification.md`. The final checkpoint
must cite fresh successful output for all required commands, including the core
set below:

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

## Expected artifact paths

Final evidence should retain these artifact families when the functional proof is
ready:

- `target/release/g131-final-e2e-quality-gate.json` from
  `npm --prefix apps/ui run test:final-e2e-g131`.
- `target/release/g010-two-profile-harness/report.json` from the non-dry-run
  two-profile release harness.
- `target/release/g010-two-profile-harness/plan.json` from the dry-run harness.
- `target/release/g010-two-profile-harness/*stdout.log` and `*stderr.log` for
  browser, desktop, public-adapter, and optional built-Tauri rows.
- `target/release/g010-two-profile-harness/playwright-output/` for browser flow
  output.
- `target/g010-release-harness/<run-id>/manifest.json`, profile state files,
  logs, screenshots, and Playwright output for the older G010 harness surface
  when used.
- `target/g012-e2e/` or an equivalent leader-approved G012 artifact directory
  for the real two-installed-profile text-plus-voice proof. No current static
  G010/G131 script creates this directory by default.

## Current blockers to final completion

The final lane is not complete while these blockers remain:

1. `cargo test --workspace` currently fails in `discrypt-desktop`:
   - `tests::two_profile_app_ui_flow_mixes_invites_and_persists_channel_receipt`
     expects Alice's persisted voice participant volume `55` but observes `82`.
   - `tests::typed_command_errors_surface_actionable_codes` expects the missing
     participant recovery hint to contain `visible participant`.
2. `cargo clippy --workspace --all-targets -- -D warnings` currently fails in
   `crates/transport` for denied `expect_used`, `panic`, `needless_borrow`, and
   `too_many_arguments` lints.
3. The real G012 two-installed-user Tauri text-plus-voice artifact bundle is not
   present yet; existing G010/G131 static gates remain readiness wiring only.
4. Final checkpointing must be done by the leader with a fresh Codex goal
   snapshot; workers must not mutate `.omx/ultragoal`.

## Worker-2 readiness checks run during prep

Fresh local prep checks that passed in this lane:

- `npm --prefix apps/ui run test:release-two-profile-harness-g010`
- `npm --prefix apps/ui run release:two-profile-harness-g010:dry-run`
- `npm --prefix apps/ui run test:g011-boundary`
- `npm --prefix apps/ui run test:ui-integration-g130`
- `npm --prefix apps/ui run test:release-no-fallback-g129`
- `npm --prefix apps/ui run test:release-verification-matrix`
- `npm --prefix apps/ui run test:security-privacy-g009`
- `npm --prefix apps/ui run test:honesty`
- `npm --prefix apps/ui run test:command-coverage`


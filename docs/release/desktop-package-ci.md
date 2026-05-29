# Desktop package CI

Discrypt desktop package CI is split between a cheap validation job and explicit
runner-backed package jobs.

## Workflow

`.github/workflows/package-desktop.yml` provides:

- `validate-package-ci` on `ubuntu-latest` for workflow/static release checks.
- `package-linux` on `ubuntu-latest`, gated by `workflow_dispatch` input
  `package_linux`, running `npm --prefix apps/ui run release:linux` and
  `npm --prefix apps/ui run smoke:linux-packages`.
- `package-macos` on `macos-latest`, gated by `workflow_dispatch` input
  `package_macos`, building unsigned Tauri artifacts with
  `tauri-runtime,production-network,production-media`.
- `package-windows` on `windows-latest`, gated by `workflow_dispatch` input
  `package_windows`, building unsigned Tauri artifacts with
  `tauri-runtime,production-network,production-media`.

The macOS and Windows jobs intentionally do not claim signing, notarization, or
installer trust-chain completion. They prove runner/toolchain package generation
when those runners are explicitly invoked. Signing and notarization require real
secrets, keychain setup, timestamping policy, and separate release governance
before public distribution.

## Local validation

Run:

```sh
npm --prefix apps/ui run test:desktop-package-ci
```

This validates that the workflow keeps Linux release/smoke gates wired and keeps
macOS/Windows package jobs explicitly runner-gated.

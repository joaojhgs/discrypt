# Phase B storage persistence verification

G013 adds a Linux-friendly harness gate for the Phase B persistence/keychain
slice. It verifies the encrypted `AppStore` boundary that production Tauri
integration must consume, without treating deterministic test keychains as a
platform keychain replacement.

## Harness coverage

- `harness/multinode/src/lib.rs`
  - `storage_persistence_smoke` opens a fresh encrypted app DB and verifies it
    starts empty before first profile creation.
  - It writes sensitive app-state bytes, reopens the store through a new handle,
    and confirms restart loads the same state through the `AppStore` boundary.
  - It scans DB, WAL, and temp sidecar paths for forbidden plaintext profile,
    message, and content-key bytes.
  - It overwrites the store with malformed legacy JSON and requires load to fail
    closed instead of silently reseeding application state.
  - It verifies secure delete fails until DB, WAL, and keychain material are all
    included in the deletion set.
- `harness/multinode/src/main.rs`
  - The command-line harness now includes the Phase B storage smoke in the
    readiness gate and prints storage booleans in the smoke output.

## Linux evidence command

Run the storage gate directly:

```bash
cargo test -p discrypt-multinode-harness storage_persistence_smoke_covers_phase_b_gates
```

Run the integrated smoke:

```bash
cargo run -p discrypt-multinode-harness --quiet
```

## Platform repeat plan

- Linux: current CI/harness target for fresh install, restart, corrupt-store
  rejection, forbidden-byte scanning, and secure-delete negative behavior.
- macOS and Windows: repeat the same harness once the platform keychain adapter
  is wired; keep the forbidden-byte scanner pointed at DB, WAL/journal, temp, and
  keychain-backed material where platform APIs expose test handles.
- Android: repeat after the mobile keystore adapter exists; retain the same
  fail-closed corruption and no-plaintext assertions, with platform-specific
  caveats for OS snapshots, crash dumps, and backups.

## Explicit non-claims

- The deterministic harness keychain is not an OS keychain and does not satisfy
  the platform-keychain checklist by itself.
- Corrupt legacy JSON is rejected by the encrypted store gate; forward/backward
  schema migrations remain covered by the storage migration lane.

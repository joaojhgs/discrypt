# Phase 4 retention, shred, live-key, and storage recovery review

G005 implements deterministic foundations for the retention/deletion guarantees in
the approved plan. These are local facades and harness oracles, not claims about
OS-level secure deletion beyond the simulated stores enumerated here.

## Implementation map

- `crates/content-keys/src/lib.rs`
  - `RetentionWindow` includes all required presets plus warned unlimited.
  - `RetentionTransition` enforces shorten-retroactive and lengthen-future-only
    semantics while preserving lock-not-vanish placeholders.
  - `CrossDeviceShredState` models best-effort own-device tombstone propagation:
    online devices stop serving immediately; offline devices are pending until
    reconnect/sync, then must not serve shredded keys.
  - `LiveKeyOracle` checks local epoch membership, rate-limits authorized members,
    and returns decoys for non-members to avoid a non-member decryptability oracle.
- `crates/storage/src/lib.rs`
  - `SecureDeleteSimulator` snapshots local stores, supports restore on failed
    verification, and proves SQLite/WAL/key-store paths no longer contain key bytes
    after a verified delete.
  - `seal_account_backup` remains account-continuity only and excludes content keys.
- `harness/multinode/src/lib.rs`
  - `retention_shred_smoke` covers default lock behavior, retention transitions,
    cross-device shred sync, live-key membership/rate-limit/decoy behavior,
    secure-delete negatives, and recovery-not-resurrecting content keys.

## Acceptance coverage

- AC10/AC10b: default 7-day lock boundary, warned unlimited, shorten retroactive,
  and lengthen future-only are covered by unit and multinode smoke tests.
- AC11: cooperative shred and cross-device offline caveat are captured explicitly:
  pending offline own devices may serve until reconnect, then tombstones block serve.
- AC-PRESENCE: non-members receive decoys, authorized members are locally checked and
  rate-limited, and over-limit responses do not return keys.
- AC-SHRED-PERSIST: SQLite/WAL/key-store material is removed in the simulator; failed
  verification can restore the snapshot before final destroy.
- AC-RECOVERY/AC12 foundation: backups are account-continuity only and do not contain
  archival content keys, so restore cannot resurrect shredded/expired content.

## Production-hardening notes

- `SecureDeleteSimulator` is a test oracle. Native implementations must enumerate real
  SQLite/WAL/keychain paths and still carry the plan's OS caveats: swap, crash dumps,
  and filesystem snapshots remain out of absolute control.
- `LiveKeyOracle` proves the local authorization shape; production membership proofs
  must bind to signed MLS group-state credentials and current epoch authority.
- Cross-device shred remains cooperative; UX must keep the approved copy: deleted on
  online devices now, pending on offline devices until reconnect.

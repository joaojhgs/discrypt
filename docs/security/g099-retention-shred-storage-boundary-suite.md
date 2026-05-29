# G099 retention/shred store and keychain boundary suite

This gate binds Phase N retention and shred claims to the encrypted application store and the keychain trait boundary used by the Rust backend.

## Covered cases

- **Encrypted retention restart**: cached, locked, and shredded retention records round-trip through `EncryptedAppDb` and deserialize after restart.
- **No plaintext/key leakage**: DB, WAL, temp, and keychain snapshots are scanned for retained message plaintext and content-key bytes.
- **Keychain-required restore**: deleting the wrapping key makes the retained ciphertext file unreadable.
- **Complete secure deletion**: DB, WAL, temp, and keychain boundaries must all be enumerated; deleting only the DB/journal is insufficient.
- **Recovery after shred**: account-continuity recovery restores identity/member metadata only and cannot resurrect content keys.

## Harness location

- `harness/multinode/src/lib.rs` exposes `RetentionShredStorageBoundarySmoke` and `retention_shred_storage_boundary_smoke`.
- The regression test is `retention_shred_storage_boundary_smoke_covers_real_store_and_keychain_gates`.
- The gate uses `discrypt-storage` `EncryptedAppDb`, `AppDbKeychain`, `MemoryAppDbKeychain`, `LocalStore`, `RecipientCacheEntry`, and recovery APIs.

## Boundary

This gate uses real file-backed encrypted app DB writes and the keychain trait seam with deterministic test keychain storage. OS keychain provider coverage remains part of the platform packaging and production-storage release gates.

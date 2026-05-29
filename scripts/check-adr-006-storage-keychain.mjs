#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const files = {
  adr: 'docs/adr/adr-006-storage-keychain.md',
  appdb: 'crates/storage/src/appdb.rs',
  lib: 'crates/storage/src/lib.rs',
  cargo: 'crates/storage/Cargo.toml',
  lock: 'Cargo.lock',
};

const read = (path) => readFileSync(resolve(repoRoot, path), 'utf8');
const contents = Object.fromEntries(Object.entries(files).map(([key, path]) => [key, read(path)]));

const required = [
  ['ADR mentions EncryptedAppDb', contents.adr, 'EncryptedAppDb'],
  ['ADR states AES-256-GCM', contents.adr, 'AES-256-GCM'],
  ['ADR states serde_json envelope', contents.adr, 'serde_json'],
  ['ADR is honest about no plaintext SQLite pages', contents.adr, 'persist plaintext SQLite pages'],
  ['ADR covers AppDbSchema', contents.adr, 'AppDbSchema'],
  ['ADR covers VERSION_1_DDL', contents.adr, 'VERSION_1_DDL'],
  ['ADR covers openmls_sqlite_storage', contents.adr, 'openmls_sqlite_storage'],
  ['ADR covers keyring version', contents.adr, 'keyring 3.6.3'],
  ['ADR covers LinuxOsKeychain', contents.adr, 'LinuxOsKeychain'],
  ['ADR covers MemoryAppDbKeychain', contents.adr, 'MemoryAppDbKeychain'],
  ['ADR covers sqlite_wal_path', contents.adr, 'sqlite_wal_path'],
  ['ADR covers quarantine_corrupt_store', contents.adr, 'quarantine_corrupt_store'],
  ['ADR covers SecureDeleteSimulator', contents.adr, 'SecureDeleteSimulator'],
  ['ADR states best-effort secure delete', contents.adr, 'best-effort'],
  ['ADR names SSD limit', contents.adr, 'SSD'],
  ['ADR names cloud snapshot limit', contents.adr, 'cloud snapshot copies'],
  ['ADR covers AppDbMigrationPlan', contents.adr, 'AppDbMigrationPlan'],
  ['ADR covers validate_observed_schema', contents.adr, 'validate_observed_schema'],
  ['appdb has decision struct', contents.appdb, 'StorageKeychainDecision'],
  ['appdb has decision fn', contents.appdb, 'storage_keychain_decision'],
  ['appdb has ADR coverage predicate', contents.appdb, 'covers_adr_006'],
  ['appdb has encrypted DB', contents.appdb, 'EncryptedAppDb'],
  ['appdb has AES-GCM', contents.appdb, 'Aes256Gcm'],
  ['appdb stores wrapped data key', contents.appdb, 'wrapped_data_key'],
  ['appdb zeroizes data key', contents.appdb, 'data_key.zeroize'],
  ['appdb has WAL helper', contents.appdb, 'sqlite_wal_path'],
  ['appdb has forward DDL', contents.appdb, 'VERSION_1_DDL'],
  ['appdb has rollback DDL', contents.appdb, 'VERSION_1_ROLLBACK'],
  ['appdb has quarantine', contents.appdb, 'quarantine_corrupt_store'],
  ['lib exports decision', contents.lib, 'storage_keychain_decision'],
  ['lib exports secure delete simulator', contents.lib, 'SecureDeleteSimulator'],
  ['cargo has keyring 3.6.3', contents.cargo, 'keyring = { version = "3.6.3"'],
  ['cargo has production-storage feature', contents.cargo, 'production-storage'],
  ['cargo has aes-gcm', contents.cargo, 'aes-gcm'],
  ['cargo has zeroize', contents.cargo, 'zeroize'],
  ['lock includes keyring', contents.lock, 'name = "keyring"'],
];

let failed = false;
for (const [label, haystack, needle] of required) {
  if (!haystack.includes(needle)) {
    console.error(`ADR-006 storage/keychain check failed: ${label} missing ${JSON.stringify(needle)}`);
    failed = true;
  }
}

if (/TODO|FIXME|placeholder|facade|shim/i.test(contents.adr)) {
  console.error('ADR-006 storage/keychain check failed: ADR contains TODO/FIXME/placeholder/facade/shim wording');
  failed = true;
}

const commands = [
  ['cargo', ['test', '-p', 'discrypt-storage', 'storage_keychain_decision_covers_adr_006', '--quiet']],
  ['cargo', ['test', '-p', 'discrypt-storage', 'encrypted_app_db_round_trips_without_plaintext_in_db_or_wal', '--quiet']],
  ['cargo', ['test', '-p', 'discrypt-storage', 'encrypted_app_db_persists_wrapped_key_separately_from_keychain', '--quiet']],
  ['cargo', ['test', '-p', 'discrypt-storage', 'migration_from_empty_store_creates_required_schema', '--quiet']],
  ['cargo', ['test', '-p', 'discrypt-storage', 'backward_migration_drops_required_schema_for_recovery_tests', '--quiet']],
  ['cargo', ['test', '-p', 'discrypt-storage', 'corrupt_store_quarantine_moves_db_and_sidecars', '--quiet']],
  ['cargo', ['test', '-p', 'discrypt-storage', 'secure_delete_removes_material_and_snapshot_restores_on_failed_verify', '--quiet']],
];

for (const [cmd, args] of commands) {
  const result = spawnSync(cmd, args, { cwd: repoRoot, stdio: 'inherit' });
  if (result.status !== 0) {
    console.error(`ADR-006 storage/keychain check failed: ${cmd} ${args.join(' ')}`);
    failed = true;
  }
}

if (failed) process.exit(1);
console.log('ADR-006 storage/keychain check passed.');

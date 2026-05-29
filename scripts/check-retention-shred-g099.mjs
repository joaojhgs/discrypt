#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const docs = read("docs/security/g099-retention-shred-storage-boundary-suite.md");
const harness = read("harness/multinode/src/lib.rs");
const storage = read("crates/storage/src/lib.rs");
const appdb = read("crates/storage/src/appdb.rs");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "# G099 retention/shred store and keychain boundary suite",
  "Encrypted retention restart",
  "No plaintext/key leakage",
  "Keychain-required restore",
  "Complete secure deletion",
  "Recovery after shred",
  "OS keychain provider coverage remains part of the platform packaging",
]) requireText("docs", docs, token);

for (const token of [
  "pub struct RetentionShredStorageBoundarySmoke",
  "retention_shred_storage_boundary_smoke",
  "retention_state_round_trips_encrypted_store",
  "store_and_keychain_exclude_plaintext_and_content_keys",
  "keychain_required_for_restore",
  "secure_delete_enumerates_store_journal_temp_and_keychain",
  "recovery_after_shred_excludes_content_keys",
  "retention_shred_storage_boundary_smoke_covers_real_store_and_keychain_gates",
]) requireText("harness", harness, token);

for (const token of ["LocalStore", "RecipientCacheEntry", "KeyState", "recover_account", "seal_account_backup"]) requireText("storage", storage, token);
for (const token of ["EncryptedAppDb", "AppDbKeychain", "MemoryAppDbKeychain", "delete_wrapping_key", "wrapped_data_key"]) requireText("appdb", appdb, token);

if (/TODO|FIXME|unimplemented!|todo!/i.test(docs)) failures.push("retention shred docs contain unfinished-work marker");

const commands = [
  ["cargo", ["test", "-p", "discrypt-multinode-harness", "retention_shred_storage_boundary_smoke_covers_real_store_and_keychain_gates", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-storage", "encrypted_app_db", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-storage", "recovery", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-content-keys", "--quiet"]],
];
for (const [cmd, args] of commands) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G099 retention shred boundary check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G099 retention shred boundary check passed");

#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const adr = read("docs/adr/adr-002-openmls-provider-storage.md");
const provider = read("crates/mls-core/src/provider.rs");
const engine = read("crates/mls-core/src/openmls_engine.rs");
const exporter = read("crates/mls-core/src/exporter.rs");
const delivery = read("crates/mls-delivery/src/lib.rs");
const workspaceCargo = read("Cargo.toml");
const mlsCargo = read("crates/mls-core/Cargo.toml");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "# ADR-002: OpenMLS provider, persistent storage, exporters, and repair integration",
  "Status: accepted",
  "`openmls = 0.8.1`",
  "openmls_rust_crypto::RustCrypto",
  "openmls_sqlite_storage::SqliteStorageProvider<JsonOpenMlsCodec, Connection>",
  "MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519",
  "ED25519",
  "MlsGroup::load",
  "OpenMlsGroupEngine::export_secret",
  "Welcome and GroupInfo bytes",
  "repair plans re-add/rejoin through OpenMLS",
]) requireText("ADR-002", adr, token);

for (const token of [
  "pub struct OpenMlsProviderDecision",
  "provider_decision",
  "openmls_version: \"0.8.1\"",
  "openmls_rust_crypto::RustCrypto",
  "SqliteStorageProvider<JsonOpenMlsCodec, Connection>",
  "MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519",
  "exporter_is_rust_only",
  "provider_decision_covers_adr_002_launch_hint",
]) requireText("provider metadata", provider, token);

for (const token of [
  "DiscryptOpenMlsProvider",
  "RustCrypto",
  "SqliteStorageProvider",
  "run_migrations",
  "MlsGroup::new_with_group_id",
  "add_members",
  "remove_members",
  "merge_pending_commit",
  "StagedWelcome::new_from_welcome",
  "export_secret",
  "MlsGroup::load",
  "CommitMismatch",
  "WelcomeGroupIdMismatch",
]) requireText("openmls engine", engine, token);

for (const token of ["ExportLabel", "Text", "Media", "ContentKey", "derive_epoch_secret"]) requireText("exporter", exporter, token);
for (const token of ["detect_fork_or_replay", "RepairPlan", "DivergentCommitReplay", "rejoin"] ) requireText("delivery", delivery, token);
for (const token of ["openmls = { version = \"0.8.1\"", "openmls_rust_crypto = \"0.5.1\"", "openmls_sqlite_storage = \"0.2.0\""]) requireText("workspace Cargo", workspaceCargo, token);
for (const token of ["openmls = { workspace = true }", "openmls_sqlite_storage = { workspace = true }"]) requireText("mls-core Cargo", mlsCargo, token);

if (/TODO|FIXME|unimplemented!|todo!/i.test(adr)) failures.push("ADR-002 contains unfinished-work marker");

if (failures.length === 0) {
  const commands = [
    ["cargo", ["test", "-p", "discrypt-mls-core", "provider_decision_covers_adr_002_launch_hint", "--quiet"]],
    ["cargo", ["test", "-p", "discrypt-mls-core", "openmls_group_create_add_merge_export_and_reload", "--quiet"]],
    ["cargo", ["test", "-p", "discrypt-mls-core", "openmls_join_from_welcome_validates_and_converges", "--quiet"]],
    ["cargo", ["test", "-p", "discrypt-mls-delivery", "repair", "--quiet"]],
  ];
  for (const [cmd, args] of commands) {
    const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
    if (run.status !== 0) {
      failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
      break;
    }
  }
}

if (failures.length > 0) {
  console.error("ADR-002 OpenMLS provider/storage check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("ADR-002 OpenMLS provider/storage check passed");

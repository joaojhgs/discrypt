#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const canonical = "apps/desktop/src-tauri/src/production_status.rs";
const synchronizedCopies = [
  canonical,
  "crates/abuse/src/production_status.rs",
  "crates/admission/src/production_status.rs",
  "crates/content-keys/src/production_status.rs",
  "crates/core/src/production_status.rs",
  "crates/media/src/production_status.rs",
  "crates/mls-core/src/production_status.rs",
  "crates/mls-delivery/src/production_status.rs",
  "crates/push/src/production_status.rs",
  "crates/relay-overlay/src/production_status.rs",
  "crates/storage/src/production_status.rs",
  "crates/transport/src/production_status.rs",
  "harness/multinode/src/production_status.rs",
];

const failures = [];
function read(path) {
  return readFileSync(resolve(repoRoot, path), "utf8");
}
const canonicalText = read(canonical);
for (const token of [
  "Build-time production/harness status for this crate.",
  "without inferring production readiness from deterministic test adapters",
  "pub struct ProductionStatus",
  "cfg!(feature = \"harness\")",
  "cfg!(feature = \"local-dev\")",
  "cfg!(feature = \"production-network\")",
  "cfg!(feature = \"production-media\")",
  "cfg!(feature = \"production-storage\")",
  "requires_non_production_label",
]) {
  if (!canonicalText.includes(token)) failures.push(`canonical production_status missing token: ${token}`);
}
for (const path of synchronizedCopies) {
  const text = read(path);
  if (text !== canonicalText) {
    failures.push(`${path} drifted from ${canonical}; update the synchronized production-status gate copy or factor it before release`);
  }
}
if (failures.length > 0) {
  console.error("G011 production-status duplicate/dead-code sync gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`G011 production-status sync gate passed: ${synchronizedCopies.length} duplicated gate modules match ${canonical}.`);

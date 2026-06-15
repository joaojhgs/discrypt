#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const deny = read("deny.toml");
const adr = read("docs/adr/adr-008-supply-chain.md");
const packageJson = read("apps/ui/package.json");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "[advisories]",
  "yanked = \"deny\"",
  "RUSTSEC-2026-0173",
  "docs/security/g122-rust-advisory-waivers.md",
  "[licenses]",
  "allow = [",
  "MIT",
  "Apache-2.0",
  "AGPL-3.0-or-later",
  "[bans]",
  "wildcards = \"deny\"",
  "[sources]",
  "unknown-registry = \"deny\"",
  "unknown-git = \"deny\"",
]) requireText("deny.toml", deny, token);

for (const token of [
  "test:cargo-deny-g121",
  "cargo deny check --hide-inclusion-graph",
  "advisory",
  "license",
  "unknown-registry",
  "unknown-git",
  "documented exception names the advisory",
]) requireText("ADR-008", adr, token);

requireText("package.json", packageJson, "test:cargo-deny-g121");

const run = spawnSync("cargo", ["deny", "check", "--hide-inclusion-graph"], {
  cwd: repoRoot,
  encoding: "utf8",
});
if (run.status !== 0) {
  failures.push(`cargo deny check --hide-inclusion-graph failed:\n${run.stdout}\n${run.stderr}`.trim());
}
const output = `${run.stdout}\n${run.stderr}`;
for (const token of ["advisories ok", "bans ok", "licenses ok", "sources ok"]) {
  if (!output.includes(token)) failures.push(`cargo deny output missing token: ${token}`);
}

if (failures.length > 0) {
  console.error("G121 cargo-deny supply-chain gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G121 cargo-deny supply-chain gate passed");

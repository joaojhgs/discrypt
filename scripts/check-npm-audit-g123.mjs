#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const packageJson = read("apps/ui/package.json");
const packageLock = read("apps/ui/package-lock.json");
const adr = read("docs/adr/adr-008-supply-chain.md");
const doc = read("docs/security/g123-npm-advisory-scan.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "test:npm-audit-g123",
  "npm --prefix apps/ui audit --audit-level=high --omit=dev",
  "npm --prefix apps/ui audit --audit-level=high",
  "zero high-or-critical advisories",
  "There are no G123 npm\nadvisory waivers",
]) requireText("docs/security/g123-npm-advisory-scan.md", doc, token);
for (const token of [
  "test:npm-audit-g123",
  "npm audit --audit-level=high",
  "documented non-release waiver",
]) requireText("ADR-008", adr, token);
requireText("package.json", packageJson, "test:npm-audit-g123");
for (const token of ["\"lockfileVersion\"", "\"packages\"", "\"node_modules/vite\""]) {
  requireText("apps/ui/package-lock.json", packageLock, token);
}

function runAudit(label, args) {
  const result = spawnSync("npm", args, {
    cwd: repoRoot,
    encoding: "utf8",
    maxBuffer: 1024 * 1024 * 16,
  });
  if (result.status !== 0) {
    failures.push(`${label} failed:\n${result.stdout}\n${result.stderr}`.trim());
    return;
  }
  let parsed;
  try {
    parsed = JSON.parse(result.stdout);
  } catch (error) {
    failures.push(`${label} did not emit parseable JSON: ${error.message}`);
    return;
  }
  const counts = parsed.metadata?.vulnerabilities ?? {};
  const high = Number(counts.high ?? 0);
  const critical = Number(counts.critical ?? 0);
  if (high > 0 || critical > 0) {
    failures.push(`${label} reported high=${high}, critical=${critical}`);
  }
}

runAudit("npm production audit", ["--prefix", "apps/ui", "audit", "--audit-level=high", "--omit=dev", "--json"]);
runAudit("npm full UI audit", ["--prefix", "apps/ui", "audit", "--audit-level=high", "--json"]);

if (failures.length > 0) {
  console.error("G123 npm-audit gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G123 npm-audit gate passed: zero high-or-critical advisories");

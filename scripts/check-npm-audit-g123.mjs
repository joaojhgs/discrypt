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

function packageNameFromLockPath(lockPath) {
  const parts = lockPath.split("node_modules/");
  return parts[parts.length - 1];
}

function auditRequestBody({ omitDev }) {
  const lock = JSON.parse(packageLock);
  const body = {};
  for (const [lockPath, entry] of Object.entries(lock.packages ?? {})) {
    if (!lockPath || !entry?.version) continue;
    if (omitDev && entry.dev === true) continue;
    const name = entry.name ?? packageNameFromLockPath(lockPath);
    if (!name || name === lockPath) continue;
    body[name] ??= [];
    if (!body[name].includes(entry.version)) body[name].push(entry.version);
  }
  return body;
}

function postBulkAdvisories(body) {
  const payload = JSON.stringify(body);
  const result = spawnSync("curl", [
    "-fsS",
    "--http1.1",
    "--retry",
    "2",
    "--max-time",
    "30",
    "-H",
    "content-type: application/json",
    "-H",
    "accept: application/json",
    "--data-binary",
    "@-",
    "https://registry.npmjs.org/-/npm/v1/security/advisories/bulk",
  ], {
    cwd: repoRoot,
    input: payload,
    encoding: "utf8",
    maxBuffer: 1024 * 1024 * 16,
  });
  if (result.status !== 0) {
    throw new Error(`bulk advisory endpoint failed:\n${result.stdout}\n${result.stderr}`.trim());
  }
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    throw new Error(`bulk advisory endpoint emitted invalid JSON: ${error.message}`);
  }
}

function isEndpointUnavailable(result) {
  return /audit endpoint returned an error|ETIMEDOUT|ECONNRESET|ENOTFOUND|EAI_AGAIN|FetchError/i.test(`${result.stdout}\n${result.stderr}`);
}

function runBulkAdvisoryFallback(label, { omitDev }) {
  let advisories;
  try {
    advisories = postBulkAdvisories(auditRequestBody({ omitDev }));
  } catch (error) {
    failures.push(`${label} bulk advisory fallback failed: ${error.message}`);
    return;
  }
  const severe = [];
  for (const [name, entries] of Object.entries(advisories ?? {})) {
    for (const advisory of entries ?? []) {
      if (["high", "critical"].includes(String(advisory.severity))) {
        severe.push(`${name}: ${advisory.severity} ${advisory.title ?? advisory.url ?? advisory.id ?? "advisory"}`);
      }
    }
  }
  if (severe.length > 0) failures.push(`${label} bulk advisory fallback reported high/critical advisories:\n${severe.join("\n")}`);
}

function runAudit(label, args, options) {
  const result = spawnSync("npm", args, {
    cwd: repoRoot,
    encoding: "utf8",
    maxBuffer: 1024 * 1024 * 16,
  });
  if (result.status !== 0) {
    if (isEndpointUnavailable(result)) {
      console.warn(`${label}: npm audit endpoint unavailable; using npm bulk advisory endpoint fallback`);
      runBulkAdvisoryFallback(label, options);
      return;
    }
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

runAudit("npm production audit", ["--prefix", "apps/ui", "audit", "--audit-level=high", "--omit=dev", "--json"], { omitDev: true });
runAudit("npm full UI audit", ["--prefix", "apps/ui", "audit", "--audit-level=high", "--json"], { omitDev: false });

if (failures.length > 0) {
  console.error("G123 npm-audit gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G123 npm-audit gate passed: zero high-or-critical advisories");

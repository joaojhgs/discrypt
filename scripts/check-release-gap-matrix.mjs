#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const matrixPath = "docs/release/release-gap-matrix-2026-06-15.md";
const docs = readFileSync(resolve(repoRoot, matrixPath), "utf8");
const failures = [];

const requiredTokens = [
  "# Discrypt Release Gap Matrix - 2026-06-15",
  "This matrix is the current release truth source for the Phase 0 reset.",
  "supersedes older \"complete\", \"green\", or production-ready ledgers",
  "Final report verdict: not production-ready.",
  "Blockers exist: yes.",
  "`verified`",
  "`implemented-unverified`",
  "`planned`",
  "`blocked`",
  "## Release gap matrix",
  "## Stale ledger handling",
  "## Known blocker mapping",
  "REG-INVITE-BROKEN-GROUP",
  "REG-MANUAL-ADMISSION-INVISIBLE",
  "REG-PRESENCE-OFFLINE",
  "REG-WEBRTC-ICE-STATE-NEW",
  "REG-STORAGE-VAULT-REINSTALL-FAILURE",
  "Do not promote the final report to production-ready while any row remains",
];

for (const token of requiredTokens) {
  if (!docs.includes(token)) failures.push(`${matrixPath} missing token: ${token}`);
}

const allowedStatuses = new Set([
  "verified",
  "implemented-unverified",
  "planned",
  "blocked",
]);
const rowPattern = /^\| ([^|]+) \| ([^|]+) \| ([^|]+) \| ([^|]+) \|$/gm;
const rows = [...docs.matchAll(rowPattern)]
  .map((match) => ({
    feature: match[1].trim(),
    status: match[2].trim(),
  }))
  .filter((row) => row.feature !== "---" && row.feature !== "Feature / gate");

if (rows.length < 12) failures.push(`expected at least 12 release matrix rows, found ${rows.length}`);

const statusCounts = new Map();
for (const row of rows) {
  if (!allowedStatuses.has(row.status)) {
    failures.push(`row "${row.feature}" has invalid status "${row.status}"`);
  }
  statusCounts.set(row.status, (statusCounts.get(row.status) ?? 0) + 1);
}

for (const status of allowedStatuses) {
  if (!statusCounts.has(status)) failures.push(`matrix has no row labeled ${status}`);
}

const hasUnresolvedRows = rows.some((row) => row.status !== "verified");
const blockerTextPresent = /\bBlockers exist:\s*yes\b/i.test(docs) || /\bblocked\b/.test(docs);
const finalReportClaimsReady =
  /\bFinal report verdict:\s*production-ready\b/i.test(docs) ||
  /\bFinal report status:\s*production-ready\b/i.test(docs) ||
  /\bCurrent verdict:\s*production-ready\b/i.test(docs);

if ((hasUnresolvedRows || blockerTextPresent) && finalReportClaimsReady) {
  failures.push("final report claims production-ready while blockers or non-verified rows exist");
}

if (failures.length > 0) {
  console.error("release gap matrix check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("release gap matrix check passed");

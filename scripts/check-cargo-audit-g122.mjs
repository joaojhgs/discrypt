#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const waiverDocPath = "docs/security/g122-rust-advisory-waivers.md";
const waiverDoc = read(waiverDocPath);
const adr = read("docs/adr/adr-008-supply-chain.md");
const packageJson = read("apps/ui/package.json");
const failures = [];

const requiredWarningIds = [
  "RUSTSEC-2024-0413",
  "RUSTSEC-2024-0416",
  "RUSTSEC-2024-0412",
  "RUSTSEC-2024-0418",
  "RUSTSEC-2024-0411",
  "RUSTSEC-2024-0415",
  "RUSTSEC-2024-0420",
  "RUSTSEC-2024-0419",
  "RUSTSEC-2024-0384",
  "RUSTSEC-2024-0436",
  "RUSTSEC-2024-0370",
  "RUSTSEC-2026-0173",
  "RUSTSEC-2025-0081",
  "RUSTSEC-2025-0075",
  "RUSTSEC-2025-0080",
  "RUSTSEC-2025-0100",
  "RUSTSEC-2025-0098",
  "RUSTSEC-2024-0429",
];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const id of requiredWarningIds) requireText(waiverDocPath, waiverDoc, id);
for (const token of [
  "test:cargo-audit-g122",
  "cargo audit",
  "permits no vulnerability waivers",
]) requireText("ADR-008", adr, token);
requireText("package.json", packageJson, "test:cargo-audit-g122");

const audit = spawnSync("cargo", ["audit", "--json"], {
  cwd: repoRoot,
  encoding: "utf8",
  maxBuffer: 1024 * 1024 * 16,
});
let auditJson = null;
try {
  auditJson = JSON.parse(audit.stdout);
} catch (error) {
  failures.push(`cargo audit --json did not produce parseable JSON: ${error.message}\n${audit.stdout}\n${audit.stderr}`.trim());
}

if (auditJson) {
  const vulnerabilities = auditJson.vulnerabilities?.list ?? [];
  for (const vulnerability of vulnerabilities) {
    const id = vulnerability.advisory?.id ?? "<unknown>";
    const packageName = vulnerability.package?.name ?? "<unknown package>";
    failures.push(
      `cargo audit vulnerability ${id} in ${packageName}; production gate allows no vulnerability waivers`,
    );
  }

  const warningIds = Object.values(auditJson.warnings ?? {})
    .flat()
    .map((warning) => warning.advisory?.id)
    .filter(Boolean)
    .sort();
  const required = [...requiredWarningIds].sort();
  for (const id of required) {
    if (!warningIds.includes(id)) {
      failures.push(`documented warning ${id} not present in cargo audit JSON; update ${waiverDocPath}`);
    }
  }
  for (const id of warningIds) {
    if (!required.includes(id)) {
      failures.push(`cargo audit warning ${id} is not documented in ${waiverDocPath}`);
    }
  }
}

const auditStrict = spawnSync("cargo", ["audit"], {
  cwd: repoRoot,
  encoding: "utf8",
  maxBuffer: 1024 * 1024 * 16,
});
if (auditStrict.status !== 0) {
  failures.push(`cargo audit failed without waivers:
${auditStrict.stdout}
${auditStrict.stderr}`.trim());
}

if (failures.length > 0) {
  console.error("G122 cargo-audit gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G122 cargo-audit gate passed with zero vulnerabilities and documented warning watchlist");

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

const allowedVulnerabilityWaivers = new Map([
  [
    "RUSTSEC-2026-0124",
    {
      package: "libcrux-chacha20poly1305",
      tokens: [
        "RUSTSEC-2026-0124",
        "libcrux-chacha20poly1305",
        "Owner: supply-chain release owner",
        "Release disposition: non-release waiver only",
        "Reason:",
        "Mitigation:",
        "Upgrade path:",
        "Expiry: 2026-07-31",
        "cargo tree --workspace --target all --locked -i\n  libcrux-chacha20poly1305",
      ],
    },
  ],
]);

const requiredWarningIds = [
  "RUSTSEC-2024-0413",
  "RUSTSEC-2024-0416",
  "RUSTSEC-2024-0412",
  "RUSTSEC-2024-0418",
  "RUSTSEC-2024-0411",
  "RUSTSEC-2024-0415",
  "RUSTSEC-2024-0420",
  "RUSTSEC-2024-0419",
  "RUSTSEC-2024-0370",
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

for (const [id, waiver] of allowedVulnerabilityWaivers) {
  for (const token of waiver.tokens) requireText(waiverDocPath, waiverDoc, token);
  requireText(waiverDocPath, waiverDoc, id);
}
for (const id of requiredWarningIds) requireText(waiverDocPath, waiverDoc, id);
for (const token of [
  "test:cargo-audit-g122",
  "cargo audit",
  "documented non-release waivers",
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
    const id = vulnerability.advisory?.id;
    const packageName = vulnerability.package?.name;
    const waiver = allowedVulnerabilityWaivers.get(id);
    if (!waiver) {
      failures.push(`unwaived cargo audit vulnerability ${id ?? "<unknown>"} in ${packageName ?? "<unknown package>"}`);
      continue;
    }
    if (packageName !== waiver.package) {
      failures.push(`waiver ${id} expected package ${waiver.package}, saw ${packageName}`);
    }
  }
  for (const id of requiredWarningIds) {
    const found = Object.values(auditJson.warnings ?? {})
      .flat()
      .some((warning) => warning.advisory?.id === id);
    if (!found) failures.push(`documented warning ${id} not present in cargo audit JSON; update ${waiverDocPath}`);
  }
}

const activeTree = spawnSync(
  "cargo",
  ["tree", "--workspace", "--target", "all", "--locked", "-i", "libcrux-chacha20poly1305"],
  { cwd: repoRoot, encoding: "utf8" },
);
const activeTreeOutput = `${activeTree.stdout}\n${activeTree.stderr}`;
if (activeTreeOutput.includes("libcrux-chacha20poly1305 v")) {
  failures.push(`RUSTSEC-2026-0124 waiver is no longer non-release; active cargo tree contains libcrux-chacha20poly1305:\n${activeTreeOutput}`.trim());
}

const auditWithWaivers = spawnSync("cargo", ["audit", "--ignore", "RUSTSEC-2026-0124"], {
  cwd: repoRoot,
  encoding: "utf8",
  maxBuffer: 1024 * 1024 * 16,
});
if (auditWithWaivers.status !== 0) {
  failures.push(`cargo audit with documented waiver failed:\n${auditWithWaivers.stdout}\n${auditWithWaivers.stderr}`.trim());
}

if (failures.length > 0) {
  console.error("G122 cargo-audit gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G122 cargo-audit gate passed with enforced non-release waiver for RUSTSEC-2026-0124");

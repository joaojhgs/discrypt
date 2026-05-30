#!/usr/bin/env node
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const ci = read(".github/workflows/ci.yml");
const releaseMatrix = read("docs/release/release-verification-matrix.md");
const acDashboard = JSON.parse(read(".omx/artifacts/production-readiness/ac-dashboard.json"));
const g131Doc = read("docs/release/g131-final-e2e-verification.md");
const failures = [];

const qualityGatePath = resolve(repoRoot, "target/release/g131-final-e2e-quality-gate.json");
const requiredCommandEvidence = [
  "cargo fmt --all --check",
  "npm --prefix apps/ui run test:final-e2e-g131",
  "npm --prefix apps/ui run test:e2e",
  "npm --prefix apps/ui run test:ui-integration-g130",
  "npm --prefix apps/ui run test:release-no-fallback-g129",
  "npm --prefix apps/ui run test:placeholder-allowlist-g128",
  "npm --prefix apps/ui run test:no-placeholders-g127",
  "npm --prefix apps/ui run test:release-linux",
  "npm --prefix apps/ui run test:linux-package-smoke",
  "npm --prefix apps/ui run test:desktop-package-ci",
  "npm --prefix apps/ui run test:android-gate",
  "npm --prefix apps/ui run test:signaling-relay-ops",
  "npm --prefix apps/ui run test:release-governance",
  "npm --prefix apps/ui run test:release-verification-matrix",
  "npm --prefix apps/ui run test:ac-dashboard-g101",
  "npm --prefix apps/ui run test:pcap-suite-g096",
  "npm --prefix apps/ui run test:malicious-relay-g097",
  "npm --prefix apps/ui run test:malicious-member-g098",
  "npm --prefix apps/ui run test:retention-shred-g099",
  "npm --prefix apps/ui run test:performance-soak-g100",
  "npm --prefix apps/ui run test:presence-g115",
  "npm --prefix apps/ui run test:abuse-g120",
  "npm --prefix apps/ui run test:cargo-deny-g121",
  "npm --prefix apps/ui run test:cargo-audit-g122",
  "npm --prefix apps/ui run test:npm-audit-g123",
  "npm --prefix apps/ui run test:sbom-g124",
  "npm --prefix apps/ui run test:crypto-sensitive-g125",
  "npm --prefix apps/ui run test:repro-g126",
  "npm --prefix apps/ui run test:honesty",
  "npm --prefix apps/ui run test:command-coverage",
  "npm --prefix apps/ui run build",
  "cargo check --workspace --quiet",
  "cargo test --workspace --quiet",
  "cargo clippy --workspace --all-targets --quiet -- -D warnings",
  "git diff --check",
];

function requireScript(name) {
  if (!packageJson.scripts?.[name]) failures.push(`package script missing ${name}`);
}
function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing ${token}`);
}


for (const script of [
  "test:e2e",
  "test:ui-integration-g130",
  "test:release-no-fallback-g129",
  "test:placeholder-allowlist-g128",
  "test:no-placeholders-g127",
  "test:release-linux",
  "test:linux-package-smoke",
  "test:desktop-package-ci",
  "test:android-gate",
  "test:signaling-relay-ops",
  "test:release-governance",
  "test:release-verification-matrix",
  "test:ac-dashboard-g101",
  "test:pcap-suite-g096",
  "test:malicious-relay-g097",
  "test:malicious-member-g098",
  "test:retention-shred-g099",
  "test:performance-soak-g100",
  "test:presence-g115",
  "test:abuse-g120",
  "test:cargo-deny-g121",
  "test:cargo-audit-g122",
  "test:npm-audit-g123",
  "test:sbom-g124",
  "test:crypto-sensitive-g125",
  "test:repro-g126",
  "test:final-e2e-g131",
]) requireScript(script);

if (!packageJson.scripts?.["test:e2e"]?.includes("--workers=1")) {
  failures.push("test:e2e must run Playwright serially; the local-dev fallback state harness is single-user and parallel workers can kill/reuse the preview server mid-suite");
}

for (const token of [
  "test:ui-integration-g130",
  "test:release-no-fallback-g129",
  "test:placeholder-allowlist-g128",
  "test:no-placeholders-g127",
  "test:final-e2e-g131",
]) requireText("ci", ci, token);

for (const token of [
  "G131 final E2E verification",
  "8 Chromium tests passed",
  "multi-process/multi-host coverage is represented by maintained Rust/process\nharness gates",
  "cargo clippy --workspace --all-targets --quiet -- -D warnings",
]) requireText("G131 final verification doc", g131Doc, token);

for (const token of [
  "npm --prefix apps/ui run release:linux",
  "npm --prefix apps/ui run smoke:linux-packages",
  "npm --prefix apps/ui run test:desktop-package-ci",
  "npm --prefix apps/ui run test:android-gate",
  "npm --prefix apps/ui run test:release-verification-matrix",
  "Sensitive data exclusion",
  "signaling admin audit tokens",
  "TURN static auth secrets",
]) requireText("release verification matrix", releaseMatrix, token);

if (acDashboard.schema_version !== "discrypt.ac_dashboard.v2") failures.push("AC dashboard schema mismatch");
if (acDashboard.no_skipped_blockers !== true) failures.push("AC dashboard must declare no skipped blockers");
if (!Array.isArray(acDashboard.rows) || acDashboard.rows.length < 25) failures.push("AC dashboard must enumerate original acceptance criteria");
for (const row of acDashboard.rows ?? []) {
  if (["missing", "blocked", "skipped", "unverified", "partial_foundation", "placeholder_or_foundation"].includes(String(row.dashboard_status))) {
    failures.push(`${row.id} has non-final dashboard status ${row.dashboard_status}`);
  }
}

for (const path of [
  "docs/security/g130-ui-integration-gate.md",
  "docs/security/g129-release-no-fallback-gate.md",
  "docs/security/g128-placeholder-allowlist.md",
  "docs/security/g127-static-no-placeholder-gate.md",
  "scripts/generate-sbom-g124.mjs",
  "scripts/reproducible-release-evidence-g126.mjs",
]) {
  if (!existsSync(resolve(repoRoot, path))) failures.push(`required final evidence artifact missing ${path}`);
}

if (failures.length > 0) {
  console.error("G131 final E2E readiness gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

const qualityGate = {
  schema_version: "discrypt.g131.final_e2e_quality_gate.v1",
  generated_at: new Date().toISOString(),
  status: "readiness-gate-passed",
  gate: "G131-full-production-e2e-verification-acr",
  serialized_playwright_e2e: true,
  required_command_evidence: requiredCommandEvidence,
  readiness_inputs: {
    ci_workflow: ".github/workflows/ci.yml",
    release_matrix: "docs/release/release-verification-matrix.md",
    ac_dashboard: ".omx/artifacts/production-readiness/ac-dashboard.json",
    package_scripts: "apps/ui/package.json",
  },
  note: "This JSON records the static readiness gate. The final checkpoint evidence must also cite a fresh successful run of every required command in required_command_evidence plus final ai-slop-cleaner and code-review evidence.",
};
mkdirSync(dirname(qualityGatePath), { recursive: true });
writeFileSync(qualityGatePath, `${JSON.stringify(qualityGate, null, 2)}\n`);
console.log(`G131 final E2E readiness gate passed: release, UI, package, adversarial, supply-chain, SBOM, and AC-dashboard gates are wired with required evidence artifacts. Quality gate: ${qualityGatePath}`);

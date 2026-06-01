#!/usr/bin/env node
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const qualityGatePath = resolve(repoRoot, "target/release/g131-final-e2e-quality-gate.json");
const requiredCommandEvidence = [
  "cargo fmt --all --check",
  "npm --prefix apps/ui run test:final-e2e-g131",
  "npm --prefix apps/ui run test:release-two-profile-harness-g010",
  "npm --prefix apps/ui run release:two-profile-harness-g010:dry-run",
  "npm --prefix apps/ui run test:g011-boundary",
  "npm --prefix apps/ui run test:e2e",
  "npm --prefix apps/ui run test:ui-integration-g130",
  "npm --prefix apps/ui run test:release-no-fallback-g129",
  "npm --prefix apps/ui run test:placeholder-allowlist-g128",
  "npm --prefix apps/ui run test:no-placeholders-g127",
  "npm --prefix apps/ui run test:release-linux",
  "npm --prefix apps/ui run test:linux-package-smoke",
  "npm --prefix apps/ui run test:desktop-package-ci",
  "npm --prefix apps/ui run test:android-gate",
  "npm --prefix apps/ui run test:release-governance",
  "npm --prefix apps/ui run test:release-verification-matrix",
  "npm --prefix apps/ui run test:pcap-suite-g096",
  "npm --prefix apps/ui run test:malicious-relay-g097",
  "npm --prefix apps/ui run test:malicious-member-g098",
  "npm --prefix apps/ui run test:retention-shred-g099",
  "npm --prefix apps/ui run test:performance-soak-g100",
  "npm --prefix apps/ui run test:security-privacy-g009",
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

function read(path) {
  return readFileSync(resolve(repoRoot, path), "utf8");
}

function readJson(path) {
  return JSON.parse(read(path));
}

function writeQualityGate(status, details = {}) {
  const qualityGate = {
    schema_version: "discrypt.g131.final_e2e_quality_gate.v1",
    generated_at: new Date().toISOString(),
    status,
    gate: "G131-final-e2e-verification-acr",
    serialized_playwright_e2e: true,
    required_command_evidence: requiredCommandEvidence,
    readiness_inputs: {
      ci_workflow: ".github/workflows/ci.yml",
      release_matrix: "docs/release/release-verification-matrix.md",
      package_scripts: "apps/ui/package.json",
    },
    note: "This JSON records the static readiness gate. The final checkpoint evidence must also cite a fresh successful run of every required command in required_command_evidence plus final ai-slop-cleaner and code-review evidence.",
    ...details,
  };
  mkdirSync(dirname(qualityGatePath), { recursive: true });
  writeFileSync(qualityGatePath, `${JSON.stringify(qualityGate, null, 2)}\n`);
}

function requireScript(packageJson, failures, name) {
  if (!packageJson.scripts?.[name]) failures.push(`package script missing ${name}`);
}

function requireText(failures, name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing ${token}`);
}

function run() {
  const packageJson = readJson("apps/ui/package.json");
  const ci = read(".github/workflows/ci.yml");
  const releaseMatrix = read("docs/release/release-verification-matrix.md");
  const g131Doc = read("docs/release/g131-final-e2e-verification.md");
  const failures = [];

  for (const script of [
    "test:e2e",
    "test:g011-boundary",
    "test:release-two-profile-harness-g010",
    "release:two-profile-harness-g010",
    "release:two-profile-harness-g010:dry-run",
    "test:ui-integration-g130",
    "test:release-no-fallback-g129",
    "test:placeholder-allowlist-g128",
    "test:no-placeholders-g127",
    "test:release-linux",
    "test:linux-package-smoke",
    "test:desktop-package-ci",
    "test:android-gate",
    "test:release-governance",
    "test:release-verification-matrix",
    "test:pcap-suite-g096",
    "test:malicious-relay-g097",
    "test:malicious-member-g098",
    "test:retention-shred-g099",
    "test:performance-soak-g100",
    "test:security-privacy-g009",
    "test:presence-g115",
    "test:abuse-g120",
    "test:cargo-deny-g121",
    "test:cargo-audit-g122",
    "test:npm-audit-g123",
    "test:sbom-g124",
    "test:crypto-sensitive-g125",
    "test:repro-g126",
    "test:final-e2e-g131",
  ]) requireScript(packageJson, failures, script);

  if (!packageJson.scripts?.["test:e2e"]?.includes("--workers=1")) {
    failures.push("test:e2e must run Playwright serially; the local-dev fallback state harness is single-user and parallel workers can kill/reuse the preview server mid-suite");
  }

  for (const token of [
    "test:ui-integration-g130",
    "test:release-no-fallback-g129",
    "test:placeholder-allowlist-g128",
    "test:no-placeholders-g127",
    "test:final-e2e-g131",
  ]) requireText(failures, "ci", ci, token);

  for (const token of [
    "G131 final E2E verification",
    "9 Chromium tests passed",
    "two independent browser profiles",
    "multi-process/multi-host coverage is represented by maintained Rust/process\nharness gates",
    "cargo clippy --workspace --all-targets --quiet -- -D warnings",
    "npm --prefix apps/ui run test:release-two-profile-harness-g010",
    "npm --prefix apps/ui run release:two-profile-harness-g010:dry-run",
    "npm --prefix apps/ui run test:g011-boundary",
  "npm --prefix apps/ui run test:g011-boundary",
  ]) requireText(failures, "G131 final verification doc", g131Doc, token);

  for (const token of [
    "npm --prefix apps/ui run release:linux",
    "npm --prefix apps/ui run smoke:linux-packages",
    "npm --prefix apps/ui run test:desktop-package-ci",
    "npm --prefix apps/ui run test:android-gate",
    "npm --prefix apps/ui run test:release-verification-matrix",
    "Sensitive data exclusion",
    "signaling admin audit tokens",
    "TURN static auth secrets",
    "G011/G012 boundary and unsupported-path gate",
  ]) requireText(failures, "release verification matrix", releaseMatrix, token);


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
    writeQualityGate("readiness-gate-failed", { failures });
    console.error("G131 final E2E readiness gate failed:");
    for (const failure of failures) console.error(`- ${failure}`);
    process.exit(1);
  }

  writeQualityGate("readiness-gate-passed");
  console.log(`G131 final E2E readiness gate passed: release, UI, package, adversarial, supply-chain, and SBOM gates are wired with required evidence artifacts. Quality gate: ${qualityGatePath}`);
}

try {
  run();
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  writeQualityGate("readiness-gate-failed", { failures: [message] });
  console.error("G131 final E2E readiness gate failed:");
  console.error(`- ${message}`);
  process.exit(1);
}

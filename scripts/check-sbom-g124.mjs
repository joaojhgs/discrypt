#!/usr/bin/env node
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const failures = [];
function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}
function parseJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

const packageJson = read("apps/ui/package.json");
const adr = read("docs/adr/adr-008-supply-chain.md");
const releaseScript = read("scripts/release-linux.mjs");
const releaseCheck = read("scripts/check-release-linux.mjs");
const doc = read("docs/release/sbom-g124.md");
const workflow = read(".github/workflows/ci.yml");

for (const token of ["test:sbom-g124", "sbom:g124", "generate-sbom-g124.mjs"]) {
  requireText("package.json", packageJson, token);
}
for (const token of [
  "cargo sbom --output-format spdx_json_2_3",
  "npm sbom --sbom-format spdx",
  "discrypt-packaged-artifacts.spdx.json",
  "target/release/bundle",
]) requireText("docs/release/sbom-g124.md", doc, token);
for (const token of ["generate-sbom-g124.mjs", "--require-packaged-artifacts", "target/sbom"]) {
  requireText("release-linux.mjs", releaseScript, token);
}
for (const token of ["generate-sbom-g124.mjs", "target/sbom"]) requireText("check-release-linux.mjs", releaseCheck, token);
for (const token of ["cargo-sbom", "discrypt-sbom", "target/sbom"]) requireText("ci.yml", workflow, token);
for (const token of ["test:sbom-g124", "SBOM generated for Rust, npm, and packaged artifacts"]) {
  requireText("ADR-008", adr, token);
}

const outDir = mkdtempSync(resolve(tmpdir(), "discrypt-g124-sbom-"));
try {
  const result = spawnSync(process.execPath, [
    "scripts/generate-sbom-g124.mjs",
    "--out-dir",
    outDir,
    "--require-packaged-artifacts",
  ], { cwd: repoRoot, encoding: "utf8", maxBuffer: 1024 * 1024 * 32 });
  if (result.status !== 0) {
    failures.push(`generate-sbom-g124 failed:\n${result.stdout}\n${result.stderr}`.trim());
  } else {
    const index = JSON.parse(result.stdout);
    const rust = parseJson(resolve(outDir, "discrypt-rust.spdx.json"));
    const npm = parseJson(resolve(outDir, "discrypt-ui-npm.spdx.json"));
    const packaged = parseJson(resolve(outDir, "discrypt-packaged-artifacts.spdx.json"));
    if (rust.spdxVersion !== "SPDX-2.3" || (rust.packages?.length ?? 0) === 0) failures.push("Rust SBOM is not populated SPDX 2.3");
    if (npm.spdxVersion !== "SPDX-2.3" || (npm.packages?.length ?? 0) === 0) failures.push("npm SBOM is not populated SPDX 2.3");
    const packageNames = (packaged.packages ?? []).map((pkg) => pkg.name).join("\n");
    for (const token of [".deb", ".rpm", ".AppImage"]) {
      if (!packageNames.includes(token)) failures.push(`packaged artifact SBOM missing ${token}`);
    }
    for (const entry of index.files ?? []) {
      if (!entry.sha256 || !entry.packageCount) failures.push(`SBOM index entry incomplete: ${JSON.stringify(entry)}`);
    }
    if (!index.source?.cargoLock?.sha256 || !index.source?.packageLock?.sha256) failures.push("SBOM index missing lockfile hashes");
    const targets = (index.source?.linuxBundleTargets ?? []).join(",").toLowerCase();
    if (!targets.includes("all") && !targets.includes("deb")) failures.push("SBOM index missing Linux bundle targets");
    for (const token of [".deb", ".rpm", ".AppImage"]) {
      if (!(index.packagedArtifacts ?? []).some((artifact) => artifact.path.includes(token) && artifact.sha256 && artifact.size > 0)) {
        failures.push(`SBOM index missing package artifact hash for ${token}`);
      }
    }
  }
} finally {
  rmSync(outDir, { recursive: true, force: true });
}

if (failures.length > 0) {
  console.error("G124 SBOM gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G124 SBOM gate passed: Rust, npm, and packaged artifact SPDX SBOMs generated");

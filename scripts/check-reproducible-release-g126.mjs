#!/usr/bin/env node
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
function requireText(name, text, token) { if (!text.includes(token)) failures.push(`${name} missing token: ${token}`); }
const rustToolchain = read("rust-toolchain.toml");
const nodeVersion = read(".node-version").trim();
const packageJson = read("apps/ui/package.json");
const parsedPackageJson = JSON.parse(packageJson);
const releaseScript = read("scripts/release-linux.mjs");
const releaseCheck = read("scripts/check-release-linux.mjs");
const doc = read("docs/release/reproducible-release-g126.md");
const adr = read("docs/adr/adr-008-supply-chain.md");
const ci = read(".github/workflows/ci.yml");
for (const token of ["channel = \"1.96.0\"", "rustfmt", "clippy"]) requireText("rust-toolchain.toml", rustToolchain, token);
if (nodeVersion !== "22.22.0") failures.push(`.node-version expected 22.22.0, saw ${nodeVersion}`);
for (const token of ["test:repro-g126", "repro:g126", "test:g011-production-status-sync"]) requireText("package.json", packageJson, token);
if (parsedPackageJson.packageManager !== "npm@10.9.4") failures.push(`apps/ui/package.json expected packageManager npm@10.9.4, saw ${parsedPackageJson.packageManager ?? "<missing>"}`);
for (const token of ["reproducible-release-evidence-g126.mjs", "SOURCE_DATE_EPOCH", "target/release/reproducibility-g126.json"]) requireText("release-linux.mjs", releaseScript, token);
for (const token of ["reproducible-release-evidence-g126.mjs", "target/release/reproducibility-g126.json"]) requireText("check-release-linux.mjs", releaseCheck, token);
for (const token of ["dtolnay/rust-toolchain@1.96.0", "node-version-file: .node-version", "target/sbom"]) requireText("ci.yml", ci, token);
for (const token of ["test:repro-g126", "Release build can be reproduced from lockfiles and documented toolchain versions"]) requireText("ADR-008", adr, token);
for (const token of ["Cargo.lock", "apps/ui/package-lock.json", "rust-toolchain.toml", ".node-version", "@tauri-apps/cli@2.11.2", "SOURCE_DATE_EPOCH"]) requireText("docs/release/reproducible-release-g126.md", doc, token);
const outDir = mkdtempSync(resolve(tmpdir(), "discrypt-g126-"));
const bundleDir = resolve(outDir, "bundle");
const sbomDir = resolve(outDir, "sbom");
try {
  for (const [subdir, name] of [
    ["deb", "discrypt_0.1.0_amd64.deb"],
    ["rpm", "discrypt-0.1.0-1.x86_64.rpm"],
    ["appimage", "discrypt_0.1.0_amd64.AppImage"],
  ]) {
    const dir = resolve(bundleDir, subdir);
    mkdirSync(dir, { recursive: true });
    writeFileSync(resolve(dir, name), `discrypt-g126-test-artifact:${name}\n`);
  }
  mkdirSync(sbomDir, { recursive: true });
  writeFileSync(resolve(sbomDir, "discrypt-test.spdx.json"), "{\"spdxVersion\":\"SPDX-2.3\"}\n");
  const result = spawnSync(process.execPath, [
    "scripts/reproducible-release-evidence-g126.mjs",
    "--out",
    resolve(outDir, "evidence.json"),
    "--bundle-dir",
    bundleDir,
    "--sbom-dir",
    sbomDir,
  ], {
    cwd: repoRoot,
    encoding: "utf8",
    env: { ...process.env, SOURCE_DATE_EPOCH: "1780330000" },
    maxBuffer: 1024 * 1024 * 16,
  });
  if (result.status !== 0) failures.push(`reproducible evidence generation failed:\n${result.stdout}\n${result.stderr}`.trim());
  else {
    const evidence = JSON.parse(result.stdout);
    if (!evidence.git?.commit || !evidence.git?.commitTimestamp) failures.push("evidence missing git commit/timestamp");
    if (!evidence.lockfiles?.cargoLock?.sha256 || !evidence.lockfiles?.packageLock?.sha256) failures.push("evidence missing lockfile hashes");
    if (!String(evidence.toolchain?.rustc ?? "").includes("1.96.0")) failures.push("evidence rustc version does not match rust-toolchain.toml");
    if (evidence.toolchain?.nodeVersionFile !== "22.22.0" || evidence.toolchain?.node !== "v22.22.0") failures.push("evidence node version does not match .node-version");
    if (!String(evidence.toolchain?.tauriCli ?? "").includes("2.11.2")) failures.push("evidence missing pinned Tauri CLI 2.11.2");
    if ((evidence.artifacts ?? []).length === 0) failures.push("evidence missing package artifact hashes; run release:linux before G126");
    if ((evidence.sboms ?? []).length === 0) failures.push("evidence missing SBOM hashes; run sbom:g124 before G126");
  }
} finally { rmSync(outDir, { recursive: true, force: true }); }
if (failures.length > 0) {
  console.error("G126 reproducible-release gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G126 reproducible-release gate passed: lockfiles, toolchain versions, package hashes, and SBOM hashes recorded");

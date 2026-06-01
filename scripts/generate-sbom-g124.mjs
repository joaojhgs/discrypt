#!/usr/bin/env node
import { existsSync, mkdirSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const outDir = resolve(repoRoot, valueAfter("--out-dir") ?? "target/sbom");
const bundleRoot = resolve(repoRoot, valueAfter("--bundle-dir") ?? "target/release/bundle");
const requirePackagedArtifacts = args.includes("--require-packaged-artifacts");
const allowMissingPackagedArtifacts = args.includes("--allow-missing-packaged-artifacts");

function valueAfter(flag) {
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : undefined;
}
function fail(message) {
  console.error(`generate-sbom-g124: ${message}`);
  process.exit(1);
}
function run(command, commandArgs, stdoutPath) {
  const result = spawnSync(command, commandArgs, {
    cwd: repoRoot,
    encoding: "utf8",
    maxBuffer: 1024 * 1024 * 64,
  });
  if (result.status !== 0) {
    fail(`${[command, ...commandArgs].join(" ")} failed:\n${result.stdout}\n${result.stderr}`.trim());
  }
  writeFileSync(stdoutPath, result.stdout);
  return JSON.parse(result.stdout);
}
function walk(dir, predicate, output = []) {
  if (!existsSync(dir)) return output;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = resolve(dir, entry.name);
    if (entry.isDirectory()) walk(path, predicate, output);
    else if (entry.isFile() && predicate(path)) output.push(path);
  }
  return output;
}
function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}
function stableSourceDateEpoch() {
  const envEpoch = process.env.SOURCE_DATE_EPOCH;
  if (envEpoch && /^\d+$/.test(envEpoch)) return Number(envEpoch);
  const result = spawnSync("git", ["log", "-1", "--format=%ct"], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (result.status === 0 && /^\d+$/.test(result.stdout.trim())) {
    return Number(result.stdout.trim());
  }
  return 0;
}
function isoFromEpoch(epochSeconds) {
  return new Date(epochSeconds * 1000).toISOString().replace(/\.\d{3}Z$/, "Z");
}
function namespaceSeed(artifactPaths) {
  const hash = createHash("sha256");
  hash.update(sha256(resolve(repoRoot, "Cargo.lock")));
  hash.update(sha256(resolve(repoRoot, "apps/ui/package-lock.json")));
  for (const path of artifactPaths) {
    hash.update(relative(repoRoot, path));
    hash.update(sha256(path));
  }
  return hash.digest("hex");
}
function spdxId(value) {
  return `SPDXRef-${value.replace(/[^A-Za-z0-9.-]+/g, "-").replace(/^-+|-+$/g, "")}`;
}

mkdirSync(outDir, { recursive: true });
const rustPath = resolve(outDir, "discrypt-rust.spdx.json");
const npmPath = resolve(outDir, "discrypt-ui-npm.spdx.json");
const packagesPath = resolve(outDir, "discrypt-packaged-artifacts.spdx.json");
const indexPath = resolve(outDir, "discrypt-sbom-index.json");

const rustSbom = run("cargo", ["sbom", "--output-format", "spdx_json_2_3"], rustPath);
const npmSbom = run("npm", ["--prefix", "apps/ui", "sbom", "--sbom-format", "spdx", "--sbom-type", "application"], npmPath);

const artifactPaths = walk(bundleRoot, (path) => /\.(deb|rpm|AppImage)$/i.test(path)).sort();
if (requirePackagedArtifacts && artifactPaths.length === 0 && !allowMissingPackagedArtifacts) {
  fail("no packaged artifacts found under target/release/bundle; run npm --prefix apps/ui run release:linux first");
}

const sourceDateEpoch = stableSourceDateEpoch();
const now = isoFromEpoch(sourceDateEpoch);
const documentSeed = namespaceSeed(artifactPaths);
const artifactPackages = artifactPaths.map((path) => {
  const rel = relative(repoRoot, path);
  const size = statSync(path).size;
  return {
    name: rel,
    SPDXID: spdxId(rel),
    downloadLocation: "NOASSERTION",
    filesAnalyzed: false,
    versionInfo: "0.1.0",
    checksums: [{ algorithm: "SHA256", checksumValue: sha256(path) }],
    externalRefs: [
      {
        referenceCategory: "PACKAGE-MANAGER",
        referenceType: "purl",
        referenceLocator: `pkg:generic/discrypt/${encodeURIComponent(rel)}?size=${size}`,
      },
    ],
    supplier: "Organization: Discrypt project",
    originator: "Organization: Discrypt project",
    licenseConcluded: "AGPL-3.0-or-later",
    licenseDeclared: "AGPL-3.0-or-later",
    copyrightText: "NOASSERTION",
  };
});
const packageDocument = {
  spdxVersion: "SPDX-2.3",
  dataLicense: "CC0-1.0",
  SPDXID: "SPDXRef-DOCUMENT",
  name: "discrypt-packaged-artifacts",
  documentNamespace: `https://example.invalid/discrypt/sbom/packaged-artifacts/${documentSeed}`,
  creationInfo: {
    created: now,
    creators: ["Tool: scripts/generate-sbom-g124.mjs", "Organization: Discrypt project"],
  },
  documentDescribes: artifactPackages.map((pkg) => pkg.SPDXID),
  packages: artifactPackages,
  relationships: artifactPackages.map((pkg) => ({
    spdxElementId: "SPDXRef-DOCUMENT",
    relationshipType: "DESCRIBES",
    relatedSpdxElement: pkg.SPDXID,
  })),
};
writeFileSync(packagesPath, `${JSON.stringify(packageDocument, null, 2)}\n`);

const tauriConfig = JSON.parse(readFileSync(resolve(repoRoot, "apps/desktop/src-tauri/tauri.conf.json"), "utf8"));
const configuredTargets = tauriConfig.bundle?.targets;
const bundleTargets = Array.isArray(configuredTargets) ? configuredTargets : [configuredTargets ?? "all"];
const index = {
  generatedAt: now,
  source: {
    cargoLock: { path: "Cargo.lock", sha256: sha256(resolve(repoRoot, "Cargo.lock")) },
    packageLock: { path: "apps/ui/package-lock.json", sha256: sha256(resolve(repoRoot, "apps/ui/package-lock.json")) },
    packageArtifacts: relative(repoRoot, bundleRoot),
    linuxBundleTargets: bundleTargets,
    sourceDateEpoch,
  },
  packagedArtifacts: artifactPaths.map((path) => ({
    path: relative(repoRoot, path),
    sha256: sha256(path),
    size: statSync(path).size,
  })),
  files: [
    { kind: "rust-spdx", path: relative(repoRoot, rustPath), packageCount: rustSbom.packages?.length ?? 0, sha256: sha256(rustPath) },
    { kind: "npm-spdx", path: relative(repoRoot, npmPath), packageCount: npmSbom.packages?.length ?? 0, sha256: sha256(npmPath) },
    { kind: "packaged-artifacts-spdx", path: relative(repoRoot, packagesPath), packageCount: artifactPackages.length, sha256: sha256(packagesPath) },
  ],
};
writeFileSync(indexPath, `${JSON.stringify(index, null, 2)}\n`);

console.log(JSON.stringify(index, null, 2));

#!/usr/bin/env node
import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriConfigPath = resolve(repoRoot, "apps/desktop/src-tauri/tauri.conf.json");
const uiPackageDir = resolve(repoRoot, "apps/ui");
const targetDir = resolve(repoRoot, "target/release/bundle");
const tauriCli = process.env.DISCRYPT_TAURI_CLI ?? "@tauri-apps/cli@2.11.2";
const releaseFeatures = (
  process.env.DISCRYPT_RELEASE_FEATURES ??
  "tauri-runtime,production-network,production-media,production-storage"
)
  .split(",")
  .map((feature) => feature.trim())
  .filter(Boolean);
const bundles = (process.env.DISCRYPT_LINUX_BUNDLES ?? "deb,rpm,appimage")
  .split(",")
  .map((bundle) => bundle.trim().toLowerCase())
  .filter(Boolean);
const dryRun =
  process.argv.includes("--dry-run") ||
  process.env.DISCRYPT_RELEASE_DRY_RUN === "1";

function fail(message) {
  console.error(`release-linux: ${message}`);
  process.exit(1);
}

function run(command, args, options = {}) {
  const rendered = [command, ...args].join(" ");
  if (dryRun) return { command, args, rendered, skipped: true };
  console.log(`$ ${rendered}`);
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    stdio: "inherit",
    env: process.env,
    ...options,
  });
  if (result.status !== 0) {
    fail(`${rendered} failed with status ${result.status ?? "unknown"}`);
  }
  return { command, args, rendered, skipped: false };
}

if (process.platform !== "linux" && !dryRun) {
  fail("Linux package bundles must be built on a Linux host");
}
if (!existsSync(tauriConfigPath)) fail(`missing ${tauriConfigPath}`);
if (!existsSync(resolve(uiPackageDir, "package.json"))) {
  fail("missing apps/ui/package.json");
}

const tauriConfig = JSON.parse(readFileSync(tauriConfigPath, "utf8"));
const configuredTargets = tauriConfig.bundle?.targets;
const configuredTargetList = Array.isArray(configuredTargets)
  ? configuredTargets.map((target) => String(target).toLowerCase())
  : [String(configuredTargets ?? "").toLowerCase()];
const targetSupported =
  configuredTargetList.includes("all") ||
  bundles.every((bundle) => configuredTargetList.includes(bundle));

if (!tauriConfig.bundle?.active) fail("Tauri bundling must be active");
if (!targetSupported) {
  fail(
    `Tauri bundle.targets must be "all" or include every requested Linux bundle (${bundles.join(",")})`,
  );
}
if (!releaseFeatures.includes("tauri-runtime")) {
  fail("DISCRYPT_RELEASE_FEATURES must include tauri-runtime for desktop packaging");
}

const steps = [];
steps.push(
  run("npm", ["--prefix", "apps/ui", "ci"]),
  run("npm", ["--prefix", "apps/ui", "run", "test:honesty"]),
  run("npm", ["--prefix", "apps/ui", "run", "test:command-coverage"]),
  run("npm", ["--prefix", "apps/ui", "run", "build"]),
  run("cargo", ["test", "-p", "discrypt-desktop", "--features", releaseFeatures.join(",")]),
  run("npx", [
    "--yes",
    tauriCli,
    "build",
    "--config",
    tauriConfigPath,
    "--bundles",
    bundles.join(","),
    "--features",
    releaseFeatures.join(","),
  ]),
  run(process.execPath, [
    "scripts/generate-sbom-g124.mjs",
    "--out-dir",
    "target/sbom",
    "--require-packaged-artifacts",
  ]),
);

const plan = {
  productName: tauriConfig.productName,
  version: tauriConfig.version,
  identifier: tauriConfig.identifier,
  bundles,
  releaseFeatures,
  tauriConfigPath,
  targetDir,
  dryRun,
  steps,
};

if (dryRun) console.log(JSON.stringify(plan, null, 2));
else console.log(`release-linux: bundles written under ${targetDir}`);

#!/usr/bin/env node
import { existsSync, mkdirSync, readdirSync, statSync, writeFileSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const out = resolve(repoRoot, valueAfter("--out") ?? "target/release/reproducibility-g126.json");
const bundleRoot = resolve(repoRoot, valueAfter("--bundle-dir") ?? "target/release/bundle");
const sbomRoot = resolve(repoRoot, valueAfter("--sbom-dir") ?? "target/sbom");
function valueAfter(flag) { const i = args.indexOf(flag); return i >= 0 ? args[i + 1] : undefined; }
function run(command, commandArgs) {
  const result = spawnSync(command, commandArgs, { cwd: repoRoot, encoding: "utf8", maxBuffer: 1024 * 1024 * 8 });
  if (result.status !== 0) throw new Error(`${[command, ...commandArgs].join(" ")} failed:\n${result.stdout}\n${result.stderr}`.trim());
  return result.stdout.trim();
}
function sha256(path) { return createHash("sha256").update(readFileSync(resolve(repoRoot, path))).digest("hex"); }
function readOptional(path) {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}
function parseOsRelease(text) {
  const entries = {};
  for (const line of text.split(/\r?\n/)) {
    const match = line.match(/^([A-Z0-9_]+)=(.*)$/);
    if (!match) continue;
    entries[match[1]] = match[2].replace(/^"|"$/g, "");
  }
  return entries;
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
const rustToolchain = readFileSync(resolve(repoRoot, "rust-toolchain.toml"), "utf8");
const nodeVersion = readFileSync(resolve(repoRoot, ".node-version"), "utf8").trim();
const osRelease = parseOsRelease(readOptional("/etc/os-release"));
const packageArtifacts = walk(bundleRoot, (path) => /\.(deb|rpm|AppImage)$/i.test(path)).sort();
const sboms = walk(sbomRoot, (path) => /\.json$/i.test(path)).sort();
const evidence = {
  schema: "discrypt.g126.reproducible-release-evidence.v1",
  git: {
    commit: run("git", ["rev-parse", "HEAD"]),
    commitTimestamp: Number(run("git", ["log", "-1", "--format=%ct"])),
  },
  lockfiles: {
    cargoLock: { path: "Cargo.lock", sha256: sha256("Cargo.lock") },
    packageLock: { path: "apps/ui/package-lock.json", sha256: sha256("apps/ui/package-lock.json") },
  },
  toolchain: {
    rustToolchainToml: rustToolchain,
    rustc: run("rustc", ["--version"]),
    cargo: run("cargo", ["--version"]),
    nodeVersionFile: nodeVersion,
    node: run("node", ["--version"]),
    npm: run("npm", ["--version"]),
    tauriCli: run("npx", ["--yes", "@tauri-apps/cli@2.11.2", "--version"]),
    cargoSbom: run("cargo", ["sbom", "--version"]),
    cargoAudit: run("cargo", ["audit", "--version"]),
    cargoDeny: run("cargo", ["deny", "--version"]),
  },
  deterministicInputs: {
    sourceDateEpoch: Number(process.env.SOURCE_DATE_EPOCH || run("git", ["log", "-1", "--format=%ct"])),
    tauriCliPackage: "@tauri-apps/cli@2.11.2",
    releaseFeatures: (process.env.DISCRYPT_RELEASE_FEATURES ?? "tauri-runtime,production-network,production-media,production-storage,mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter").split(","),
    linuxBuildBaseline: {
      prettyName: osRelease.PRETTY_NAME ?? "",
      id: osRelease.ID ?? "",
      versionId: osRelease.VERSION_ID ?? "",
      imageDigest: process.env.DISCRYPT_LINUX_BUILD_IMAGE_DIGEST ?? process.env.GITHUB_ACTIONS_RUNNER_IMAGE_DIGEST ?? "",
      runnerImage: process.env.ImageOS ?? process.env.RUNNER_OS ?? "",
    },
  },
  artifacts: packageArtifacts.map((path) => ({ path: relative(repoRoot, path), size: statSync(path).size, sha256: createHash("sha256").update(readFileSync(path)).digest("hex") })),
  sboms: sboms.map((path) => ({ path: relative(repoRoot, path), size: statSync(path).size, sha256: createHash("sha256").update(readFileSync(path)).digest("hex") })),
};
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, `${JSON.stringify(evidence, null, 2)}\n`);
console.log(JSON.stringify(evidence, null, 2));

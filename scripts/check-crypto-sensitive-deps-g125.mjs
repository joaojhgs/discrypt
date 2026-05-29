#!/usr/bin/env node
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const inventoryPath = resolve(repoRoot, "docs/security/g125-crypto-sensitive-dependencies.json");
const writeMode = process.argv.includes("--write");
const failures = [];
const directSensitive = [
  ["aes-gcm", "AEAD payload protection"],
  ["ed25519-dalek", "device and governance signatures"],
  ["libopus-rs", "voice codec boundary"],
  ["openmls", "MLS group state and epochs"],
  ["openmls_basic_credential", "MLS credential support"],
  ["openmls_rust_crypto", "OpenMLS crypto provider"],
  ["openmls_sqlite_storage", "OpenMLS persistent storage"],
  ["openmls_traits", "OpenMLS provider traits"],
  ["rand", "randomness source wrapper"],
  ["rand_core", "randomness trait boundary"],
  ["sha2", "hashing and commitments"],
  ["zeroize", "secret memory clearing"],
];
const sensitiveName = /^(aead|aes|aes-gcm|chacha20|chacha20poly1305|cipher|crypto-bigint|crypto-common|digest|ed25519|ed25519-dalek|fiat-crypto|getrandom|hkdf|hmac|hpke-rs|hpke-rs-crypto|hpke-rs-libcrux|hpke-rs-rust-crypto|k256|libcrux|libopus-rs|openmls|openmls_basic_credential|openmls_memory_storage|openmls_rust_crypto|openmls_sqlite_storage|openmls_traits|p256|poly1305|rand|rand_chacha|rand_core|ring|rtc-dtls|rustls|rustls-pki-types|rustls-webpki|sha2|signature|tls_codec|tls_codec_derive|webrtc|x25519-dalek|zeroize|zeroize_derive)/;

function read(path) { return readFileSync(resolve(repoRoot, path), "utf8"); }
function sha256(path) { return createHash("sha256").update(readFileSync(resolve(repoRoot, path))).digest("hex"); }
function fail(message) { failures.push(message); }
function requireText(name, text, token) { if (!text.includes(token)) fail(`${name} missing token: ${token}`); }
function run(command, args) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8", maxBuffer: 1024 * 1024 * 32 });
  if (result.status !== 0) fail(`${[command, ...args].join(" ")} failed:\n${result.stdout}\n${result.stderr}`.trim());
  return result.stdout;
}
function workspaceConstraint(cargoToml, crateName) {
  const line = cargoToml.split(/\r?\n/).find((candidate) => candidate.startsWith(`${crateName} = `));
  if (!line) return null;
  const value = line.slice(line.indexOf("=") + 1).trim();
  const quoted = value.match(/^"([^"]+)"/);
  const tableVersion = value.match(/version\s*=\s*"([^"]+)"/);
  return { line, value, versionRequirement: quoted?.[1] ?? tableVersion?.[1] ?? null };
}
function buildInventory() {
  const cargoToml = read("Cargo.toml");
  const metadata = JSON.parse(run("cargo", ["metadata", "--locked", "--format-version", "1"]));
  const packagesByName = new Map();
  for (const pkg of metadata.packages) {
    if (!packagesByName.has(pkg.name)) packagesByName.set(pkg.name, []);
    packagesByName.get(pkg.name).push(pkg);
  }
  const directCrates = directSensitive.map(([crate, reason]) => {
    const constraint = workspaceConstraint(cargoToml, crate);
    if (!constraint) fail(`Cargo.toml missing direct crypto-sensitive dependency ${crate}`);
    if (constraint?.versionRequirement === "*" || constraint?.value.includes("*") || constraint?.value.includes("git")) {
      fail(`direct crypto-sensitive dependency ${crate} is not release-pinnable: ${constraint.value}`);
    }
    const packages = packagesByName.get(crate) ?? [];
    if (packages.length === 0) fail(`Cargo.lock/metadata missing ${crate}`);
    const requirement = constraint?.versionRequirement ?? "";
    const matching = packages.filter((pkg) => requirement && pkg.version.startsWith(`${requirement}.`));
    const selected = matching.length === 1 ? matching[0] : packages.length === 1 ? packages[0] : null;
    if (!selected) fail(`direct crypto-sensitive dependency ${crate} cannot be mapped to one locked version for requirement ${requirement}; saw ${packages.map((pkg) => pkg.version).join(",")}`);
    return {
      crate,
      reason,
      manifestConstraint: constraint?.versionRequirement ?? constraint?.value ?? "missing",
      lockedVersion: selected?.version ?? "missing",
      source: selected?.source ?? "missing",
      pinSource: "Cargo.lock",
      vendored: false,
    };
  });
  const transitiveWatchlist = metadata.packages
    .filter((pkg) => sensitiveName.test(pkg.name))
    .map((pkg) => ({ name: pkg.name, version: pkg.version, source: pkg.source ?? "workspace" }))
    .sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`));
  const packageLock = JSON.parse(read("apps/ui/package-lock.json"));
  const npmPackages = Object.entries(packageLock.packages ?? {})
    .filter(([path]) => path.startsWith("node_modules/"))
    .map(([path, pkg]) => ({ path, version: pkg.version ?? "missing", integrityPresent: Boolean(pkg.integrity) }))
    .sort((a, b) => a.path.localeCompare(b.path));
  return {
    schema: "discrypt.g125.crypto-sensitive-dependencies.v1",
    policy: "lockfile-first; vendoring is not the default unless release isolation requires it",
    cargoLockSha256: sha256("Cargo.lock"),
    packageLockSha256: sha256("apps/ui/package-lock.json"),
    directCrates,
    transitiveWatchlist,
    npmLockSummary: {
      lockfileVersion: packageLock.lockfileVersion,
      packageCount: npmPackages.length,
      allLockedEntriesHaveVersion: npmPackages.every((pkg) => pkg.version !== "missing"),
      entriesWithoutIntegrity: npmPackages.filter((pkg) => !pkg.integrityPresent).map((pkg) => pkg.path),
    },
  };
}

const inventory = buildInventory();
if (writeMode) {
  writeFileSync(inventoryPath, `${JSON.stringify(inventory, null, 2)}\n`);
}

const doc = read("docs/security/g125-crypto-sensitive-dependency-policy.md");
const adr = read("docs/adr/adr-008-supply-chain.md");
const packageJson = read("apps/ui/package.json");
for (const token of [
  "test:crypto-sensitive-g125",
  "Crypto-sensitive dependency policy",
  "Cargo.lock",
  "apps/ui/package-lock.json",
  "lockfile-first",
  "vendoring is not the default",
  "docs/security/g125-crypto-sensitive-dependencies.json",
]) requireText("docs/security/g125-crypto-sensitive-dependency-policy.md", doc, token);
for (const token of ["test:crypto-sensitive-g125", "Crypto-sensitive dependencies are pinned or vendored"]) requireText("ADR-008", adr, token);
requireText("package.json", packageJson, "test:crypto-sensitive-g125");

if (!existsSync(inventoryPath)) fail("missing docs/security/g125-crypto-sensitive-dependencies.json; run check-crypto-sensitive-deps-g125.mjs --write");
else {
  const committed = JSON.parse(readFileSync(inventoryPath, "utf8"));
  if (JSON.stringify(committed, null, 2) !== JSON.stringify(inventory, null, 2)) {
    fail("crypto-sensitive dependency inventory is stale; run check-crypto-sensitive-deps-g125.mjs --write and review lockfile changes");
  }
}
if (!inventory.npmLockSummary.allLockedEntriesHaveVersion) fail("package-lock has entries without versions");
if (inventory.npmLockSummary.entriesWithoutIntegrity.length > 0) {
  fail(`package-lock entries missing integrity: ${inventory.npmLockSummary.entriesWithoutIntegrity.join(", ")}`);
}

if (failures.length > 0) {
  console.error("G125 crypto-sensitive dependency gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`G125 crypto-sensitive dependency gate passed: ${inventory.directCrates.length} direct crates and ${inventory.transitiveWatchlist.length} sensitive transitive entries pinned by lockfiles`);

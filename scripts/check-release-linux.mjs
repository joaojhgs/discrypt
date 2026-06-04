#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const result = spawnSync(process.execPath, ["scripts/release-linux.mjs", "--dry-run"], {
  cwd: repoRoot,
  encoding: "utf8",
  env: { ...process.env, DISCRYPT_RELEASE_DRY_RUN: "1" },
});

if (result.status !== 0) {
  process.stderr.write(result.stderr);
  process.exit(result.status ?? 1);
}

const plan = JSON.parse(result.stdout);
const failures = [];
for (const bundle of ["deb", "rpm", "appimage"]) {
  if (!plan.bundles.includes(bundle)) failures.push(`missing Linux bundle target: ${bundle}`);
}
for (const dep of ["gnome-keyring", "dbus-user-session", "libpam-gnome-keyring"]) {
  if (!plan.linuxRuntimeDependencies?.deb?.includes(dep)) {
    failures.push(`release plan missing Debian runtime dependency: ${dep}`);
  }
}
if (!plan.linuxRuntimeDependencies?.rpm?.includes("gnome-keyring")) {
  failures.push("release plan missing RPM runtime dependency: gnome-keyring");
}
for (const feature of [
  "tauri-runtime",
  "production-network",
  "production-media",
  "production-storage",
  "mqtt-adapter",
  "nostr-adapter",
  "ipfs-pubsub-adapter",
  "discrypt-quic-rendezvous-adapter",
]) {
  if (!plan.releaseFeatures.includes(feature)) failures.push(`missing release feature: ${feature}`);
}
const rendered = plan.steps.map((step) => step.rendered).join("\n");
for (const token of [
  "npm --prefix apps/ui ci",
  "npm --prefix apps/ui run test:honesty",
  "npm --prefix apps/ui run test:command-coverage",
  "npm --prefix apps/ui run test:release-no-fallback-g129",
  "npm --prefix apps/ui run test:ui-integration-g130",
  "npm --prefix apps/ui run build",
  "cargo test -p discrypt-desktop --features",
  "production_storage_persists_sealed_envelope_without_plain_state",
  "npx --yes @tauri-apps/cli@2.11.2 build",
  "--bundles deb,rpm,appimage",
  "node scripts/generate-sbom-g124.mjs --out-dir target/sbom --require-packaged-artifacts",
  "node scripts/reproducible-release-evidence-g126.mjs --out target/release/reproducibility-g126.json",
]) {
  if (!rendered.includes(token)) failures.push(`release plan missing command token: ${token}`);
}
if (!String(plan.tauriConfigPath).endsWith("apps/desktop/src-tauri/tauri.release.conf.json")) {
  failures.push("release plan must use the production desktop Tauri release config");
}
for (const feature of ["harness", "local-dev"]) {
  if (plan.tauriBuildFeatures?.includes(feature)) failures.push(`release Tauri config must exclude harness/local-dev feature: ${feature}`);
  if (plan.effectiveReleaseFeatures?.includes(feature)) failures.push(`effective release features must exclude harness/local-dev feature: ${feature}`);
}
if (!plan.sourceDateEpoch) failures.push("release plan missing SOURCE_DATE_EPOCH");
for (const feature of ["harness", "local-dev"]) {
  if (plan.releaseFeatures.includes(feature)) failures.push(`release plan must exclude harness/local-dev feature: ${feature}`);
}

if (failures.length > 0) {
  console.error("release-linux dry-run check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(
  `release-linux dry-run check passed: ${plan.bundles.join(",")} with ${plan.releaseFeatures.join(",")}`,
);

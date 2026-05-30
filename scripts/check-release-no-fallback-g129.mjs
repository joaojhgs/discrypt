#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const files = {
  uiCommands: read("apps/ui/src/commands.ts"),
  uiMain: read("apps/ui/src/main.tsx"),
  desktop: read("apps/desktop/src-tauri/src/lib.rs"),
  releaseLinux: read("scripts/release-linux.mjs"),
  releaseCheck: read("scripts/check-release-linux.mjs"),
  packageJson: read("apps/ui/package.json"),
};
const failures = [];

function requireText(name, token) {
  if (!files[name].includes(token)) failures.push(`${name} missing token: ${token}`);
}
function rejectText(name, token) {
  if (files[name].includes(token)) failures.push(`${name} must not contain production fallback token: ${token}`);
}
function runNode(label, args, env = {}) {
  return spawnSync(process.execPath, args, {
    cwd: repoRoot,
    encoding: "utf8",
    env: { ...process.env, ...env },
  });
}

for (const token of [
  "import.meta.env.DEV ||",
  'import.meta.env.VITE_DISCRYPT_LOCAL_DEV_FALLBACK === "1"',
  "if (!LOCAL_DEV_FALLBACK_ENABLED)",
  "local fallback requires VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1",
]) requireText("uiCommands", token);

for (const token of [
  "forbiddenReleaseFeatures",
  '"harness", "local-dev"',
  "release builds must not include non-production features",
  '"test:release-no-fallback-g129"',
]) requireText("releaseLinux", token);
for (const token of ["test:release-no-fallback-g129", "harness/local-dev"]) requireText("releaseCheck", token);
requireText("packageJson", "test:release-no-fallback-g129");

for (const token of [
  "Demo fallback active",
  "VITE_DISCRYPT_LOCAL_DEV_FALLBACK",
  "FALLBACK_STORAGE_KEY",
  "fallbackState",
  "local-dev-harness",
]) rejectText("uiMain", token);

for (const token of [
  'cfg!(feature = "production-network")',
  'cfg!(feature = "production-media")',
  'cfg!(all(target_os = "linux", feature = "production-storage"))',
  'cfg!(any(test, feature = "harness", feature = "local-dev"))',
]) requireText("desktop", token);

const planResult = runNode("release dry-run", ["scripts/release-linux.mjs", "--dry-run"], {
  DISCRYPT_RELEASE_DRY_RUN: "1",
});
if (planResult.status !== 0) {
  failures.push(`release dry-run unexpectedly failed:\n${planResult.stdout}\n${planResult.stderr}`.trim());
} else {
  const plan = JSON.parse(planResult.stdout);
  const rendered = plan.steps.map((step) => step.rendered).join("\n");
  if (!rendered.includes("npm --prefix apps/ui run test:release-no-fallback-g129")) {
    failures.push("release dry-run plan does not run test:release-no-fallback-g129 before packaging");
  }
  if (rendered.includes("VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1")) {
    failures.push("release dry-run must not enable VITE_DISCRYPT_LOCAL_DEV_FALLBACK");
  }
  const forbidden = plan.releaseFeatures.filter((feature) => ["harness", "local-dev"].includes(feature));
  if (forbidden.length > 0) failures.push(`release dry-run includes forbidden features: ${forbidden.join(",")}`);
}

for (const feature of ["local-dev", "harness"]) {
  const negative = runNode(`release dry-run rejects ${feature}`, ["scripts/release-linux.mjs", "--dry-run"], {
    DISCRYPT_RELEASE_DRY_RUN: "1",
    DISCRYPT_RELEASE_FEATURES: `tauri-runtime,production-network,production-media,production-storage,${feature}`,
  });
  if (negative.status === 0) {
    failures.push(`release dry-run must fail when ${feature} is requested`);
  }
  if (!negative.stderr.includes("release builds must not include non-production features")) {
    failures.push(`release dry-run rejection for ${feature} missing non-production feature error`);
  }
}

if (failures.length > 0) {
  console.error("G129 release no-fallback gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G129 release no-fallback gate passed: release plan rejects harness/local-dev and runs before packaging.");

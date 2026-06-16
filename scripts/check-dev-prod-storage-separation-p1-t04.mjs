#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

function rejectText(name, text, token) {
  if (text.includes(token)) failures.push(`${name} must not contain token: ${token}`);
}

const packageJson = JSON.parse(read("apps/ui/package.json"));
const desktop = read("apps/desktop/src-tauri/src/lib.rs");
const tauriDev = read("apps/desktop/src-tauri/tauri.conf.json");
const tauriRelease = read("apps/desktop/src-tauri/tauri.release.conf.json");
const twoProfileLaunch = read("scripts/g010-tauri-two-profile-launch.mjs");
const p1Plan = read(".omx/plans/P1-T04-dev-prod-storage-separation-2026-06-16.md");

if (
  packageJson.scripts?.["test:p1-t04-dev-prod-storage"] !==
  "node ../../scripts/check-dev-prod-storage-separation-p1-t04.mjs"
) {
  failures.push("apps/ui/package.json missing test:p1-t04-dev-prod-storage script");
}

for (const token of [
  'const APP_STATE_PRODUCTION_DIR_NAME: &str = "discrypt"',
  'const APP_STATE_LOCAL_DEV_DIR_NAME: &str = "discrypt-local-dev"',
  "enum AppStatePathDomain",
  "fn default_app_state_path_domain() -> AppStatePathDomain",
  'all(feature = "harness", not(feature = "production-storage"))',
  'all(feature = "local-dev", not(feature = "production-storage"))',
  'cfg!(all(not(test), feature = "production-storage"))',
  "fn production_app_store_path() -> PathBuf",
  "production_app_store_path()",
  "PathBuf::from(app_dir).join(APP_STATE_STORE_FILENAME)",
  "dev_app_store_path_uses_local_dev_domain_by_default",
  "explicit_env_override_can_select_profile_path_in_local_dev",
  "production_app_store_path_stays_on_host_profile_domain",
  "OpenMLS sidecar storage must follow the explicitly selected app-state profile",
]) requireText("desktop storage path boundary", desktop, token);

for (const token of [
  '"local-dev"',
  '"tauri-runtime"',
  '"production-media"',
]) requireText("tauri dev config", tauriDev, token);
rejectText("tauri dev config", tauriDev, '"production-storage"');

for (const token of [
  '"production-storage"',
  '"production-network"',
  '"tauri-runtime"',
]) requireText("tauri release config", tauriRelease, token);
rejectText("tauri release config", tauriRelease, '"local-dev"');
rejectText("tauri release config", tauriRelease, '"harness"');

for (const token of [
  "DISCRYPT_APP_STATE_PATH",
  "profiles = {",
  "alice/app-state.discrypt-store",
  "bob/app-state.discrypt-store",
  "must include local-dev or harness so DISCRYPT_APP_STATE_PATH profile isolation is honored",
  "must not include production-storage because production-storage builds do not honor DISCRYPT_APP_STATE_PATH profile isolation",
  "not a production release claim",
]) requireText("G010 two-profile launch", twoProfileLaunch, token);

for (const token of [
  "cargo tauri dev",
  "discrypt-local-dev",
  "DISCRYPT_APP_STATE_PATH",
  "Production/package run",
  "not a broad production-ready storage claim",
]) requireText("P1-T04 plan", p1Plan, token);

if (failures.length > 0) {
  console.error("P1-T04 dev/prod storage separation check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("P1-T04 dev/prod storage separation check passed: local-dev defaults use discrypt-local-dev, release config keeps production-storage without local-dev/harness, and two-profile dev runs require explicit DISCRYPT_APP_STATE_PATH isolation.");

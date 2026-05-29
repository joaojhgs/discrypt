#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const docs = read("docs/release/update-rollback-privacy-secrets.md");
const secretsText = read("deploy/release/secrets-inventory.json");
const tauriConfig = JSON.parse(read("apps/desktop/src-tauri/tauri.conf.json"));
const secrets = JSON.parse(secretsText);

const failures = [];
for (const token of [
  "# Update, rollback, crash-report privacy, and secrets policy",
  "Tauri updater configuration is intentionally absent",
  "Rollback policy",
  "Crash-report privacy policy",
  "Crash reporting is opt-in only",
  "Forbidden crash data",
  "SDP",
  "ICE credentials",
  "STUN/TURN",
  "MLS secrets",
  "SFrame keys",
  "retention capped at 30 days",
  "Secrets management",
  "deploy/release/secrets-inventory.json",
  "TAURI_PRIVATE_KEY",
  "EXTERNAL_TURN_STATIC_AUTH_SECRET",
  "Rollback verification repeats install/launch smoke",
]) {
  if (!docs.includes(token)) failures.push(`release governance docs missing token: ${token}`);
}

if (tauriConfig.plugins?.updater || JSON.stringify(tauriConfig).includes('"updater"')) {
  failures.push("Tauri updater config is present but G093 policy says updater is not enabled yet");
}

if (secrets.schema_version !== 1) failures.push("secrets inventory schema_version must be 1");
if (secrets.rotation_policy_days !== 90) failures.push("secrets inventory rotation policy must be 90 days");
const requiredSecrets = [
  "TAURI_PRIVATE_KEY",
  "APPLE_IDENTITY_P12_BASE64",
  "APPLE_NOTARIZATION_CREDENTIALS",
  "WINDOWS_SIGNING_CERTIFICATE_BASE64",
  "EXTERNAL_SIGNALING_ADMIN_AUDIT_TOKEN_HEX",
  "EXTERNAL_TURN_STATIC_AUTH_SECRET",
  "CRASH_REPORT_UPLOAD_TOKEN",
];
const names = new Set(secrets.secrets?.map((entry) => entry.name));
for (const name of requiredSecrets) {
  if (!names.has(name)) failures.push(`secrets inventory missing ${name}`);
}
for (const entry of secrets.secrets ?? []) {
  for (const field of ["name", "scope", "storage", "rotation_trigger", "runtime_exposure"]) {
    if (!entry[field]) failures.push(`secret ${entry.name ?? "<unnamed>"} missing ${field}`);
  }
}
for (const forbidden of [/TODO|FIXME|unimplemented!|todo!/i, /send message bodies/i, /raw database rows are allowed/i]) {
  if (forbidden.test(docs) || forbidden.test(secretsText)) {
    failures.push(`release governance artifacts contain forbidden marker: ${forbidden}`);
  }
}
if (/(TAURI_PRIVATE_KEY|EXTERNAL_TURN_STATIC_AUTH_SECRET).*000000/i.test(secretsText)) {
  failures.push("secrets inventory must describe secret names without dummy secret values");
}

if (failures.length > 0) {
  console.error("release governance check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("release governance check passed");

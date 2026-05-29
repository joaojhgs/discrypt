#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const files = {
  adr: read("docs/adr/adr-005-opaque-pake-admission-helper.md"),
  admission: read("crates/admission/src/lib.rs"),
  uiCommands: read("apps/ui/src/commands.ts"),
  uiMain: read("apps/ui/src/main.tsx"),
  tauri: read("apps/desktop/src-tauri/src/lib.rs"),
};

const failures = [];
function requireToken(file, token) {
  if (!files[file].includes(token)) failures.push(`${file} missing token: ${token}`);
}
function run(label, command, args) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8" });
  if (result.status !== 0) failures.push(`${label} failed:\n${result.stdout}\n${result.stderr}`.trim());
}

for (const token of [
  "# ADR-005: Password-gated admission helper",
  "OnlineAdmissionHelper",
  "PasswordGate::OnlineAuthorizedHelper",
  "AuthorizedHelperProof",
  "PasswordGate::OfflineVerifier",
  "OfflineVerifierRejected",
  "PasswordRejected",
  "AuthorizedWelcome",
  "MLS Welcome/add",
  "OPAQUE/PAKE remains a reserved future path",
  "password_rejected",
  "helper_mismatch",
  "helper_proof_expired",
  "welcome_required",
  "welcome_invalid",
  "offline_verifier_rejected",
]) requireToken("adr", token);

for (const token of [
  "AdmissionPasswordProtocol",
  "AdmissionPasswordDecision",
  "admission_password_decision",
  "covers_adr_005",
  "OnlineAdmissionHelper",
  "attempts_by_subject",
  "max_attempts",
  "AuthorizedHelperProof",
  "PasswordGate::OfflineVerifier",
  "InviteError::OfflineVerifierRejected",
  "InviteError::PasswordRejected",
  "finalize_helper_admission",
  "AuthorizedWelcome",
  "admission_password_decision_covers_adr_005",
  "online_helper_failure_privacy_uses_uniform_rejection",
]) requireToken("admission", token);

for (const token of ["OPAQUE/PAKE or an online authorized helper", "no offline verifier", "authorized MLS Welcome/add"]) requireToken("uiCommands", token);
for (const token of ["Invite admission", "MLS admission", "Password-gate status"]) requireToken("uiMain", token);
for (const token of ["Final admission still requires an authorized MLS Welcome/add", "Waiting for an authorized member or helper"]) requireToken("tauri", token);

if (/TODO|FIXME|unimplemented!|todo!/i.test(files.adr)) failures.push("adr contains unfinished-work marker");

run("ADR-005 decision unit", "cargo", ["test", "-p", "discrypt-admission", "admission_password_decision_covers_adr_005", "--quiet"]);
run("online helper proof", "cargo", ["test", "-p", "discrypt-admission", "online_helper_flow_rate_limits_and_signs_expiring_proofs", "--quiet"]);
run("uniform helper failure", "cargo", ["test", "-p", "discrypt-admission", "online_helper_failure_privacy_uses_uniform_rejection", "--quiet"]);
run("helper welcome required", "cargo", ["test", "-p", "discrypt-admission", "helper_admission_requires_matching_gate_and_welcome", "--quiet"]);
run("offline verifier rejected", "cargo", ["test", "-p", "discrypt-admission", "admission_rejects_offline_verifier_and_requires_welcome", "--quiet"]);
run("UI honesty", "npm", ["--prefix", "apps/ui", "run", "test:honesty"]);

if (failures.length > 0) {
  console.error("ADR-005 PAKE/admission helper check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("ADR-005 PAKE/admission helper check passed");

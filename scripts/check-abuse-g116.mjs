#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const desktop = read("apps/desktop/src-tauri/src/lib.rs");
const delivery = read("crates/mls-delivery/src/lib.rs");
const admission = read("crates/admission/src/lib.rs");
const signaling = read("external/signaling-repository/src/server.rs");
const abuse = read("crates/abuse/src/lib.rs");
const docs = read("docs/phase-5-governance-admission-recovery-abuse.md");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "PersistedAbuseState",
  "allow_invite_create",
  "allow_invite_consume",
  "allow_admission_helper",
  "allow_signaling_publish_take",
  "allow_text_send",
  "invite_create_rate_limited",
  "invite_consume_rate_limited",
  "admission_helper_rate_limited",
  "signaling_publish_rate_limited",
  "text_send_rate_limited",
  "abuse_rate_limits_invite_consume_helper_and_text_send_commands",
]) requireText("desktop", desktop, token);

for (const token of [
  "TextSendAbuseGate",
  "send_with_abuse_gate",
  "TextSendRateLimited",
  "AbuseControls",
  "outbound_text_pipeline_enforces_abuse_gate_before_storage_or_transport",
]) requireText("mls-delivery", delivery, token);

for (const token of [
  "OnlineAdmissionHelper",
  "attempt_online_helper",
  "finalize_helper_admission",
  "OfflineVerifierRejected",
]) requireText("admission", admission, token);

for (const token of [
  "authorize_request",
  "rate_limit_max_requests",
  "ReplayNonce",
  "RateLimited",
  "guarded_requests_reject_replay_and_rate_limit_with_structured_errors",
]) requireText("signaling", signaling, token);

for (const token of ["RateLimiter", "AbuseControls", "allow_invite", "allow_message"]) {
  requireText("abuse", abuse, token);
}

for (const token of [
  "invite/spam rate limits",
  "production must wire OPAQUE/PAKE or an online authorized admission helper",
]) requireText("docs", docs, token);

const checks = [
  ["cargo", ["test", "-p", "discrypt-abuse", "rate_limits_invites_and_spam_and_scores_freeloading", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-admission", "online_helper_flow_rate_limits_and_signs_expiring_proofs", "--quiet"]],
  ["cargo", ["test", "-p", "external-signaling", "guarded_requests_reject_replay_and_rate_limit_with_structured_errors", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-mls-delivery", "outbound_text_pipeline_enforces_abuse_gate_before_storage_or_transport", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-desktop", "abuse_rate_limits_invite_consume_helper_and_text_send_commands", "--quiet"]],
];
for (const [cmd, args] of checks) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G116 abuse production gate check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G116 abuse production gate check passed");

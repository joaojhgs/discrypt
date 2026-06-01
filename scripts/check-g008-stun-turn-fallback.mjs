#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];
const skips = [];
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

function run(label, command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    env: { ...process.env, ...(options.env || {}) },
  });
  if (result.status !== 0) {
    failures.push(`${label} failed:\n${result.stdout}\n${result.stderr}`.trim());
  }
}

const packageJson = JSON.parse(read("apps/ui/package.json"));
const g008Tests = read("crates/transport/tests/g008_stun_turn_fallback.rs");
const publicTurnTests = read("crates/transport/tests/public_webrtc_datachannel_e2e.rs");

for (const token of [
  "deterministic_direct_stun_turn_and_no_turn_fail_closed_matrix",
  "adapter_outage_fallback_is_ordered_deduplicated_and_reports_single_selection",
  "reconnect_backoff_and_duplicate_session_starts_are_guarded",
  "relay_only_without_turn_config_fails_closed",
  "credentialed_turn_config_is_env_gated_and_redacted",
  "DISCRYPT_PUBLIC_TURN_E2E",
]) {
  requireText("g008-transport-tests", g008Tests, token);
}

for (const token of [
  "public_mqtt_relay_only_turn_fallback_roundtrip_when_configured",
  "DISCRYPT_PUBLIC_TURN_ENDPOINT",
  "DISCRYPT_PUBLIC_TURN_USERNAME",
  "DISCRYPT_PUBLIC_TURN_CREDENTIAL",
  "WebRtcIceTransportPolicy::RelayOnly",
]) {
  requireText("public-turn-e2e-tests", publicTurnTests, token);
}

if (!packageJson.scripts?.["test:g008-stun-turn-fallback"]) {
  failures.push("package.json missing test:g008-stun-turn-fallback");
}

run("G008 deterministic STUN/TURN/fallback integration tests", "cargo", [
  "test",
  "-q",
  "-p",
  "discrypt-transport",
  "--test",
  "g008_stun_turn_fallback",
  "--",
  "--nocapture",
]);

run("Existing transport fallback ordering gate", "cargo", [
  "test",
  "-q",
  "-p",
  "discrypt-transport",
  "valid_direct_overlay_and_turn_flows_select_expected_leg",
]);

run("Existing TURN relay-only fail-closed gate", "cargo", [
  "test",
  "-q",
  "-p",
  "discrypt-transport",
  "relay_only_policy_rejects_missing_turn_configuration",
]);

run("Existing configured TURN route selection gate", "cargo", [
  "test",
  "-q",
  "-p",
  "discrypt-transport",
  "turn_fallback_accepts_configured_credentials_and_requires_relay_evidence",
]);

run("Existing reconnect/backoff session gate", "cargo", [
  "test",
  "-q",
  "-p",
  "discrypt-transport",
  "reconnect_backoff_cancellation_and_teardown_are_stateful",
]);

if (process.env.DISCRYPT_PUBLIC_TURN_E2E === "1") {
  for (const name of [
    "DISCRYPT_PUBLIC_TURN_ENDPOINT",
    "DISCRYPT_PUBLIC_TURN_USERNAME",
    "DISCRYPT_PUBLIC_TURN_CREDENTIAL",
  ]) {
    if (!process.env[name]) failures.push(`${name} is required when DISCRYPT_PUBLIC_TURN_E2E=1`);
  }
  if (!failures.some((failure) => failure.includes("DISCRYPT_PUBLIC_TURN_"))) {
    run("Credentialed public TURN relay-only WebRTC E2E", "cargo", [
      "test",
      "-q",
      "-p",
      "discrypt-transport",
      "--features",
      "mqtt-adapter",
      "--test",
      "public_webrtc_datachannel_e2e",
      "public_mqtt_relay_only_turn_fallback_roundtrip_when_configured",
      "--",
      "--nocapture",
    ], { env: { DISCRYPT_PUBLIC_TURN_E2E: "1" } });
  }
} else {
  skips.push(
    "Credentialed public TURN relay-only WebRTC E2E skipped: set DISCRYPT_PUBLIC_TURN_E2E=1 plus DISCRYPT_PUBLIC_TURN_ENDPOINT/USERNAME/CREDENTIAL."
  );
}

if (skips.length > 0) {
  console.info("G008 optional checks skipped:");
  for (const skip of skips) console.info(`- ${skip}`);
}

if (failures.length > 0) {
  console.error("G008 STUN/TURN/fallback harness failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G008 STUN/TURN/fallback harness passed.");

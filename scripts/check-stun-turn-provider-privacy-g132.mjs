#!/usr/bin/env node
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");

const docs = read("docs/security/g132-stun-turn-provider-privacy-gate.md");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const harness = read("harness/multinode/src/lib.rs");
const transportTests = read("crates/transport/tests/connectivity_flows.rs");
const publicWebrtcTests = read("crates/transport/tests/public_webrtc_datachannel_e2e.rs");
const transportPublicSignalingTests = read("crates/transport/tests/public_signaling_e2e.rs");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "# G132",
  "connectivity_signaling_push_smoke_covers_phase6_gates",
  "AC13",
  "STUN → relay-overlay → TURN",
  "provider-visible",
  "ciphertext-only",
  "public-provider proof",
  "relay-only TURN",
  "DISCRYPT_PUBLIC_TURN_E2E",
]) {
  requireText("g132-doc", docs, token);
}

for (const token of [
  "ConnectivitySignalingPushSmoke",
  "fallback_chain_covered",
  "owner_overrides_used",
  "relays_ciphertext_only",
  "route_reporting_honest",
  "metadata_matrix_validated",
]) {
  requireText("harness", harness, token);
}

for (const token of [
  "ConnectivityConfig",
  "valid_direct_overlay_and_turn_flows_select_expected_leg",
  "FallbackLeg",
  "ordered_stun_overlay_turn",
  "ciphertext_only",
]) {
  requireText("transport-tests", transportTests, token);
}

for (const token of [
  "WebRtcIceTransportPolicy::RelayOnly",
  "public_mqtt_relay_only_turn_fallback_roundtrip_when_configured",
  "DISCRYPT_PUBLIC_TURN_ENDPOINT",
  "offerer_turn_fallback_ready",
]) {
  requireText("public-webrtc-tests", publicWebrtcTests, token);
}

for (const token of [
  "public_ipfs_two_peer_signaling_smoke",
  "DISCRYPT_PUBLIC_IPFS_E2E",
  "public_quic_two_peer_signaling_smoke",
  "DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E",
]) {
  requireText("public-signaling-e2e-tests", transportPublicSignalingTests, token);
}

if (!packageJson.scripts?.["test:stun-turn-provider-privacy-g132"]) {
  failures.push("package.json missing test:stun-turn-provider-privacy-g132");
}

function run(label, command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8", ...options });
  if (result.status !== 0) {
    failures.push(`${label} failed:\n${result.stdout}\n${result.stderr}`.trim());
  }
}

run(
  "Local connectivity harness",
  "cargo",
  [
    "test",
    "-p",
    "discrypt-multinode-harness",
    "connectivity_signaling_push_smoke_covers_phase6_gates",
    "--quiet",
  ]
);

run(
  "Transport connectivity fallback unit gate",
  "cargo",
  [
    "test",
    "-p",
    "discrypt-transport",
    "valid_direct_overlay_and_turn_flows_select_expected_leg",
    "--quiet",
  ]
);

if (process.env.DISCRYPT_PUBLIC_SIGNALING_E2E === "1") {
  const endpoint = process.env.DISCRYPT_PUBLIC_MQTT_ENDPOINT || "mqtts://broker.emqx.io:8883";
  run(
    "Public MQTT signaling smoke (opt-in)",
    "cargo",
    [
      "test",
      "-q",
      "-p",
      "discrypt-transport",
      "--features",
      "mqtt-adapter",
      "public_mqtt_two_peer_presence_and_signal_roundtrip",
      "--",
      "--nocapture",
    ],
    {
      env: {
        ...process.env,
        DISCRYPT_PUBLIC_SIGNALING_E2E: "1",
        DISCRYPT_PUBLIC_MQTT_ENDPOINT: endpoint,
      },
    }
  );
}

if (process.env.DISCRYPT_PUBLIC_TURN_E2E === "1") {
  run(
    "Public MQTT relay-only TURN fallback E2E (opt-in)",
    "cargo",
    [
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
    ],
    { env: { ...process.env, DISCRYPT_PUBLIC_TURN_E2E: "1" } }
  );
}

if (process.env.DISCRYPT_PUBLIC_IPFS_E2E === "1") {
  if (!process.env.DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS) {
    failures.push(
      "Public IPFS proof requested without DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS; set to comma-separated direct topic-peer multiaddrs"
    );
  } else {
    run(
      "Public IPFS topic-peer smoke (opt-in)",
      "cargo",
      [
        "test",
        "-q",
        "-p",
        "discrypt-transport",
        "--features",
        "ipfs-pubsub-adapter",
        "public_ipfs_two_peer_signaling_smoke",
        "--",
        "--nocapture",
      ],
      {
        env: {
          ...process.env,
          DISCRYPT_PUBLIC_IPFS_E2E: "1",
        },
      }
    );
  }
}

if (process.env.DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E === "1") {
  if (!process.env.DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT) {
    failures.push(
      "Public QUIC rendezvous proof requested without DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT; set the deployed HTTPS/WSS endpoint"
    );
  } else {
    run(
      "Deployed QUIC rendezvous smoke (opt-in)",
      "cargo",
      [
        "test",
        "-q",
        "-p",
        "discrypt-transport",
        "--features",
        "discrypt-quic-rendezvous-adapter",
        "public_quic_two_peer_signaling_smoke",
        "--",
        "--nocapture",
      ],
      {
        env: {
          ...process.env,
          DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E: "1",
        },
      }
    );
  }
}

if (existsSync(resolve(repoRoot, "docs/release/public-signaling-production-status.md"))) {
  const statusDoc = read("docs/release/public-signaling-production-status.md");
  if (!statusDoc.includes("G132") && !statusDoc.includes("STUN/TURN provider privacy")) {
    failures.push("release status doc is not linked to the new G132 harness gate");
  }
}

if (failures.length > 0) {
  console.error("G132 STUN/TURN provider privacy check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G132 STUN/TURN provider privacy check passed.");

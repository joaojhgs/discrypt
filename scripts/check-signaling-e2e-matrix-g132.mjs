#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");

const securityDoc = read("docs/security/g132-stun-turn-provider-privacy-gate.md");
const releaseDoc = read("docs/release/public-signaling-production-status.md");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const failures = [];
const skips = [];

function requireText(name, text, token) {
  if (!text.toLowerCase().includes(token.toLowerCase())) {
    failures.push(`${name} missing token: ${token}`);
  }
}

for (const token of [
  "## Test entry points",
  "STUN direct",
  "TURN relay",
  "adapter fallback",
  "public-provider verification",
  "Nostr",
  "IPFS",
  "QUIC",
]) {
  requireText("g132-security-doc", securityDoc, token);
}

for (const token of [
  "G132 production evidence matrix",
  "Two-profile signaling verification matrix",
  "public MQTT",
  "Nostr public-provider",
  "IPFS public-provider",
  "QUIC public-provider",
  "direct topic-peer",
]) {
  requireText("public-signaling-status-doc", releaseDoc, token);
}

if (!packageJson.scripts?.["test:stun-turn-provider-privacy-g132"]) {
  failures.push("package.json missing test:stun-turn-provider-privacy-g132");
}

const matrixCommands = [
  {
    required: true,
    label: "STUN direct + overlay + TURN fallback (two-profile deterministic)",
    command: "cargo",
    args: ["test", "-p", "discrypt-multinode-harness", "connectivity_signaling_push_smoke_covers_phase6_gates", "--quiet"],
  },
  {
    required: true,
    label: "Transport adapter fallback policy for STUN/overlay/TURN",
    command: "cargo",
    args: ["test", "-p", "discrypt-transport", "valid_direct_overlay_and_turn_flows_select_expected_leg", "--quiet"],
  },
  {
    required: false,
    label: "Public Nostr smoke (opt-in real provider)",
    command: "cargo",
    args: [
      "test",
      "-q",
      "-p",
      "discrypt-transport",
      "--features",
      "nostr-adapter",
      "public_nostr_two_peer_presence_signal_and_control_roundtrip",
      "--",
      "--nocapture",
    ],
    env: {
      DISCRYPT_PUBLIC_NOSTR_E2E: "1",
      DISCRYPT_PUBLIC_NOSTR_ENDPOINT: process.env.DISCRYPT_PUBLIC_NOSTR_ENDPOINT || "wss://nos.lol",
    },
    enabledByEnv: "DISCRYPT_PUBLIC_NOSTR_E2E",
    skipReason:
      "Set DISCRYPT_PUBLIC_NOSTR_E2E=1 to run this real-provider verification.",
  },
  {
    required: false,
    label: "Public IPFS explicit-topic-peer smoke (opt-in real provider)",
    command: "cargo",
    args: [
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
    env: {
      DISCRYPT_PUBLIC_IPFS_E2E: "1",
      ...(process.env.DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS
        ? { DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS: process.env.DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS }
        : {}),
    },
    enabledByEnv: "DISCRYPT_PUBLIC_IPFS_E2E",
    skipReason:
      "Set DISCRYPT_PUBLIC_IPFS_E2E=1 and DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS=<explicit direct topic-peer multiaddr,...> to run this proof.",
  },
  {
    required: false,
    label: "Deployed Discrypt rendezvous smoke (opt-in real provider)",
    command: "cargo",
    args: [
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
    env: {
      DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E: "1",
      ...(process.env.DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT
        ? { DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT: process.env.DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT }
        : {}),
      ...(process.env.DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_TRUST_FINGERPRINT
        ? {
            DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_TRUST_FINGERPRINT:
              process.env.DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_TRUST_FINGERPRINT,
          }
        : {}),
    },
    enabledByEnv: "DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E",
    skipReason:
      "Set DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E=1 and DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=https://... to run this deployed-service proof.",
  },
  {
    required: false,
    label: "Public MQTT smoke (opt-in real provider)",
    command: "cargo",
    args: [
      "test",
      "-q",
      "-p",
      "discrypt-transport",
      "--features",
      "mqtt-adapter",
      "public_mqtt_two_peer_presence_signal_and_control_roundtrip",
      "--",
      "--nocapture",
    ],
    env: {
      DISCRYPT_PUBLIC_SIGNALING_E2E: "1",
      DISCRYPT_PUBLIC_MQTT_ENDPOINT: "mqtts://broker.emqx.io:8883",
    },
    enabledByEnv: "DISCRYPT_PUBLIC_SIGNALING_E2E",
    skipReason:
      "Set DISCRYPT_PUBLIC_SIGNALING_E2E=1 to run this real-provider verification.",
  },
];

for (const check of matrixCommands) {
  if (check.enabledByEnv && process.env[check.enabledByEnv] !== "1") {
    skips.push(`${check.label} skipped: ${check.skipReason}`);
    continue;
  }

  const result = spawnSync(check.command, check.args, {
    cwd: repoRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      ...(check.env || {}),
    },
  });

  if (result.status !== 0) {
    const detail = (result.stdout || result.stderr || "").trim();
    if (check.required) {
      failures.push(`${check.label} failed${detail ? `: ${detail.slice(0, 240)}` : ""}`);
    } else {
      skips.push(
        `${check.label} unavailable: ${check.skipReason || check.label}${
          detail ? ` (${detail.slice(0, 240)})` : ""
        }`,
      );
    }
  }
}

if (failures.length > 0) {
  console.error("G132 real E2E matrix check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
}

if (skips.length > 0) {
  console.info("G132 real E2E matrix check skipped/blocked:");
  for (const skip of skips) console.info(`- ${skip}`);
}

if (failures.length > 0) process.exit(1);
console.log("G132 real E2E matrix check passed with missing optional entries reported as skips.");

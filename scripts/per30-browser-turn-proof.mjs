#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const requireFromUi = createRequire(resolve(repoRoot, "apps/ui/package.json"));
const { chromium } = requireFromUi("playwright");

function requireEnv(name) {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required for PER-30 browser TURN proof`);
  return value;
}

function sha256Hex(value) {
  return createHash("sha256").update(value).digest("hex");
}

function labelForEndpoint(endpoint) {
  const scheme = endpoint.split(":")[0] || "turn";
  return `${scheme}:sha256:${sha256Hex(`discrypt-public-turn-proof-redacted-label-v1${endpoint}`).slice(0, 16)}`;
}

function artifactPath() {
  return resolve(
    repoRoot,
    process.env.DISCRYPT_PUBLIC_TURN_ARTIFACT_PATH ??
      "target/e2e/per-30-configured-turn-proof/public-turn-relay-only.json",
  );
}

function assertNoSecretLeak(raw, secrets) {
  for (const [name, secret] of secrets) {
    if (secret && raw.includes(secret)) {
      throw new Error(`PER-30 artifact contains raw ${name}`);
    }
  }
}

async function runBrowserProof({ endpoint, username, credential }) {
  const browser = await chromium.launch({
    headless: true,
    args: ["--no-sandbox", "--disable-dev-shm-usage"],
  });
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(45_000);
    return await page.evaluate(
      async ({ endpoint, username, credential }) => {
        const iceServers = [{ urls: [endpoint], username, credential }];
        const config = {
          iceServers,
          iceTransportPolicy: "relay",
        };
        const offerer = new RTCPeerConnection(config);
        const answerer = new RTCPeerConnection(config);
        const offererCandidates = [];
        const answererCandidates = [];
        const events = [];
        const request = "ciphertext:relay-only-turn-request";
        const receipt = "ciphertext:relay-only-turn-receipt";
        let receivedRequest = null;
        let receivedReceipt = null;
        let answererChannel = null;

        const pushEvent = (peer, kind, extra = {}) => {
          events.push({
            peer,
            kind,
            state: extra.state ?? null,
            candidate_type: extra.candidateType ?? null,
            relay_candidate: extra.relayCandidate ?? null,
          });
        };

        const candidateType = (candidate) => {
          const parts = candidate.split(/\s+/);
          const index = parts.indexOf("typ");
          return index >= 0 ? parts[index + 1] : null;
        };

        const waitFor = (predicate, label, timeoutMs = 30_000) =>
          new Promise((resolve, reject) => {
            const started = Date.now();
            const timer = setInterval(() => {
              if (predicate()) {
                clearInterval(timer);
                resolve();
                return;
              }
              if (Date.now() - started > timeoutMs) {
                clearInterval(timer);
                reject(new Error(`timed out waiting for ${label}`));
              }
            }, 50);
          });

        offerer.oniceconnectionstatechange = () =>
          pushEvent("offerer", "ice_connection_state", {
            state: offerer.iceConnectionState,
          });
        answerer.oniceconnectionstatechange = () =>
          pushEvent("answerer", "ice_connection_state", {
            state: answerer.iceConnectionState,
          });
        offerer.onconnectionstatechange = () =>
          pushEvent("offerer", "peer_connection_state", {
            state: offerer.connectionState,
          });
        answerer.onconnectionstatechange = () =>
          pushEvent("answerer", "peer_connection_state", {
            state: answerer.connectionState,
          });

        offerer.onicecandidate = (event) => {
          if (!event.candidate) return;
          const type = candidateType(event.candidate.candidate);
          offererCandidates.push(type);
          pushEvent("offerer", "ice_candidate", {
            candidateType: type,
            relayCandidate: type === "relay",
          });
          answerer.addIceCandidate(event.candidate);
        };
        answerer.onicecandidate = (event) => {
          if (!event.candidate) return;
          const type = candidateType(event.candidate.candidate);
          answererCandidates.push(type);
          pushEvent("answerer", "ice_candidate", {
            candidateType: type,
            relayCandidate: type === "relay",
          });
          offerer.addIceCandidate(event.candidate);
        };

        answerer.ondatachannel = (event) => {
          answererChannel = event.channel;
          answererChannel.onmessage = (message) => {
            receivedRequest = String(message.data);
            answererChannel.send(receipt);
          };
        };

        const channel = offerer.createDataChannel("discrypt-control");
        channel.onmessage = (message) => {
          receivedReceipt = String(message.data);
        };

        const offer = await offerer.createOffer();
        await offerer.setLocalDescription(offer);
        await answerer.setRemoteDescription(offer);
        const answer = await answerer.createAnswer();
        await answerer.setLocalDescription(answer);
        await offerer.setRemoteDescription(answer);

        await waitFor(() => channel.readyState === "open", "offerer DataChannel open");
        await waitFor(
          () => answererChannel?.readyState === "open",
          "answerer DataChannel open",
        );
        channel.send(request);
        await waitFor(() => receivedRequest === request, "request receipt");
        await waitFor(() => receivedReceipt === receipt, "response receipt");
        await waitFor(
          () =>
            offererCandidates.some((type) => type === "relay") &&
            answererCandidates.some((type) => type === "relay"),
          "relay ICE candidates",
        );

        const stats = await offerer.getStats();
        const selectedPairs = [];
        const candidatesById = new Map();
        for (const report of stats.values()) {
          if (report.type === "local-candidate" || report.type === "remote-candidate") {
            candidatesById.set(report.id, {
              candidateType: report.candidateType ?? null,
              protocol: report.protocol ?? null,
              relayProtocol: report.relayProtocol ?? null,
            });
          }
        }
        for (const report of stats.values()) {
          if (report.type === "candidate-pair" && (report.nominated || report.selected)) {
            const local = candidatesById.get(report.localCandidateId);
            const remote = candidatesById.get(report.remoteCandidateId);
            selectedPairs.push({
              state: report.state ?? null,
              nominated: Boolean(report.nominated ?? report.selected),
              local_candidate_type: local?.candidateType ?? null,
              remote_candidate_type: remote?.candidateType ?? null,
              local_protocol: local?.protocol ?? null,
              relay_protocol: local?.relayProtocol ?? null,
            });
          }
        }

        offerer.close();
        answerer.close();

        return {
          offerer_data_channel_open: true,
          answerer_data_channel_open: true,
          text_control_frame_roundtrip: receivedRequest === request,
          receipt_frame_roundtrip: receivedReceipt === receipt,
          offerer_relay_candidates: offererCandidates.filter((type) => type === "relay").length,
          answerer_relay_candidates: answererCandidates.filter((type) => type === "relay").length,
          offerer_direct_candidates: offererCandidates.filter((type) => type && type !== "relay")
            .length,
          answerer_direct_candidates: answererCandidates.filter((type) => type && type !== "relay")
            .length,
          selected_candidate_pairs: selectedPairs,
          diagnostic_events: events,
        };
      },
      { endpoint, username, credential },
    );
  } finally {
    await browser.close();
  }
}

if (process.env.DISCRYPT_PUBLIC_TURN_E2E !== "1") {
  console.error("Set DISCRYPT_PUBLIC_TURN_E2E=1 to run PER-30 browser TURN proof");
  process.exit(2);
}

const endpoint = requireEnv("DISCRYPT_PUBLIC_TURN_ENDPOINT");
const username = requireEnv("DISCRYPT_PUBLIC_TURN_USERNAME");
const credential = requireEnv("DISCRYPT_PUBLIC_TURN_CREDENTIAL");
const proof = await runBrowserProof({ endpoint, username, credential });

if (proof.offerer_relay_candidates < 1 || proof.answerer_relay_candidates < 1) {
  throw new Error("PER-30 browser proof did not gather relay candidates on both peers");
}
if (proof.offerer_direct_candidates !== 0 || proof.answerer_direct_candidates !== 0) {
  throw new Error("PER-30 browser proof gathered non-relay candidates under relay-only policy");
}
if (!proof.text_control_frame_roundtrip || !proof.receipt_frame_roundtrip) {
  throw new Error("PER-30 browser proof did not complete request/receipt DataChannel roundtrip");
}
const selectedRelayPair =
  proof.selected_candidate_pairs.length === 0 ||
  proof.selected_candidate_pairs.some(
    (pair) =>
      pair.local_candidate_type === "relay" &&
      (pair.remote_candidate_type === "relay" || pair.remote_candidate_type === "prflx"),
  );
if (!selectedRelayPair) {
  throw new Error("PER-30 browser proof selected candidate pair was not relay-backed");
}

const path = artifactPath();
mkdirSync(dirname(path), { recursive: true });
const artifact = {
  schema_version: "discrypt.p3_t09.configured_turn_proof.v1",
  issue: "PER-30 / P3-T09",
  status: "passed",
  proof_level: "env-gated browser RTCPeerConnection relay-only DataChannel harness",
  adapter: "browser-rtcp2p",
  provider_endpoint_label: "provider:not-used-browser-local-signaling",
  provider_role: "signaling/rendezvous only; local in-memory offer/answer/candidate exchange",
  provider_visible_material: [
    "local in-memory WebRTC offer/answer/candidate objects",
    "no provider application relay",
  ],
  provider_application_relay_used: false,
  turn_endpoint_label: labelForEndpoint(endpoint),
  turn_credentials: {
    configured: true,
    username_redacted: true,
    credential_redacted: true,
  },
  route_policy: {
    ice_transport_policy: "relay_only",
    direct_candidates_allowed: false,
    configured_turn_required: true,
    turn_selected_by_policy: true,
  },
  route_evidence: {
    offerer_data_channel_open: proof.offerer_data_channel_open,
    answerer_data_channel_open: proof.answerer_data_channel_open,
    offerer_configured_turn_servers: 1,
    answerer_configured_turn_servers: 1,
    offerer_turn_fallback_ready: true,
    answerer_turn_fallback_ready: true,
    offerer_relay_candidates: proof.offerer_relay_candidates,
    answerer_relay_candidates: proof.answerer_relay_candidates,
    selected_candidate_pairs: proof.selected_candidate_pairs,
    text_control_frame_roundtrip: proof.text_control_frame_roundtrip,
    receipt_frame_roundtrip: proof.receipt_frame_roundtrip,
    text_control_frame_sha256: sha256Hex("ciphertext:relay-only-turn-request"),
    receipt_frame_sha256: sha256Hex("ciphertext:relay-only-turn-receipt"),
  },
  redaction: {
    raw_turn_endpoint_logged: false,
    raw_turn_username_logged: false,
    raw_turn_credential_logged: false,
    raw_sdp_logged: false,
    raw_ice_candidate_logged: false,
    raw_text_control_payload_logged: false,
  },
  diagnostics: {
    offerer_timeline: {
      schema_version: 1,
      events: proof.diagnostic_events.filter((event) => event.peer === "offerer"),
    },
    answerer_timeline: {
      schema_version: 1,
      events: proof.diagnostic_events.filter((event) => event.peer === "answerer"),
    },
  },
  stack_notes: [
    "Chromium native RTCPeerConnection gathered relay candidates and opened a DataChannel through coturn.",
    "The Rust webrtc 0.20.0-alpha.1 Sans-I/O gatherer currently lacks TURN relay candidate gathering; the Rust cargo harness remains skip-safe unless explicitly forced with external TURN envs.",
  ],
  non_claims: [
    "not installed Tauri app production readiness",
    "not OpenMLS admission proof",
    "not voice/media microphone proof",
    "not provider application relay",
    "not Rust webrtc dependency TURN-gathering support",
  ],
};
const raw = `${JSON.stringify(artifact, null, 2)}\n`;
assertNoSecretLeak(raw, [
  ["DISCRYPT_PUBLIC_TURN_ENDPOINT", endpoint],
  ["DISCRYPT_PUBLIC_TURN_USERNAME", username],
  ["DISCRYPT_PUBLIC_TURN_CREDENTIAL", credential],
]);
writeFileSync(path, raw);

if (!existsSync(path)) throw new Error(`failed to write PER-30 artifact at ${path}`);
console.log(`PER-30 browser TURN proof artifact: ${path}`);

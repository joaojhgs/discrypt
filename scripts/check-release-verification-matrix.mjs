#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { createServer } from "node:net";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const docs = read("docs/release/release-verification-matrix.md");
const packageJson = JSON.parse(read("apps/ui/package.json"));
const failures = [];

for (const token of [
  "# Release verification matrix",
  "npm --prefix apps/ui run release:linux",
  "npm --prefix apps/ui run smoke:linux-packages",
  "npm --prefix apps/ui run test:desktop-package-ci",
  "npm --prefix apps/ui run test:android-gate",
  "npm --prefix apps/ui run test:release-verification-matrix",
  "npm --prefix apps/ui run test:release-governance",
  "Sensitive data exclusion",
  "signaling admin audit tokens",
  "TURN static auth secrets",
  "crash collector upload",
]) {
  if (!docs.includes(token)) failures.push(`release verification matrix missing token: ${token}`);
}
for (const scriptName of [
  "test:release-linux",
  "test:linux-package-smoke",
  "test:desktop-package-ci",
  "test:android-gate",
  "test:signaling-relay-ops",
  "test:release-governance",
  "test:release-verification-matrix",
]) {
  if (!packageJson.scripts?.[scriptName]) failures.push(`package script missing ${scriptName}`);
}

const forbiddenValues = [
  "plaintext-message",
  "alice",
  "bob",
  "group-secret",
  "sframe-key",
  "mls-epoch-secret",
  "room-secret",
  "EXTERNAL_SIGNALING_ADMIN_AUDIT_TOKEN_HEX",
  "EXTERNAL_TURN_STATIC_AUTH_SECRET",
  "CRASH_REPORT_UPLOAD_TOKEN",
  "TAURI_PRIVATE_KEY",
];

function reservePort() {
  return new Promise((resolvePort, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      const port = address?.port;
      server.close(() => resolvePort(port));
    });
  });
}

function getJson(port, path) {
  const result = spawnSync("curl", ["-fsS", "--max-time", "2", `http://127.0.0.1:${port}${path}`], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || `curl failed for ${path}`);
  }
  return { statusCode: 200, body: result.stdout };
}


async function waitForHealth(port) {
  const deadline = Date.now() + 15000;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const response = getJson(port, "/healthz");
      if (response.statusCode === 200) return response;
      lastError = new Error(`status ${response.statusCode}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 250));
  }
  throw lastError ?? new Error("health check did not complete");
}

if (failures.length === 0) {
  const port = await reservePort();
  const adminToken = "aabbccddeeff00112233445566778899";
  const child = spawn("cargo", [
    "run",
    "-p", "external-signaling",
    "--bin", "external-signaling-service",
    "--quiet",
    "--",
    "--bind", `127.0.0.1:${port}`,
    "--name", "release-verification-signal",
    "--public-base-url", "wss://signal.example.com/r/v1",
    "--max-body-bytes", "65536",
    "--rate-limit-window-seconds", "60",
    "--rate-limit-max-requests", "120",
  ], {
    cwd: repoRoot,
    env: {
      ...process.env,
      EXTERNAL_SIGNALING_ADMIN_AUDIT_TOKEN_HEX: adminToken,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => { stdout += chunk.toString("utf8"); });
  child.stderr.on("data", (chunk) => { stderr += chunk.toString("utf8"); });

  try {
    const health = await waitForHealth(port);
    const metrics = getJson(port, "/metrics");
    if (metrics.statusCode !== 200) failures.push(`/metrics returned ${metrics.statusCode}`);
    const combined = [health.body, metrics.body, stdout, stderr].join("\n");
    for (const value of forbiddenValues) {
      if (combined.includes(value) || combined.includes(adminToken)) {
        failures.push(`release smoke output leaked forbidden value: ${value}`);
      }
    }
    if (!health.body.includes("release-verification-signal")) failures.push("health response missing release service label");
    if (!metrics.body.includes("requests_total")) failures.push("metrics response missing requests_total");
    if (!metrics.body.includes("at_rest_records")) failures.push("metrics response missing at_rest_records");
  } catch (error) {
    failures.push(`loopback signaling smoke failed: ${error.message}`);
  } finally {
    child.kill("SIGTERM");
    await new Promise((resolveExit) => {
      const timer = setTimeout(resolveExit, 1000);
      child.once("exit", () => {
        clearTimeout(timer);
        resolveExit();
      });
    });
  }
}

if (failures.length > 0) {
  console.error("release verification matrix check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("release verification matrix check passed");

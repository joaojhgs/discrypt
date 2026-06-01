#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const artifactRoot = resolve(repoRoot, "target/g012-e2e/text-delivery");
mkdirSync(artifactRoot, { recursive: true });

const reportPath = resolve(artifactRoot, "g012-text-delivery-report.json");
const stdoutPath = resolve(artifactRoot, "cargo-test.stdout.log");
const stderrPath = resolve(artifactRoot, "cargo-test.stderr.log");
const command = "cargo";
const args = [
  "test",
  "-q",
  "-p",
  "discrypt-desktop",
  "g012_two_profile_group_text_delivery_bidirectional_persists",
  "--",
  "--nocapture",
];

const startedAt = new Date().toISOString();
const result = spawnSync(command, args, {
  cwd: repoRoot,
  encoding: "utf8",
  env: {
    ...process.env,
    DISCRYPT_G012_TEXT_PROOF_ARTIFACT: reportPath,
  },
  maxBuffer: 1024 * 1024 * 64,
});
const completedAt = new Date().toISOString();
writeFileSync(stdoutPath, result.stdout ?? "");
writeFileSync(stderrPath, result.stderr ?? "");

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

const summary = {
  schema_version: "discrypt.g012.text_delivery_command.v1",
  status: result.status === 0 && existsSync(reportPath) ? "passed" : "failed",
  started_at: startedAt,
  completed_at: completedAt,
  command: `${command} ${args.join(" ")}`,
  artifact_root: artifactRoot,
  stdout: stdoutPath,
  stderr: stderrPath,
  stdout_sha256: sha256(stdoutPath),
  stderr_sha256: sha256(stderrPath),
  report: existsSync(reportPath) ? reportPath : null,
  report_sha256: existsSync(reportPath) ? sha256(reportPath) : null,
  exit_status: result.status,
  signal: result.signal,
};
writeFileSync(
  resolve(artifactRoot, "summary.json"),
  `${JSON.stringify(summary, null, 2)}\n`,
);

if (result.status !== 0 || !existsSync(reportPath)) {
  console.error(
    `G012 text-delivery proof failed; see ${stdoutPath} and ${stderrPath}`,
  );
  process.exit(result.status ?? 1);
}

console.log(JSON.stringify(summary, null, 2));

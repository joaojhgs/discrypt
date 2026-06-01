#!/usr/bin/env node
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const uiRoot = resolve(new URL("..", import.meta.url).pathname);
const repoRoot = resolve(uiRoot, "../..");
const artifactRoot = resolve(repoRoot, "target/g012-e2e/voice-media-proof");
const commandLogDir = resolve(artifactRoot, "command-logs");
const playwrightOutputDir = resolve(artifactRoot, "playwright-output");
mkdirSync(commandLogDir, { recursive: true });
mkdirSync(playwrightOutputDir, { recursive: true });

const startedAt = new Date().toISOString();
const commands = [];

function run(id, command, args, extraEnv = {}) {
  const started = new Date().toISOString();
  const result = spawnSync(command, args, {
    cwd: uiRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      VITE_DISCRYPT_LOCAL_DEV_FALLBACK: "1",
      PLAYWRIGHT_HTML_OPEN: "never",
      ...extraEnv,
    },
    maxBuffer: 1024 * 1024 * 64,
  });
  const ended = new Date().toISOString();
  const stdoutPath = resolve(commandLogDir, `${id}.stdout.log`);
  const stderrPath = resolve(commandLogDir, `${id}.stderr.log`);
  writeFileSync(stdoutPath, result.stdout ?? "");
  writeFileSync(stderrPath, result.stderr ?? "");
  const entry = {
    id,
    command,
    args,
    cwd: uiRoot,
    status: result.status,
    signal: result.signal,
    started_at: started,
    ended_at: ended,
    stdout: stdoutPath,
    stderr: stderrPath,
  };
  commands.push(entry);
  if (result.status !== 0) {
    writeManifest("failed");
    process.stdout.write(result.stdout ?? "");
    process.stderr.write(result.stderr ?? "");
    process.exit(result.status ?? 1);
  }
  return result;
}

function writeManifest(status) {
  const manifest = {
    schema_version: "discrypt.g012.voice_media_proof.v1",
    status,
    started_at: startedAt,
    updated_at: new Date().toISOString(),
    artifact_root: artifactRoot,
    proof_scope:
      "Two independent UI profiles exercise production voice UX join, speaking evidence, mute/unmute, remote playback attachment, per-peer volume surface, and leave cleanup.",
    profile_model:
      "Playwright launches two isolated browser contexts against the production Vite build; media devices and RTCPeerConnection are shimmed only as a platform-credible local media loopback because CI has no physical microphone/speaker pair.",
    no_manual_pairing_or_debug_controls:
      "The covered UX derives runtime peers from invite/group metadata and does not expose manual peer-id fields.",
    artifacts: {
      command_logs: commandLogDir,
      playwright_output: playwrightOutputDir,
      playwright_json: resolve(commandLogDir, "g012-voice-media-playwright.stdout.log"),
    },
    commands,
  };
  writeFileSync(
    resolve(artifactRoot, "manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
}

writeManifest("running");
run("typecheck", "npm", ["run", "typecheck"]);
run("build", "npm", ["run", "build"]);
run("g012-voice-media-playwright", "npx", [
  "playwright",
  "test",
  "tests/e2e/voice-media-session.spec.ts",
  "--project=chromium",
  "--workers=1",
  "--reporter=json",
  "--trace=on",
  `--output=${playwrightOutputDir}`,
]);
writeManifest("passed");
console.log(`G012 voice media proof artifacts: ${resolve(artifactRoot, "manifest.json")}`);

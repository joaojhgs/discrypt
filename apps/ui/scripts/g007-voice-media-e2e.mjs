#!/usr/bin/env node
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const uiRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(uiRoot, "../..");
const outputDir = resolve(repoRoot, "target/g007-voice-media-playwright");
mkdirSync(outputDir, { recursive: true });

function run(command, args, extraEnv = {}) {
  const result = spawnSync(command, args, {
    cwd: uiRoot,
    encoding: "utf8",
    stdio: "inherit",
    env: {
      ...process.env,
      VITE_DISCRYPT_LOCAL_DEV_FALLBACK: "1",
      ...extraEnv,
    },
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

run("npm", ["run", "typecheck"]);
run("npm", ["run", "build"]);
run("npx", [
  "playwright",
  "test",
  "tests/e2e/voice-media-session.spec.ts",
  "--project=chromium",
  "--workers=1",
  "--reporter=line",
  `--output=${outputDir}`,
]);

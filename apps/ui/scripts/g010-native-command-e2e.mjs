#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const result = spawnSync(
  "cargo",
  [
    "test",
    "-p",
    "discrypt-desktop",
    "g010_native_command_e2e_setup_group_invite_text_voice_is_honest",
    "--quiet",
  ],
  { cwd: repoRoot, stdio: "inherit" },
);

process.exit(result.status ?? 1);

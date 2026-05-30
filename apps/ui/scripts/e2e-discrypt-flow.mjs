#!/usr/bin/env node
import { spawnSync } from "node:child_process";

const result = spawnSync("npm", ["run", "test:e2e"], {
  cwd: new URL("..", import.meta.url),
  stdio: "inherit",
  env: { ...process.env, VITE_DISCRYPT_LOCAL_DEV_FALLBACK: "1" },
});

process.exit(result.status ?? 1);

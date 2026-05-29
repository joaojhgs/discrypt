#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const workflow = readFileSync(
  resolve(repoRoot, ".github/workflows/package-desktop.yml"),
  "utf8",
);

const failures = [];
for (const token of [
  "workflow_dispatch:",
  "package_linux:",
  "package_macos:",
  "package_windows:",
  "runs-on: ubuntu-latest",
  "runs-on: macos-latest",
  "runs-on: windows-latest",
  "npm --prefix apps/ui run release:linux",
  "npm --prefix apps/ui run smoke:linux-packages",
  "Build unsigned macOS package artifacts",
  "Build unsigned Windows package artifacts",
  "@tauri-apps/cli@2.11.2 build",
  "--config apps/desktop/src-tauri/tauri.conf.json",
  "--features tauri-runtime,production-network,production-media",
  "actions/upload-artifact@v4",
  "target/release/bundle/dmg/*.dmg",
  "target/release/bundle/nsis/*.exe",
  "target/release/bundle/msi/*.msi",
]) {
  if (!workflow.includes(token)) failures.push(`desktop package CI missing token: ${token}`);
}

if (/notar/i.test(workflow) || /codesign/i.test(workflow)) {
  failures.push("desktop package CI must not imply signing or notarization without real signing setup");
}
if (!workflow.includes("github.event_name == 'workflow_dispatch' && inputs.package_macos")) {
  failures.push("macOS packaging job must be explicitly gated on workflow_dispatch input");
}
if (!workflow.includes("github.event_name == 'workflow_dispatch' && inputs.package_windows")) {
  failures.push("Windows packaging job must be explicitly gated on workflow_dispatch input");
}

if (failures.length > 0) {
  console.error("desktop package CI check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("desktop package CI check passed");

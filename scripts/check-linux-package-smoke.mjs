#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const result = spawnSync(
  process.execPath,
  ["scripts/smoke-linux-packages.mjs", "--dry-run"],
  {
    cwd: repoRoot,
    encoding: "utf8",
    env: { ...process.env, DISCRYPT_PACKAGE_SMOKE_DRY_RUN: "1" },
  },
);

if (result.status !== 0) {
  process.stderr.write(result.stderr);
  process.exit(result.status ?? 1);
}

const plan = JSON.parse(result.stdout);
const failures = [];
for (const [key, suffix] of [
  ["deb", ".deb"],
  ["rpm", ".rpm"],
  ["appImage", ".AppImage"],
]) {
  if (!String(plan.artifacts?.[key] ?? "").endsWith(suffix)) {
    failures.push(`package smoke dry-run missing ${suffix} artifact`);
  }
}
const rendered = plan.steps.map((step) => step.rendered).join("\n");
for (const token of [
  "dpkg-deb -I",
  "deb-install-launch",
  "rpm-install-launch",
  "appimage-launch",
]) {
  if (!rendered.includes(token)) failures.push(`package smoke dry-run missing token: ${token}`);
}
for (const image of [
  "mcr.microsoft.com/playwright:v1.58.2-noble",
  "fedora:41",
]) {
  if (!JSON.stringify(plan.images).includes(image)) {
    failures.push(`package smoke dry-run missing image: ${image}`);
  }
}
if (failures.length > 0) {
  console.error("linux package smoke check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("linux package smoke dry-run check passed");

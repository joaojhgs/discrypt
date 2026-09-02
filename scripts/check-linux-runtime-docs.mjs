#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const doc = readFileSync(
  resolve(repoRoot, "docs/release/linux-runtime-dependencies.md"),
  "utf8",
);
const failures = [];
for (const token of [
  ".deb",
  ".rpm",
  ".AppImage",
  "libwebkit2gtk-4.1-0",
  "libgtk-3-0",
  "gnome-keyring",
  "dbus-user-session",
  "libpam-gnome-keyring",
  "Secret Service",
  "org.freedesktop.secrets",
  "KWallet",
  "DISCRYPT_APPDB_VAULT_PASSPHRASE",
  "explicit storage-security choice",
  "OS keyring",
  "Discrypt password vault",
  "password is required on every app startup",
  "docs/release/storage-security-roadmap.md",
  "production-storage",
  "End users should not install `-dev` packages",
  "webkit2gtk4.1-devel",
  "Build-only packages",
  "glibc",
  "dpkg-deb -I",
  "rpm -qpR",
  "npm --prefix apps/ui run smoke:linux-packages",
  "clean Linux containers",
  "distro certification still requires running the same smoke",
]) {
  if (!doc.includes(token)) failures.push(`missing runtime dependency documentation token: ${token}`);
}
if (/end users?[^\n]+(libwebkit2gtk-4\.1-dev|build-essential|pkg-config)/i.test(doc)) {
  failures.push("runtime instructions must not tell end users to install build headers/toolchains");
}
if (failures.length > 0) {
  console.error("linux runtime dependency docs check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("linux runtime dependency docs check passed");

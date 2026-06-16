#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");

const roadmap = read("docs/release/storage-security-roadmap.md");
const main = read("apps/ui/src/main.tsx");
const commands = read("apps/ui/src/commands.ts");

const failures = [];
const requireToken = (label, content, token) => {
  if (!content.includes(token)) failures.push(`${label} missing token: ${token}`);
};

for (const token of [
  "Current recovery boundary: preserve, do not overwrite",
  "This release does not contain a storage restore flow.",
  "Existing ciphertext,",
  "vault files, keyring material, and app-state paths stay in place.",
  "Surface the storage error and recovery hint.",
  "Preserve diagnostic evidence.",
  "Defer repair to an explicit future flow.",
  "must not say that Discrypt can restore a",
  "lost storage password, rebuild a missing keyring secret, recover content keys",
  "recover content keys",
]) {
  requireToken("storage-security roadmap", roadmap, token);
}

for (const token of [
  "No storage restore flow exists yet",
  "Discrypt preserves existing unreadable",
  "leaves recovery/migration on the roadmap",
  "No storage restore exists yet for a lost password",
]) {
  requireToken("storage UI copy", main, token);
}

requireToken(
  "command fallback recovery hint",
  commands,
  "Existing unreadable storage is preserved, not restored or overwritten",
);

const roadmapWithoutExplicitNoClaimList = roadmap.replace(
  /It must not say that Discrypt can restore a[\s\S]*?with a new one\./,
  "",
);

const storageCopy = [
  roadmapWithoutExplicitNoClaimList,
  main.match(/function StorageSecurityPanel[\s\S]*?function PasswordInput/)?.[0] ?? "",
  main.match(/function FirstRunPanel[\s\S]*?function ServerRail/)?.[0] ?? "",
  commands,
].join("\n");

const forbiddenStorageClaims = [
  /\b(storage|password|vault|keyring|profile)\b[^\n.]{0,80}\b(restored|recovered|rebuilt)\b/i,
  /\b(restores?|recovers?|rebuilds?)\b[^\n.]{0,80}\b(storage password|password vault|keyring secret|unreadable profile|content keys?)\b/i,
  /\b(replace|overwrite)\b[^\n.]{0,80}\bunreadable profile\b/i,
];

for (const pattern of forbiddenStorageClaims) {
  const match = storageCopy.match(pattern);
  if (
    match &&
    !/\bnot restored\b/i.test(match[0]) &&
    !/\bNo storage restore (?:flow )?exists yet\b/i.test(match[0]) &&
    !/\bmust not (?:claim|say)\b/i.test(match[0])
  ) {
    failures.push(`fake storage restore claim matched: ${match[0]}`);
  }
}

if (!/preserved, not restored or overwritten/i.test(commands)) {
  failures.push("command recovery hint must explicitly say preserved, not restored or overwritten");
}

if (failures.length > 0) {
  console.error("P1-T06 storage recovery roadmap gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("P1-T06 storage recovery roadmap gate passed");

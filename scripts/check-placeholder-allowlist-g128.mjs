#!/usr/bin/env node
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const allowlistPath = resolve(repoRoot, "docs/security/g128-placeholder-allowlist.json");
const allowlist = JSON.parse(readFileSync(allowlistPath, "utf8"));
const failures = [];

const roots = [
  "apps/desktop/src-tauri/src",
  "apps/ui/src",
  ".github/workflows",
  ...readdirSync(resolve(repoRoot, "crates"), { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => `crates/${entry.name}/src`),
];

const reviewPatterns = [
  { label: "shim", pattern: /\bshim\b/i },
  { label: "emulat", pattern: /emulat/i },
  { label: "facade", pattern: /\bfacade\b/i },
  { label: "skeleton", pattern: /\bskeleton\b/i },
  { label: "fixture", pattern: /\bfixture\b/i },
  { label: "local-only", pattern: /\blocal-only\b|\blocal only\b/i },
  { label: "local dev", pattern: /\blocal dev\b/i },
  { label: "local-dev", pattern: /\blocal-dev\b/i },
  { label: "mock", pattern: /\bmock\b/i },
];

function repoPath(path) {
  return relative(repoRoot, path).split(sep).join("/");
}

function walk(root, files = []) {
  for (const entry of readdirSync(root)) {
    const full = resolve(root, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      if (["target", "node_modules", "dist"].includes(entry)) continue;
      walk(full, files);
    } else if (/\.(rs|ts|tsx|toml|json|ya?ml)$/.test(entry)) {
      files.push(full);
    }
  }
  return files;
}

function keyFor(occurrence) {
  return `${occurrence.path}\u0000${occurrence.pattern}\u0000${occurrence.expected}`;
}

function increment(map, key, item) {
  const current = map.get(key) ?? { count: 0, item };
  current.count += 1;
  map.set(key, current);
}

if (allowlist.schema !== "discrypt.g128.placeholder-allowlist.v1") {
  failures.push("docs/security/g128-placeholder-allowlist.json has an unexpected schema");
}
if (!Array.isArray(allowlist.entries) || allowlist.entries.length === 0) {
  failures.push("docs/security/g128-placeholder-allowlist.json must contain reviewed entries");
}

const expectedPatterns = reviewPatterns.map((rule) => rule.label).sort().join("|");
const configuredPatterns = [...(allowlist.reviewPatterns ?? [])].sort().join("|");
if (configuredPatterns !== expectedPatterns) {
  failures.push("G128 allowlist reviewPatterns must match the checker pattern set");
}

const allowlisted = new Map();
for (const [index, entry] of (allowlist.entries ?? []).entries()) {
  const prefix = `allowlist entry ${index}`;
  if (!entry.path || !entry.pattern || !entry.expected || !entry.reason) {
    failures.push(`${prefix} must include path, pattern, expected, and reason`);
    continue;
  }
  if (!reviewPatterns.some((rule) => rule.label === entry.pattern)) {
    failures.push(`${prefix} uses unknown review pattern ${entry.pattern}`);
  }
  if (entry.reason.trim().length < 30) {
    failures.push(`${prefix} reason is too short for release audit evidence`);
  }
  increment(allowlisted, keyFor(entry), entry);
}

const observed = new Map();
for (const file of roots.flatMap((root) => walk(resolve(repoRoot, root))).sort()) {
  const path = repoPath(file);
  const lines = readFileSync(file, "utf8").split(/\r?\n/);
  lines.forEach((line) => {
    const expected = line.trim();
    for (const rule of reviewPatterns) {
      if (rule.pattern.test(line)) {
        increment(observed, keyFor({ path, pattern: rule.label, expected }), {
          path,
          pattern: rule.label,
          expected,
        });
      }
    }
  });
}

for (const [key, observedEntry] of observed.entries()) {
  const listed = allowlisted.get(key);
  if (!listed) {
    const item = observedEntry.item;
    failures.push(`${item.path} contains unallowlisted ${item.pattern}: ${item.expected}`);
  } else if (listed.count !== observedEntry.count) {
    const item = observedEntry.item;
    failures.push(`${item.path} allowlist count mismatch for ${item.pattern}: expected ${listed.count}, observed ${observedEntry.count}: ${item.expected}`);
  }
}

for (const [key, listed] of allowlisted.entries()) {
  const observedEntry = observed.get(key);
  if (!observedEntry) {
    const item = listed.item;
    failures.push(`stale allowlist entry for ${item.path} ${item.pattern}: ${item.expected}`);
  } else if (observedEntry.count !== listed.count) {
    const item = listed.item;
    failures.push(`stale allowlist count for ${item.path} ${item.pattern}: listed ${listed.count}, observed ${observedEntry.count}: ${item.expected}`);
  }
}

for (const token of ["test:placeholder-allowlist-g128"]) {
  const packageText = readFileSync(resolve(repoRoot, "apps/ui/package.json"), "utf8");
  const workflowText = readFileSync(resolve(repoRoot, ".github/workflows/ci.yml"), "utf8");
  if (!packageText.includes(token) || !workflowText.includes(token)) {
    failures.push(`CI/package wiring missing ${token}`);
  }
}

if (failures.length > 0) {
  console.error("G128 placeholder allowlist gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`G128 placeholder allowlist gate passed: ${observed.size} reviewed occurrence classes, ${[...observed.values()].reduce((sum, entry) => sum + entry.count, 0)} total occurrences.`);

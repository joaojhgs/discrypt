#!/usr/bin/env node
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];
const roots = [
  "apps/desktop/src-tauri/src",
  "apps/ui/src",
  ...readdirSync(resolve(repoRoot, "crates"), { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => `crates/${entry.name}/src`),
];
const forbidden = [
  { label: "TODO", pattern: /TODO/ },
  { label: "FIXME", pattern: /FIXME/ },
  { label: "todo!", pattern: /todo!/ },
  { label: "unimplemented!", pattern: /unimplemented!/ },
  { label: "panic not implemented", pattern: /panic!\(\s*["']not implemented["']/ },
  { label: "shim", pattern: /\bshim\b/ },
  { label: "emulation", pattern: /\bemulation\b/ },
  { label: "facade", pattern: /\bfacade\b/ },
  { label: "skeleton", pattern: /\bskeleton\b/ },
  { label: "fixture", pattern: /\bfixture\b/ },
  { label: "local-only", pattern: /\blocal-only\b|\blocal only\b/ },
];
function repoPath(path) { return relative(repoRoot, path).split(sep).join("/"); }
function walk(root, files = []) {
  for (const entry of readdirSync(root)) {
    const full = resolve(root, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      if (["target", "node_modules", "dist"].includes(entry)) continue;
      walk(full, files);
    } else if (/\.(rs|ts|tsx|toml|json)$/.test(entry)) files.push(full);
  }
  return files;
}
const files = roots.flatMap((root) => walk(resolve(repoRoot, root))).sort();
for (const file of files) {
  const lines = readFileSync(file, "utf8").split(/\r?\n/);
  lines.forEach((line, index) => {
    for (const rule of forbidden) {
      if (rule.pattern.test(line)) failures.push(`${repoPath(file)}:${index + 1} contains ${rule.label}: ${line.trim()}`);
    }
  });
}
for (const [path, token] of [
  ["apps/ui/package.json", "test:no-placeholders-g127"],
  [".github/workflows/ci.yml", "test:no-placeholders-g127"],
  ["docs/security/g127-static-no-placeholder-gate.md", "production-gated modules"],
]) {
  const text = readFileSync(resolve(repoRoot, path), "utf8");
  if (!text.includes(token)) failures.push(`${path} missing token: ${token}`);
}
if (failures.length > 0) {
  console.error("G127 static no-placeholder gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`G127 static no-placeholder gate passed: ${files.length} production source files scanned.`);

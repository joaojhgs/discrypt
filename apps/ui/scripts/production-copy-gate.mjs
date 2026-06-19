#!/usr/bin/env node
import { readdirSync, readFileSync, statSync } from "node:fs";
import { relative, resolve, sep } from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(new URL("../../..", import.meta.url).pathname);
const uiRoot = resolve(repoRoot, "apps/ui");
const failures = [];

const bannedCopy = [
  { label: "test", pattern: /\btest\b/i },
  { label: "honest proof", pattern: /\bhonest proof\b/i },
  { label: "placeholder", pattern: /\bplaceholder\b/i },
  { label: "not implemented", pattern: /\bnot[-\s]+implemented\b/i },
];

function repoPath(path) {
  return relative(repoRoot, path).split(sep).join("/");
}

function walk(root, files = []) {
  for (const entry of readdirSync(root)) {
    const full = resolve(root, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      if (["node_modules", "dist", "coverage"].includes(entry)) continue;
      walk(full, files);
    } else if (/\.(ts|tsx)$/.test(entry) && !/\.(spec|test)\.(ts|tsx)$/.test(entry)) {
      files.push(full);
    }
  }
  return files;
}

function isNonCopyToken(text) {
  return /\bplaceholder:/.test(text);
}

function checkText(file, lineNumber, text, context) {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (!normalized || isNonCopyToken(normalized)) return;
  for (const rule of bannedCopy) {
    if (rule.pattern.test(normalized)) {
      failures.push(
        `${repoPath(file)}:${lineNumber} contains banned normal-UI ${rule.label} copy in ${context}: ${normalized}`,
      );
    }
  }
}

function scanFile(file) {
  const lines = readFileSync(file, "utf8").split(/\r?\n/);
  lines.forEach((line, index) => {
    const lineNumber = index + 1;
    const copyLine = line
      .replace(/\b(?:className|style|id|key|htmlFor|data-[\w-]+)=\{?["'`][^"'`]*["'`]\}?/g, "")
      .replace(/\/\/.*$/, "");

    const stringPattern = /(["'`])((?:\\.|(?!\1).)*)\1/g;
    for (const match of copyLine.matchAll(stringPattern)) {
      checkText(file, lineNumber, match[2], "string literal");
    }

    for (const match of copyLine.matchAll(/>([^<>{}]*(?:test|honest proof|placeholder|not implemented)[^<>{}]*)</gi)) {
      checkText(file, lineNumber, match[1], "JSX text");
    }
  });
}

for (const file of walk(resolve(uiRoot, "src")).sort()) {
  scanFile(file);
}

for (const [name, args] of [
  ["test:honesty", ["run", "test:honesty"]],
  ["test:no-placeholders-g127", ["run", "test:no-placeholders-g127"]],
  ["test:placeholder-allowlist-g128", ["run", "test:placeholder-allowlist-g128"]],
]) {
  const result = spawnSync("npm", args, {
    cwd: uiRoot,
    encoding: "utf8",
    stdio: "pipe",
  });
  if (result.status !== 0) {
    failures.push(`${name} failed:\n${result.stdout}${result.stderr}`.trim());
  }
}

if (failures.length > 0) {
  console.error("Production copy gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("Production copy gate passed: normal UI copy and placeholder/honesty gates are clean.");

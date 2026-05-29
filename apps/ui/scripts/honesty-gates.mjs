#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { relative, resolve, sep } from "node:path";

const repoRoot = resolve(new URL("../../..", import.meta.url).pathname);
const failures = [];

function repoPath(path) {
  return relative(repoRoot, path).split(sep).join("/");
}

function readRepoFile(path) {
  return readFileSync(resolve(repoRoot, path), "utf8");
}

function walkFiles(root, predicate, files = []) {
  for (const entry of readdirSync(root)) {
    const fullPath = resolve(root, entry);
    const stat = statSync(fullPath);
    if (stat.isDirectory()) {
      if (["node_modules", "target", ".git"].includes(entry)) continue;
      walkFiles(fullPath, predicate, files);
    } else if (predicate(fullPath)) {
      files.push(fullPath);
    }
  }
  return files;
}

const rust = readRepoFile("apps/desktop/src-tauri/src/lib.rs");
const commands = readRepoFile("apps/ui/src/commands.ts");
const main = readRepoFile("apps/ui/src/main.tsx");

const sourceFiles = [
  ...walkFiles(resolve(repoRoot, "apps/ui/src"), (path) =>
    /\.(ts|tsx)$/.test(path),
  ),
  resolve(repoRoot, "apps/desktop/src-tauri/src/lib.rs"),
];

const capabilityClaimPatterns = [
  { label: "P2P", pattern: /\bp2p\b/i },
  { label: "WebRTC", pattern: /\bwebrtc\b/i },
  { label: "connected", pattern: /\bconnected\b/i },
  { label: "relay active", pattern: /\brelay\s+(?:is\s+)?active\b/i },
  { label: "TURN active", pattern: /\bturn\s+(?:is\s+)?active\b/i },
  { label: "delivered", pattern: /\bdelivered\b/i },
  { label: "encrypted", pattern: /\bencrypted\b/i },
];

const honestQualifier = /\b(?:local|local-first|local-only|command-backed|harness|fixture|facade|fallback|demo|preview|placeholder|disabled|release-gated|not\s+(?:connected|joined|claimed|delivered)|no\s+(?:remote|relay|production|socket)|does\s+not\s+claim|only|pending|plaintext\s+allowed)\b/i;
const backendProof = /\b(?:backend[-\s]proved|backend\s+state\s+proves|backend\s+verified|tauri\s+command\s+state|command\s+state\s+proves|state\.(?:connectivity|voice_session|messages)|message\.status|session\.route_copy)\b/i;
const productionReadyAdvert = /\b(?:production[-\s]ready|ready\s+for\s+production|production\s+(?:p2p|webrtc|relay|turn|network|media|storage|command|delivery))\b/i;

for (const file of sourceFiles) {
  const lines = readFileSync(file, "utf8").split(/\r?\n/);
  lines.forEach((line, index) => {
    const claim = capabilityClaimPatterns.find(({ pattern }) => pattern.test(line));
    if (!claim) return;
    if (honestQualifier.test(line) || backendProof.test(line)) return;
    failures.push(
      `${repoPath(file)}:${index + 1} claims ${claim.label} without local/harness qualifier or backend-state proof: ${line.trim()}`,
    );
  });
}

const rustManifest = [...rust.matchAll(/ipc_commands::([a-zA-Z0-9_]+)/g)].map(
  (match) => match[1],
);
const uniqueRustManifest = [...new Set(rustManifest)].sort();
const tsInvokedCommands = [
  ...commands.matchAll(/invokeOrFallback<[^>]+>\(\s*["']([a-zA-Z0-9_]+)["']/g),
].map((match) => match[1]);
const uniqueTsCommands = [...new Set(tsInvokedCommands)].sort();

if (uniqueRustManifest.length === 0) {
  failures.push("G006 command enumeration found no Rust IPC commands");
}
if (uniqueTsCommands.length === 0) {
  failures.push("G006 command enumeration found no TypeScript command clients");
}

for (const command of uniqueRustManifest) {
  if (!uniqueTsCommands.includes(command)) {
    failures.push(`G006 command path missing TypeScript client: ${command}`);
  }
}
for (const command of uniqueTsCommands) {
  if (!uniqueRustManifest.includes(command)) {
    failures.push(`G006 TypeScript client is not registered in Rust: ${command}`);
  }
}

function tsFunctionBlock(exportName) {
  const start = commands.search(
    new RegExp(`export\\s+async\\s+function\\s+${exportName}\\b`),
  );
  if (start === -1) return "";
  const next = commands
    .slice(start + 1)
    .search(/\nexport\s+async\s+function\s+\w+\b/);
  return next === -1
    ? commands.slice(start)
    : commands.slice(start, start + 1 + next);
}

function rustFunctionBlock(command) {
  const start = rust.search(new RegExp(`pub\\s+fn\\s+${command}\\b`));
  if (start === -1) return "";
  const next = rust.slice(start + 1).search(/\npub\s+fn\s+\w+\b/);
  return next === -1 ? rust.slice(start) : rust.slice(start, start + 1 + next);
}

const exportToCommand = [
  ...commands.matchAll(
    /export\s+async\s+function\s+([a-zA-Z0-9_]+)[\s\S]*?invokeOrFallback<[^>]+>\(\s*["']([a-zA-Z0-9_]+)["']/g,
  ),
].map((match) => ({ exportName: match[1], command: match[2] }));

for (const { exportName, command } of exportToCommand) {
  const block = tsFunctionBlock(exportName);
  const returnsLocalOnly = honestQualifier.test(block);
  if (returnsLocalOnly && productionReadyAdvert.test(block)) {
    failures.push(
      `G006 ${exportName}/${command} mixes local-only command copy with production-ready advertising`,
    );
  }
}

for (const command of uniqueRustManifest) {
  const block = rustFunctionBlock(command);
  if (!block) {
    failures.push(`G006 registered Rust command has no public function block: ${command}`);
    continue;
  }
  const returnsLocalOnly = honestQualifier.test(block);
  if (returnsLocalOnly && productionReadyAdvert.test(block)) {
    failures.push(
      `G006 Rust command ${command} mixes local-only command copy with production-ready advertising`,
    );
  }
}

const uiText = `${commands}\n${main}`;
if (productionReadyAdvert.test(uiText) && !backendProof.test(uiText)) {
  failures.push(
    "G003 production-ready UI copy exists without backend-state proof marker",
  );
}

const docsPath = resolve(repoRoot, "docs/ui-honesty-gates.md");
if (!existsSync(docsPath)) {
  failures.push("Good Taste UI honesty constraints doc is missing: docs/ui-honesty-gates.md");
} else {
  const docs = readFileSync(docsPath, "utf8");
  for (const required of [
    /backend state/i,
    /local-dev|local\/harness/i,
    /production labels/i,
    /Good Taste/i,
  ]) {
    if (!required.test(docs)) {
      failures.push(
        `Good Taste UI honesty constraints doc missing required concept: ${required}`,
      );
    }
  }
}

if (failures.length > 0) {
  console.error("Static UI honesty gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(
  `Static UI honesty gate passed: ${sourceFiles.length} source files scanned; ${uniqueRustManifest.length} command paths enumerated (${uniqueRustManifest.join(", ")}).`,
);

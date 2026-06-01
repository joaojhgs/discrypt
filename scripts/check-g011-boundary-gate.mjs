#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

function requirePattern(name, text, pattern, description) {
  if (!pattern.test(text)) failures.push(`${name} missing pattern: ${description}`);
}

function rejectUnqualifiedClaim(name, text, pattern, allowedNegation) {
  for (const [index, line] of text.split(/\r?\n/).entries()) {
    if (!pattern.test(line)) continue;
    if (allowedNegation.test(line)) continue;
    failures.push(`${name}:${index + 1} has an unqualified G011/G012 production claim: ${line.trim()}`);
  }
}

const packageJson = JSON.parse(read("apps/ui/package.json"));
const productionGap = read("docs/release/production-gap-matrix-2026-06-01.md");
const releaseMatrix = read("docs/release/release-verification-matrix.md");
const g010Adapter = read("docs/release/g010-adapter-public-matrix.md");
const g009Doc = read("docs/security/g009-security-privacy-no-shim-gates.md");
const ipfsDoc = read("docs/adapters/ipfs-pubsub-adapter-readiness.md");
const quicDoc = read("docs/adapters/quic-rendezvous-adapter-readiness.md");
const commands = read("apps/ui/src/commands.ts");
const main = read("apps/ui/src/main.tsx");
const voiceMedia = read("apps/ui/src/voice-media.ts");
const desktop = read("apps/desktop/src-tauri/src/lib.rs");
const providerAdapters = read("crates/transport/src/provider_adapters.rs");

if (packageJson.scripts?.["test:g011-boundary"] !== "node ../../scripts/check-g011-boundary-gate.mjs") {
  failures.push("apps/ui/package.json missing test:g011-boundary script");
}

for (const token of [
  "Discrypt is **not production-ready yet**",
  "does not prove Tauri IPC, actual provider signaling, remote WebRTC data delivery, or voice media",
  "| G011 Production ready | Not done |",
  "| G012 Two-user Tauri E2E text + voice | Not done |",
  "not complete until real Tauri and two-user artifacts exist",
]) requireText("production gap matrix", productionGap, token);

for (const token of [
  "G011/G012 are not claimed",
  "does **not** prove final production readiness",
  "two installed Tauri users completing text and voice flows",
]) requireText("G010 adapter matrix doc", g010Adapter, token);

for (const token of [
  "does not claim G011 production readiness or G012 two-installed-user Tauri E2E",
  "does not claim final production readiness, public TURN success, or two installed Tauri text/voice E2E",
  "Same-process command-layer restart proof; does not claim live provider text delivery, remote voice audio, credentialed TURN success, or two installed Tauri processes.",
  "Same-process Tauri harness proof; not yet two installed app processes.",
  "Credentialed TURN remains opt-in",
  "Local deterministic gates do not prove production TURN closure",
]) requireText("release verification matrix", releaseMatrix, token);

requirePattern(
  "G009 doc",
  g009Doc,
  /does \*\*not\*\* by itself claim final production readiness or final two-user\s+Tauri voice\/text E2E\. Those are G011 and G012\./,
  "G009 remains below final G011/G012 evidence boundary",
);

for (const token of [
  "not production-default yet",
  "default public bootstrap is now disabled",
  "production profiles must provide explicit direct `/ip4` or `/ip6` multiaddrs",
  "keep IPFS non-default until this passes on real public peers without DNS bootstrap",
  "full two-installed-app E2E over that route",
]) requireText("IPFS adapter readiness", ipfsDoc, token);

for (const token of [
  "native `quic://` transport still reserved; not production-ready",
  "Native `quic://` is still reserved",
  "rejects reserved native QUIC scheme",
  "production readiness still requires staged service evidence",
]) requireText("QUIC adapter readiness", quicDoc, token);

for (const token of [
  "Production labels disabled until backend state proves network, media, and storage services are configured",
  "Remote audio transport remains disabled until backend media-route evidence attaches",
]) requireText("UI command fallback copy", commands, token);

for (const token of [
  "No TURN relay is configured. If backend route checks report TURN required, voice/text transport must fail closed instead of claiming a connection.",
  "relay success remains blocked until credentialed backend route evidence exists",
  "success still requires backend route proof",
]) requireText("UI TURN honesty copy", main, token);

for (const token of [
  "keep relay use fail-closed here until a backend-proved",
  "credential-bearing RTCIceServer handoff exists",
]) requireText("webview voice media TURN boundary", voiceMedia, token);

for (const token of [
  "remote WebRTC audio transport remains fail-closed until media-route evidence attaches",
  "production claims require the",
  "cfg!(any(test, feature = \"harness\", feature = \"local-dev\"))",
]) requireText("desktop production boundary", desktop, token);

for (const token of [
  "Adapter feature is not enabled in this build.",
  "fail-closed boundary when feature is disabled",
  "adapter is not enabled; compile with Cargo feature",
  "feature is enabled but no audited production provider client is wired",
]) requireText("provider adapter fail-closed boundary", providerAdapters, token);

const releaseDocs = {
  "production-gap-matrix": productionGap,
  "release-verification-matrix": releaseMatrix,
  "g010-adapter-public-matrix": g010Adapter,
};
for (const [name, text] of Object.entries(releaseDocs)) {
  rejectUnqualifiedClaim(
    name,
    text,
    /\b(?:G011|G012|production[- ]ready|final production|two installed Tauri)\b.*\b(?:complete|completed|done|passed|green|claimed|proves?)\b/i,
    /\b(?:not|does not|do not|no |without|unless|until|remain|remains|blocked|partial|pending|skipped|not yet)\b/i,
  );
}

if (failures.length > 0) {
  console.error("G011/G012 boundary and unsupported-path gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G011/G012 boundary gate passed: production-readiness copy stays below G012, and unsupported IPFS/QUIC/TURN/voice paths remain non-default, credential-gated, or fail-closed with honest recovery copy.");

#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const docs = read("docs/security/g100-performance-soak-suite.md");
const harness = read("harness/multinode/src/lib.rs");
const overlay = read("crates/relay-overlay/src/manager.rs");
const media = read("crates/media/src/sframe.rs");
const transport = read("crates/transport/src/lib.rs");
const failures = [];

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "# G100 performance soak suite",
  "16 members",
  "8+ voice senders",
  "1-3 relay hops",
  "Packet loss",
  "NAT switching",
  "Android doze",
  "Restart/reconnect",
  "final release E2E gate still owns wall-clock soak duration",
]) requireText("docs", docs, token);

for (const token of [
  "pub struct PerformanceSoakSmoke",
  "performance_soak_smoke",
  "sixteen_members_represented",
  "eight_voice_senders_verified",
  "one_to_three_relay_hops_covered",
  "packet_loss_redelivery_bounded",
  "nat_switching_fallbacks_covered",
  "android_doze_deprioritized",
  "restart_reconnect_recovers_route",
  "performance_soak_smoke_covers_phase_n_load_and_reconnect_gates",
]) requireText("harness", harness, token);

for (const token of ["OverlayRouteUse", "mark_failed_media_and_reroute", "ChurnDampingPolicy", "ConstructedOverlayRoute"]) requireText("overlay", overlay, token);
for (const token of ["SFrameSender", "SFrameReceiver", "ReplayWindow", "SenderBinding"]) requireText("media", media, token);
for (const token of ["ConnectivityPlanner", "SimulatedNat", "FallbackLeg"]) requireText("transport", transport, token);

if (/TODO|FIXME|unimplemented!|todo!/i.test(docs)) failures.push("performance soak docs contain unfinished-work marker");

const commands = [
  ["cargo", ["test", "-p", "discrypt-multinode-harness", "performance_soak_smoke_covers_phase_n_load_and_reconnect_gates", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-relay-overlay", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-media", "sframe", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-transport", "connectivity", "--quiet"]],
  ["cargo", ["test", "-p", "discrypt-storage", "encrypted_app_db", "--quiet"]],
];
for (const [cmd, args] of commands) {
  const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
  if (run.status !== 0) failures.push(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
}

if (failures.length > 0) {
  console.error("G100 performance soak check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G100 performance soak check passed");

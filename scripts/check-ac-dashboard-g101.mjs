#!/usr/bin/env node
import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const dashboardPath = ".omx/artifacts/production-readiness/ac-dashboard.json";
const dashboardMdPath = ".omx/artifacts/production-readiness/ac-dashboard.md";
const dashboard = JSON.parse(read(dashboardPath));
const dashboardMd = read(dashboardMdPath);
const failures = [];

const required = [
  "AC1",
  "AC2",
  "AC3",
  "AC4",
  "AC5",
  "AC6",
  "AC7",
  "AC8",
  "AC8b",
  "AC-MLS-FORK",
  "AC-GOV",
  "AC9",
  "AC10",
  "AC10b",
  "AC-PRESENCE",
  "AC11",
  "AC-SHRED-PERSIST",
  "AC12",
  "AC13",
  "AC14",
  "AC15",
  "AC16",
  "AC17",
  "AC18",
  "AC-METADATA",
  "AC-ABUSE",
  "AC-RECOVERY",
];
const forbiddenStates = new Set(["missing", "blocked", "skipped", "rejected", "unverified", "partial_foundation", "foundation_only", "placeholder_or_foundation"]);
const requiredCommands = [
  "npm --prefix apps/ui run test:pcap-suite-g096",
  "npm --prefix apps/ui run test:malicious-relay-g097",
  "npm --prefix apps/ui run test:malicious-member-g098",
  "npm --prefix apps/ui run test:retention-shred-g099",
  "npm --prefix apps/ui run test:performance-soak-g100",
  "npm --prefix apps/ui run test:release-verification-matrix",
];

function fail(message) {
  failures.push(message);
}
function requireText(name, text, token) {
  if (!text.includes(token)) fail(`${name} missing token: ${token}`);
}

if (dashboard.schema_version !== "discrypt.ac_dashboard.v2") fail("dashboard schema_version must be discrypt.ac_dashboard.v2");
if (dashboard.story !== "G101-phase-n-security-adversarial-and-rel") fail("dashboard story must name G101");
if (dashboard.no_skipped_blockers !== true) fail("dashboard must declare no_skipped_blockers=true");
if (!Array.isArray(dashboard.rows)) fail("dashboard rows must be an array");
if (!Array.isArray(dashboard.required_acceptance_criteria)) fail("dashboard required_acceptance_criteria must be an array");

const rows = dashboard.rows ?? [];
const rowById = new Map(rows.map((row) => [row.id, row]));
for (const id of required) {
  if (!rowById.has(id)) fail(`dashboard missing original acceptance criterion row: ${id}`);
  requireText("dashboard markdown", dashboardMd, id);
}
if (rows.length !== required.length) fail(`dashboard must have one row per original criterion: expected ${required.length}, got ${rows.length}`);

for (const row of rows) {
  for (const key of ["id", "original_requirement", "owning_lane", "dashboard_status", "blocker_review", "review_status"]) {
    if (!row[key]) fail(`${row.id ?? "unknown row"} missing ${key}`);
  }
  if (forbiddenStates.has(String(row.dashboard_status))) fail(`${row.id} has forbidden dashboard_status ${row.dashboard_status}`);
  if (row.blocker_review !== "not_skipped") fail(`${row.id} blocker_review must be not_skipped`);
  if (row.review_status !== "dashboard_checked") fail(`${row.id} review_status must be dashboard_checked`);
  const commands = row.fresh_evidence?.commands;
  const files = row.fresh_evidence?.files;
  if (!Array.isArray(commands) || commands.length === 0) fail(`${row.id} missing fresh command evidence`);
  if (!Array.isArray(files) || files.length === 0) fail(`${row.id} missing file evidence`);
  for (const evidencePath of files ?? []) {
    if (!existsSync(resolve(repoRoot, evidencePath))) fail(`${row.id} evidence file does not exist: ${evidencePath}`);
  }
  const serialized = JSON.stringify(row);
  if (/TODO|FIXME|unimplemented!|todo!/i.test(serialized)) fail(`${row.id} contains unfinished or skipped-blocker marker`);
}
for (const command of requiredCommands) {
  if (!dashboard.fresh_evidence_commands?.includes(command)) fail(`dashboard fresh evidence commands missing ${command}`);
  requireText("dashboard markdown", dashboardMd, command);
}
requireText("dashboard markdown", dashboardMd, "No skipped blockers: **true**");
requireText("dashboard markdown", dashboardMd, "blocker_review=not_skipped");
if (/TODO|FIXME|unimplemented!|todo!/i.test(dashboardMd)) fail("dashboard markdown contains unfinished or skipped-blocker marker");

if (failures.length === 0) {
  const commandsToRun = [
    ["npm", ["--prefix", "apps/ui", "run", "test:pcap-suite-g096"]],
    ["npm", ["--prefix", "apps/ui", "run", "test:malicious-relay-g097"]],
    ["npm", ["--prefix", "apps/ui", "run", "test:malicious-member-g098"]],
    ["npm", ["--prefix", "apps/ui", "run", "test:retention-shred-g099"]],
    ["npm", ["--prefix", "apps/ui", "run", "test:performance-soak-g100"]],
    ["npm", ["--prefix", "apps/ui", "run", "test:release-verification-matrix"]],
  ];
  for (const [cmd, args] of commandsToRun) {
    const run = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8" });
    if (run.status !== 0) {
      fail(`${cmd} ${args.join(" ")} failed:\n${run.stdout}\n${run.stderr}`);
      break;
    }
  }
}

if (failures.length > 0) {
  console.error("G101 AC dashboard check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G101 AC dashboard check passed");

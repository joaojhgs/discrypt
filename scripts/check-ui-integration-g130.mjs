#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const files = {
  main: read("apps/ui/src/main.tsx"),
  commands: read("apps/ui/src/commands.ts"),
  config: read("apps/ui/src/app-config.ts"),
  statefulE2e: read("apps/ui/tests/e2e/stateful-ui.spec.ts"),
  twoProfileE2e: read("apps/ui/tests/e2e/two-profile-flow.spec.ts"),
  recoveryE2e: read("apps/ui/tests/e2e/recovery.spec.ts"),
  packageJson: read("apps/ui/package.json"),
  workflow: read(".github/workflows/ci.yml"),
  styles: read("apps/ui/src/styles.css"),
};
const failures = [];

function requireText(file, token) {
  if (!files[file].includes(token)) failures.push(`${file} missing ${token}`);
}
function forbidRegex(file, regex, label) {
  if (regex.test(files[file])) failures.push(`${file} must not contain ${label}`);
}

for (const token of [
  '@/components/ui/avatar',
  '@/components/ui/badge',
  '@/components/ui/button',
  '@/components/ui/card',
  '@/components/ui/input',
  '@/components/ui/label',
  '@/components/ui/scroll-area',
  '@/components/ui/select',
  '@/components/ui/separator',
  '@/components/ui/slider',
  '@/components/ui/switch',
]) requireText("main", token);
forbidRegex("main", /<button\b/, "raw button elements; use shadcn Button");
forbidRegex("main", /<select\b/, "raw select elements; use shadcn Select");

for (const token of [
  "createUser",
  "recoverUser",
  "startDm",
  "createGroup",
  "joinGroup",
  "createInvite",
  "createChannelCommand",
  "sendMessage",
  "joinVoice",
  "leaveVoice",
  "setSelfMute",
  "setSpeakerVolume",
  "verifySafetyNumber",
  "savePreferences",
  "resetAppState",
]) requireText("main", token);

for (const token of [
  "Set up your local discrypt profile",
  "Workspace sections",
  "Direct messages",
  "Create group",
  "Create invite for active group",
  "Signaling endpoint",
  "Trust fingerprint",
  "Room secret commitment",
  "ICE/STUN metadata",
  "TURN metadata",
  "Message",
  "Join call",
  "Mute my microphone",
  "Media route proof",
  "Remote audio blocker",
  "Speaker",
  "Transport status",
]) requireText("main", token);

for (const token of [
  "themes: [",
  "midnight-steel",
  "graphite-calm",
  "ocean-contrast",
  "templates: [",
  "command-center",
  "compact-ops",
  "accentIntent",
  "no neon/purple gradients",
  "shadcnComponentInventory",
  "src/components/ui/button.tsx",
  "src/components/ui/select.tsx",
  "--template-shell-grid",
  "--template-shell-grid-inspector",
  "--template-panel-radius",
]) requireText("config", token);

for (const token of [
  "var(--template-shell-grid)",
  "var(--template-shell-grid-inspector)",
]) requireText("main", token);
for (const token of [
  "--template-font-size",
  "--template-panel-radius",
]) requireText("styles", token);

for (const token of [
  "group invite join text channel and voice controls work without fake members",
  "direct message send stays command-backed",
  "local-dev e2e persistence survives browser reload",
  "production UX hides diagnostics and manual transport controls by default",
  "small-window navigation exposes setup groups invites text and voice without overflow",
  "setViewportSize({ width: 390, height: 820 })",
  "New contact · friend",
  "Ops relay",
  "toHaveCount(0)",
]) requireText("statefulE2e", token);
for (const token of [
  "two independent profiles exercise DM, invite join, and voice attempts honestly",
  "browser.newContext",
  "alice to bob local DM harness ping",
  "bob to alice local DM harness pong",
  "encrypted media transport remains gated by media-frame E2E",
  "New contact · friend",
  "Ops relay",
  "toHaveCount(0)",
]) requireText("twoProfileE2e", token);
for (const token of [
  "first-run recovery restores account continuity without content-key claims",
  "recover existing user",
  "content-key recovery",
]) requireText("recoveryE2e", token);

for (const token of [
  "test:e2e",
  "test:ui-integration-g130",
]) requireText("packageJson", token);
requireText("workflow", "test:ui-integration-g130");

for (const token of [
  "create_group",
  "join_group",
  "create_invite",
  "create_channel",
  "send_message",
  "join_voice",
  "leave_voice",
  "set_self_mute",
  "update_voice_activity",
  "set_speaker_volume",
  "save_preferences",
]) requireText("commands", token);

if (failures.length > 0) {
  console.error("G130 UI integration gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G130 UI integration gate passed: production commands are surfaced through shadcn-owned UI, configurable themes/templates, and Playwright coverage for setup/recovery/DM/group/invite/text/voice/persistence.");

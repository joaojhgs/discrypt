#!/usr/bin/env node
import { readFileSync } from 'node:fs';

const main = readFileSync(new URL('../src/main.tsx', import.meta.url), 'utf8');
const commands = readFileSync(new URL('../src/commands.ts', import.meta.url), 'utf8');
const failures = [];

const requiredCommandExports = [
  'loadAppSnapshot',
  'verifySafetyNumber',
  'createGroup',
  'joinGroup',
  'createChannel',
  'savePreferences',
  'joinVoice',
  'leaveVoice',
  'setSelfMute',
  'setSpeakerVolume',
  'sendMessage',
];
for (const name of requiredCommandExports) {
  if (!new RegExp(`export\\s+async\\s+function\\s+${name}\\b`).test(commands)) {
    failures.push(`missing command client export: ${name}`);
  }
}

const requiredMainUsages = [
  'loadAppSnapshot(',
  'verifySafetyNumber(',
  'createGroup(',
  'joinGroup(',
  'createChannelCommand(',
  'savePreferences(',
  'joinVoice(',
  'leaveVoice(',
  'setSelfMute(',
  'setSpeakerVolume(',
];
for (const usage of requiredMainUsages) {
  if (!main.includes(usage)) {
    failures.push(`main.tsx does not use command-backed path: ${usage}`);
  }
}

const forbiddenLocalProductState = [
  'localChannels',
  'groupMode',
  'initialVoiceRoster',
  'setParticipants',
  'setVoiceJoined] = useState',
  'setSelfMuted] = useState',
];
for (const token of forbiddenLocalProductState) {
  if (main.includes(token) || commands.includes(token)) {
    failures.push(`forbidden local-only product state token found: ${token}`);
  }
}

const forbiddenClaims = [
  'Relay active',
  'Deleted after',
  'local shell state only',
  'backend channel persistence is intentionally outside',
];
for (const claim of forbiddenClaims) {
  if (main.includes(claim)) {
    failures.push(`unsafe UI claim found in main.tsx: ${claim}`);
  }
}

const commandBackedCopy = [
  'command-backed',
  'AppService command',
  'media-frame E2E gate',
  'pending on offline devices',
];
for (const copy of commandBackedCopy) {
  if (!main.includes(copy) && !commands.includes(copy)) {
    failures.push(`expected honest/command-backed copy missing: ${copy}`);
  }
}

if (failures.length > 0) {
  console.error('UI command coverage gate failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`UI command coverage gate passed: ${requiredCommandExports.length} command clients and ${requiredMainUsages.length} UI usages verified.`);

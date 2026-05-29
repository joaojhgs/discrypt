#!/usr/bin/env node
import { readFileSync } from 'node:fs';

const main = readFileSync(new URL('../src/main.tsx', import.meta.url), 'utf8');
const commands = readFileSync(new URL('../src/commands.ts', import.meta.url), 'utf8');
const failures = [];

const requiredCommandExports = [
  'loadAppState',
  'createUser',
  'recoverUser',
  'verifySafetyNumber',
  'savePreferences',
  'startDm',
  'createGroup',
  'joinGroup',
  'createInvite',
  'createChannel',
  'sendMessage',
  'joinVoice',
  'leaveVoice',
  'setSelfMute',
  'setSpeakerVolume',
];
for (const name of requiredCommandExports) {
  if (!new RegExp(`export\\s+async\\s+function\\s+${name}\\b`).test(commands)) {
    failures.push(`missing command client export: ${name}`);
  }
}

const requiredMainUsages = [
  'loadAppState(',
  'createUser(',
  'recoverUser(',
  'verifySafetyNumber(',
  'startDm(',
  'createGroup(',
  'joinGroup(',
  'createInvite(',
  'createChannel(',
  'sendMessage(',
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

const requiredDtoKeys = [
  'display_name',
  'device_name',
  'recovery_code',
  'peer_label',
  'group_id',
  'channel_id',
  'session_id',
  'participant_id',
  'MessageTarget',
];
for (const key of requiredDtoKeys) {
  if (!commands.includes(key)) failures.push(`command DTO key/type missing: ${key}`);
}

const forbiddenTokens = [
  'currentSnapshot.servers[0]',
  'loadAppSnapshot().then',
  'localChannels',
  'setParticipants',
  'setVoiceJoined] = useState',
  'setSelfMuted] = useState',
  'createChannelCommand',
  'channel_name',
  'server_name',
];
for (const token of forbiddenTokens) {
  if (main.includes(token)) failures.push(`forbidden UI drift/local-state token found in main.tsx: ${token}`);
}

const honestCopy = [
  'QR/cross-device recovery is not enabled',
  'production media path waits for adapter/E2E gates',
  'local encrypted-message facade persisted',
  'pending on offline devices',
];
for (const copy of honestCopy) {
  if (!main.includes(copy) && !commands.includes(copy)) {
    failures.push(`expected honest/command-backed copy missing: ${copy}`);
  }
}

if (failures.length > 0) {
  console.error('UI command coverage gate failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`UI command coverage gate passed: ${requiredCommandExports.length} command clients, ${requiredMainUsages.length} UI usages, DTO drift guards active.`);

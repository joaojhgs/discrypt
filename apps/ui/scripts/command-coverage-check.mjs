#!/usr/bin/env node
import { readFileSync } from "node:fs";

const rust = readFileSync(
  new URL("../../desktop/src-tauri/src/lib.rs", import.meta.url),
  "utf8",
);
const commands = readFileSync(
  new URL("../src/commands.ts", import.meta.url),
  "utf8",
);
const main = readFileSync(new URL("../src/main.tsx", import.meta.url), "utf8");
const failures = [];

const expectedCommands = [
  {
    command: "app_state",
    exportName: "loadAppState",
    args: [],
    returns: "AppState",
  },
  {
    command: "app_snapshot",
    exportName: "loadCompatibilityAppSnapshot",
    args: [],
    returns: "AppSnapshot",
    compatibility: true,
  },
  {
    command: "create_user",
    exportName: "createUser",
    args: ["display_name", "device_name"],
    returns: "AppState",
  },
  {
    command: "recover_user",
    exportName: "recoverUser",
    args: ["display_name", "recovery_code", "device_name"],
    returns: "AppState",
  },
  {
    command: "verify_safety_number",
    exportName: "verifySafetyNumber",
    args: ["friend_id", "provided"],
    returns: "SafetyVerificationResult",
  },
  {
    command: "save_preferences",
    exportName: "savePreferences",
    args: ["theme_id", "template_id"],
    returns: "AppState",
  },
  {
    command: "start_dm",
    exportName: "startDm",
    args: ["display_name"],
    returns: "AppState",
  },
  {
    command: "create_group",
    exportName: "createGroup",
    args: ["name", "retention"],
    returns: "AppState",
  },
  {
    command: "set_active_group",
    exportName: "setActiveGroup",
    args: ["group_id"],
    returns: "AppState",
  },
  {
    command: "join_group",
    exportName: "joinGroup",
    args: ["invite_code", "group_name"],
    returns: "AppState",
  },
  {
    command: "create_invite",
    exportName: "createInvite",
    args: ["group_id", "expires", "max_use"],
    returns: "AppState",
  },
  {
    command: "create_channel",
    exportName: "createChannel",
    args: ["group_id", "name", "kind", "retention_status"],
    returns: "AppState",
  },
  {
    command: "send_message",
    exportName: "sendMessage",
    args: ["target", "kind", "dm_id", "group_id", "channel_id", "body"],
    returns: "AppState",
  },
  {
    command: "join_voice",
    exportName: "joinVoice",
    args: ["group_id", "channel_id"],
    returns: "AppState",
  },
  {
    command: "leave_voice",
    exportName: "leaveVoice",
    args: ["session_id"],
    returns: "AppState",
  },
  {
    command: "set_self_mute",
    exportName: "setSelfMute",
    args: ["session_id", "muted"],
    returns: "AppState",
  },
  {
    command: "set_speaker_volume",
    exportName: "setSpeakerVolume",
    args: ["session_id", "participant_id", "volume"],
    returns: "AppState",
  },
  {
    command: "poll_app_events",
    exportName: "pollAppEvents",
    args: [],
    returns: "AppEventView[]",
  },
  {
    command: "deletion_warning",
    exportName: "deletionWarning",
    args: [],
    returns: "string",
  },
  {
    command: "metadata_warning",
    exportName: "metadataWarning",
    args: [],
    returns: "string",
  },
  {
    command: "command_health",
    exportName: "commandHealth",
    args: [],
    returns: "CommandHealth",
  },
  {
    command: "reset_app_state",
    exportName: "resetAppState",
    args: [],
    returns: "AppState",
  },
];

const rustManifest = [...rust.matchAll(/ipc_commands::([a-zA-Z0-9_]+)/g)].map(
  (match) => match[1],
);
const uniqueRustManifest = [...new Set(rustManifest)];
const expectedNames = expectedCommands.map((entry) => entry.command);
for (const command of uniqueRustManifest) {
  if (!expectedNames.includes(command)) {
    failures.push(
      `Rust invoke_handler command missing from TS manifest expectations: ${command}`,
    );
  }
}
for (const command of expectedNames) {
  if (!uniqueRustManifest.includes(command)) {
    failures.push(
      `Expected command not registered in Rust invoke_handler: ${command}`,
    );
  }
}

const invokedCommands = [
  ...commands.matchAll(/invokeOrFallback<[^>]+>\(\s*["']([a-zA-Z0-9_]+)["']/g),
].map((match) => match[1]);
for (const command of expectedNames) {
  if (!invokedCommands.includes(command)) {
    failures.push(
      `TS command client does not invoke backend command: ${command}`,
    );
  }
}
for (const command of invokedCommands) {
  if (!expectedNames.includes(command)) {
    failures.push(
      `TS command client invokes command outside strict manifest: ${command}`,
    );
  }
}

function functionBlock(name) {
  const start = commands.search(
    new RegExp(`export\\s+async\\s+function\\s+${name}\\b`),
  );
  if (start === -1) return "";
  const next = commands
    .slice(start + 1)
    .search(/\nexport\s+async\s+function\s+\w+\b/);
  return next === -1
    ? commands.slice(start)
    : commands.slice(start, start + 1 + next);
}

for (const entry of expectedCommands) {
  const signature = new RegExp(
    `export\\s+async\\s+function\\s+${entry.exportName}\\b[\\s\\S]*?:\\s*Promise<${entry.returns.replace(/[\[\]]/g, "\\$&")}>`,
  );
  if (!signature.test(commands)) {
    failures.push(
      `missing or wrong return type for ${entry.exportName}: expected Promise<${entry.returns}>`,
    );
  }
  const block = functionBlock(entry.exportName);
  if (!block) {
    failures.push(`missing command client export: ${entry.exportName}`);
    continue;
  }
  if (
    !block.includes(`"${entry.command}"`) &&
    !block.includes(`'${entry.command}'`)
  ) {
    failures.push(`${entry.exportName} does not invoke ${entry.command}`);
  }
}

const requestTypes = [
  ["CreateUserRequest", ["display_name", "device_name"]],
  ["RecoverUserRequest", ["display_name", "recovery_code", "device_name"]],
  ["SavePreferencesRequest", ["theme_id", "template_id"]],
  ["StartDmRequest", ["display_name"]],
  ["CreateGroupRequest", ["name", "retention"]],
  ["JoinGroupRequest", ["invite_code", "group_name"]],
  ["SetActiveGroupRequest", ["group_id"]],
  ["CreateInviteRequest", ["group_id", "expires", "max_use"]],
  ["CreateChannelRequest", ["group_id", "name", "kind", "retention_status"]],
  ["SendMessageRequest", ["target", "body"]],
  ["MessageTargetView", ["kind", "dm_id", "group_id", "channel_id"]],
  ["JoinVoiceRequest", ["group_id", "channel_id"]],
  ["LeaveVoiceRequest", ["session_id"]],
  ["SelfMuteRequest", ["session_id", "muted"]],
  ["SpeakerVolumeRequest", ["session_id", "participant_id", "volume"]],
];
for (const [typeName, fields] of requestTypes) {
  const match = commands.match(
    new RegExp(`export\\s+type\\s+${typeName}\\s*=\\s*{([\\s\\S]*?)^};`, "m"),
  );
  if (!match) {
    failures.push(`missing DTO type: ${typeName}`);
    continue;
  }
  for (const field of fields) {
    if (!new RegExp(`\\b${field}\\??\\s*:`).test(match[1])) {
      failures.push(`DTO ${typeName} missing field: ${field}`);
    }
  }
}

const mutationExportNames = expectedCommands
  .filter(
    (entry) => entry.returns === "AppState" && entry.command !== "app_snapshot",
  )
  .map((entry) => entry.exportName);
for (const name of mutationExportNames) {
  const block = functionBlock(name);
  if (/Promise<AppSnapshot>/.test(block)) {
    failures.push(`${name} is typed as AppSnapshot instead of AppState`);
  }
}

if (
  !commands.includes("export async function loadAppState(): Promise<AppState>")
) {
  failures.push(
    "primary app load path must be loadAppState(): Promise<AppState>",
  );
}
if (!commands.includes('"app_state"')) {
  failures.push("frontend command client must invoke app_state");
}

const forbiddenLegacyDtoTokens = [
  "server_name: string",
  "channel: string;\n  body: string",
  "export async function joinVoice(): Promise<AppSnapshot>",
  "export async function leaveVoice(): Promise<AppSnapshot>",
  "export type SelfMuteRequest = {\n  muted: boolean",
  "export type SpeakerVolumeRequest = {\n  participant_id: string;\n  volume: number",
];
for (const token of forbiddenLegacyDtoTokens) {
  if (commands.includes(token)) {
    failures.push(
      `legacy drift token still present in commands.ts: ${token.replace(/\n/g, " ")}`,
    );
  }
}

const forbiddenLocalProductState = [
  "localChannels",
  "groupMode",
  "initialVoiceRoster",
  "setParticipants",
  "setVoiceJoined] = useState",
  "setSelfMuted] = useState",
];
for (const token of forbiddenLocalProductState) {
  if (main.includes(token) || commands.includes(token)) {
    failures.push(`forbidden local-only product state token found: ${token}`);
  }
}

const commandBackedCopy = [
  "command-backed",
  "media-frame E2E",
  "pending on offline devices",
];
for (const copy of commandBackedCopy) {
  if (!main.includes(copy) && !commands.includes(copy)) {
    failures.push(`expected honest/command-backed copy missing: ${copy}`);
  }
}

if (failures.length > 0) {
  console.error("UI command coverage gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(
  `UI command coverage gate passed: ${expectedCommands.length} strict command clients mirror ${uniqueRustManifest.length} Rust IPC commands.`,
);

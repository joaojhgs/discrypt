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
const statefulE2e = readFileSync(
  new URL("../tests/e2e/stateful-ui.spec.ts", import.meta.url),
  "utf8",
);
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
    args: [
      "display_name",
      "recovery_code",
      "device_name",
      "recovery_room_memberships",
      "recovered_device_count",
      "use_sealed_account_backup",
    ],
    returns: "AppState",
  },
  {
    command: "verify_safety_number",
    exportName: "verifySafetyNumber",
    args: ["friend_id", "provided"],
    returns: "SafetyVerificationResult",
  },
  {
    command: "create_device_pairing_payload",
    exportName: "createDevicePairingPayload",
    args: ["requested_label", "current_epoch", "valid_for_epochs"],
    returns: "DevicePairingPayloadView",
  },
  {
    command: "accept_device_pairing_payload",
    exportName: "acceptDevicePairingPayload",
    args: ["payload", "device_name", "current_epoch"],
    returns: "AppState",
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
    command: "set_active_channel",
    exportName: "setActiveChannel",
    args: ["group_id", "channel_id"],
    returns: "AppState",
  },
  {
    command: "set_active_dm",
    exportName: "setActiveDm",
    args: ["dm_id"],
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
    command: "create_dm_invite",
    exportName: "createDmInvite",
    args: ["dm_id", "expires", "max_use"],
    returns: "AppState",
  },
  {
    command: "accept_dm_invite",
    exportName: "acceptDmInvite",
    args: ["invite_code", "display_name"],
    returns: "AppState",
  },
  {
    command: "create_channel",
    exportName: "createChannel",
    args: ["group_id", "name", "kind", "retention_status"],
    returns: "AppState",
  },
  {
    command: "set_active_channel",
    exportName: "setActiveChannel",
    args: ["group_id", "channel_id"],
    returns: "AppState",
  },
  {
    command: "set_active_dm",
    exportName: "setActiveDm",
    args: ["dm_id"],
    returns: "AppState",
  },
  {
    command: "start_signaling_session",
    exportName: "startSignalingSession",
    args: [
      "scope_label",
      "adapter_probe",
      "data_channel_probe",
      "adapter_kind",
    ],
    returns: "AppState",
  },
  {
    command: "stop_signaling_session",
    exportName: "stopSignalingSession",
    args: ["session_id"],
    returns: "AppState",
  },
  {
    command: "start_text_session",
    exportName: "startTextSession",
    args: ["scope_label", "data_channel_probe", "adapter_kind"],
    returns: "AppState",
  },
  {
    command: "stop_text_session",
    exportName: "stopTextSession",
    args: ["session_id"],
    returns: "AppState",
  },
  {
    command: "attach_text_control_transport_runtime",
    exportName: "attachTextControlTransportRuntime",
    args: ["session_id", "runtime_role", "local_peer_id", "remote_peer_id"],
    returns: "AppState",
  },
  {
    command: "send_message",
    exportName: "sendMessage",
    args: ["target", "kind", "dm_id", "group_id", "channel_id", "body"],
    returns: "AppState",
  },
  {
    command: "apply_text_delivery_receipt",
    exportName: "applyTextDeliveryReceipt",
    args: ["message_id", "receipt", "recipient_verifying_key_hex"],
    returns: "AppState",
  },
  {
    command: "receive_text_delivery_envelope",
    exportName: "receiveTextDeliveryEnvelope",
    args: ["target", "envelope", "sender_verifying_key_hex", "recipient_leaf"],
    returns: "ReceiveTextDeliveryEnvelopeResponse",
  },
  {
    command: "list_pending_text_control_frames",
    exportName: "listPendingTextControlFrames",
    args: ["target", "limit", "operation_timeout_ms"],
    returns: "ListPendingTextControlFramesResponse",
  },
  {
    command: "pump_text_control_transport_once",
    exportName: "pumpTextControlTransportOnce",
    args: ["target", "limit", "operation_timeout_ms"],
    returns: "TextControlTransportPumpReportView",
  },
  {
    command: "mark_text_control_frame_sent",
    exportName: "markTextControlFrameSent",
    args: ["message_id", "frame_sha256", "transport_session_id"],
    returns: "AppState",
  },
  {
    command: "handle_text_control_frame",
    exportName: "handleTextControlFrame",
    args: ["frame"],
    returns: "HandleTextControlFrameResponse",
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
    command: "update_voice_activity",
    exportName: "updateVoiceActivity",
    args: ["session_id", "rms_i16", "peak_i16", "captured_at_ms"],
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
    args: ["after", "kinds", "limit"],
    returns: "AppEventStreamView",
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
    args: ["confirmation"],
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
  [
    "CreateDevicePairingPayloadRequest",
    ["requested_label", "current_epoch", "valid_for_epochs"],
  ],
  [
    "AcceptDevicePairingPayloadRequest",
    ["payload", "device_name", "current_epoch"],
  ],
  ["SavePreferencesRequest", ["theme_id", "template_id"]],
  ["StartDmRequest", ["display_name"]],
  ["CreateGroupRequest", ["name", "retention"]],
  ["JoinGroupRequest", ["invite_code", "group_name"]],
  ["SetActiveGroupRequest", ["group_id"]],
  ["SetActiveChannelRequest", ["group_id", "channel_id"]],
  ["SetActiveDmRequest", ["dm_id"]],
  ["CreateInviteRequest", ["group_id", "expires", "max_use"]],
  ["CreateDmInviteRequest", ["dm_id", "expires", "max_use"]],
  ["AcceptDmInviteRequest", ["invite_code", "display_name"]],
  ["CreateChannelRequest", ["group_id", "name", "kind", "retention_status"]],
  [
    "StartSignalingSessionRequest",
    ["scope_label", "adapter_probe", "data_channel_probe", "adapter_kind"],
  ],
  ["StopSignalingSessionRequest", ["session_id"]],
  [
    "StartTextSessionRequest",
    ["scope_label", "data_channel_probe", "adapter_kind"],
  ],
  ["StopTextSessionRequest", ["session_id"]],
  [
    "AttachTextControlTransportRuntimeRequest",
    ["session_id", "runtime_role", "local_peer_id", "remote_peer_id"],
  ],
  ["SendMessageRequest", ["target", "body"]],
  [
    "ApplyTextDeliveryReceiptRequest",
    ["message_id", "receipt", "recipient_verifying_key_hex"],
  ],
  [
    "ReceiveTextDeliveryEnvelopeRequest",
    ["target", "envelope", "sender_verifying_key_hex", "recipient_leaf"],
  ],
  [
    "ListPendingTextControlFramesRequest",
    ["target", "limit", "operation_timeout_ms"],
  ],
  [
    "MarkTextControlFrameSentRequest",
    ["message_id", "frame_sha256", "transport_session_id"],
  ],
  ["HandleTextControlFrameRequest", ["frame"]],
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
if (!commands.includes("RESET_APP_CONFIRMATION_PHRASE")) {
  failures.push(
    "resetAppState must require the shared destructive confirmation phrase",
  );
}
if (!commands.includes('"confirmation_required"')) {
  failures.push(
    "resetAppState fallback must surface typed confirmation_required errors",
  );
}
if (
  commands.includes("export async function resetAppState(): Promise<AppState>")
) {
  failures.push(
    "resetAppState must not be callable without an explicit confirmation request",
  );
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

const productionClaimTerms = [
  "P2P",
  "WebRTC",
  "connected",
  "relay active",
  "TURN active",
  "delivered",
  "encrypted",
];
const allowedProductionClaimContext = [
  "local",
  "local-only",
  "local-first",
  "command-backed",
  "harness",
  "facade",
  "release-gated",
  "not connected",
  "not-connected",
  "not claimed",
  "No relay is active",
  "policy",
  "ciphertext",
  "content-private",
  "metadata-minimizing",
  "pending on offline devices",
  "backend state",
  "backend-state",
  "returned by state",
  "honest_copy_ready",
  "backend media-route evidence",
  "socket/media adapter E2E",
  "webrtc-datachannel-proofed",
  "webrtc-datachannel-failed",
  "webrtc-datachannel-not-run",
];

function stringLiteralValues(source) {
  const values = [];
  const pattern = /(["'`])((?:\\.|(?!\1)[\s\S])*?)\1/g;
  for (const match of source.matchAll(pattern)) {
    values.push({ raw: match[0], value: match[2], index: match.index ?? 0 });
  }
  return values;
}

function lineAndColumn(source, index) {
  const prefix = source.slice(0, index);
  const lines = prefix.split("\n");
  return { line: lines.length, column: lines[lines.length - 1].length + 1 };
}

function hasProductionClaim(value, term) {
  if (term === "P2P") return /\bP2P\b/.test(value);
  return value.toLowerCase().includes(term.toLowerCase());
}

function hasAllowedProductionClaimContext(value) {
  return allowedProductionClaimContext.some((context) =>
    value.toLowerCase().includes(context.toLowerCase()),
  );
}

for (const [label, source] of [
  ["apps/ui/src/main.tsx", main],
  ["apps/ui/src/commands.ts", commands],
]) {
  for (const literal of stringLiteralValues(source)) {
    const terms = productionClaimTerms.filter((term) =>
      hasProductionClaim(literal.value, term),
    );
    if (terms.length === 0) continue;
    if (hasAllowedProductionClaimContext(literal.value)) continue;
    const { line, column } = lineAndColumn(source, literal.index);
    failures.push(
      `${label}:${line}:${column} production claim token(s) ${terms.join(
        ", ",
      )} lack backend-state/local-only/release-gated context: ${literal.raw.slice(
        0,
        120,
      )}`,
    );
  }
}

if (!rust.includes("honest_copy_ready")) {
  failures.push(
    "Tauri command_health must expose honest_copy_ready for copy gates",
  );
}
if (!commands.includes("honest_copy_ready")) {
  failures.push("TS CommandHealth must carry honest_copy_ready for copy gates");
}
if (!rust.includes("transport_status: Vec<TransportStatusView>")) {
  failures.push(
    "Tauri AppStateView must expose backend-derived transport_status",
  );
}
if (!commands.includes("transport_status: TransportStatusView[]")) {
  failures.push("TS AppState must carry backend-derived transport_status");
}
if (!rust.includes("join_progress: Vec<JoinProgressStepView>")) {
  failures.push("Tauri AppStateView must expose backend-derived join_progress");
}
if (!commands.includes("join_progress: JoinProgressStepView[]")) {
  failures.push("TS AppState must carry backend-derived join_progress");
}
if (!main.includes("JoinProgressCard")) {
  failures.push("UI must render backend-derived group join progress");
}
if (!rust.includes("text_state_legend: Vec<TextStateView>")) {
  failures.push("Tauri AppStateView must expose text_state_legend");
}
if (!commands.includes("text_state_legend: TextStateView[]")) {
  failures.push("TS AppState must carry text_state_legend");
}
if (!main.includes("TextStateLegend")) {
  failures.push("UI must render text message state legend");
}
if (!rust.includes("voice_states: Vec<VoiceStateView>")) {
  failures.push("Tauri AppStateView must expose backend-derived voice_states");
}
if (!commands.includes("voice_states: VoiceStateView[]")) {
  failures.push("TS AppState must carry backend-derived voice_states");
}
if (!main.includes("VoiceStateGrid")) {
  failures.push("UI must render backend-derived voice state grid");
}
if (!main.includes("voiceStateBadgeVariant")) {
  failures.push("voice UI must map backend voice states to visible badges");
}
if (!rust.includes("runtime_mode: RuntimeModeView")) {
  failures.push(
    "Tauri AppStateView must expose runtime_mode for production label gating",
  );
}
if (!commands.includes("runtime_mode: RuntimeModeView")) {
  failures.push(
    "TS AppState must carry runtime_mode for production label gating",
  );
}
if (!main.includes("RuntimeModeBanner")) {
  failures.push("UI must visibly mark local-dev/harness runtime mode");
}
if (!rust.includes("UI_THEME_IDS") || !rust.includes("UI_TEMPLATE_IDS")) {
  failures.push(
    "backend preferences must constrain theme/template IDs to app-config-compatible allowlists",
  );
}
if (
  !commands.includes("discryptUiConfig.themes") ||
  !commands.includes("discryptUiConfig.templates")
) {
  failures.push(
    "frontend preferences must normalize through apps/ui/src/app-config.ts definitions",
  );
}
if (
  !main.includes("discryptUiConfig.themes") ||
  !main.includes("savePreferences")
) {
  failures.push(
    "UI must keep theme/template selectors sourced from app-config and persisted through savePreferences",
  );
}
if (!main.includes("runtimeMode.production_labels_enabled")) {
  failures.push(
    "UI must gate production label badge state from runtimeMode.production_labels_enabled",
  );
}
if (
  !main.includes("message.state_label") ||
  !main.includes("message.state_detail")
) {
  failures.push(
    "Message bubbles must display per-message state label and detail",
  );
}
if (!main.includes("Group join progress")) {
  failures.push(
    "join UI must label the command-backed group join progress timeline",
  );
}
if (!main.includes("evidence-gated by command state")) {
  failures.push(
    "join progress UI must explain progress requires command-state evidence",
  );
}
if (!main.includes("TransportStatusStrip")) {
  failures.push("UI must render backend-derived transport statuses");
}
if (!main.includes("Backend-derived state only")) {
  failures.push(
    "transport status UI must explain that connectivity claims require backend evidence",
  );
}
if (
  main.includes('id="runtime-local-peer"') ||
  main.includes('id="runtime-remote-peer"')
) {
  failures.push(
    "production UI must not expose manual text runtime peer pairing inputs",
  );
}
if (
  !main.includes("not user-entered pairing fields") ||
  !main.includes("ensureTextRuntimeForActiveScope")
) {
  failures.push(
    "text runtime attachment must derive peers from invite/connectivity state and start automatically",
  );
}
if (!main.includes("messageTransportProof || Boolean(window.__TAURI__?.core?.invoke)")) {
  failures.push("native/Tauri text sends must request transport proof automatically without manual pairing controls");
}
if (!main.includes('tauriListen<AppEventStreamView>("app_event"')) {
  failures.push("native/Tauri UI must subscribe to app_event push events");
}
if (!main.includes("tauriListen ? 30000 : 5000")) {
  failures.push("poll_app_events must be fallback/health-resync only when app_event push is available");
}
if (!main.includes("window.__TAURI__?.event?.listen")) {
  failures.push("native UI must install a Tauri app_event listener");
}
if (
  !main.includes("APP_EVENT_FALLBACK_POLL_MS") ||
  !main.includes("startFallbackPolling")
) {
  failures.push("poll_app_events must remain a named fallback path, not the primary native update path");
}
if (!main.includes("APP_EVENT_HEALTH_RESYNC_MS")) {
  failures.push("native app_event listener must retain a slow health-resync poll");
}
const voiceCleanupEffects = (
  main.match(/return \(\) => \{\n      voiceCaptureRef\.current\?\.getTracks\(\)\.forEach/g) ?? []
).length;
if (voiceCleanupEffects !== 1) {
  failures.push(
    `voice media unmount cleanup must have exactly one effect, found ${voiceCleanupEffects}`,
  );
}
for (const forbiddenManualTextControl of [
  "runtime-local-peer",
  "runtime-remote-peer",
  "Listen as answerer",
  "Connect as offerer",
]) {
  if (main.includes(forbiddenManualTextControl)) {
    failures.push(
      `production UI must not expose manual text runtime pairing controls (${forbiddenManualTextControl})`,
    );
  }
}
if (main.includes("id=\"runtime-local-peer\"") || main.includes("id=\"runtime-remote-peer\"")) {
  failures.push("production UI must not expose manual text runtime peer pairing inputs");
}
if (!main.includes("not user-entered pairing fields") || !main.includes("ensureTextRuntimeForActiveScope")) {
  failures.push("text runtime attachment must derive peers from invite/connectivity state and start automatically");
}
for (const inviteUiToken of [
  "Latest invite descriptor",
  "Signaling endpoint",
  "Revocation status",
  "Password-gate status",
  "MLS admission state",
  "Max-use limit",
]) {
  if (!main.includes(inviteUiToken)) {
    failures.push(
      `invite UI missing production metadata surface: ${inviteUiToken}`,
    );
  }
}
if (
  !main.includes("Danger zone") ||
  !main.includes("resetPhrase !== RESET_APP_CONFIRMATION_PHRASE")
) {
  failures.push(
    "UI must gate destructive reset behind the typed danger-zone confirmation phrase",
  );
}
for (const e2eToken of [
  "setup workflow remains readable and completes",
  "group invite join text channel and voice controls work without fake members",
  "local-dev e2e persistence survives browser reload",
  "small-window navigation exposes topbar controls without overflow",
  "production UX hides diagnostics and manual transport controls by default",
  "mediaDevices",
  'toHaveValue("61")',
]) {
  if (!statefulE2e.includes(e2eToken)) {
    failures.push(`Playwright stateful UX coverage missing: ${e2eToken}`);
  }
}
if (
  !commands.includes("FALLBACK_STORAGE_KEY") ||
  !commands.includes("persistFallbackState")
) {
  failures.push(
    "local-dev fallback must persist command-backed state for reload UX E2E coverage",
  );
}

const commandBackedCopy = [
  "command-backed",
  "backend media-route evidence",
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

export type ChannelKind = "Text" | "Voice";

export type FriendView = {
  alias: string;
  friend_code: string;
  safety_number: string;
  verified: boolean;
};

export type DeviceView = {
  device_id: string;
  label: string;
  leaf_index: number;
  identity_key: string;
  device_key: string;
  local: boolean;
  authorized: boolean;
  revoked: boolean;
  added_at_epoch: number;
  revoked_at_epoch: number | null;
};

export type ChannelView = {
  name: string;
  kind: ChannelKind;
  retention_status: string;
};

export type ServerView = {
  name: string;
  role: string;
  channels: ChannelView[];
};

export type InviteFlowView = {
  expires: string;
  max_use: string;
  password_gate: string;
  welcome_required: string;
};

export type RetentionSettingsView = {
  presets: string[];
  selected: string;
  unlimited_warning: string;
  transition_copy: string;
};

export type VoiceRoomView = {
  route: string;
  relay_copy: string;
  android_path: string;
};

export type VoiceParticipantView = {
  id: string;
  name: string;
  role: string;
  speaking: boolean;
  muted: boolean;
  volume: number;
};

export type VoiceDeviceKind = "audio_input" | "audio_output";

export type VoiceDeviceDescriptor = {
  device_id: string;
  label: string;
  kind: VoiceDeviceKind;
};

export type SnapshotVoiceSessionView = {
  joined: boolean;
  microphone_permission: string;
  input_device: VoiceDeviceDescriptor | null;
  output_device: VoiceDeviceDescriptor | null;
  participants: VoiceParticipantView[];
  status_copy: string;
  route_copy: string;
  permission_denied_copy: string;
};

export type PreferencesView = {
  theme_id: string;
  template_id: string;
};

export type MessageView = {
  id: string;
  channel: string;
  author: string;
  body: string;
  state: string;
};

export type ConnectivityView = {
  fallback_chain: string;
  metadata_copy: string;
  push_copy: string;
};

export type SecurityCopyView = {
  metadata: string;
  deletion: string;
  malicious_member: string;
};

export type AppSnapshot = {
  schema_version: number;
  friend: FriendView;
  devices: DeviceView[];
  servers: ServerView[];
  invite: InviteFlowView;
  retention: RetentionSettingsView;
  voice: VoiceRoomView;
  voice_session: SnapshotVoiceSessionView;
  preferences: PreferencesView;
  messages: MessageView[];
  activity_feed: string[];
  connectivity: ConnectivityView;
  security_copy: SecurityCopyView;
};

export type AppLifecycle = "first_run" | "ready";

export type UserProfileView = {
  user_id: string;
  display_name: string;
  device_name: string;
  recovery_status: string;
};

export type DirectConversationView = {
  dm_id: string;
  participant_id: string;
  display_name: string;
  local_only_copy: string;
};

export type ChannelStateView = {
  channel_id: string;
  name: string;
  kind: ChannelKind;
  retention_status: string;
};

export type GroupView = {
  group_id: string;
  name: string;
  role: string;
  channels: ChannelStateView[];
};

export type ActiveContextView = {
  kind: "dm" | "text_channel" | "voice_channel" | "group" | string;
  group_id: string | null;
  channel_id: string | null;
  dm_id: string | null;
};

export type MessageTargetView = {
  kind: "dm" | "channel" | string;
  dm_id: string | null;
  group_id: string | null;
  channel_id: string | null;
};

export type AppMessageView = {
  message_id: string;
  target: MessageTargetView;
  author_id: string;
  author: string;
  body: string;
  status: string;
  sent_at: string;
};

export type IceTurnServerView = {
  endpoint: string;
  credential_declared: boolean;
  credential_expires_at: string | null;
};

export type TransportStatusView = {
  label: string;
  status: string;
  detail: string;
};

export type JoinProgressStepView = {
  key: string;
  label: string;
  status: string;
  detail: string;
};

export type InviteView = {
  invite_id: string;
  invite_key: string;
  group_id: string;
  code: string;
  room_secret_hash: string;
  signaling_endpoint: string;
  signaling_trust_fingerprint: string;
  signaling_trust_status: string;
  endpoint_policy: string;
  ice_stun_servers: string[];
  ice_turn_servers: IceTurnServerView[];
  expires: string;
  expires_at: string;
  max_use: string;
  uses: number;
  revoked: boolean;
  admission_copy: string;
};

export type VoiceSessionView = {
  session_id: string;
  group_id: string;
  channel_id: string;
  joined: boolean;
  self_muted: boolean;
  microphone_permission: string;
  input_device: VoiceDeviceDescriptor | null;
  output_device: VoiceDeviceDescriptor | null;
  participants: VoiceParticipantView[];
  route_copy: string;
  status_copy: string;
  permission_denied_copy: string;
};

export type AppEventView = {
  sequence: number;
  kind: string;
  summary: string;
};

export type CommandErrorView = {
  code: string;
  command: string;
  message: string;
  recovery_hint: string;
};

export type PollAppEventsRequest = {
  after?: number | null;
  kinds?: string[];
  limit?: number | null;
};

export type AppEventStreamView = {
  events: AppEventView[];
  cursor: number;
  next_cursor: number;
  has_more: boolean;
  subscribed_kinds: string[];
};

export type AppState = {
  schema_version: number;
  lifecycle: AppLifecycle;
  profile: UserProfileView | null;
  preferences: PreferencesView;
  dms: DirectConversationView[];
  groups: GroupView[];
  active_context: ActiveContextView | null;
  messages: AppMessageView[];
  voice_session: VoiceSessionView | null;
  invites: InviteView[];
  devices: DeviceView[];
  security_copy: SecurityCopyView;
  events: AppEventView[];
  event_cursor: number;
  last_command_error: CommandErrorView | null;
  transport_status: TransportStatusView[];
  join_progress: JoinProgressStepView[];
  snapshot: AppSnapshot;
};

export type SafetyVerificationRequest = {
  friend_id: string;
  provided: string;
};

export type SafetyVerificationResult = {
  verified: boolean;
  message: string;
};

export type CreateUserRequest = {
  display_name: string;
  device_name?: string | null;
};

export type RecoverUserRequest = {
  display_name: string;
  recovery_code: string;
  device_name?: string | null;
  recovery_room_memberships?: string[];
  recovered_device_count?: number | null;
  use_sealed_account_backup?: boolean;
};

export type CreateDevicePairingPayloadRequest = {
  requested_label: string;
  current_epoch?: number | null;
  valid_for_epochs?: number | null;
};

export type AcceptDevicePairingPayloadRequest = {
  payload: string;
  device_name?: string | null;
  current_epoch?: number | null;
};

export type DevicePairingPayloadView = {
  payload: string;
  authorizing_device_id: string;
  requested_label: string;
  expires_epoch: number;
  rejected_reason: string | null;
};

export type CreateGroupRequest = {
  name: string;
  retention: string;
};

export type JoinGroupRequest = {
  invite_code: string;
  group_name?: string | null;
};

export type SetActiveGroupRequest = {
  group_id: string;
};

export type CreateInviteRequest = {
  group_id?: string | null;
  expires: string;
  max_use: string;
};

export type CreateChannelRequest = {
  group_id: string;
  name: string;
  kind: ChannelKind;
  retention_status: string;
};

export type SavePreferencesRequest = {
  theme_id: string;
  template_id: string;
};

export type StartDmRequest = {
  display_name: string;
};

export type SendMessageRequest = {
  target: MessageTargetView;
  body: string;
};

export type JoinVoiceRequest = {
  group_id: string;
  channel_id: string;
  microphone_permission: "granted" | "denied" | "prompt" | "unknown";
  input_device_id?: string | null;
  input_device_label?: string | null;
  output_device_id?: string | null;
  output_device_label?: string | null;
};

export type LeaveVoiceRequest = {
  session_id: string;
};

export type SelfMuteRequest = {
  session_id: string;
  muted: boolean;
};

export type SpeakerVolumeRequest = {
  session_id: string;
  participant_id: string;
  volume: number;
};

export const RESET_APP_CONFIRMATION_PHRASE = "DELETE LOCAL DISCRYPT STATE";

export type ResetAppStateRequest = {
  confirmation: string;
};

export type CommandHealth = {
  snapshot_ready: boolean;
  verification_ready: boolean;
  app_state_ready: boolean;
  identity_ready: boolean;
  collaboration_ready: boolean;
  voice_ready: boolean;
  honest_copy_ready: boolean;
};

type TauriInvoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;

const LOCAL_DEV_FALLBACK_ENABLED =
  import.meta.env.DEV ||
  import.meta.env.VITE_DISCRYPT_LOCAL_DEV_FALLBACK === "1";

const fallbackFriendIdentity = createFallbackFriendIdentity("New contact");

declare global {
  interface Window {
    __TAURI__?: {
      core?: {
        invoke?: TauriInvoke;
      };
    };
  }
}

const fallbackSnapshot: AppSnapshot = {
  schema_version: 1,
  friend: {
    alias: fallbackFriendIdentity.alias,
    friend_code: fallbackFriendIdentity.friendCode,
    safety_number: fallbackFriendIdentity.safetyNumber,
    verified: false,
  },
  devices: [],
  servers: [],
  invite: {
    expires: "Invite expires and can be revoked",
    max_use: "Max-use is enforced before MLS admission",
    password_gate:
      "Password rooms use OPAQUE/PAKE or an online authorized helper; no offline verifier",
    welcome_required:
      "Final admission still requires an authorized MLS Welcome/add",
  },
  retention: {
    presets: [
      "1 hour",
      "24 hours",
      "7 days",
      "30 days",
      "90 days",
      "custom",
      "warned unlimited / never-lock",
    ],
    selected: "7 days",
    unlimited_warning:
      "Unlimited keeps local keys longer and weakens lock behavior; opt in explicitly",
    transition_copy:
      "Shortening re-locks older messages retroactively; lengthening applies only to future messages",
  },
  voice: {
    route:
      "Local voice controls only; network media route is not connected in this build",
    relay_copy:
      "No relay is active in the desktop harness until real media/socket E2E gates pass",
    android_path:
      "Android media routing remains release-gated until platform E2E passes",
  },
  voice_session: {
    joined: false,
    microphone_permission: "unknown",
    input_device: null,
    output_device: null,
    participants: [],
    status_copy: "Not joined; command-backed local voice controls are idle",
    route_copy:
      "Route copy is harness-backed until socket/media adapter E2E passes",
    permission_denied_copy: "",
  },
  preferences: { theme_id: "graphite-calm", template_id: "command-center" },
  messages: [],
  activity_feed: [
    "Demo fallback active: packaged Tauri builds must use IPC-backed commands",
    "Recovery restores account continuity only; no content-key recovery claim",
    "Deletion copy includes offline-device caveat",
  ],
  connectivity: {
    fallback_chain:
      "Command-backed policy: STUN → relay-overlay → TURN; runtime transport remains release-gated until E2E passes",
    metadata_copy:
      "Content-private and metadata-minimizing, not metadata-anonymous",
    push_copy:
      "Android FCM wake is content-free and carries no room, sender, or message body",
  },
  security_copy: {
    metadata:
      "Passive infrastructure can see IPs and timing; discrypt does not claim anonymity",
    deletion:
      "Deleted on your online devices now; pending on offline devices until they reconnect",
    malicious_member:
      "Crypto-shred cannot erase screenshots, exports, modified clients, or plaintext already saved by a recipient",
  },
};

const fallbackState: AppState = {
  schema_version: 1,
  lifecycle: "first_run",
  profile: null,
  preferences: fallbackSnapshot.preferences,
  dms: [],
  groups: [],
  active_context: null,
  messages: [],
  voice_session: null,
  invites: [],
  devices: [],
  security_copy: fallbackSnapshot.security_copy,
  events: [
    {
      sequence: 1,
      kind: "app.first_run",
      summary: "No local profile exists; setup/recovery is required",
    },
  ],
  event_cursor: 1,
  last_command_error: null,
  transport_status: [],
  join_progress: [],
  snapshot: fallbackSnapshot,
};

function cloneState(state: AppState): AppState {
  return structuredClone(state);
}

function syncSnapshot(state: AppState): AppState {
  state.snapshot.schema_version = state.schema_version;
  state.snapshot.preferences = state.preferences;
  state.snapshot.devices = state.devices;
  state.snapshot.security_copy = state.security_copy;
  state.snapshot.servers = state.groups.map((group) => ({
    name: group.name,
    role: group.role,
    channels: group.channels.map((channel) => ({
      name: channel.name,
      kind: channel.kind,
      retention_status: channel.retention_status,
    })),
  }));
  state.snapshot.messages = state.messages.map((message) => ({
    id: message.message_id,
    channel:
      state.groups
        .flatMap((group) => group.channels)
        .find((channel) => channel.channel_id === message.target.channel_id)
        ?.name ??
      message.target.dm_id ??
      "#general",
    author: message.author,
    body: message.body,
    state: message.status,
  }));
  state.snapshot.voice_session = state.voice_session
    ? {
        joined: state.voice_session.joined,
        microphone_permission: state.voice_session.microphone_permission,
        input_device: state.voice_session.input_device,
        output_device: state.voice_session.output_device,
        participants: state.voice_session.participants,
        status_copy: state.voice_session.status_copy,
        route_copy: state.voice_session.route_copy,
        permission_denied_copy: state.voice_session.permission_denied_copy,
      }
    : {
        joined: false,
        microphone_permission: "unknown",
        input_device: null,
        output_device: null,
        participants: [],
        status_copy: "Not joined; command-backed local voice controls are idle",
        route_copy:
          "Local voice controls only; network media route is not connected in this build",
        permission_denied_copy: "",
      };
  state.snapshot.activity_feed = state.events
    .slice()
    .reverse()
    .map((event) => event.summary);
  state.event_cursor = state.events.at(-1)?.sequence ?? 0;
  state.transport_status = deriveTransportStatus(state);
  state.join_progress = deriveJoinProgress(state);
  return state;
}

function deriveJoinProgress(state: AppState): JoinProgressStepView[] {
  const latestInvite = state.invites.at(-1) ?? null;
  const hasInvite = Boolean(latestInvite);
  const openedFromInvite = state.events.some(
    (event) =>
      event.kind === "group.joined" || event.kind === "group.opened_from_invite",
  );
  const hasActiveGroup = Boolean(state.active_context?.group_id);
  const voiceJoined = Boolean(state.voice_session?.joined);
  return [
    {
      key: "invite_parsed",
      label: "Invite parsed",
      status: hasInvite ? "complete" : "waiting-for-invite",
      detail: latestInvite
        ? `Invite ${latestInvite.invite_key} parsed with signaling endpoint ${latestInvite.signaling_endpoint}`
        : "Paste or create an invite before join progress can start",
    },
    {
      key: "rendezvous",
      label: "Rendezvous link",
      status: hasInvite ? "waiting-for-backend-event" : "blocked",
      detail:
        "Rendezvous connected is marked only when backend state reports an authenticated publish/take exchange",
    },
    {
      key: "authorized_member",
      label: "Authorized member",
      status: hasInvite ? "waiting-for-authorized-member" : "blocked",
      detail:
        "Waiting for an authorized member or helper to approve admission; the invite link alone is insufficient",
    },
    {
      key: "welcome",
      label: "Welcome package",
      status: openedFromInvite ? "local-admission-recorded" : "pending-welcome",
      detail:
        "Welcome received becomes complete only after backend state records a verified MLS Welcome/add",
    },
    {
      key: "mls_joined",
      label: "MLS group state",
      status: hasActiveGroup ? "local-group-open" : "pending-mls-proof",
      detail:
        "MLS joined requires command state for the active group plus epoch/member verification",
    },
    {
      key: "transport",
      label: "Transport route",
      status: voiceJoined ? "media-gated" : "waiting-route-proof",
      detail:
        "Transport connected is shown only after backend state provides direct, overlay, or TURN route evidence",
    },
  ];
}

function deriveTransportStatus(state: AppState): TransportStatusView[] {
  const latestInvite = state.invites.at(-1) ?? null;
  const hasGroup = state.groups.length > 0;
  const voiceJoined = Boolean(state.voice_session?.joined);
  const hasStun = Boolean(latestInvite?.ice_stun_servers.length);
  const hasTurn = Boolean(latestInvite?.ice_turn_servers.length);
  const lastError = state.last_command_error;
  return [
    {
      label: "signaling",
      status: latestInvite ? "signed-endpoint-ready" : "waiting-for-invite",
      detail: latestInvite
        ? `Signed endpoint ${latestInvite.signaling_endpoint} with trust fingerprint ${latestInvite.signaling_trust_fingerprint}; no identity-room topology is stored by the signaling service`
        : "Create or paste an invite before signaling can be used",
    },
    {
      label: "ICE",
      status: hasStun || hasTurn ? "configured" : "waiting-for-signed-invite",
      detail: latestInvite
        ? `${latestInvite.ice_stun_servers.length} STUN and ${latestInvite.ice_turn_servers.length} redacted TURN endpoint(s) parsed from signed invite metadata`
        : "No ICE server metadata is available until an invite descriptor is present",
    },
    {
      label: "direct",
      status: voiceJoined ? "media-gated" : "no-direct-proof",
      detail:
        "Direct path is only shown as connected after backend state proves it; this command state has no direct route proof yet",
    },
    {
      label: "overlay",
      status: hasGroup ? "available-policy" : "idle",
      detail:
        "Relay-overlay policy is listed as a fallback path; ciphertext-only route proof is required before claiming active relay use",
    },
    {
      label: "TURN",
      status: hasTurn ? "configured" : "not-configured",
      detail:
        "TURN endpoints are redacted from signed invite metadata and are not treated as active without backend route evidence",
    },
    {
      label: "degraded",
      status: lastError ? "attention" : "clear",
      detail: lastError
        ? `Last command issue ${lastError.code}: ${lastError.message}`
        : "No degraded command state is currently reported",
    },
    {
      label: "reconnecting",
      status: "idle",
      detail:
        "Reconnect orchestration is displayed only when event state reports reconnect attempts",
    },
    {
      label: "failed",
      status: lastError ? "last-command-error" : "clear",
      detail: lastError?.recovery_hint ??
        "No failed transport command is currently reported",
    },
  ];
}

function pushEvent(state: AppState, kind: string, summary: string): void {
  const lastSequence = state.events.at(-1)?.sequence ?? 0;
  state.events.push({ sequence: lastSequence + 1, kind, summary });
  state.event_cursor = lastSequence + 1;
}

function slugify(value: string): string {
  return (
    value
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9_-]+/g, "-")
      .replace(/^-+|-+$/g, "") || "local"
  );
}

function defaultGroupChannels(): ChannelStateView[] {
  return [
    {
      channel_id: "channel-general",
      name: "#general",
      kind: "Text",
      retention_status: "7 days",
    },
    {
      channel_id: "channel-voice-lobby",
      name: "Voice Lobby",
      kind: "Voice",
      retention_status: "session",
    },
  ];
}

function stableHash(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return Array.from({ length: 8 }, (_, index) =>
    ((hash >>> ((index % 4) * 8)) & 0xff).toString(16).padStart(2, "0"),
  )
    .join("")
    .repeat(4);
}

function randomHex(bytes: number): string {
  const buffer = new Uint8Array(bytes);
  globalThis.crypto?.getRandomValues(buffer);
  return Array.from(buffer, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

function groupSafetyHash(value: string): string {
  return stableHash(value)
    .match(/.{1,4}/g)
    ?.slice(0, 12)
    .join(" ") ?? "0000 0000 0000 0000";
}

function createFallbackFriendIdentity(alias: string): {
  alias: string;
  friendCode: string;
  safetyNumber: string;
} {
  const publicKey = randomHex(32);
  const fingerprint = stableHash(publicKey).slice(0, 20);
  return {
    alias,
    friendCode: `discrypt://friend/v1/${slugify(alias)}?ik=${publicKey}&fp=${fingerprint}`,
    safetyNumber: groupSafetyHash(`local-dev-fallback:${publicKey}`),
  };
}

function fallbackIdentityKey(): string {
  return (
    fallbackState.snapshot.friend.friend_code
      .split("ik=")
      .at(1)
      ?.split("&")
      .at(0) ?? stableHash(fallbackState.snapshot.friend.friend_code)
  );
}

function canonicalPairingMessage(payload: {
  version: number;
  authorizing_device_id: string;
  identity_key: string;
  requested_label: string;
  challenge: string;
  expires_epoch: number;
}): string {
  return `discrypt-device-pairing-v${payload.version}|authorizer=${payload.authorizing_device_id}|identity=${payload.identity_key}|label=${payload.requested_label}|challenge=${payload.challenge}|expires_epoch=${payload.expires_epoch}`;
}

function fallbackPairingSignature(payload: {
  version: number;
  authorizing_device_id: string;
  identity_key: string;
  requested_label: string;
  challenge: string;
  expires_epoch: number;
}): string {
  return stableHash(
    `${canonicalPairingMessage(payload)}|${fallbackState.snapshot.friend.friend_code}`,
  );
}

function parseFallbackPairingPayload(payload: string):
  | {
      version: number;
      authorizing_device_id: string;
      identity_key: string;
      requested_label: string;
      challenge: string;
      expires_epoch: number;
      signature: string;
    }
  | null {
  try {
    const parsed = JSON.parse(payload) as Record<string, unknown>;
    if (
      typeof parsed.version !== "number" ||
      typeof parsed.authorizing_device_id !== "string" ||
      typeof parsed.identity_key !== "string" ||
      typeof parsed.requested_label !== "string" ||
      typeof parsed.challenge !== "string" ||
      typeof parsed.expires_epoch !== "number" ||
      typeof parsed.signature !== "string"
    ) {
      return null;
    }
    return {
      version: parsed.version,
      authorizing_device_id: parsed.authorizing_device_id,
      identity_key: parsed.identity_key,
      requested_label: parsed.requested_label,
      challenge: parsed.challenge,
      expires_epoch: parsed.expires_epoch,
      signature: parsed.signature,
    };
  } catch {
    return null;
  }
}

function participantIdFromFriendCode(friendCode: string): string {
  const fingerprint = friendCode.split("&fp=").at(1)?.split("&").at(0);
  return `friend-${(fingerprint ?? stableHash(friendCode)).slice(0, 10)}`;
}

function localUserId(state: AppState): string {
  return state.profile?.user_id ?? "local-profile-pending";
}

function inviteExpirationHorizon(label: string): string {
  const lower = label.toLowerCase();
  const now = Date.now();
  const millis =
    lower.includes("hour") || lower.includes("1 h")
      ? 60 * 60 * 1000
      : lower.includes("day") || lower.includes("24") || lower.includes("1 d")
        ? 24 * 60 * 60 * 1000
        : lower.includes("30")
          ? 30 * 24 * 60 * 60 * 1000
          : lower.includes("90")
            ? 90 * 24 * 60 * 60 * 1000
            : 7 * 24 * 60 * 60 * 1000;
  return new Date(now + millis).toISOString();
}

function parseMaxUses(label: string): number {
  const match = label.match(/\d+/);
  return match ? Number(match[0]) || 5 : 5;
}

function parseInviteGroupName(inviteCode: string): string {
  const tail = inviteCode.trim().split("/").filter(Boolean).at(-1) ?? "";
  const name = tail.includes("-")
    ? tail.slice(tail.indexOf("-") + 1).replace(/-/g, " ")
    : "joined group";
  return name.trim() || "joined group";
}

type ParsedInviteMetadata = {
  inviteKey: string;
  roomSecretHash: string;
  signalingEndpoint: string;
  signalingTrustFingerprint: string;
  signalingTrustStatus: string;
  endpointPolicy: string;
  iceStunServers: string[];
  iceTurnServers: IceTurnServerView[];
  expiresAt: string;
  maxUses: number;
};

function defaultSignalingEndpoint(): string {
  return "https://signaling.discrypt.invalid/v1/rendezvous";
}

function defaultIceStunServers(): string[] {
  return ["stun:default.discrypt.invalid:3478"];
}

function defaultRedactedTurnServers(): IceTurnServerView[] {
  return [
    {
      endpoint: "turns:default.discrypt.invalid:5349",
      credential_declared: false,
      credential_expires_at: null,
    },
  ];
}

function productionInviteLink(metadata: ParsedInviteMetadata): string {
  const query = new URLSearchParams({
    endpoint: metadata.signalingEndpoint,
    policy: metadata.endpointPolicy,
    trust_fp: metadata.signalingTrustFingerprint,
    trust: metadata.signalingTrustStatus,
    commitment: metadata.roomSecretHash,
    exp: metadata.expiresAt,
    max: String(metadata.maxUses),
  });
  for (const endpoint of metadata.iceStunServers) {
    query.append("stun", endpoint);
  }
  for (const server of metadata.iceTurnServers) {
    query.append("turn", server.endpoint);
  }
  return `discrypt://join/v1/${metadata.inviteKey}?${query.toString()}`;
}

function parseInviteMetadata(inviteCode: string): ParsedInviteMetadata | null {
  const trimmed = inviteCode.trim();
  const [path, query = ""] = trimmed.split("?", 2);
  if (!path.startsWith("discrypt://join/v1/") || !query) return null;
  const inviteKey = path.split("/").filter(Boolean).at(-1);
  if (!inviteKey) return null;
  const params = new URLSearchParams(query);
  const signalingEndpoint = params.get("endpoint") ?? "";
  const endpointPolicy = params.get("policy") ?? "";
  const signalingTrustFingerprint = params.get("trust_fp") ?? "";
  const signalingTrustStatus = params.get("trust") ?? "";
  if (
    !signalingEndpoint ||
    !endpointPolicy ||
    !/^[a-fA-F0-9]{64}$/.test(signalingTrustFingerprint) ||
    !signalingTrustStatus
  ) {
    return null;
  }
  return {
    inviteKey,
    roomSecretHash: params.get("commitment") ?? "",
    signalingEndpoint,
    signalingTrustFingerprint,
    signalingTrustStatus,
    endpointPolicy,
    iceStunServers: params.getAll("stun"),
    iceTurnServers: params.getAll("turn").map((endpoint) => ({
      endpoint,
      credential_declared: true,
      credential_expires_at: null,
    })),
    expiresAt: params.get("exp") ?? "",
    maxUses: Number(params.get("max") ?? 1) || 1,
  };
}

function invokeOrFallback<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  fallback: () => T,
): Promise<T> {
  const tauriInvoke = window.__TAURI__?.core?.invoke;
  if (!tauriInvoke) {
    if (!LOCAL_DEV_FALLBACK_ENABLED) {
      return Promise.reject(
        new Error(
          `Tauri IPC unavailable for ${command}; local fallback requires VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1 in a local-dev/test harness`,
        ),
      );
    }
    return Promise.resolve(fallback());
  }
  return tauriInvoke<T>(command, args);
}

function mutateFallback(update: (state: AppState) => void): AppState {
  fallbackState.last_command_error = null;
  update(fallbackState);
  return cloneState(syncSnapshot(fallbackState));
}

export function commandErrorToAction(error: CommandErrorView | null): string {
  return error?.recovery_hint ?? "";
}

function pushCommandError(
  state: AppState,
  eventKind: string,
  command: string,
  code: string,
  message: string,
  recoveryHint: string,
): void {
  state.last_command_error = {
    code,
    command,
    message,
    recovery_hint: recoveryHint,
  };
  pushEvent(state, eventKind, `${code}: ${message}`);
}

function ensureFallbackReady(
  displayName = "Alice",
  deviceName = "Desktop",
  recovered = false,
): void {
  if (fallbackState.lifecycle === "ready") {
    if (recovered && fallbackState.profile) {
      fallbackState.profile.recovery_status =
        "Account continuity restored; content keys restored: false";
    }
    return;
  }
  fallbackState.lifecycle = "ready";
  fallbackState.profile = {
    user_id: `user-${slugify(displayName)}`,
    display_name: displayName,
    device_name: deviceName,
    recovery_status: recovered
      ? "Account-continuity recovery accepted with verified local identity material; content keys restored: false"
      : "New local identity generated from command signing material; recovery export is account-continuity only",
  };
  fallbackState.devices = [
    {
      device_id: slugify(deviceName),
      label: deviceName,
      leaf_index: 1,
      identity_key: stableHash(`${displayName}:account-key`),
      device_key: randomHex(32),
      local: true,
      authorized: true,
      revoked: false,
      added_at_epoch: 1,
      revoked_at_epoch: null,
    },
  ];
  const dmId = `dm-${slugify(fallbackState.snapshot.friend.friend_code).slice(0, 24)}`;
  fallbackState.dms = [
    {
      dm_id: dmId,
      participant_id: participantIdFromFriendCode(
        fallbackState.snapshot.friend.friend_code,
      ),
      display_name: fallbackState.snapshot.friend.alias,
      local_only_copy:
        "Local DM seeded from a generated friend-code/QR payload; no remote delivery is claimed",
    },
  ];
  fallbackState.active_context = {
    kind: "dm",
    group_id: null,
    channel_id: null,
    dm_id: dmId,
  };
  pushEvent(
    fallbackState,
    recovered ? "identity.recovered" : "identity.created",
    `Profile ready for ${displayName}`,
  );
}

function applyFallbackAccountRecovery(request: RecoverUserRequest): void {
  const rooms = [...new Set(request.recovery_room_memberships ?? [])]
    .map((room) => room.trim())
    .filter(Boolean);
  const deviceCount = Math.max(1, Math.floor(request.recovered_device_count ?? 1));
  if (fallbackState.profile) {
    fallbackState.profile.recovery_status = `Account continuity restored for ${rooms.length} room(s) and ${deviceCount} device(s); content keys restored: false`;
  }
  const localDevice = fallbackState.devices[0] ?? {
    device_id: slugify(request.device_name ?? "Desktop"),
    label: request.device_name ?? "Desktop",
    leaf_index: 1,
    identity_key: stableHash(`${request.display_name}:recovered-account-key`),
    device_key: randomHex(32),
    local: true,
    authorized: true,
    revoked: false,
    added_at_epoch: 1,
    revoked_at_epoch: null,
  };
  fallbackState.devices = [localDevice];
  for (let index = 2; index <= deviceCount; index += 1) {
    fallbackState.devices.push({
      device_id: `recovered-device-${index}`,
      label: `Recovered device ${index}`,
      leaf_index: index,
      identity_key: stableHash(`${request.display_name}:recovered-device:${index}`),
      device_key: randomHex(32),
      local: false,
      authorized: true,
      revoked: false,
      added_at_epoch: index,
      revoked_at_epoch: null,
    });
  }
  for (const room of rooms) {
    if (fallbackState.groups.some((group) => group.name === room)) continue;
    const groupId = `group-${slugify(room)}`;
    fallbackState.groups.push({
      group_id: groupId,
      name: room,
      role: "member",
      channels: [
        {
          channel_id: `${groupId}-general`,
          name: "#general",
          kind: "Text",
          retention_status: "7 days",
        },
        {
          channel_id: `${groupId}-voice`,
          name: "Voice Lobby",
          kind: "Voice",
          retention_status: "session",
        },
      ],
    });
  }
}

export async function loadAppState(): Promise<AppState> {
  return invokeOrFallback<AppState>("app_state", undefined, () =>
    cloneState(syncSnapshot(fallbackState)),
  );
}

export async function loadCompatibilityAppSnapshot(): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>(
    "app_snapshot",
    undefined,
    () => cloneState(syncSnapshot(fallbackState)).snapshot,
  );
}

export async function loadAppSnapshot(): Promise<AppSnapshot> {
  const state = await loadAppState();
  return state.snapshot;
}

export async function createUser(
  request: CreateUserRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("create_user", { request }, () =>
    mutateFallback((state) => {
      const displayName = request.display_name.trim() || "Alice";
      const deviceName = request.device_name?.trim() || "Desktop";
      ensureFallbackReady(displayName, deviceName);
    }),
  );
}

export async function recoverUser(
  request: RecoverUserRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("recover_user", { request }, () =>
    mutateFallback((state) => {
      const displayName = request.display_name.trim() || "Alice recovered";
      const deviceName = request.device_name?.trim() || "Desktop";
      ensureFallbackReady(displayName, deviceName);
      const recoveredDeviceCount = Math.max(
        1,
        Math.floor(request.recovered_device_count ?? (state.devices.length || 1)),
      );
      while (state.devices.length < recoveredDeviceCount) {
        const leafIndex = state.devices.length + 1;
        state.devices.push({
          device_id: `recovered-${leafIndex}-${slugify(deviceName)}`,
          label: `${deviceName} ${leafIndex}`,
          leaf_index: leafIndex,
          identity_key: fallbackIdentityKey(),
          device_key: stableHash(`recovered:${deviceName}:${leafIndex}`),
          local: false,
          authorized: true,
          revoked: false,
          added_at_epoch: leafIndex,
          revoked_at_epoch: null,
        });
      }
      for (const room of request.recovery_room_memberships ?? []) {
        const name = room.trim();
        if (!name) continue;
        const groupId = `group-${slugify(name)}`;
        if (!state.groups.some((group) => group.group_id === groupId)) {
          state.groups.push({
            group_id: groupId,
            name,
            role: "member",
            channels: defaultGroupChannels(),
          });
        }
      }
      if (state.profile) {
        state.profile.recovery_status =
          "Recovered account continuity locally; content keys restored: false";
      }
      pushEvent(
        state,
        "identity.recovered",
        `Account-continuity recovery accepted; rooms=${request.recovery_room_memberships?.length ?? 0} devices=${recoveredDeviceCount} content_keys_restored=false`,
      );
    }),
  );
}

export async function createDevicePairingPayload(
  request: CreateDevicePairingPayloadRequest,
): Promise<DevicePairingPayloadView> {
  return invokeOrFallback<DevicePairingPayloadView>(
    "create_device_pairing_payload",
    { request },
    () => {
      ensureFallbackReady();
      const authorizingDevice = fallbackState.devices.find(
        (device) => device.authorized,
      );
      const requestedLabel = request.requested_label.trim() || "paired device";
      const currentEpoch =
        request.current_epoch ?? fallbackState.events.at(-1)?.sequence ?? 1;
      const expiresEpoch =
        currentEpoch + Math.max(1, request.valid_for_epochs ?? 3);
      if (!authorizingDevice) {
        const rejected = "No authorized local device is available";
        pushEvent(fallbackState, "device.pairing_rejected", rejected);
        return {
          payload: "",
          authorizing_device_id: "",
          requested_label: requestedLabel,
          expires_epoch: currentEpoch,
          rejected_reason: rejected,
        };
      }
      const payload = {
        version: 1,
        authorizing_device_id: authorizingDevice.device_id,
        identity_key: fallbackIdentityKey(),
        requested_label: requestedLabel,
        challenge: crypto.randomUUID?.() ?? `fallback-${Date.now()}`,
        expires_epoch: expiresEpoch,
      };
      const signed = {
        ...payload,
        signature: fallbackPairingSignature(payload),
      };
      pushEvent(
        fallbackState,
        "device.pairing_payload_created",
        `Pairing payload created for ${requestedLabel}`,
      );
      return {
        payload: JSON.stringify(signed),
        authorizing_device_id: authorizingDevice.device_id,
        requested_label: requestedLabel,
        expires_epoch: expiresEpoch,
        rejected_reason: null,
      };
    },
  );
}

export async function acceptDevicePairingPayload(
  request: AcceptDevicePairingPayloadRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>(
    "accept_device_pairing_payload",
    { request },
    () =>
      mutateFallback((state) => {
        ensureFallbackReady();
        const parsed = parseFallbackPairingPayload(request.payload);
        const currentEpoch =
          request.current_epoch ?? state.events.at(-1)?.sequence ?? 1;
        const reject = (reason: string) =>
          pushEvent(state, "device.pairing_rejected", `Pairing rejected: ${reason}`);
        if (!parsed) {
          reject("invalid pairing payload");
          return;
        }
        if (parsed.identity_key !== fallbackIdentityKey()) {
          reject("pairing payload identity does not match local identity");
          return;
        }
        if (!state.devices.some((device) => device.device_id === parsed.authorizing_device_id && device.authorized)) {
          reject("authorizing device is not an active device for this identity");
          return;
        }
        if (currentEpoch > parsed.expires_epoch) {
          reject("pairing payload expired");
          return;
        }
        if (parsed.signature !== fallbackPairingSignature(parsed)) {
          reject("pairing payload signature verification failed");
          return;
        }
        const label = request.device_name?.trim() || parsed.requested_label;
        const deviceId = `paired-${slugify(label)}-${state.devices.length + 1}`;
        if (!state.devices.some((device) => device.device_id === deviceId)) {
          state.devices.push({
            device_id: deviceId,
            label,
            leaf_index: state.devices.length + 1,
            identity_key: fallbackIdentityKey(),
            device_key: stableHash(`paired:${label}:${state.devices.length + 1}`),
            local: false,
            authorized: true,
            revoked: false,
            added_at_epoch: state.events.at(-1)?.sequence ?? 1,
            revoked_at_epoch: null,
          });
        }
        pushEvent(state, "device.paired", `Authorized paired device ${label}`);
      }),
  );
}

export async function verifySafetyNumber(
  request: SafetyVerificationRequest,
): Promise<SafetyVerificationResult> {
  return invokeOrFallback<SafetyVerificationResult>(
    "verify_safety_number",
    { request },
    () => {
      const verified =
        request.friend_id === fallbackState.snapshot.friend.friend_code &&
        request.provided === fallbackState.snapshot.friend.safety_number;
      fallbackState.snapshot.friend.verified = verified;
      return {
        verified,
        message: verified
          ? "Safety number verified; MITM risk accepted by explicit user comparison"
          : "Safety number mismatch; do not trust this device or DM",
      };
    },
  );
}

export async function savePreferences(
  request: SavePreferencesRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("save_preferences", { request }, () =>
    mutateFallback((state) => {
      state.preferences = request;
      pushEvent(state, "preferences.saved", "Theme/template preferences saved");
    }),
  );
}

export async function startDm(request: StartDmRequest): Promise<AppState> {
  return invokeOrFallback<AppState>("start_dm", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const displayName =
        request.display_name.trim() || state.snapshot.friend.alias;
      const dmId = `dm-${slugify(displayName)}`;
      if (!state.dms.some((dm) => dm.dm_id === dmId)) {
        state.dms.push({
          dm_id: dmId,
          participant_id: slugify(displayName),
          display_name: displayName,
          local_only_copy:
            "Local harness-backed DM; no remote delivery is claimed",
        });
      }
      state.active_context = {
        kind: "dm",
        group_id: null,
        channel_id: null,
        dm_id: dmId,
      };
      pushEvent(state, "dm.started", `Opened local DM with ${displayName}`);
    }),
  );
}

export async function createGroup(
  request: CreateGroupRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("create_group", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const name = request.name.trim() || "private lab";
      const groupId = `group-${slugify(name)}`;
      if (!state.groups.some((group) => group.group_id === groupId)) {
        state.groups.push({
          group_id: groupId,
          name,
          role: "owner",
          channels: defaultGroupChannels(),
        });
      }
      state.active_context = {
        kind: "group",
        group_id: groupId,
        channel_id: null,
        dm_id: null,
      };
      pushEvent(state, "group.created", `Created group ${name}`);
    }),
  );
}

export async function joinGroup(request: JoinGroupRequest): Promise<AppState> {
  return invokeOrFallback<AppState>("join_group", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const inviteCode = request.invite_code.trim();
      const localInvite = state.invites.find(
        (invite) => invite.code === inviteCode,
      );
      if (localInvite) {
        state.active_context = {
          kind: "group",
          group_id: localInvite.group_id,
          channel_id: null,
          dm_id: null,
        };
        pushEvent(
          state,
          "group.opened_from_invite",
          "Opened group from local invite",
        );
        return;
      }
      const parsedInvite = parseInviteMetadata(inviteCode);
      const name =
        request.group_name?.trim() || parseInviteGroupName(request.invite_code);
      const groupId = `group-${slugify(name)}`;
      if (!state.groups.some((group) => group.group_id === groupId)) {
        state.groups.push({
          group_id: groupId,
          name,
          role: "member",
          channels: defaultGroupChannels(),
        });
      }
      state.active_context = {
        kind: "group",
        group_id: groupId,
        channel_id: null,
        dm_id: null,
      };
      if (parsedInvite) {
        state.invites.push({
          invite_id: `invite-${parsedInvite.inviteKey}`,
          invite_key: parsedInvite.inviteKey,
          group_id: groupId,
          code: inviteCode,
          room_secret_hash: parsedInvite.roomSecretHash,
          signaling_endpoint: parsedInvite.signalingEndpoint,
          signaling_trust_fingerprint: parsedInvite.signalingTrustFingerprint,
          signaling_trust_status: parsedInvite.signalingTrustStatus,
          endpoint_policy: parsedInvite.endpointPolicy,
          ice_stun_servers: parsedInvite.iceStunServers,
          ice_turn_servers: parsedInvite.iceTurnServers,
          expires: "Invite expiry from signed descriptor",
          expires_at: parsedInvite.expiresAt,
          max_use: String(parsedInvite.maxUses),
          uses: 1,
          revoked: false,
          admission_copy:
            "Parsed production invite metadata; final admission still requires authorized MLS Welcome/add",
        });
      }
      pushEvent(
        state,
        "group.joined",
        `Joined ${name} via ${request.invite_code}`,
      );
    }),
  );
}

export async function setActiveGroup(
  request: SetActiveGroupRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("set_active_group", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const group = state.groups.find(
        (item) => item.group_id === request.group_id,
      );
      if (!group) {
        pushCommandError(
          state,
          "group.focus_missing",
          "set_active_group",
          "group_not_found",
          "Requested group does not exist",
          "Pick a group from the server rail before focusing it",
        );
        return;
      }
      state.active_context = {
        kind: "group",
        group_id: group.group_id,
        channel_id: null,
        dm_id: null,
      };
      pushEvent(state, "group.focused", `Focused group ${group.name}`);
    }),
  );
}

export async function createInvite(
  request: CreateInviteRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("create_invite", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const groupId =
        request.group_id ??
        state.active_context?.group_id ??
        state.groups[0]?.group_id;
      if (!groupId) {
        pushCommandError(
          state,
          "invite.rejected",
          "create_invite",
          "group_not_found",
          "No group exists for invite creation",
          "Create or select a group before creating an invite",
        );
        return;
      }
      const group = state.groups.find((item) => item.group_id === groupId);
      const inviteKey =
        crypto.randomUUID?.() ?? `local-${state.invites.length + 1}`;
      const roomSecretHash = stableHash(
        `${groupId}:${inviteKey}:${state.invites.length}`,
      );
      const expires = request.expires || fallbackState.snapshot.invite.expires;
      const maxUse = request.max_use || fallbackState.snapshot.invite.max_use;
      const expiresAt = inviteExpirationHorizon(expires);
      const signalingEndpoint = defaultSignalingEndpoint();
      const signalingTrustFingerprint = stableHash(
        `external-signaling-endpoint-fingerprint-v1:${signalingEndpoint}`,
      );
      const endpointPolicy = "production_tls";
      const trustStatus =
        "signed endpoint fingerprint; verify before MLS Welcome";
      state.invites.push({
        invite_id: `invite-${inviteKey}`,
        invite_key: inviteKey,
        group_id: groupId,
        code: productionInviteLink({
          inviteKey,
          signalingEndpoint,
          endpointPolicy,
          signalingTrustFingerprint,
          signalingTrustStatus: trustStatus,
          iceStunServers: defaultIceStunServers(),
          iceTurnServers: defaultRedactedTurnServers(),
          roomSecretHash,
          expiresAt,
          maxUses: parseMaxUses(maxUse),
        }),
        room_secret_hash: roomSecretHash,
        signaling_endpoint: signalingEndpoint,
        signaling_trust_fingerprint: signalingTrustFingerprint,
        signaling_trust_status: trustStatus,
        endpoint_policy: endpointPolicy,
        ice_stun_servers: defaultIceStunServers(),
        ice_turn_servers: defaultRedactedTurnServers(),
        expires,
        expires_at: expiresAt,
        max_use: maxUse,
        uses: 0,
        revoked: false,
        admission_copy:
          "Final admission still requires an authorized MLS Welcome/add; the room-secret link alone is insufficient",
      });
      pushEvent(
        state,
        "invite.created",
        `Invite created for ${group?.name ?? "group"}`,
      );
    }),
  );
}

export async function createChannel(
  request: CreateChannelRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("create_channel", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const group = state.groups.find(
        (item) => item.group_id === request.group_id,
      );
      if (!group) {
        pushCommandError(
          state,
          "channel.rejected",
          "create_channel",
          "group_not_found",
          "No matching group for channel creation",
          "Select an existing group before adding a text or voice channel",
        );
        return;
      }
      const name =
        request.kind === "Text"
          ? `#${request.name.replace(/^#/, "") || "secure-room"}`
          : request.name || "Voice Lobby";
      if (!group.channels.some((channel) => channel.name === name)) {
        group.channels.push({
          channel_id: `channel-${slugify(name)}`,
          name,
          kind: request.kind,
          retention_status: request.retention_status,
        });
      }
      const channel = group.channels.find((item) => item.name === name);
      state.active_context = {
        kind: request.kind === "Text" ? "text_channel" : "voice_channel",
        group_id: group.group_id,
        channel_id: channel?.channel_id ?? null,
        dm_id: null,
      };
      pushEvent(state, "channel.created", `Created channel ${name}`);
    }),
  );
}

export async function joinVoice(request: JoinVoiceRequest): Promise<AppState> {
  return invokeOrFallback<AppState>("join_voice", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const captureAllowed =
        request.microphone_permission === "granted" &&
        Boolean(request.input_device_id || request.input_device_label);
      const inputDevice = request.input_device_id || request.input_device_label
        ? {
            device_id: request.input_device_id ?? "default",
            label: request.input_device_label ?? "Default microphone",
            kind: "audio_input" as const,
          }
        : null;
      const outputDevice = request.output_device_id || request.output_device_label
        ? {
            device_id: request.output_device_id ?? "default",
            label: request.output_device_label ?? "Default speaker",
            kind: "audio_output" as const,
          }
        : null;
      state.voice_session = {
        session_id: `voice-${request.channel_id}`,
        group_id: request.group_id,
        channel_id: request.channel_id,
        joined: captureAllowed,
        self_muted: state.voice_session?.self_muted ?? false,
        microphone_permission: request.microphone_permission,
        input_device: inputDevice,
        output_device: outputDevice,
        participants: [
          {
            id: localUserId(state),
            name: "You",
            role: "you",
            speaking: false,
            muted: false,
            volume: 82,
          },
        ],
        route_copy: captureAllowed
          ? "Local capture permission and device selection are ready; encrypted media transport remains gated by media-frame E2E; speaking indicators wait for media audio-level/VAD events"
          : "No voice route opened because microphone permission/input selection is not granted",
        status_copy: captureAllowed
          ? `Microphone capture authorized using ${inputDevice?.label ?? "Default microphone"} and playback routed to ${outputDevice?.label ?? "system default"}`
          : "Microphone permission denied; voice was not joined and no capture is running",
        permission_denied_copy: captureAllowed
          ? ""
          : "Grant microphone permission and select an input device before joining voice",
      };
      state.active_context = {
        kind: "voice_channel",
        group_id: request.group_id,
        channel_id: request.channel_id,
        dm_id: null,
      };
      if (captureAllowed) {
        pushEvent(state, "voice.joined", "Joined command-backed local voice session");
      } else {
        pushCommandError(
          state,
          "voice.permission_denied",
          "join_voice",
          "voice_permission_required",
          "Microphone permission/input device required before joining voice",
          "Grant microphone permission and select an input device before joining voice",
        );
      }
    }),
  );
}

export async function leaveVoice(
  request: LeaveVoiceRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("leave_voice", { request }, () =>
    mutateFallback((state) => {
      if (
        !state.voice_session ||
        state.voice_session.session_id !== request.session_id
      ) {
        pushCommandError(
          state,
          "voice.leave_ignored",
          "leave_voice",
          "voice_session_not_found",
          state.voice_session
            ? "Leave request did not match active session"
            : "No active voice session to leave",
          state.voice_session
            ? "Use the currently joined voice session before leaving"
            : "Join a voice channel before trying to leave",
        );
        return;
      }
      state.voice_session.joined = false;
      state.voice_session.status_copy =
        "Not joined; command-backed local voice controls are idle";
      state.voice_session.participants = state.voice_session.participants.map(
        (participant) => ({
          ...participant,
          speaking: false,
        }),
      );
      pushEvent(state, "voice.left", "Left command-backed local voice session");
    }),
  );
}

export async function setSelfMute(request: SelfMuteRequest): Promise<AppState> {
  return invokeOrFallback<AppState>("set_self_mute", { request }, () =>
    mutateFallback((state) => {
      if (
        !state.voice_session ||
        state.voice_session.session_id !== request.session_id
      ) {
        pushCommandError(
          state,
          "voice.self_mute_rejected",
          "set_self_mute",
          "voice_session_not_found",
          state.voice_session
            ? "Mute request did not match active session"
            : "No active voice session to mute",
          state.voice_session
            ? "Join the voice session again before changing mute state"
            : "Join a voice channel before muting yourself",
        );
        return;
      }
      state.voice_session.self_muted = request.muted;
      state.voice_session.participants = state.voice_session.participants.map(
        (participant) =>
          participant.id === localUserId(state)
            ? {
                ...participant,
                muted: request.muted,
                speaking: request.muted ? false : participant.speaking,
              }
            : participant,
      );
      pushEvent(
        state,
        "voice.self_mute",
        request.muted ? "Self muted" : "Self unmuted",
      );
    }),
  );
}

export async function setSpeakerVolume(
  request: SpeakerVolumeRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("set_speaker_volume", { request }, () =>
    mutateFallback((state) => {
      if (
        !state.voice_session ||
        state.voice_session.session_id !== request.session_id
      ) {
        pushCommandError(
          state,
          "voice.volume_rejected",
          "set_speaker_volume",
          "voice_session_not_found",
          state.voice_session
            ? "Volume request did not match active session"
            : "No active voice session for speaker volume",
          state.voice_session
            ? "Use the active voice session before changing speaker volume"
            : "Join a voice channel before changing speaker volume",
        );
        return;
      }
      const participantExists = state.voice_session.participants.some(
        (participant) => participant.id === request.participant_id,
      );
      if (!participantExists) {
        pushCommandError(
          state,
          "voice.volume_rejected",
          "set_speaker_volume",
          "voice_participant_not_found",
          "No matching voice participant for speaker volume",
          "Choose a visible participant from the voice member list",
        );
        return;
      }
      state.voice_session.participants = state.voice_session.participants.map(
        (participant) =>
          participant.id === request.participant_id
            ? {
                ...participant,
                volume: Math.max(0, Math.min(100, request.volume)),
              }
            : participant,
      );
      pushEvent(state, "voice.volume", `Set ${request.participant_id} volume`);
    }),
  );
}

export async function sendMessage(
  request: SendMessageRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("send_message", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const body = request.body.trim();
      if (!body) {
        pushCommandError(
          state,
          "message.rejected",
          "send_message",
          "message_empty",
          "Empty message was not sent",
          "Type a non-empty message before sending",
        );
        return;
      }
      state.messages.push({
        message_id: `fallback-${state.messages.length + 1}`,
        target: request.target,
        author_id: localUserId(state),
        author: state.profile?.display_name ?? "Alice",
        body,
        status:
          "local encrypted author log; remote delivery/read receipts not claimed without signed receipt",
        sent_at: `local-${state.messages.length + 1}`,
      });
      pushEvent(
        state,
        "message.sent",
        "Message appended to local encrypted timeline; remote delivery/read receipts are not claimed",
      );
    }),
  );
}

export async function pollAppEvents(
  request: PollAppEventsRequest = {},
): Promise<AppEventStreamView> {
  return invokeOrFallback<AppEventStreamView>(
    "poll_app_events",
    { request },
    () => {
      const state = cloneState(syncSnapshot(fallbackState));
      const cursor = request.after ?? 0;
      const subscribedKinds = [...new Set(request.kinds ?? [])]
        .map((kind) => kind.trim().toLowerCase())
        .filter((kind) =>
          ["message", "invite", "group", "device", "transport", "voice"].includes(
            kind,
          ),
        )
        .sort();
      const limit = Math.max(1, Math.min(request.limit ?? 64, 256));
      const matchesTopic = (event: AppEventView) =>
        subscribedKinds.length === 0 ||
        subscribedKinds.includes(event.kind.split(".")[0] ?? event.kind);
      const filtered = state.events.filter(
        (event) => event.sequence > cursor && matchesTopic(event),
      );
      const events = filtered.slice(0, limit);
      return {
        events,
        cursor,
        next_cursor: events.at(-1)?.sequence ?? state.event_cursor,
        has_more: filtered.length > limit,
        subscribed_kinds: subscribedKinds,
      };
    },
  );
}

export async function deletionWarning(): Promise<string> {
  return invokeOrFallback<string>(
    "deletion_warning",
    undefined,
    () => fallbackState.security_copy.deletion,
  );
}

export async function metadataWarning(): Promise<string> {
  return invokeOrFallback<string>(
    "metadata_warning",
    undefined,
    () => fallbackState.security_copy.metadata,
  );
}

export async function commandHealth(): Promise<CommandHealth> {
  return invokeOrFallback<CommandHealth>("command_health", undefined, () => ({
    snapshot_ready: fallbackState.snapshot.schema_version >= 1,
    verification_ready: Boolean(fallbackState.snapshot.friend.safety_number),
    app_state_ready: fallbackState.schema_version >= 1,
    identity_ready: ["first_run", "ready"].includes(fallbackState.lifecycle),
    collaboration_ready: fallbackState.messages.every((message) =>
      message.status.includes("not claimed"),
    ),
    voice_ready: false,
    honest_copy_ready:
      fallbackState.security_copy.deletion.includes("pending on offline devices") &&
      fallbackState.security_copy.metadata.includes("does not claim anonymity"),
  }));
}

export async function resetAppState(
  request: ResetAppStateRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("reset_app_state", { request }, () => {
    fallbackState.last_command_error = null;
    if (request.confirmation.trim() !== RESET_APP_CONFIRMATION_PHRASE) {
      pushCommandError(
        fallbackState,
        "state.reset_rejected",
        "reset_app_state",
        "confirmation_required",
        "Local state reset requires the exact confirmation phrase",
        `Type ${RESET_APP_CONFIRMATION_PHRASE} to erase local app state`,
      );
      return cloneState(syncSnapshot(fallbackState));
    }
    fallbackState.lifecycle = "first_run";
    fallbackState.profile = null;
    fallbackState.dms = [];
    fallbackState.groups = [];
    fallbackState.active_context = null;
    fallbackState.messages = [];
    fallbackState.voice_session = null;
    fallbackState.invites = [];
    fallbackState.devices = [];
    fallbackState.events = [
      {
        sequence: 1,
        kind: "app.first_run",
        summary: "No local profile exists; setup/recovery is required",
      },
    ];
    fallbackState.last_command_error = null;
    pushEvent(
      fallbackState,
      "state.reset",
      "Local app state reset after explicit typed confirmation",
    );
    return cloneState(syncSnapshot(fallbackState));
  });
}

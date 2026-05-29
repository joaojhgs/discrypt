export type ChannelKind = "Text" | "Voice";

export type FriendView = {
  alias: string;
  friend_code: string;
  safety_number: string;
  verified: boolean;
};

export type DeviceView = {
  device_id: string;
  leaf_index: number;
  local: boolean;
  authorized: boolean;
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

export type SnapshotVoiceSessionView = {
  joined: boolean;
  participants: VoiceParticipantView[];
  status_copy: string;
  route_copy: string;
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

export type InviteView = {
  invite_id: string;
  invite_key: string;
  group_id: string;
  code: string;
  room_secret_hash: string;
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
  participants: VoiceParticipantView[];
  route_copy: string;
  status_copy: string;
};

export type AppEventView = {
  sequence: number;
  kind: string;
  summary: string;
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
    alias: "Bob",
    friend_code: "friend:bob:stable-fixture",
    safety_number: "0231 1597 2653 5897",
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
    participants: [],
    status_copy: "Not joined; command-backed local voice controls are idle",
    route_copy:
      "Route copy is harness-backed until socket/media adapter E2E passes",
  },
  preferences: { theme_id: "graphite-calm", template_id: "command-center" },
  messages: [],
  activity_feed: [
    "Demo fallback active: packaged Tauri builds must use IPC-backed commands",
    "Recovery is local/test-build placeholder only; no history/key recovery claim",
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
        participants: state.voice_session.participants,
        status_copy: state.voice_session.status_copy,
        route_copy: state.voice_session.route_copy,
      }
    : {
        joined: false,
        participants: [],
        status_copy: "Not joined; command-backed local voice controls are idle",
        route_copy:
          "Local voice controls only; network media route is not connected in this build",
      };
  state.snapshot.activity_feed = state.events
    .slice()
    .reverse()
    .map((event) => event.summary);
  return state;
}

function pushEvent(state: AppState, kind: string, summary: string): void {
  const lastSequence = state.events.at(-1)?.sequence ?? 0;
  state.events.push({ sequence: lastSequence + 1, kind, summary });
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
  update(fallbackState);
  return cloneState(syncSnapshot(fallbackState));
}

function ensureFallbackReady(
  displayName = "Alice",
  deviceName = "Desktop",
): void {
  if (fallbackState.lifecycle === "ready") return;
  fallbackState.lifecycle = "ready";
  fallbackState.profile = {
    user_id: `user-${slugify(displayName)}`,
    display_name: displayName,
    device_name: deviceName,
    recovery_status:
      "New local profile; recovery export remains a local placeholder",
  };
  fallbackState.devices = [
    {
      device_id: slugify(deviceName),
      leaf_index: 1,
      local: true,
      authorized: true,
    },
  ];
  fallbackState.dms = [
    {
      dm_id: "dm-bob",
      participant_id: "bob",
      display_name: "Bob",
      local_only_copy:
        "Default local DM fixture; no remote delivery is claimed",
    },
  ];
  fallbackState.active_context = {
    kind: "dm",
    group_id: null,
    channel_id: null,
    dm_id: "dm-bob",
  };
  pushEvent(
    fallbackState,
    "identity.created",
    `Profile ready for ${displayName}`,
  );
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
      if (state.profile) {
        state.profile.recovery_status =
          "Recovered locally from placeholder code; no cloud or cross-device history recovery claimed";
      }
      pushEvent(
        state,
        "identity.recovered",
        "Local recovery placeholder accepted; no history/key recovery was claimed",
      );
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
      const displayName = request.display_name.trim() || "Bob";
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
      const localInvite = state.invites.find(
        (invite) => invite.code === request.invite_code.trim(),
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
      if (!group) return;
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
      if (!groupId) return;
      const group = state.groups.find((item) => item.group_id === groupId);
      const inviteKey =
        crypto.randomUUID?.() ?? `local-${state.invites.length + 1}`;
      const roomSecretHash = stableHash(
        `${groupId}:${inviteKey}:${state.invites.length}`,
      );
      const roomSecret = roomSecretHash.slice(0, 32);
      const expires = request.expires || fallbackState.snapshot.invite.expires;
      const maxUse = request.max_use || fallbackState.snapshot.invite.max_use;
      state.invites.push({
        invite_id: `invite-${inviteKey}`,
        invite_key: inviteKey,
        group_id: groupId,
        code: `discrypt://join/v1/${inviteKey}?room_secret=${roomSecret}&exp=${encodeURIComponent(inviteExpirationHorizon(expires))}&max=${parseMaxUses(maxUse)}`,
        room_secret_hash: roomSecretHash,
        expires,
        expires_at: inviteExpirationHorizon(expires),
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
      if (!group) return;
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
      state.voice_session = {
        session_id: `voice-${request.channel_id}`,
        group_id: request.group_id,
        channel_id: request.channel_id,
        joined: true,
        self_muted: state.voice_session?.self_muted ?? false,
        participants: [
          {
            id: "local-user",
            name: "You",
            role: "you",
            speaking: true,
            muted: false,
            volume: 82,
          },
        ],
        route_copy:
          "Local voice controls only; network media route is not connected in this build",
        status_copy:
          "Voice session state joined locally; real audio-frame media remains release-gated",
      };
      state.active_context = {
        kind: "voice_channel",
        group_id: request.group_id,
        channel_id: request.channel_id,
        dm_id: null,
      };
      pushEvent(
        state,
        "voice.joined",
        "Joined command-backed local voice session",
      );
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
      )
        return;
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
      )
        return;
      state.voice_session.self_muted = request.muted;
      state.voice_session.participants = state.voice_session.participants.map(
        (participant) =>
          participant.id === "local-user"
            ? {
                ...participant,
                muted: request.muted,
                speaking:
                  state.voice_session?.joined === true && !request.muted,
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
      )
        return;
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
      if (!body) return;
      state.messages.push({
        message_id: `fallback-${state.messages.length + 1}`,
        target: request.target,
        author_id: "local-user",
        author: state.profile?.display_name ?? "Alice",
        body,
        status:
          "plaintext allowed by current local retention cache; encrypted envelope facade recorded by harness",
        sent_at: `local-${state.messages.length + 1}`,
      });
      pushEvent(
        state,
        "message.sent",
        "Message appended to local encrypted timeline facade",
      );
    }),
  );
}

export async function pollAppEvents(): Promise<AppEventView[]> {
  return invokeOrFallback<AppEventView[]>(
    "poll_app_events",
    undefined,
    () => cloneState(syncSnapshot(fallbackState)).events,
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

export async function resetAppState(): Promise<AppState> {
  return invokeOrFallback<AppState>("reset_app_state", undefined, () => {
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
    return cloneState(syncSnapshot(fallbackState));
  });
}

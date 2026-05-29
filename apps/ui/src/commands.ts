export type ChannelKind = "Text" | "Voice";
export type LifecycleStage = "needs_identity" | "ready";

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

export type LegacyChannelView = {
  name: string;
  kind: ChannelKind;
  retention_status: string;
};

export type LegacyServerView = {
  name: string;
  role: string;
  channels: LegacyChannelView[];
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

export type PreferencesView = {
  theme_id: string;
  template_id: string;
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
  servers: LegacyServerView[];
  invite: InviteFlowView;
  retention: RetentionSettingsView;
  voice: VoiceRoomView;
  voice_session: unknown;
  preferences: PreferencesView;
  messages: unknown[];
  activity_feed: string[];
  connectivity: ConnectivityView;
  security_copy: SecurityCopyView;
};

export type UserIdentityView = {
  user_id: string;
  display_name: string;
  device_name: string;
  recovery_hint: string;
};

export type AppChannelView = {
  channel_id: string;
  name: string;
  kind: ChannelKind;
  retention_status: string;
};

export type GroupView = {
  group_id: string;
  name: string;
  role: string;
  channels: AppChannelView[];
  invite_codes: string[];
};

export type DmView = {
  dm_id: string;
  peer_label: string;
};

export type MessageTarget =
  | { kind: "dm"; dm_id: string }
  | { kind: "channel"; group_id: string; channel_id: string };

export type MessageView = {
  message_id: string;
  target: MessageTarget;
  author: string;
  body: string;
  status: string;
  sent_at: string;
};

export type VoiceParticipantView = {
  id: string;
  name: string;
  role: string;
  speaking: boolean;
  muted: boolean;
  volume: number;
};

export type VoiceSessionView = {
  session_id: string;
  group_id: string;
  channel_id: string;
  joined: boolean;
  self_muted: boolean;
  participants: VoiceParticipantView[];
  route: string;
};

export type InviteView = {
  invite_id: string;
  code: string;
  group_id: string;
  expires: string;
  max_use: string;
  admission_copy: string;
};

export type AppEventView = {
  sequence: number;
  kind: string;
  summary: string;
};

export type AppStateView = {
  snapshot: AppSnapshot;
  lifecycle: LifecycleStage;
  user: UserIdentityView | null;
  preferences: PreferencesView;
  dms: DmView[];
  groups: GroupView[];
  active_group_id: string | null;
  active_dm_id: string | null;
  messages: MessageView[];
  voice_sessions: VoiceSessionView[];
  active_voice_session_id: string | null;
  events: AppEventView[];
  active_invite: InviteView | null;
  recovery_copy: string;
};

export type SafetyVerificationRequest = {
  friend_id: string;
  provided: string;
};

export type SafetyVerificationResult = {
  verified: boolean;
  message: string;
};

export type CreateUserRequest = { display_name: string; device_name: string };
export type RecoverUserRequest = {
  display_name: string;
  device_name: string;
  recovery_code: string;
};
export type SavePreferencesRequest = { theme_id: string; template_id: string };
export type StartDmRequest = { peer_label: string };
export type CreateGroupRequest = { name: string; retention: string };
export type JoinGroupRequest = { invite_code: string; group_name?: string };
export type CreateInviteRequest = {
  group_id: string;
  expires: string;
  max_use: string;
};
export type CreateChannelRequest = {
  group_id: string;
  name: string;
  kind: ChannelKind;
  retention_status: string;
};
export type SendMessageRequest = { target: MessageTarget; body: string };
export type JoinVoiceRequest = { group_id: string; channel_id: string };
export type LeaveVoiceRequest = { session_id: string };
export type SelfMuteRequest = { session_id: string; muted: boolean };
export type SpeakerVolumeRequest = {
  session_id: string;
  participant_id: string;
  volume: number;
};

type TauriInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

declare global {
  interface Window {
    __TAURI__?: { core?: { invoke?: TauriInvoke } };
  }
}

const fallbackState: AppStateView = createFallbackState();

function invoke(): TauriInvoke | null {
  return window.__TAURI__?.core?.invoke ?? null;
}

async function invokeOrFallback<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  fallback: () => T,
): Promise<T> {
  const tauriInvoke = invoke();
  if (tauriInvoke) {
    return tauriInvoke<T>(command, args);
  }
  return cloneState(fallback() as T);
}

export async function loadAppState(): Promise<AppStateView> {
  return invokeOrFallback<AppStateView>("app_state", undefined, () => fallbackState);
}

export async function loadAppSnapshot(): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>("app_snapshot", undefined, () => fallbackState.snapshot);
}

export async function createUser(request: CreateUserRequest): Promise<AppStateView> {
  return invokeOrFallback("create_user", { request }, () => {
    fallbackState.lifecycle = "ready";
    fallbackState.user = {
      user_id: stableId("user", request.display_name),
      display_name: request.display_name || "Alice",
      device_name: request.device_name || "this device",
      recovery_hint: "Local recovery placeholder created on this device; QR/cross-device recovery is not enabled yet.",
    };
    ensureFallbackReady();
    pushEvent("identity.created", `Created local user ${fallbackState.user.display_name}`);
    return fallbackState;
  });
}

export async function recoverUser(request: RecoverUserRequest): Promise<AppStateView> {
  return invokeOrFallback("recover_user", { request }, () => {
    fallbackState.lifecycle = "ready";
    fallbackState.user = {
      user_id: stableId("user", `${request.display_name}-${request.recovery_code}`),
      display_name: request.display_name || "Recovered user",
      device_name: request.device_name || "recovered device",
      recovery_hint: "Recovered a local profile placeholder only. This build does not claim QR, backup, or cross-device content-key recovery.",
    };
    ensureFallbackReady();
    pushEvent("identity.recovered", "Recovered local placeholder user");
    return fallbackState;
  });
}

export async function verifySafetyNumber(
  request: SafetyVerificationRequest,
): Promise<SafetyVerificationResult> {
  return invokeOrFallback("verify_safety_number", { request }, () => {
    const verified =
      request.friend_id === fallbackState.snapshot.friend.friend_code &&
      request.provided === fallbackState.snapshot.friend.safety_number;
    fallbackState.snapshot.friend.verified = verified || fallbackState.snapshot.friend.verified;
    return {
      verified,
      message: verified
        ? "Safety number verified; MITM risk accepted by explicit user comparison"
        : "Safety number mismatch; do not trust this device or DM",
    };
  });
}

export async function savePreferences(request: SavePreferencesRequest): Promise<AppStateView> {
  return invokeOrFallback("save_preferences", { request }, () => {
    fallbackState.preferences = request;
    pushEvent("preferences.saved", "Theme/template preferences saved");
    return fallbackState;
  });
}

export async function startDm(request: StartDmRequest): Promise<AppStateView> {
  return invokeOrFallback("start_dm", { request }, () => {
    ensureFallbackReady();
    const dm_id = stableId("dm", request.peer_label || "Bob");
    if (!fallbackState.dms.some((dm) => dm.dm_id === dm_id)) {
      fallbackState.dms.push({ dm_id, peer_label: request.peer_label || "Bob" });
    }
    fallbackState.active_dm_id = dm_id;
    pushEvent("dm.opened", `Opened DM with ${request.peer_label || "Bob"}`);
    return fallbackState;
  });
}

export async function createGroup(request: CreateGroupRequest): Promise<AppStateView> {
  return invokeOrFallback("create_group", { request }, () => {
    ensureFallbackReady();
    const group_id = stableId("group", request.name || "private lab");
    if (!fallbackState.groups.some((group) => group.group_id === group_id)) {
      fallbackState.groups.push({
        group_id,
        name: request.name || "private lab",
        role: "owner",
        channels: defaultChannels(request.retention),
        invite_codes: [],
      });
    }
    fallbackState.active_group_id = group_id;
    pushEvent("group.created", `Created group ${request.name || "private lab"}`);
    return fallbackState;
  });
}

export async function joinGroup(request: JoinGroupRequest): Promise<AppStateView> {
  return invokeOrFallback("join_group", { request }, () => {
    ensureFallbackReady();
    const name = request.group_name || "joined group";
    const group_id = stableId("group", name);
    if (!fallbackState.groups.some((group) => group.group_id === group_id)) {
      fallbackState.groups.push({
        group_id,
        name,
        role: "member",
        channels: defaultChannels("7 days"),
        invite_codes: [request.invite_code],
      });
    }
    fallbackState.active_group_id = group_id;
    pushEvent("group.joined", `Joined ${name}`);
    return fallbackState;
  });
}

export async function createInvite(request: CreateInviteRequest): Promise<AppStateView> {
  return invokeOrFallback("create_invite", { request }, () => {
    const code = `discrypt://join/${Date.now()}-${request.group_id}`;
    fallbackState.active_invite = {
      invite_id: stableId("invite", code),
      code,
      group_id: request.group_id,
      expires: request.expires,
      max_use: request.max_use,
      admission_copy: fallbackState.snapshot.invite.welcome_required,
    };
    const group = fallbackState.groups.find((item) => item.group_id === request.group_id);
    group?.invite_codes.push(code);
    pushEvent("invite.created", `Created invite ${code}`);
    return fallbackState;
  });
}

export async function createChannel(request: CreateChannelRequest): Promise<AppStateView> {
  return invokeOrFallback("create_channel", { request }, () => {
    const group = fallbackState.groups.find((item) => item.group_id === request.group_id);
    const name = request.kind === "Text" ? `#${request.name.replace(/^#/, "") || "general"}` : request.name || "Voice Lobby";
    const channel_id = stableId("channel", `${request.group_id}-${name}`);
    if (group && !group.channels.some((channel) => channel.channel_id === channel_id)) {
      group.channels.push({ channel_id, name, kind: request.kind, retention_status: request.retention_status });
    }
    pushEvent("channel.created", `Created channel ${name}`);
    return fallbackState;
  });
}

export async function sendMessage(request: SendMessageRequest): Promise<AppStateView> {
  return invokeOrFallback("send_message", { request }, () => {
    fallbackState.messages.push({
      message_id: `msg-${fallbackState.messages.length + 1}`,
      target: request.target,
      author: fallbackState.user?.display_name ?? "local user",
      body: request.body,
      status: "local encrypted-message facade persisted; relay/network delivery not claimed",
      sent_at: `local-${fallbackState.messages.length + 1}`,
    });
    pushEvent("message.sent", "Message persisted locally");
    return fallbackState;
  });
}

export async function joinVoice(request: JoinVoiceRequest): Promise<AppStateView> {
  return invokeOrFallback("join_voice", { request }, () => {
    let session = fallbackState.voice_sessions.find(
      (item) => item.group_id === request.group_id && item.channel_id === request.channel_id,
    );
    if (!session) {
      session = {
        session_id: stableId("voice", `${request.group_id}-${request.channel_id}`),
        group_id: request.group_id,
        channel_id: request.channel_id,
        joined: false,
        self_muted: false,
        participants: defaultVoiceParticipants(fallbackState.user?.display_name ?? "You"),
        route: "local voice session only; production media path waits for adapter/E2E gates",
      };
      fallbackState.voice_sessions.push(session);
    }
    fallbackState.voice_sessions.forEach((item) => {
      item.joined = false;
      item.participants.forEach((participant) => (participant.speaking = false));
    });
    session.joined = true;
    session.self_muted = false;
    session.participants[0].speaking = true;
    fallbackState.active_voice_session_id = session.session_id;
    pushEvent("voice.joined", "Joined voice session");
    return fallbackState;
  });
}

export async function leaveVoice(request: LeaveVoiceRequest): Promise<AppStateView> {
  return invokeOrFallback("leave_voice", { request }, () => {
    const session = fallbackState.voice_sessions.find((item) => item.session_id === request.session_id);
    if (session) {
      session.joined = false;
      session.participants.forEach((participant) => (participant.speaking = false));
    }
    if (fallbackState.active_voice_session_id === request.session_id) fallbackState.active_voice_session_id = null;
    pushEvent("voice.left", "Left voice session");
    return fallbackState;
  });
}

export async function setSelfMute(request: SelfMuteRequest): Promise<AppStateView> {
  return invokeOrFallback("set_self_mute", { request }, () => {
    const session = fallbackState.voice_sessions.find((item) => item.session_id === request.session_id);
    if (session) {
      session.self_muted = request.muted;
      const self = session.participants.find((participant) => participant.id === "local");
      if (self) {
        self.muted = request.muted;
        self.speaking = session.joined && !request.muted;
      }
    }
    return fallbackState;
  });
}

export async function setSpeakerVolume(request: SpeakerVolumeRequest): Promise<AppStateView> {
  return invokeOrFallback("set_speaker_volume", { request }, () => {
    const session = fallbackState.voice_sessions.find((item) => item.session_id === request.session_id);
    const participant = session?.participants.find((item) => item.id === request.participant_id);
    if (participant) participant.volume = Math.max(0, Math.min(100, request.volume));
    return fallbackState;
  });
}

function createFallbackState(): AppStateView {
  return {
    snapshot: {
      schema_version: 2,
      friend: {
        alias: "Bob",
        friend_code: "friend:bob:stable-fixture",
        safety_number: "0231 1597 2653 5897",
        verified: false,
      },
      devices: [
        { device_id: "local-device", leaf_index: 1, local: true, authorized: true },
      ],
      servers: [],
      invite: {
        expires: "Invite expires and can be revoked",
        max_use: "Max-use is enforced before MLS admission",
        password_gate: "Password rooms require online authorization; no offline verifier",
        welcome_required: "Final admission still requires an authorized MLS Welcome/add",
      },
      retention: {
        presets: ["1 hour", "24 hours", "7 days", "30 days", "90 days"],
        selected: "7 days",
        unlimited_warning: "Unlimited keeps local keys longer and weakens lock behavior; opt in explicitly",
        transition_copy: "Shortening re-locks older messages retroactively; lengthening applies only to future messages",
      },
      voice: {
        route: "STUN → peer relay overlay → TURN",
        relay_copy: "Relays see SFrame ciphertext only after harness gates; production media is not claimed yet",
        android_path: "Android QR/device pairing is future work in this build",
      },
      voice_session: {},
      preferences: { theme_id: "graphite-calm", template_id: "command-center" },
      messages: [],
      activity_feed: [],
      connectivity: {
        fallback_chain: "local harness → socket adapter → production relay",
        metadata_copy: "Metadata is minimized but this build does not claim anonymity",
        push_copy: "Push wake is harnessed only",
      },
      security_copy: {
        metadata: "Metadata is minimized but this build does not claim anonymity",
        deletion: "Deletion is cooperative and pending on offline devices",
        malicious_member: "A malicious recipient can copy plaintext after decryption",
      },
    },
    lifecycle: "needs_identity",
    user: null,
    preferences: { theme_id: "graphite-calm", template_id: "command-center" },
    dms: [],
    groups: [],
    active_group_id: null,
    active_dm_id: null,
    messages: [],
    voice_sessions: [],
    active_voice_session_id: null,
    events: [{ sequence: 1, kind: "app.needs_identity", summary: "Choose create user or recover user" }],
    active_invite: null,
    recovery_copy: "Recovery is local-only in this build. QR/cross-device recovery is not enabled yet; do not assume remote history or content-key restoration.",
  };
}

function ensureFallbackReady() {
  if (fallbackState.dms.length === 0) {
    fallbackState.dms.push({ dm_id: stableId("dm", "Bob"), peer_label: "Bob" });
    fallbackState.active_dm_id = fallbackState.dms[0].dm_id;
  }
  if (fallbackState.groups.length === 0) {
    const group_id = stableId("group", "discrypt lab");
    fallbackState.groups.push({ group_id, name: "discrypt lab", role: "owner", channels: defaultChannels("7 days"), invite_codes: [] });
    fallbackState.active_group_id = group_id;
  }
  fallbackState.groups.forEach((group) => {
    group.channels
      .filter((channel) => channel.kind === "Voice")
      .forEach((channel) => {
        const session_id = stableId("voice", `${group.group_id}-${channel.channel_id}`);
        if (!fallbackState.voice_sessions.some((session) => session.session_id === session_id)) {
          fallbackState.voice_sessions.push({
            session_id,
            group_id: group.group_id,
            channel_id: channel.channel_id,
            joined: false,
            self_muted: false,
            participants: defaultVoiceParticipants(fallbackState.user?.display_name ?? "You"),
            route: "local voice session only; production media path waits for adapter/E2E gates",
          });
        }
      });
  });
}

function defaultChannels(retention: string): AppChannelView[] {
  return [
    { channel_id: stableId("channel", "general"), name: "#general", kind: "Text", retention_status: retention || "7 days" },
    {
      channel_id: stableId("channel", "voice-lobby"),
      name: "Voice Lobby",
      kind: "Voice",
      retention_status: "Session-state only; media-frame E2E gate required before production voice claims",
    },
  ];
}

function defaultVoiceParticipants(displayName: string): VoiceParticipantView[] {
  return [
    { id: "local", name: displayName || "You", role: "you", speaking: false, muted: false, volume: 100 },
    { id: "peer-bob", name: "Bob", role: "peer", speaking: false, muted: false, volume: 72 },
    { id: "relay", name: "Relay route", role: "route", speaking: false, muted: true, volume: 40 },
  ];
}

function pushEvent(kind: string, summary: string) {
  fallbackState.events.unshift({ sequence: fallbackState.events.length + 1, kind, summary });
  fallbackState.events = fallbackState.events.slice(0, 24);
}

function stableId(prefix: string, value: string): string {
  const normalized = value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return `${prefix}-${normalized || "local"}`;
}

function cloneState<T>(value: T): T {
  return typeof structuredClone === "function" ? structuredClone(value) : JSON.parse(JSON.stringify(value));
}

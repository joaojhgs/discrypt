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

export type VoiceSessionView = {
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
  voice_session: VoiceSessionView;
  preferences: PreferencesView;
  messages: MessageView[];
  activity_feed: string[];
  connectivity: ConnectivityView;
  security_copy: SecurityCopyView;
};

export type SafetyVerificationRequest = {
  friend_id: string;
  provided: string;
};

export type SafetyVerificationResult = {
  verified: boolean;
  message: string;
};

export type CreateGroupRequest = {
  name: string;
  retention: string;
};

export type JoinGroupRequest = {
  invite_code: string;
};

export type CreateChannelRequest = {
  server_name: string;
  name: string;
  kind: ChannelKind;
};

export type SavePreferencesRequest = {
  theme_id: string;
  template_id: string;
};

export type SelfMuteRequest = {
  muted: boolean;
};

export type SpeakerVolumeRequest = {
  participant_id: string;
  volume: number;
};

export type SendMessageRequest = {
  channel: string;
  body: string;
};

type TauriInvoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;

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
  devices: [
    { device_id: "alice-laptop", leaf_index: 1, local: true, authorized: true },
    { device_id: "alice-phone", leaf_index: 2, local: false, authorized: true },
  ],
  servers: [
    {
      name: "discrypt lab",
      role: "owner",
      channels: [
        {
          name: "#general",
          kind: "Text",
          retention_status: "7 day default; older messages lock, not vanish",
        },
        {
          name: "#ops",
          kind: "Text",
          retention_status: "shorten is retroactive; lengthen is future-only",
        },
        {
          name: "Voice Lobby",
          kind: "Voice",
          retention_status:
            "Session-state only; media-frame E2E gate required before production voice claims",
        },
      ],
    },
  ],
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
    route: "STUN → peer relay overlay → TURN",
    relay_copy:
      "Relays see SFrame ciphertext only in harness gates; production media waits for real audio-frame E2E",
    android_path:
      "Android uses encoded transforms when available, otherwise the native webrtc-rs contingency",
  },
  voice_session: {
    joined: true,
    participants: [
      {
        id: "alice",
        name: "Alice",
        role: "you",
        speaking: true,
        muted: false,
        volume: 82,
      },
      {
        id: "bob",
        name: "Bob",
        role: "friend",
        speaking: true,
        muted: false,
        volume: 68,
      },
      {
        id: "ops",
        name: "Ops relay",
        role: "route",
        speaking: false,
        muted: true,
        volume: 38,
      },
    ],
    status_copy:
      "Voice session state is command-backed; real audio-frame transport remains release-gated",
    route_copy:
      "Route copy is harness-backed until socket/media adapter E2E passes",
  },
  preferences: { theme_id: "graphite-calm", template_id: "command-center" },
  messages: [
    {
      id: "local-msg-1",
      channel: "#general",
      author: "Alice",
      body: "Local-first command-backed timeline is persisted by AppStore.",
      state:
        "plaintext allowed by current local retention cache; encrypted envelope facade recorded by harness",
    },
    {
      id: "locked-msg-1",
      channel: "#general",
      author: "Bob",
      body: "Locked placeholder — author device must be online for a live-key request.",
      state: "locked",
    },
  ],
  activity_feed: [
    "Demo fallback active: packaged Tauri builds must use IPC-backed commands",
    "Invite policy checked: expiry + max-use + revoke controls",
    "Android wake path is content-free",
    "Relay route carries ciphertext only in harness gates",
    "Deletion copy includes offline-device caveat",
  ],
  connectivity: {
    fallback_chain:
      "STUN → relay-overlay → TURN; owner endpoints may override defaults",
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

function invokeOrFallback<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  fallback: () => T,
): Promise<T> {
  const tauriInvoke = window.__TAURI__?.core?.invoke;
  if (!tauriInvoke) {
    return Promise.resolve(fallback());
  }
  return tauriInvoke<T>(command, args);
}

export async function loadAppSnapshot(): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>(
    "app_snapshot",
    undefined,
    () => fallbackSnapshot,
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
        request.friend_id === fallbackSnapshot.friend.friend_code &&
        request.provided === fallbackSnapshot.friend.safety_number;
      fallbackSnapshot.friend.verified = verified;
      return {
        verified,
        message: verified
          ? "Safety number verified; MITM risk accepted by explicit user comparison"
          : "Safety number mismatch; do not trust this device or DM",
      };
    },
  );
}

export async function createGroup(
  request: CreateGroupRequest,
): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>("create_group", { request }, () => {
    fallbackSnapshot.servers.unshift({
      name: request.name || "private lab",
      role: "owner",
      channels: [
        {
          name: "#general",
          kind: "Text",
          retention_status: `${request.retention || fallbackSnapshot.retention.selected}; older messages lock, not vanish`,
        },
        {
          name: "Voice Lobby",
          kind: "Voice",
          retention_status:
            "Session-state only; media-frame E2E gate required before production voice claims",
        },
      ],
    });
    return fallbackSnapshot;
  });
}

export async function joinGroup(
  request: JoinGroupRequest,
): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>("join_group", { request }, () => {
    fallbackSnapshot.servers.unshift({
      name: request.invite_code.includes("enclave")
        ? "joined enclave"
        : "joined group",
      role: "member",
      channels: [
        {
          name: "#general",
          kind: "Text",
          retention_status: `${fallbackSnapshot.retention.selected}; older messages lock, not vanish`,
        },
        {
          name: "Voice Lobby",
          kind: "Voice",
          retention_status:
            "Session-state only; media-frame E2E gate required before production voice claims",
        },
      ],
    });
    return fallbackSnapshot;
  });
}

export async function createChannel(
  request: CreateChannelRequest,
): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>("create_channel", { request }, () => {
    const server =
      fallbackSnapshot.servers.find(
        (item) => item.name === request.server_name,
      ) ?? fallbackSnapshot.servers[0];
    const name =
      request.kind === "Text"
        ? `#${request.name.replace(/^#/, "") || "secure-room"}`
        : request.name || "Voice Lobby";
    if (!server.channels.some((channel) => channel.name === name)) {
      server.channels.push({
        name,
        kind: request.kind,
        retention_status: `${fallbackSnapshot.retention.selected}; older messages lock, not vanish`,
      });
    }
    return fallbackSnapshot;
  });
}

export async function savePreferences(
  request: SavePreferencesRequest,
): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>("save_preferences", { request }, () => {
    fallbackSnapshot.preferences = request;
    return fallbackSnapshot;
  });
}

export async function joinVoice(): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>("join_voice", undefined, () => {
    fallbackSnapshot.voice_session.joined = true;
    return fallbackSnapshot;
  });
}

export async function leaveVoice(): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>("leave_voice", undefined, () => {
    fallbackSnapshot.voice_session.joined = false;
    return fallbackSnapshot;
  });
}

export async function setSelfMute(
  request: SelfMuteRequest,
): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>("set_self_mute", { request }, () => {
    fallbackSnapshot.voice_session.participants =
      fallbackSnapshot.voice_session.participants.map((participant) =>
        participant.id === "alice"
          ? {
              ...participant,
              muted: request.muted,
              speaking: fallbackSnapshot.voice_session.joined && !request.muted,
            }
          : participant,
      );
    return fallbackSnapshot;
  });
}

export async function setSpeakerVolume(
  request: SpeakerVolumeRequest,
): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>(
    "set_speaker_volume",
    { request },
    () => {
      fallbackSnapshot.voice_session.participants =
        fallbackSnapshot.voice_session.participants.map((participant) =>
          participant.id === request.participant_id
            ? { ...participant, volume: request.volume }
            : participant,
        );
      return fallbackSnapshot;
    },
  );
}

export async function sendMessage(
  request: SendMessageRequest,
): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>("send_message", { request }, () => {
    fallbackSnapshot.messages.push({
      id: `fallback-${fallbackSnapshot.messages.length + 1}`,
      channel: request.channel,
      author: "Alice",
      body: request.body,
      state:
        "plaintext allowed by current local retention cache; encrypted envelope facade recorded by harness",
    });
    return fallbackSnapshot;
  });
}

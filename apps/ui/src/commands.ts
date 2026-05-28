export type ChannelKind = 'Text' | 'Voice';

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
  connectivity: ConnectivityView;
  security_copy: SecurityCopyView;
};

type TauriInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

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
    alias: 'Bob',
    friend_code: 'friend:bob:stable-fixture',
    safety_number: '0231 1597 2653 5897',
    verified: false,
  },
  devices: [
    { device_id: 'alice-laptop', leaf_index: 1, local: true, authorized: true },
    { device_id: 'alice-phone', leaf_index: 2, local: false, authorized: true },
  ],
  servers: [
    {
      name: 'discrypt lab',
      role: 'owner',
      channels: [
        { name: '#general', kind: 'Text', retention_status: '7 day default; older messages lock, not vanish' },
        { name: '#ops', kind: 'Text', retention_status: 'shorten is retroactive; lengthen is future-only' },
        { name: 'Voice Lobby', kind: 'Voice', retention_status: 'SFrame media; relays carry ciphertext only' },
      ],
    },
  ],
  invite: {
    expires: 'Invite expires and can be revoked',
    max_use: 'Max-use is enforced before MLS admission',
    password_gate: 'Password rooms use OPAQUE/PAKE or an online authorized helper; no offline verifier',
    welcome_required: 'Final admission still requires an authorized MLS Welcome/add',
  },
  retention: {
    presets: ['1 hour', '24 hours', '7 days', '30 days', '90 days', 'custom', 'warned unlimited / never-lock'],
    selected: '7 days',
    unlimited_warning: 'Unlimited keeps local keys longer and weakens lock behavior; opt in explicitly',
    transition_copy: 'Shortening re-locks older messages retroactively; lengthening applies only to future messages',
  },
  voice: {
    route: 'STUN → peer relay overlay → TURN',
    relay_copy: 'Relays see SFrame ciphertext only and active tamper is rejected',
    android_path: 'Android uses encoded transforms when available, otherwise the native webrtc-rs contingency',
  },
  connectivity: {
    fallback_chain: 'STUN → relay-overlay → TURN; owner endpoints may override defaults',
    metadata_copy: 'Content-private and metadata-minimizing, not metadata-anonymous',
    push_copy: 'Android FCM wake is content-free and carries no room, sender, or message body',
  },
  security_copy: {
    metadata: 'Passive infrastructure can see IPs and timing; discrypt does not claim anonymity',
    deletion: 'Deleted on your online devices now; pending on offline devices until they reconnect',
    malicious_member: 'Crypto-shred cannot erase screenshots, exports, modified clients, or plaintext already saved by a recipient',
  },
};

export async function loadAppSnapshot(): Promise<AppSnapshot> {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    return fallbackSnapshot;
  }
  return invoke<AppSnapshot>('app_snapshot');
}


export type SafetyVerificationRequest = {
  friend_id: string;
  provided: string;
};

export type SafetyVerificationResult = {
  verified: boolean;
  message: string;
};

export async function verifySafetyNumber(request: SafetyVerificationRequest): Promise<SafetyVerificationResult> {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    const verified = request.friend_id === fallbackSnapshot.friend.friend_code && request.provided === fallbackSnapshot.friend.safety_number;
    return {
      verified,
      message: verified
        ? 'Safety number verified; MITM risk accepted by explicit user comparison'
        : 'Safety number mismatch; do not trust this device or DM',
    };
  }
  return invoke<SafetyVerificationResult>('verify_safety_number', { request });
}

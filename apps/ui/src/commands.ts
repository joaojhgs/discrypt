import { discryptUiConfig } from "./app-config";

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
  remote_audio_src?: string | null;
  media_stream_url?: string | null;
};

export type VoiceRemoteAudioStreamView = {
  participant_id: string;
  src: string;
};

export type VoiceRemoteAudioView = {
  participant_id: string;
  remote_peer_id: string;
  stream_id: string;
  audio_track_id: string;
  playback_element_id: string;
  local_audio_tracks_sent: number;
  received_audio_frames: number;
  attached_at_ms: number;
};

export type VoiceMediaRuntimeView = {
  runtime_id: string;
  boundary: string;
  local_capture_active: boolean;
  remote_transport_active: boolean;
  remote_audio: VoiceRemoteAudioView[];
  fail_closed_reason: string;
  status_copy: string;
  remote_audio_streams?: VoiceRemoteAudioStreamView[];
};

export type VoiceSignalingStateView = {
  session_id: string;
  local_peer_id: string;
  remote_peer_id: string;
  role: string;
  pending_local_signals: number;
  received_remote_signals: number;
  last_signal_kind?: string | null;
  status_copy: string;
};

const inactiveVoiceSignalingState: VoiceSignalingStateView = {
  session_id: "",
  local_peer_id: "",
  remote_peer_id: "",
  role: "not-started",
  pending_local_signals: 0,
  received_remote_signals: 0,
  last_signal_kind: null,
  status_copy: "Voice signaling has not started; no SDP or ICE has crossed backend state",
};

const inactiveVoiceMediaRuntime: VoiceMediaRuntimeView = {
  runtime_id: "voice-runtime:not-started",
  boundary: "not-started",
  local_capture_active: false,
  remote_transport_active: false,
  remote_audio: [],
  fail_closed_reason: "No voice media runtime has been started",
  status_copy: "Voice media runtime is not started; no capture or playback route is active",
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
  media_runtime?: VoiceMediaRuntimeView;
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
  sybil_resistance: string;
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

export type DmRuntimePeerView = {
  peer_id: string;
  role: string;
  is_local: boolean;
  source: string;
};

export type DirectConversationView = {
  dm_id: string;
  participant_id: string;
  display_name: string;
  local_only_copy: string;
  runtime_peers?: DmRuntimePeerView[];
  connectivity?: ConnectivityPolicyView | null;
};

export type ChannelStateView = {
  channel_id: string;
  name: string;
  kind: ChannelKind;
  retention_status: string;
  connectivity?: ConnectivityPolicyView | null;
};

export type GroupRuntimePeerView = {
  peer_id: string;
  role: string;
  is_local: boolean;
  source: string;
};

export type GroupRoleView = "owner" | "staff" | "member" | string;

export type GroupAdmissionModeView =
  | "automatic_when_authorized_online"
  | "manual_approval"
  | string;

export type GroupMemberView = {
  member_id: string;
  display_name: string;
  device_id?: string | null;
  role: GroupRoleView;
  status: "online" | "offline" | "unknown" | "revoked" | string;
  signer_public_key_hex?: string | null;
  joined_at: string;
  last_seen_at?: string | null;
  presence_expires_at?: string | null;
  revoked_at?: string | null;
  revoked_by?: string | null;
};

export type GroupRolePolicyView = {
  admission_mode: GroupAdmissionModeView;
  policy_epoch: number;
  updated_by: string;
  updated_at: string;
};

export type GroupAdmissionRequestView = {
  request_id: string;
  group_id: string;
  invite_id?: string | null;
  display_name: string;
  device_name?: string | null;
  member_identity: string;
  signer_public_key_hex: string;
  key_package: number[];
  status: "pending" | "approved" | "refused" | "superseded" | string;
  requested_at: string;
  decided_by?: string | null;
  decided_at?: string | null;
  decision_reason?: string | null;
  policy_epoch_at_request: number;
  admission_mode_at_request?: GroupAdmissionModeView | null;
};

export type GroupGovernanceLogEntryView = {
  event_id: string;
  group_id: string;
  event_kind: string;
  actor_member_id: string;
  target_member_id?: string | null;
  request_id?: string | null;
  role_before?: GroupRoleView | null;
  role_after?: GroupRoleView | null;
  created_at: string;
  summary: string;
};

export type GroupView = {
  group_id: string;
  name: string;
  /** Legacy/current-member role label; backend-authorized role state lives in members. */
  role: string;
  channels: ChannelStateView[];
  members?: GroupMemberView[];
  role_policy?: GroupRolePolicyView;
  admission_requests?: GroupAdmissionRequestView[];
  governance_log?: GroupGovernanceLogEntryView[];
  runtime_peers?: GroupRuntimePeerView[];
  connectivity?: ConnectivityPolicyView | null;
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

export type TextDeliveryReceiptView = {
  recipient_device_id: string;
  received_at_ms: number;
  envelope_ciphertext_hash: string;
  recipient_key_fingerprint: string;
};

export type TextDeliveryReceipt = {
  group_id_commitment: number[];
  message_id: string;
  recipient_leaf: number;
  recipient_device_id: string;
  received_at_ms: number;
  envelope_ciphertext_hash: number[];
  signature: number[];
};

export type TextRetentionMetadata = {
  policy: string;
  created_at_ms: number;
  expires_at_ms: number | null;
  delete_after_read: boolean;
};

export type TextMessageEnvelope = {
  version: number;
  group_id_commitment: number[];
  epoch: number;
  sender_leaf: number;
  sender_device_id: string;
  sequence: number;
  message_id: string;
  retention: TextRetentionMetadata;
  content_ciphertext: number[];
  signature: number[];
};

export type AppMessageView = {
  message_id: string;
  target: MessageTargetView;
  author_id: string;
  author: string;
  body: string;
  status: string;
  state_key: string;
  state_label: string;
  state_detail: string;
  peer_receipt: TextDeliveryReceiptView | null;
  sent_at: string;
};

export type TextStateView = {
  key: string;
  label: string;
  status: string;
  detail: string;
};

export type IceTurnServerView = {
  endpoint: string;
  credential_declared: boolean;
  credential_expires_at: string | null;
};


export type SignalingProfileView = {
  profile_id: string;
  adapter_kind: string;
  endpoints: string[];
  room_topic_commitment: string;
  trust_fingerprint: string;
  ttl_seconds: number;
  metadata_posture: string;
  rate_limit_policy: string;
  capabilities: string[];
  provider_policy_version: number;
  endpoint_allowlist_commitments: string[];
  provider_rotation_policy: string;
};

export type DmInviteBootstrapView = {
  inviter_identity_commitment: string;
  contact_token_commitment: string;
  reply_rendezvous_commitment: string;
};

export type GroupInviteBootstrapView = {
  group_identity_commitment: string;
  role_admission_policy_commitment: string;
  channel_policy_commitment: string;
};

export type InviteAdmissionSnapshotView = {
  group_id_commitment: string;
  group_commitment: string;
  admission_mode: GroupAdmissionModeView;
  policy_epoch: number;
  role_admission_policy_commitment: string;
  welcome_required: boolean;
};

export type InviteRevocationPolicyView = {
  revocable: boolean;
  revocation_authority_commitment: string;
  expiry_enforced: boolean;
  max_use_enforced: boolean;
};

export type InvitePasswordPolicyView = {
  required: boolean;
  protocol: "OnlineAuthorizedHelper" | "OpaquePakeReserved" | string;
  helper_id?: string | null;
  rate_limit_policy_commitment: string;
  offline_verifier_allowed: boolean;
};

export type ConnectivityPolicyView = {
  connectivity_schema_version: number;
  invite_kind: string;
  scope_id_commitment: string;
  signaling_profiles: SignalingProfileView[];
  ice_stun_servers: string[];
  ice_turn_servers: IceTurnServerView[];
  privacy_label: string;
  dm_bootstrap: DmInviteBootstrapView | null;
  group_bootstrap: GroupInviteBootstrapView | null;
};

export type TransportStatusView = {
  label: string;
  status: string;
  detail: string;
};

export type SignalingAdapterBoundaryView = {
  kind: string;
  cargo_feature: string;
  readiness: string;
  failure_class: string;
};

export type SignalingAdapterFallbackAttemptView = {
  kind: string;
  readiness: string;
  failure_class: string;
  attempted: boolean;
  selected: boolean;
};

export type TransportDiagnosticsView = {
  adapter_boundaries: SignalingAdapterBoundaryView[];
  adapter_fallback_attempts: SignalingAdapterFallbackAttemptView[];
  selected_adapter: string | null;
  route_proof_status: string;
  route_proof_detail: string;
  turn_required: string;
  adapter_probe_status: string;
  adapter_probe_detail: string;
  adapter_probe: SignalingAdapterProbeView | null;
  data_channel_probe_status: string;
  data_channel_probe_detail: string;
  data_channel_probe: ProviderWebRtcDataChannelProbeView | null;
};

export type SignalingAdapterProbeView = {
  kind: string;
  profile_id: string;
  endpoint_label: string;
  scope_commitment: string;
  rendezvous_topic: string;
  presence_roundtrip: boolean;
  signal_roundtrip: boolean;
  control_roundtrip: boolean;
};

export type ProviderWebRtcDataChannelProbeView = {
  kind: string;
  profile_id: string;
  endpoint_label: string;
  rendezvous_topic: string;
  offerer_direct_path_ready: boolean;
  answerer_direct_path_ready: boolean;
  offerer_turn_fallback_ready: boolean;
  answerer_turn_fallback_ready: boolean;
  offerer_configured_turn_servers: number;
  answerer_configured_turn_servers: number;
  offerer_local_relay_candidates_gathered: number;
  answerer_local_relay_candidates_gathered: number;
  offerer_remote_relay_candidates_applied: number;
  answerer_remote_relay_candidates_applied: number;
  offerer_data_channel_open: boolean;
  answerer_data_channel_open: boolean;
  text_control_frame_roundtrip: boolean;
  text_control_frame_sha256: string;
  receipt_frame_roundtrip: boolean;
  receipt_frame_sha256: string;
};

export type JoinProgressStepView = {
  key: string;
  label: string;
  status: string;
  detail: string;
};

export type VoiceStateView = {
  key: string;
  label: string;
  status: string;
  detail: string;
};

export type ServiceCapabilityView = {
  key: string;
  label: string;
  status: string;
  detail: string;
};

export type RuntimeModeView = {
  mode: string;
  production_labels_enabled: boolean;
  harness_badge: string;
  disabled_reason: string;
  services: ServiceCapabilityView[];
};

export type InviteView = {
  invite_id: string;
  invite_key: string;
  descriptor_schema_version?: number | null;
  group_id: string;
  dm_id?: string | null;
  connectivity_schema_version: number;
  invite_kind: string;
  scope_id_commitment: string;
  signaling_profiles: SignalingProfileView[];
  privacy_label: string;
  dm_bootstrap: DmInviteBootstrapView | null;
  group_bootstrap: GroupInviteBootstrapView | null;
  admission_snapshot?: InviteAdmissionSnapshotView | null;
  revocation_policy?: InviteRevocationPolicyView | null;
  password_policy?: InvitePasswordPolicyView | null;
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
  media_runtime: VoiceMediaRuntimeView;
  signaling: VoiceSignalingStateView;
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
  storage_security: StorageSecurityView;
  profile: UserProfileView | null;
  preferences: PreferencesView;
  dms: DirectConversationView[];
  groups: GroupView[];
  connectivity_defaults: ConnectivityPolicyView;
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
  transport_diagnostics: TransportDiagnosticsView;
  join_progress: JoinProgressStepView[];
  text_state_legend: TextStateView[];
  voice_states: VoiceStateView[];
  runtime_mode: RuntimeModeView;
  snapshot: AppSnapshot;
};

export type StorageSecurityStatus =
  | "setup_required"
  | "locked"
  | "ready"
  | "error";

export type StorageSecurityMode =
  | "unconfigured"
  | "keyring"
  | "passphrase_vault"
  | "development_store"
  | "unknown";

export type StorageSecurityView = {
  status: StorageSecurityStatus | string;
  mode: StorageSecurityMode | string;
  title: string;
  detail: string;
  recovery_hint: string;
  password_required: boolean;
  keyring_available: boolean;
  keyring_detail: string;
};

export type ConfigureStorageSecurityRequest = {
  mode: "keyring" | "passphrase_vault";
  passphrase?: string | null;
};

export type UnlockStorageSecurityRequest = {
  passphrase: string;
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

export type SignalingAdapterKind =
  | "nostr"
  | "mqtt"
  | "ipfs_pubsub"
  | "discrypt_quic_rendezvous";

export type SetConnectivityPolicyRequest = {
  scope_kind: "app" | "dm" | "group" | "channel" | string;
  group_id?: string | null;
  channel_id?: string | null;
  dm_id?: string | null;
  adapter_kind?: SignalingAdapterKind | string | null;
  signaling_endpoint?: string | null;
  ice_stun_servers?: string[] | null;
  ice_turn_servers?: IceTurnServerView[] | null;
};

export type CreateGroupRequest = {
  name: string;
  retention: string;
  admission_mode?: GroupAdmissionModeView | null;
  adapter_kind?: SignalingAdapterKind | string | null;
  signaling_endpoint?: string | null;
  ice_stun_servers?: string[] | null;
  ice_turn_servers?: IceTurnServerView[] | null;
};

export type JoinGroupRequest = {
  invite_code: string;
  group_name?: string | null;
};

export type SetGroupAdmissionModeRequest = {
  group_id: string;
  admission_mode: GroupAdmissionModeView;
};

export type ApproveGroupAdmissionRequest = {
  group_id: string;
  request_id: string;
};

export type RefuseGroupAdmissionRequest = {
  group_id: string;
  request_id: string;
  reason?: string | null;
};

export type PromoteGroupMemberRequest = {
  group_id: string;
  member_id: string;
};

export type DemoteGroupStaffRequest = {
  group_id: string;
  member_id: string;
};

export type RevokeGroupMemberAccessRequest = {
  group_id: string;
  member_id: string;
  reason?: string | null;
};

export type PublishGroupPresenceRequest = {
  group_id: string;
  member_id?: string | null;
  status?: "online" | "offline" | "unknown" | string;
  ttl_seconds?: number | null;
};

export type GroupAdmissionDecisionRequest = ApproveGroupAdmissionRequest & {
  reason?: string | null;
};

export type GroupMemberActionRequest = {
  group_id: string;
  member_id: string;
  reason?: string | null;
};

export type SetActiveGroupRequest = {
  group_id: string;
};

export type SetActiveChannelRequest = {
  group_id: string;
  channel_id: string;
};

export type SetActiveDmRequest = {
  dm_id: string;
};

export type CreateInviteRequest = {
  group_id?: string | null;
  expires: string;
  max_use: string;
  password_gate?: string | null;
  revocation_state?: "active_revocable" | string | null;
};

export type CreateDmInviteRequest = {
  dm_id?: string | null;
  expires: string;
  max_use: string;
};

export type AcceptDmInviteRequest = {
  invite_code: string;
  display_name?: string | null;
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

export type StartSignalingSessionRequest = {
  scope_label?: string | null;
  adapter_probe?: boolean;
  data_channel_probe?: boolean;
  adapter_kind?: string | null;
};

export type StopSignalingSessionRequest = {
  session_id?: string | null;
};

export type StartTextSessionRequest = {
  scope_label?: string | null;
  data_channel_probe?: boolean;
  adapter_kind?: string | null;
};

export type StopTextSessionRequest = {
  session_id?: string | null;
};

export type AttachTextControlTransportRuntimeRequest = {
  session_id?: string | null;
  runtime_role?: "offerer" | "answerer" | null;
  local_peer_id?: string | null;
  remote_peer_id?: string | null;
  derive_from_state?: boolean;
};

export type SendMessageRequest = {
  target: MessageTargetView;
  body: string;
  transport_proof?: boolean;
  adapter_kind?: string | null;
};

export type ApplyTextDeliveryReceiptRequest = {
  message_id: string;
  receipt: TextDeliveryReceipt;
  recipient_verifying_key_hex: string;
};

export type ReceiveTextDeliveryEnvelopeRequest = {
  target: MessageTargetView;
  envelope: TextMessageEnvelope;
  sender_verifying_key_hex: string;
  recipient_leaf?: number | null;
};

export type ReceiveTextDeliveryEnvelopeResponse = {
  state: AppState;
  receipt: TextDeliveryReceipt | null;
  recipient_verifying_key_hex: string | null;
};

export type VoiceSignalingMessageView = {
  signal_id: string;
  session_id: string;
  group_id: string;
  channel_id: string;
  sender_participant_id: string;
  sender_peer_id: string;
  recipient_peer_id: string;
  signal_kind: "offer" | "answer" | "candidate" | string;
  sealed_payload: string;
  created_at_ms: number;
};

export type PublishVoiceSignalingMessageRequest = {
  session_id: string;
  signal_kind: "offer" | "answer" | "candidate" | string;
  sealed_payload: string;
  signal_id?: string | null;
  created_at_ms: number;
};

export type TakePendingVoiceSignalingMessagesRequest = {
  session_id?: string | null;
  limit?: number | null;
};

export type TakePendingVoiceSignalingMessagesResponse = {
  state: AppState;
  messages: VoiceSignalingMessageView[];
};

export type NativeVoiceProtectedFrameView = {
  kid: number[];
  counter: number;
  bytes: number[];
};

export type NativeVoiceMediaSignalPayload = {
  schema_version: "discrypt.native_voice_media.v1" | string;
  session_id: string;
  group_id: string;
  channel_id: string;
  from_peer_id: string;
  to_peer_id: string;
  media_path: string;
  boundary: string;
  capture_source: string;
  rms_i16: number;
  peak_i16: number;
  speaking: boolean;
  opus_frames: number;
  protected_frames_count: number;
  opus_payload_bytes: number;
  protected_payload_bytes: number;
  protected_frames: NativeVoiceProtectedFrameView[];
  created_at_ms: number;
};

export type StartNativeVoiceMediaSessionRequest = {
  session_id: string;
  local_peer_id: string;
  remote_peer_id: string;
  muted?: boolean;
  created_at_ms: number;
};

export type StartNativeVoiceMediaSessionResponse = {
  state: AppState;
  native_media?: NativeVoiceMediaSignalPayload | null;
};

export type AcceptNativeVoiceMediaFrameRequest = {
  session_id: string;
  native_media: NativeVoiceMediaSignalPayload;
  attached_at_ms: number;
};

export type AcceptNativeVoiceMediaSignalRequest = {
  signal: VoiceSignalingMessageView;
  attached_at_ms: number;
};

export type TextControlFrameView =
  | {
      kind: "open_mls_admission_key_package";
      group_id: string;
      member_identity: string;
      signer_public_key_hex: string;
      key_package: number[];
    }
  | {
      kind: "group_admission_decision";
      group_id: string;
      request_id: string;
      approved: boolean;
      reason?: string | null;
      decided_by: string;
      decided_at: string;
    }
  | {
      kind: "open_mls_admission_welcome";
      group_id: string;
      owner_signer_public_key_hex: string;
      member_signer_public_key_hex: string;
      welcome_bytes: number[];
      epoch: number;
      confirmation_tag_sha256: string;
    }
  | {
      kind: "envelope";
      target: MessageTargetView;
      envelope: TextMessageEnvelope;
      sender_verifying_key_hex: string;
      recipient_leaf?: number | null;
    }
  | {
      kind: "group_member_role_changed";
      group_id: string;
      event_id: string;
      actor_member_id: string;
      target_member_id: string;
      role_before: GroupRoleView;
      role_after: GroupRoleView;
      created_at: string;
    }
  | {
      kind: "group_member_revoked";
      group_id: string;
      event_id: string;
      actor_member_id: string;
      target_member_id: string;
      reason?: string | null;
      created_at: string;
      crypto_removal_status: string;
      openmls_remove_commit?: number[] | null;
      openmls_epoch?: number | null;
      openmls_confirmation_tag_sha256?: string | null;
    }
  | {
      kind: "group_presence_heartbeat";
      group_id: string;
      event_id: string;
      member_id: string;
      display_name?: string | null;
      device_id?: string | null;
      role?: GroupRoleView | null;
      signer_public_key_hex?: string | null;
      last_seen_at: string;
      presence_expires_at: string;
    }
  | {
      kind: "group_governance_ack";
      group_id: string;
      event_id: string;
      applied_by_member_id: string;
      status: string;
      created_at: string;
    }
  | {
      kind: "voice_signal";
      signal: VoiceSignalingMessageView;
    }
  | {
      kind: "receipt";
      message_id: string;
      receipt: TextDeliveryReceipt;
      recipient_verifying_key_hex: string;
    };

export type HandleTextControlFrameRequest = {
  frame: TextControlFrameView;
};

export type HandleTextControlFrameResponse = {
  state: AppState;
  response_frame: TextControlFrameView | null;
};

export type TextControlOutboxFrameView = {
  message_id: string;
  target: MessageTargetView;
  frame: TextControlFrameView;
  state_key: string;
  attempts: number;
  last_transport_session_id: string | null;
  frame_sha256: string;
};

export type ListPendingTextControlFramesRequest = {
  target?: MessageTargetView | null;
  limit?: number | null;
  operation_timeout_ms?: number | null;
};

export type ListPendingTextControlFramesResponse = {
  state: AppState;
  frames: TextControlOutboxFrameView[];
};

export type WebRtcDataTransportMetrics = {
  schema_version: number;
  label: string;
  attached_channels: number;
  open: boolean;
  frames_sent: number;
  frames_received: number;
  bytes_sent: number;
  bytes_received: number;
  last_state: string;
};

export type TextControlTransportPumpReportView = {
  pending_before: number;
  frames_sent: number;
  response_frames_received: number;
  receipts_applied: number;
  failures: string[];
  metrics: WebRtcDataTransportMetrics;
};

export type MarkTextControlFrameSentRequest = {
  message_id: string;
  frame_sha256: string;
  transport_session_id?: string | null;
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

export type UpdateVoiceActivityRequest = {
  session_id: string;
  rms_i16: number;
  peak_i16: number;
  captured_at_ms: number;
};

export type AttachVoiceRemoteMediaRequest = {
  session_id: string;
  participant_id: string;
  participant_name: string;
  remote_peer_id: string;
  stream_id: string;
  audio_track_id: string;
  playback_element_id: string;
  local_audio_tracks_sent: number;
  received_audio_frames: number;
  speaking?: boolean;
  attached_at_ms: number;
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

type TauriEventCallback<T> = (event: { payload: T }) => void;
type TauriUnlisten = () => void;
type TauriListen = <T>(
  event: string,
  handler: TauriEventCallback<T>,
) => Promise<TauriUnlisten>;

const LOCAL_DEV_FALLBACK_ENABLED =
  import.meta.env.DEV ||
  import.meta.env.VITE_DISCRYPT_LOCAL_DEV_FALLBACK === "1";
const FIRST_RUN_STORAGE_E2E_KEY = "discrypt:e2e:first-run-storage-setup";
const FALLBACK_STORAGE_KEY = "discrypt.local-dev.app-state.v1";

const fallbackFriendIdentity = createFallbackFriendIdentity("New contact");

declare global {
  interface Window {
    __TAURI__?: {
      core?: {
        invoke?: TauriInvoke;
      };
      event?: {
        listen?: TauriListen;
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
    media_runtime: inactiveVoiceMediaRuntime,
    participants: [],
    status_copy: "Not joined; backend voice controls are idle",
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
      "Backend policy: direct WebRTC P2P, or explicit TURN when configured; MQTT/Nostr providers only signal SDP/candidates and never relay messages.",
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
      "Crypto-shred cannot erase screenshots, exports, modified clients, or plaintext already saved by a recipient. Authorized members can still infer some liveness from archival live-key behavior; this is not metadata anonymity.",
    sybil_resistance:
      "Abuse controls slow invite creation, invite use, admission-helper attempts, signaling publish/take, text bursts, and relay freeloading. They do not solve Sybil attacks without a central identity or reputation service; one person can still create many accounts or devices.",
  },
};

const fallbackState: AppState = {
  schema_version: 1,
  lifecycle: "first_run",
  storage_security: {
    status: "ready",
    mode: "development_store",
    title: "Development storage",
    detail: "Local-dev fallback storage is active for this browser harness.",
    recovery_hint: "Run the packaged Tauri app to configure production keyring or password-vault storage.",
    password_required: false,
    keyring_available: true,
    keyring_detail: "Development builds do not require OS-keyring preflight.",
  },
  profile: null,
  preferences: fallbackSnapshot.preferences,
  dms: [],
  groups: [],
  connectivity_defaults: appConnectivityDefaults(),
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
  transport_diagnostics: {
    adapter_boundaries: [],
    adapter_fallback_attempts: [],
    selected_adapter: null,
    route_proof_status: "route-proof-not-available",
    route_proof_detail:
      "Tauri IPC is not connected; local fallback cannot prove backend transport routes",
    turn_required: "not-proven",
    adapter_probe_status: "provider-roundtrip-not-run",
    adapter_probe_detail:
      "Tauri IPC is not connected; local fallback cannot run a provider adapter probe",
    adapter_probe: null,
    data_channel_probe_status: "webrtc-datachannel-not-run",
    data_channel_probe_detail:
      "Tauri IPC is not connected; local fallback cannot run a provider-signaled WebRTC DataChannel probe",
    data_channel_probe: null,
  },
  join_progress: [],
  text_state_legend: textStateLegend(),
  voice_states: [],
  runtime_mode: fallbackRuntimeMode(),
  snapshot: fallbackSnapshot,
};

let fallbackHydrated = false;

function isoNow(): string {
  return new Date().toISOString();
}

function normalizedAdmissionMode(
  value?: GroupAdmissionModeView | null,
): GroupAdmissionModeView {
  return value === "automatic_when_authorized_online"
    ? "automatic_when_authorized_online"
    : "manual_approval";
}

function localMemberId(state: AppState): string {
  return state.profile?.user_id ?? "local-profile-pending";
}

function governanceDisplayName(state: AppState): string {
  return state.profile?.display_name ?? state.snapshot.friend.alias ?? "Local user";
}

function ensureGroupGovernance(
  state: AppState,
  group: GroupView,
  roleOverride?: GroupRoleView,
): GroupView {
  const now = isoNow();
  const localId = localMemberId(state);
  const localRole = roleOverride ?? group.role ?? "member";
  group.members ??= [];
  if (!group.members.some((member) => member.member_id === localId)) {
    group.members.push({
      member_id: localId,
      display_name: governanceDisplayName(state),
      device_id: state.profile?.device_name ?? null,
      role: localRole,
      status: "online",
      signer_public_key_hex: null,
      joined_at: now,
      last_seen_at: now,
      presence_expires_at: new Date(Date.now() + 5 * 60_000).toISOString(),
      revoked_at: null,
      revoked_by: null,
    });
  }
  group.role_policy ??= {
    admission_mode: "manual_approval",
    policy_epoch: 1,
    updated_by: localId,
    updated_at: now,
  };
  group.admission_requests ??= [];
  group.governance_log ??= [];
  if (!group.governance_log.some((entry) => entry.event_kind === "group.created")) {
    group.governance_log.push({
      event_id: `governance-${group.group_id}-created`,
      group_id: group.group_id,
      event_kind: "group.created",
      actor_member_id: localId,
      target_member_id: localId,
      request_id: null,
      role_before: null,
      role_after: localRole,
      created_at: now,
      summary: `Initialized ${group.name} governance state`,
    });
  }
  return group;
}

function governanceLog(
  state: AppState,
  group: GroupView,
  entry: Omit<GroupGovernanceLogEntryView, "event_id" | "group_id" | "created_at"> & {
    created_at?: string;
  },
): void {
  group.governance_log ??= [];
  const createdAt = entry.created_at ?? isoNow();
  group.governance_log.push({
    event_id: stableHash(
      `${group.group_id}:${entry.event_kind}:${entry.actor_member_id}:${entry.target_member_id ?? ""}:${entry.request_id ?? ""}:${createdAt}`,
    ),
    group_id: group.group_id,
    created_at: createdAt,
    ...entry,
  });
  pushEvent(state, `group.${entry.event_kind}`, entry.summary);
}

function findGovernedGroup(state: AppState, groupId: string): GroupView {
  const group = state.groups.find((item) => item.group_id === groupId);
  if (!group) throw new Error("Requested group does not exist");
  return ensureGroupGovernance(state, group);
}

function governanceLocalRoleForGroup(state: AppState, group: GroupView): string {
  const local = group.members?.find((member) => member.member_id === localMemberId(state));
  return local?.role ?? group.role ?? "member";
}

function canModerateAdmissions(state: AppState, group: GroupView): boolean {
  return ["owner", "staff"].includes(governanceLocalRoleForGroup(state, group));
}

function canPromoteMembers(state: AppState, group: GroupView): boolean {
  return governanceLocalRoleForGroup(state, group) === "owner";
}

function canRevokeMember(state: AppState, group: GroupView, target: GroupMemberView): boolean {
  const role = governanceLocalRoleForGroup(state, group);
  if (target.member_id === localMemberId(state)) return false;
  if (role === "owner") return target.role !== "owner";
  if (role === "staff") return target.role === "member";
  return false;
}

function defaultVoiceMediaRuntime(
  sessionId: string,
  joined: boolean,
): VoiceMediaRuntimeView {
  return joined
    ? {
        runtime_id: `voice-runtime:${sessionId}`,
        boundary: "webview-local-capture",
        local_capture_active: true,
        remote_transport_active: false,
        remote_audio: [],
        fail_closed_reason:
          "Remote WebRTC audio transport is not attached; backend state proves playback claims remain gated until media-route evidence exists",
        status_copy:
          "Local microphone capture admitted through backend session boundary; remote playback remains disabled until a real media transport attaches",
      }
    : {
        runtime_id: `voice-runtime:${sessionId}`,
        boundary: "stopped",
        local_capture_active: false,
        remote_transport_active: false,
        remote_audio: [],
        fail_closed_reason: "",
        status_copy:
          "Voice media runtime stopped by leave; local tracks and remote playback are inactive",
      };
}

function normalizeVoiceSessionRuntime(state: AppState): void {
  if (!state.voice_session) return;
  state.voice_session.media_runtime ??= defaultVoiceMediaRuntime(
    state.voice_session.session_id,
    state.voice_session.joined,
  );
  state.voice_session.media_runtime.remote_audio ??= [];
  state.voice_session.signaling ??= {
    ...inactiveVoiceSignalingState,
    session_id: state.voice_session.session_id,
  };
}

function clearNonPersistentVoiceRuntime(state: AppState): void {
  if (state.active_context?.kind === "voice_channel") {
    state.active_context = null;
  }
  state.voice_session = null;
}

function cloneState(state: AppState): AppState {
  return structuredClone(state);
}

function readStoredFallbackState(): AppState | null {
  if (typeof window === "undefined") return null;
  try {
    const stored = window.localStorage.getItem(FALLBACK_STORAGE_KEY);
    if (!stored) return null;
    const parsed = JSON.parse(stored) as AppState;
    if (parsed?.schema_version !== fallbackState.schema_version) return null;
    return parsed;
  } catch {
    return null;
  }
}

function hydrateFallbackState(): void {
  if (fallbackHydrated) return;
  fallbackHydrated = true;
  const stored = readStoredFallbackState();
  if (!stored) return;
  clearNonPersistentVoiceRuntime(stored);
  Object.assign(fallbackState, stored);
  ensureGroupGovernanceDefaults(fallbackState);
  syncSnapshot(fallbackState);
}

function applyFirstRunStorageE2eState(state: AppState): void {
  if (typeof window === "undefined") return;
  if (window.localStorage.getItem(FIRST_RUN_STORAGE_E2E_KEY) !== "1") return;
  if (state.lifecycle !== "first_run") return;
  state.storage_security = {
    status: "unconfigured",
    mode: "unconfigured",
    title: "Choose local storage protection",
    detail:
      "Select the OS keyring or a Discrypt password vault before account setup.",
    recovery_hint:
      "Existing unreadable storage is preserved, not restored or overwritten; setup must configure storage before creating identity state.",
    password_required: false,
    keyring_available: true,
    keyring_detail:
      "E2E storage setup hook; packaged builds report real keyring preflight.",
  };
}

function persistFallbackState(): void {
  if (typeof window === "undefined") return;
  try {
    const persisted = cloneState(syncSnapshot(fallbackState));
    clearNonPersistentVoiceRuntime(persisted);
    window.localStorage.setItem(
      FALLBACK_STORAGE_KEY,
      JSON.stringify(syncSnapshot(persisted)),
    );
  } catch {
    // Local-dev fallback persistence is best-effort; Tauri IPC-backed builds use
    // the Rust storage boundary.
  }
}

function syncSnapshot(state: AppState): AppState {
  normalizeVoiceSessionRuntime(state);
  ensureGroupGovernanceDefaults(state);
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
        media_runtime: state.voice_session.media_runtime,
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
        media_runtime: inactiveVoiceMediaRuntime,
        participants: [],
        status_copy: "Not joined; backend voice controls are idle",
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
  state.transport_diagnostics = deriveTransportDiagnostics(state);
  state.join_progress = deriveJoinProgress(state);
  state.text_state_legend = textStateLegend();
  state.voice_states = deriveVoiceStates(state);
  state.runtime_mode = fallbackRuntimeMode();
  return state;
}

function fallbackRuntimeMode(): RuntimeModeView {
  return {
    mode: "local-dev-harness",
    production_labels_enabled: false,
    harness_badge: "local-dev / harness mode",
    disabled_reason:
      "Production labels disabled until backend state proves network, media, and storage services are configured",
    services: [
      {
        key: "network",
        label: "Network services",
        status: "not-configured",
        detail: "Signaling/relay service labels require configured network features and backend state",
      },
      {
        key: "media",
        label: "Media services",
        status: "not-configured",
        detail: "Voice media labels require configured media features and route evidence",
      },
      {
        key: "storage",
        label: "Storage services",
        status: "not-configured",
        detail: "Storage service labels require production storage feature on supported targets",
      },
    ],
  };
}

function deriveVoiceStates(state: AppState): VoiceStateView[] {
  const session = state.voice_session;
  const joined = Boolean(session?.joined);
  const permission = session?.microphone_permission ?? "unknown";
  const muted = Boolean(session?.self_muted);
  const speaking = Boolean(
    session?.participants.some((participant) => participant.speaking && !participant.muted),
  );
  const hasTurn = Boolean(state.invites.at(-1)?.ice_turn_servers.length);
  return [
    {
      key: "permission_needed",
      label: "Permission needed",
      status: permission === "granted" ? "granted" : "needed",
      detail: "Microphone permission must be granted before capture starts",
    },
    {
      key: "joining",
      label: "Joining",
      status: joined ? "joined" : "idle",
      detail: "Join creates backend voice state and records selected devices",
    },
    {
      key: "ice_checking",
      label: "ICE checking",
      status: joined ? "waiting-route-proof" : "idle",
      detail: "ICE checks require route metrics from transport state before success is displayed",
    },
    {
      key: "route",
      label: "Direct / overlay / TURN",
      status: joined ? (hasTurn ? "turn-configured" : "policy-only") : "idle",
      detail:
        "Direct, overlay, and TURN route labels stay policy-only until backend route evidence exists",
    },
    {
      key: "muted",
      label: "Muted",
      status: muted ? "muted" : "unmuted",
      detail: "Mute state is backend persisted and suppresses outbound local media frames",
    },
    {
      key: "speaking",
      label: "Speaking",
      status: speaking ? "active" : "silent",
      detail: "Speaking indicators come from participant audio-level state returned by the backend",
    },
    {
      key: "reconnecting",
      label: "Reconnecting",
      status: "idle",
      detail: "Reconnect state appears only when transport events report retry/backoff activity",
    },
    {
      key: "left",
      label: "Left",
      status: joined ? "not-left" : "left-or-not-joined",
      detail: "Leaving clears the local joined state and keeps no fabricated remote roster",
    },
  ];
}

function textStateLegend(): TextStateView[] {
  return [
    {
      key: "pending",
      label: "Pending",
      status: "available",
      detail: "Message is queued before local author-log append or transport attempt",
    },
    {
      key: "sent_local",
      label: "Sent locally",
      status: "current-send-state",
      detail:
        "Message is in the local encrypted author log; peer receipt requires backend-state proof",
    },
    {
      key: "peer_receipt",
      label: "Peer receipt",
      status: "requires-signed-receipt",
      detail:
        "Delivered to peer is shown only with backend-state signed receipt proof",
    },
    {
      key: "received",
      label: "Received",
      status: "available",
      detail: "Inbound messages use this state after membership, epoch, and ordering checks",
    },
    {
      key: "failed",
      label: "Failed",
      status: "available",
      detail: "Send or decrypt failures must retain the command error/recovery reason",
    },
    {
      key: "locked",
      label: "Locked",
      status: "available",
      detail: "Retention or key-lock policy can hide plaintext until authorized unlock",
    },
    {
      key: "shredded",
      label: "Shredded",
      status: "available",
      detail: "Crypto-shred/key deletion state; remote screenshots or exports are not erased",
    },
  ];
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


function transportTurnRequiredStatus(turnRequired: string): boolean {
  const normalized = turnRequired.toLowerCase();
  return (
    /required|needed|must|relay-only/.test(normalized) &&
    !/not|none|false|unproven/.test(normalized)
  );
}

function providerFallbackState(
  attempts: SignalingAdapterFallbackAttemptView[],
  selectedAdapter: string | null,
): { status: string; detail: string } {
  if (selectedAdapter) {
    return {
      status: "selected",
      detail: `Selected provider ${selectedAdapter} from backend fallback diagnostics`,
    };
  }
  if (!attempts.length) {
    return {
      status: "no-healthy-adapter",
      detail: "No backend-selected provider adapter is available; no retry/backoff attempts have been reported yet",
    };
  }
  const attempted = attempts.filter((attempt) => attempt.attempted).length;
  const failed = attempts.filter(
    (attempt) =>
      attempt.attempted &&
      !attempt.selected &&
      /fail|unavailable|error|timeout|denied/i.test(
        `${attempt.readiness} ${attempt.failure_class}`,
      ),
  ).length;
  if (failed > 0 && failed === attempted) {
    return {
      status: "provider-failed",
      detail: `${failed} provider fallback attempt${failed === 1 ? "" : "s"} failed or unavailable; retry/backoff must remain degraded until backend selects a healthy adapter`,
    };
  }
  return {
    status: "retrying-fallback",
    detail: `${attempted} provider fallback attempt${attempted === 1 ? "" : "s"} reported; retry/backoff is in progress until backend diagnostics select an adapter`,
  };
}

function deriveTransportStatus(state: AppState): TransportStatusView[] {
  const latestInvite = state.invites.at(-1) ?? null;
  const hasGroup = state.groups.length > 0;
  const voiceJoined = Boolean(state.voice_session?.joined);
  const hasStun = Boolean(latestInvite?.ice_stun_servers.length);
  const hasTurn = Boolean(latestInvite?.ice_turn_servers.length);
  const lastError = state.last_command_error;
  const selectedAdapter = state.transport_diagnostics?.selected_adapter ?? null;
  const fallbackAttempts =
    state.transport_diagnostics?.adapter_fallback_attempts ?? [];
  const fallbackState = providerFallbackState(fallbackAttempts, selectedAdapter);
  const fallbackAttemptCopy = fallbackAttempts.length
    ? fallbackAttempts
        .map(
          (attempt) =>
            `${attempt.kind}:${attempt.readiness}:${
              attempt.selected
                ? "selected"
                : attempt.attempted
                  ? "attempted"
                  : "skipped"
            }`,
        )
        .join(", ")
    : "no backend fallback attempts available";
  const turnRequired = state.transport_diagnostics?.turn_required ?? "not-proven";
  const turnRequiredNow = transportTurnRequiredStatus(turnRequired);
  const credentialedTurn = latestInvite?.ice_turn_servers.filter(
    (server) => server.credential_declared,
  ).length ?? 0;
  return [
    {
      label: "signaling",
      status: latestInvite ? "signed-endpoint-ready" : "waiting-for-invite",
      detail: latestInvite
        ? `Signed endpoint ${latestInvite.signaling_endpoint} with trust fingerprint ${latestInvite.signaling_trust_fingerprint}; no identity-room topology is stored by the signaling service`
        : "Create or paste an invite before signaling can be used",
    },
    {
      label: "adapter",
      status: fallbackState.status,
      detail: `${fallbackState.detail}; readiness/fallback attempts: ${fallbackAttemptCopy}`,
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
      status: turnRequiredNow
        ? hasTurn
          ? credentialedTurn > 0
            ? "credential-gated"
            : "turn-required"
          : "turn-required"
        : hasTurn
          ? "credential-gated"
          : "not-configured",
      detail: turnRequiredNow
        ? hasTurn
          ? `${credentialedTurn}/${latestInvite?.ice_turn_servers.length ?? 0} TURN endpoint(s) declare credentials; TURN-required route remains blocked until backend proves relay success`
          : "Backend diagnostics report TURN required but no TURN endpoint is configured; transport must fail closed"
        : "TURN endpoints are redacted from signed invite metadata and are credential-gated; they are not treated as active without backend route evidence",
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
      status: fallbackState.status === "retrying-fallback" ? "retrying-fallback" : "idle",
      detail:
        fallbackState.status === "retrying-fallback"
          ? fallbackState.detail
          : "Reconnect orchestration is displayed only when event state reports reconnect attempts",
    },
    {
      label: "failed",
      status: lastError ? "last-command-error" : "clear",
      detail: lastError?.recovery_hint ??
        "No failed transport command is currently reported",
    },
  ];
}

function deriveTransportDiagnostics(state: AppState): TransportDiagnosticsView {
  return {
    adapter_boundaries:
      state.transport_diagnostics?.adapter_boundaries?.length
        ? state.transport_diagnostics.adapter_boundaries
        : [
            {
              kind: "mqtt",
              cargo_feature: "mqtt-adapter",
              readiness: "local_fallback_unknown",
              failure_class: "tauri_ipc_unavailable",
            },
            {
              kind: "nostr",
              cargo_feature: "nostr-adapter",
              readiness: "implementation_unavailable",
              failure_class: "implementation_unavailable",
            },
            {
              kind: "ipfs_pubsub",
              cargo_feature: "ipfs-pubsub-adapter",
              readiness: "implementation_unavailable",
              failure_class: "implementation_unavailable",
            },
            {
              kind: "discrypt_quic_rendezvous",
              cargo_feature: "discrypt-quic-rendezvous-adapter",
              readiness: "implementation_unavailable",
              failure_class: "implementation_unavailable",
            },
          ],
    adapter_fallback_attempts:
      state.transport_diagnostics?.adapter_fallback_attempts ?? [],
    selected_adapter: state.transport_diagnostics?.selected_adapter ?? null,
    route_proof_status:
      state.transport_diagnostics?.route_proof_status ??
      "route-proof-not-available",
    route_proof_detail:
      state.transport_diagnostics?.route_proof_detail ??
      "Tauri IPC is not connected; local fallback cannot prove backend transport routes",
    turn_required: state.transport_diagnostics?.turn_required ?? "not-proven",
    adapter_probe_status:
      state.transport_diagnostics?.adapter_probe_status ??
      "provider-roundtrip-not-run",
    adapter_probe_detail:
      state.transport_diagnostics?.adapter_probe_detail ??
      "Tauri IPC is not connected; local fallback cannot run a provider adapter probe",
    adapter_probe: state.transport_diagnostics?.adapter_probe ?? null,
    data_channel_probe_status:
      state.transport_diagnostics?.data_channel_probe_status ??
      "webrtc-datachannel-not-run",
    data_channel_probe_detail:
      state.transport_diagnostics?.data_channel_probe_detail ??
      "Tauri IPC is not connected; local fallback cannot run a provider-signaled WebRTC DataChannel probe",
    data_channel_probe: state.transport_diagnostics?.data_channel_probe ?? null,
  };
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
      connectivity: null,
    },
    {
      channel_id: "channel-voice-lobby",
      name: "Voice Lobby",
      kind: "Voice",
      retention_status: "session",
      connectivity: null,
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

function localDisplayName(state: AppState): string {
  return state.profile?.display_name ?? "Local member";
}

function localDeviceId(state: AppState): string | null {
  return state.devices.find((device) => device.local)?.device_id ?? null;
}

function normalizeGroupRole(role: string | null | undefined): GroupRoleView {
  return role === "owner" || role === "staff" ? role : "member";
}

function initialGroupMember(
  state: AppState,
  role: GroupRoleView,
  joinedAt: string,
): GroupMemberView {
  return {
    member_id: localUserId(state),
    display_name: localDisplayName(state),
    device_id: localDeviceId(state),
    role,
    status: "unknown",
    signer_public_key_hex: null,
    joined_at: joinedAt,
    last_seen_at: null,
    presence_expires_at: null,
    revoked_at: null,
    revoked_by: null,
  };
}

function initialGroupRolePolicy(
  state: AppState,
  admissionMode: GroupAdmissionModeView | null | undefined,
  updatedAt: string,
): GroupRolePolicyView {
  return {
    admission_mode: admissionMode ?? "automatic_when_authorized_online",
    policy_epoch: 1,
    updated_by: localUserId(state),
    updated_at: updatedAt,
  };
}

function initialGroupGovernanceLog(
  state: AppState,
  groupId: string,
  role: GroupRoleView,
  createdAt: string,
): GroupGovernanceLogEntryView[] {
  const actor = localUserId(state);
  return [
    {
      event_id: `governance-${slugify(groupId)}-${slugify(actor)}-created`,
      group_id: groupId,
      event_kind: "group_created",
      actor_member_id: actor,
      target_member_id: actor,
      request_id: null,
      role_before: null,
      role_after: role,
      created_at: createdAt,
      summary: "Initialized group owner/staff/member governance roster",
    },
  ];
}

function ensureGroupGovernanceDefaults(state: AppState): void {
  const now = new Date().toISOString();
  for (const group of state.groups) {
    const role = normalizeGroupRole(group.role);
    group.members ??= [initialGroupMember(state, role, now)];
    if (group.members.length === 0) {
      group.members.push(initialGroupMember(state, role, now));
    }
    group.role_policy ??= initialGroupRolePolicy(
      state,
      "automatic_when_authorized_online",
      now,
    );
    group.role_policy.policy_epoch ||= 1;
    group.admission_requests ??= [];
    group.governance_log ??= initialGroupGovernanceLog(
      state,
      group.group_id,
      group.members.find((member) => member.member_id === localUserId(state))?.role ??
        role,
      now,
    );
    if (group.governance_log.length === 0) {
      group.governance_log.push(
        ...initialGroupGovernanceLog(state, group.group_id, role, now),
      );
    }
    const localMember = group.members.find(
      (member) => member.member_id === localUserId(state) && !member.revoked_at,
    );
    if (localMember) {
      group.role = normalizeGroupRole(localMember.role);
    }
  }
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

function inviteAdmissionSnapshot(
  groupId: string,
  connectivity: ConnectivityPolicyView,
): InviteAdmissionSnapshotView {
  return {
    group_id_commitment: hashCommitment("discrypt-signed-invite-group-id-v1", [
      groupId,
    ]),
    group_commitment: hashCommitment("discrypt-signed-invite-openmls-group-v1", [
      groupId,
    ]),
    admission_mode: "manual_approval",
    policy_epoch: 1,
    role_admission_policy_commitment:
      connectivity.group_bootstrap?.role_admission_policy_commitment ??
      hashCommitment("discrypt-group-admission-policy-commitment-v1", [groupId]),
    welcome_required: true,
  };
}

function inviteRevocationPolicy(
  groupId: string,
): InviteRevocationPolicyView {
  return {
    revocable: true,
    revocation_authority_commitment: hashCommitment(
      "discrypt-invite-revocation-authority-v1",
      [groupId, localUserId(fallbackState)],
    ),
    expiry_enforced: true,
    max_use_enforced: true,
  };
}

function invitePasswordPolicy(
  groupId: string,
): InvitePasswordPolicyView {
  return {
    required: true,
    protocol: "OnlineAuthorizedHelper",
    helper_id: `admission-helper-${groupId}`,
    rate_limit_policy_commitment: hashCommitment(
      "discrypt-invite-password-rate-limit-policy-v1",
      [groupId, localUserId(fallbackState)],
    ),
    offline_verifier_allowed: false,
  };
}

function groupNameFromGroupId(groupId: string | null | undefined): string | null {
  if (!groupId) return null;
  const raw = groupId.startsWith("group-") ? groupId.slice("group-".length) : groupId;
  const parts = raw.split("-").filter(Boolean);
  if (parts.length > 1 && /^\d+$/.test(parts.at(-1) ?? "")) parts.pop();
  const name = parts.join(" ").trim();
  return name && name !== "joined group" ? name : null;
}

function parseInviteGroupName(inviteCode: string): string {
  const [path] = inviteCode.trim().split("?", 2);
  const tail = path.split("/").filter(Boolean).at(-1) ?? "";
  if (/^[a-fA-F0-9-]{32,}$/.test(tail)) return "joined group";
  const name = tail.includes("-")
    ? tail.slice(tail.indexOf("-") + 1).replace(/-/g, " ")
    : "joined group";
  return name.trim() || "joined group";
}

function inviteGroupNameFromMetadata(
  inviteCode: string,
  requestedName: string | null | undefined,
  metadata: ParsedInviteMetadata | null,
): string {
  return (
    requestedName?.trim() ||
    metadata?.groupName?.trim() ||
    groupNameFromGroupId(metadata?.groupId) ||
    parseInviteGroupName(inviteCode)
  );
}


function decodeBase64UrlJson(payload: string): Record<string, any> | null {
  try {
    const normalized = payload.replace(/-/g, "+").replace(/_/g, "/");
    const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
    const binary = globalThis.atob(padded);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    return JSON.parse(new TextDecoder().decode(bytes));
  } catch {
    return null;
  }
}

function hexFromByteArray(value: unknown): string {
  return Array.isArray(value)
    ? value
        .map((byte) => Number(byte).toString(16).padStart(2, "0"))
        .join("")
    : "";
}

function endpointPolicyName(value: unknown): string {
  const label = String(value ?? "");
  return label === "ProductionTls"
    ? "production_tls"
    : label === "LocalDevLoopback"
      ? "local_dev_loopback"
      : label;
}

type ParsedInviteMetadata = {
  inviteKey: string;
  groupId?: string | null;
  groupName?: string | null;
  roomSecretHash: string;
  signalingEndpoint: string;
  signalingTrustFingerprint: string;
  signalingTrustStatus: string;
  endpointPolicy: string;
  iceStunServers: string[];
  iceTurnServers: IceTurnServerView[];
  connectivity: ConnectivityPolicyView;
  expiresAt: string;
  maxUses: number;
};

export function defaultSignalingEndpointForAdapter(
  adapterKind: SignalingAdapterKind | string,
  connectivity?: ConnectivityPolicyView,
): string {
  const matchingEndpoint = connectivity?.signaling_profiles.find(
    (profile) => profile.adapter_kind === adapterKind,
  )?.endpoints[0];
  if (matchingEndpoint) return matchingEndpoint;
  if (adapterKind === "mqtt") {
    return import.meta.env.VITE_DISCRYPT_DEFAULT_MQTT_ENDPOINT ??
      "mqtts://broker.emqx.io:8883";
  }
  if (adapterKind === "nostr") {
    return import.meta.env.VITE_DISCRYPT_DEFAULT_NOSTR_ENDPOINT ??
      "wss://relay.damus.io";
  }
  if (adapterKind === "ipfs_pubsub") {
    const configured =
      import.meta.env.VITE_DISCRYPT_DEFAULT_IPFS_BOOTSTRAP_ENDPOINT ??
      import.meta.env.VITE_DISCRYPT_DEFAULT_IPFS_BOOTSTRAP_ENDPOINTS ??
      "";
    return String(configured)
      .split(",")
      .map((endpoint: string) => endpoint.trim())
      .find(Boolean) ?? "";
  }
  if (adapterKind === "discrypt_quic_rendezvous") {
    return import.meta.env.VITE_DISCRYPT_DEFAULT_QUIC_RENDEZVOUS_ENDPOINT ?? "";
  }
  return import.meta.env.VITE_DISCRYPT_DEFAULT_NOSTR_ENDPOINT ??
    "wss://relay.damus.io";
}

function defaultSignalingEndpoint(connectivity?: ConnectivityPolicyView): string {
  const adapterKind = connectivity?.signaling_profiles[0]?.adapter_kind ?? "nostr";
  return defaultSignalingEndpointForAdapter(adapterKind, connectivity);
}

function endpointPolicyForSignalingEndpoint(endpoint: string): string {
  return /^(?:mqtt|ws|http|quic):\/\/(?:127\.0\.0\.1|localhost)(?::|\/|$)|^\/ip[46]\/(?:127\.0\.0\.1|::1)\//i.test(
    endpoint,
  )
    ? "local_dev_loopback"
    : "production_tls";
}

function defaultIceStunServers(): string[] {
  return ["stun:stun.l.google.com:19302"];
}

function defaultRedactedTurnServers(): IceTurnServerView[] {
  return [];
}

function hashCommitment(domain: string, parts: string[]): string {
  return stableHash(`${domain}:${parts.join(":")}`);
}

function signalingProfileForEndpoint(
  scopeCommitment: string,
  adapterKind: string,
  endpoint: string,
  profileId = `${adapterKind}-custom`,
): SignalingProfileView {
  return {
    profile_id: profileId,
    adapter_kind: adapterKind,
    endpoints: [endpoint],
    room_topic_commitment: hashCommitment(
      "discrypt-rendezvous-topic-commitment-v1",
      [scopeCommitment, adapterKind],
    ),
    trust_fingerprint: stableHash(
      `external-signaling-endpoint-fingerprint-v1:${endpoint}`,
    ),
    ttl_seconds: 300,
    metadata_posture: "hashed_topic",
    rate_limit_policy: "bounded publish/take with provider backoff",
    provider_policy_version: 1,
    endpoint_allowlist_commitments: [
      hashCommitment("discrypt-provider-endpoint-allowlist-v1", [
        adapterKind,
        endpoint,
      ]),
    ],
    provider_rotation_policy:
      "rotate by issuing a fresh signed invite/connectivity policy when endpoint trust, rate limits, or availability changes",
    capabilities: [
      "presence_ttl",
      "trickle_ice",
      "broadcast_control",
      "health_telemetry",
    ],
  };
}

function defaultSignalingProfiles(scopeCommitment: string): SignalingProfileView[] {
  const endpoints: Array<[string, string]> = [
    [
      "nostr",
      import.meta.env.VITE_DISCRYPT_DEFAULT_NOSTR_ENDPOINT ??
        "wss://relay.damus.io",
    ],
    [
      "mqtt",
      import.meta.env.VITE_DISCRYPT_DEFAULT_MQTT_ENDPOINT ??
        "mqtts://broker.emqx.io:8883",
    ],
  ];
  if (import.meta.env.VITE_DISCRYPT_DEFAULT_IPFS_BOOTSTRAP_ENDPOINT) {
    endpoints.push([
      "ipfs_pubsub",
      import.meta.env.VITE_DISCRYPT_DEFAULT_IPFS_BOOTSTRAP_ENDPOINT,
    ]);
  }
  if (import.meta.env.VITE_DISCRYPT_DEFAULT_QUIC_RENDEZVOUS_ENDPOINT) {
    endpoints.push([
      "discrypt_quic_rendezvous",
      import.meta.env.VITE_DISCRYPT_DEFAULT_QUIC_RENDEZVOUS_ENDPOINT,
    ]);
  }
  return endpoints.map(([adapterKind, endpoint]) =>
    signalingProfileForEndpoint(
      scopeCommitment,
      adapterKind,
      endpoint,
      `${adapterKind}-default`,
    ),
  );
}

function runtimePeerIdFromCommitment(label: string, commitment: string): string {
  return `peer-${hashCommitment("discrypt-runtime-peer-id-v1", [label, commitment]).slice(0, 16)}`;
}

function dmRuntimePeers(
  connectivity: ConnectivityPolicyView | null | undefined,
  localRole: "inviter" | "reply" | string,
): DmRuntimePeerView[] {
  const bootstrap = connectivity?.dm_bootstrap;
  if (!bootstrap) return [];
  const inviterPeerId = runtimePeerIdFromCommitment(
    "dm-inviter-runtime-peer",
    bootstrap.inviter_identity_commitment,
  );
  const replyPeerId = runtimePeerIdFromCommitment(
    "dm-reply-runtime-peer",
    bootstrap.reply_rendezvous_commitment,
  );
  const localIsInviter = localRole === "inviter";
  return [
    {
      peer_id: inviterPeerId,
      role: "inviter",
      is_local: localIsInviter,
      source: "signed_dm_bootstrap_v1",
    },
    {
      peer_id: replyPeerId,
      role: "reply",
      is_local: !localIsInviter,
      source: "signed_dm_bootstrap_v1",
    },
  ];
}

function groupRuntimePeers(
  connectivity: ConnectivityPolicyView | null | undefined,
  localRole: "owner" | "member" | string,
): GroupRuntimePeerView[] {
  const bootstrap = connectivity?.group_bootstrap;
  if (!bootstrap) return [];
  const ownerPeerId = runtimePeerIdFromCommitment(
    "group-owner-runtime-peer",
    bootstrap.group_identity_commitment,
  );
  const memberPeerId = runtimePeerIdFromCommitment(
    "group-member-runtime-peer",
    `${bootstrap.role_admission_policy_commitment}:${bootstrap.channel_policy_commitment}`,
  );
  const localIsOwner = localRole === "owner";
  return [
    {
      peer_id: ownerPeerId,
      role: "owner",
      is_local: localIsOwner,
      source: "signed_group_bootstrap_v1",
    },
    {
      peer_id: memberPeerId,
      role: "member",
      is_local: !localIsOwner,
      source: "signed_group_bootstrap_v1",
    },
  ];
}

function selectedSignalingProfiles(
  scope: string,
  adapterKind?: string | null,
  endpoint?: string | null,
): SignalingProfileView[] {
  const normalizedEndpoint = endpoint?.trim();
  if (adapterKind && normalizedEndpoint) {
    return [signalingProfileForEndpoint(scope, adapterKind, normalizedEndpoint)];
  }
  return defaultSignalingProfiles(scope);
}

function selectedIceStunServers(servers?: string[] | null): string[] {
  const normalized = (servers ?? [])
    .map((server) => server.trim())
    .filter(Boolean);
  return normalized.length ? normalized : defaultIceStunServers();
}

function selectedIceTurnServers(
  servers?: IceTurnServerView[] | null,
): IceTurnServerView[] {
  return (servers ?? []).filter((server) => server.endpoint.trim());
}

function groupConnectivityPolicy(
  groupId: string,
  options: Pick<
    CreateGroupRequest,
    "adapter_kind" | "signaling_endpoint" | "ice_stun_servers" | "ice_turn_servers"
  > = {},
): ConnectivityPolicyView {
  const scope = hashCommitment("discrypt-group-scope-commitment-v1", [groupId]);
  return {
    connectivity_schema_version: 1,
    invite_kind: "group_join",
    scope_id_commitment: scope,
    signaling_profiles: selectedSignalingProfiles(
      scope,
      options.adapter_kind,
      options.signaling_endpoint,
    ),
    ice_stun_servers: selectedIceStunServers(options.ice_stun_servers),
    ice_turn_servers: selectedIceTurnServers(options.ice_turn_servers),
    privacy_label:
      "Group invite topics are derived commitments; group names, channel names, and room secrets are not exposed",
    dm_bootstrap: null,
    group_bootstrap: {
      group_identity_commitment: scope,
      role_admission_policy_commitment: hashCommitment(
        "discrypt-group-admission-policy-commitment-v1",
        [groupId],
      ),
      channel_policy_commitment: hashCommitment(
        "discrypt-channel-policy-commitment-v1",
        [groupId],
      ),
    },
  };
}

function dmConnectivityPolicy(
  dmId: string,
  participantId: string,
): ConnectivityPolicyView {
  const scope = hashCommitment("discrypt-dm-scope-commitment-v1", [dmId]);
  return {
    connectivity_schema_version: 1,
    invite_kind: "dm_contact",
    scope_id_commitment: scope,
    signaling_profiles: defaultSignalingProfiles(scope),
    ice_stun_servers: defaultIceStunServers(),
    ice_turn_servers: defaultRedactedTurnServers(),
    privacy_label:
      "DM contact invite topics are derived commitments; aliases, safety numbers, and room secrets are not exposed",
    dm_bootstrap: {
      inviter_identity_commitment: hashCommitment(
        "discrypt-dm-inviter-identity-commitment-v1",
        [participantId],
      ),
      contact_token_commitment: hashCommitment(
        "discrypt-dm-contact-token-commitment-v1",
        [dmId, participantId],
      ),
      reply_rendezvous_commitment: hashCommitment(
        "discrypt-dm-reply-rendezvous-commitment-v1",
        [dmId],
      ),
    },
    group_bootstrap: null,
  };
}

function appConnectivityDefaults(): ConnectivityPolicyView {
  const scope = hashCommitment("discrypt-app-connectivity-defaults-v1", [
    "local-profile",
  ]);
  return {
    connectivity_schema_version: 1,
    invite_kind: "app_default",
    scope_id_commitment: scope,
    signaling_profiles: defaultSignalingProfiles(scope),
    ice_stun_servers: defaultIceStunServers(),
    ice_turn_servers: defaultRedactedTurnServers(),
    privacy_label:
      "App defaults are copied into new DM/group/channel policies; invites retarget provider topics to the signed scope commitment",
    dm_bootstrap: null,
    group_bootstrap: null,
  };
}

function retargetSignalingProfiles(
  scope: string,
  profiles: SignalingProfileView[],
): SignalingProfileView[] {
  const retargeted = profiles
    .map((profile) => {
      const endpoint = profile.endpoints.find((item) => item.trim());
      if (!endpoint) return null;
      return signalingProfileForEndpoint(
        scope,
        profile.adapter_kind,
        endpoint.trim(),
        `${profile.adapter_kind}-default`,
      );
    })
    .filter(Boolean) as SignalingProfileView[];
  return retargeted.length ? retargeted : defaultSignalingProfiles(scope);
}

function applyAppConnectivityDefaults(
  policy: ConnectivityPolicyView,
  defaults: ConnectivityPolicyView,
): ConnectivityPolicyView {
  return {
    ...policy,
    signaling_profiles: retargetSignalingProfiles(
      policy.scope_id_commitment,
      defaults.signaling_profiles,
    ),
    ice_stun_servers: defaults.ice_stun_servers,
    ice_turn_servers: defaults.ice_turn_servers,
  };
}

function requestHasConnectivityOverrides(request: CreateGroupRequest): boolean {
  return Boolean(
    request.adapter_kind?.trim() ||
      request.signaling_endpoint?.trim() ||
      request.ice_stun_servers?.some((server) => server.trim()) ||
      request.ice_turn_servers?.some((server) => server.endpoint.trim()),
  );
}

function validateSignalingEndpoint(kind: string, endpoint: string): boolean {
  if (endpoint.trim() !== endpoint || /\s/.test(endpoint)) return false;
  if (kind === "nostr") return endpoint.startsWith("wss://") || endpoint.startsWith("ws://");
  if (kind === "mqtt") return /^(mqtts|mqtt|wss|ws):\/\//.test(endpoint);
  if (kind === "ipfs_pubsub") return /^(\/ip4\/|\/ip6\/|\/dns|ipfs:\/\/)/.test(endpoint);
  if (kind === "discrypt_quic_rendezvous") return /^(quic|https|wss):\/\//.test(endpoint);
  return false;
}

function normalizeConnectivityPolicyOverride(
  base: ConnectivityPolicyView,
  request: SetConnectivityPolicyRequest,
): ConnectivityPolicyView {
  const adapterKind =
    request.adapter_kind?.trim() || base.signaling_profiles[0]?.adapter_kind || "nostr";
  const endpoint =
    request.signaling_endpoint?.trim() ||
    defaultSignalingEndpointForAdapter(adapterKind, base);
  if (!["nostr", "mqtt", "ipfs_pubsub", "discrypt_quic_rendezvous"].includes(adapterKind)) {
    throw new Error(`Unsupported signaling adapter kind ${adapterKind}`);
  }
  if (!validateSignalingEndpoint(adapterKind, endpoint)) {
    throw new Error(`Unsupported endpoint ${endpoint} for ${adapterKind}`);
  }
  const stun =
    request.ice_stun_servers?.map((item) => item.trim()).filter(Boolean) ??
    base.ice_stun_servers;
  if (!stun.length || stun.some((item) => !/^stuns?:/.test(item))) {
    throw new Error("STUN endpoints must start with stun: or stuns:");
  }
  const turn =
    request.ice_turn_servers
      ?.filter((server) => server.endpoint.trim())
      .map((server) => {
        const endpoint = server.endpoint.trim();
        if (!/^turns?:/.test(endpoint)) {
          throw new Error("TURN endpoints must start with turn: or turns:");
        }
        return { ...server, endpoint };
      }) ?? base.ice_turn_servers;
  return {
    ...base,
    signaling_profiles: [
      signalingProfileForEndpoint(
        base.scope_id_commitment,
        adapterKind,
        endpoint,
        `${adapterKind}-custom`,
      ),
    ],
    ice_stun_servers: stun,
    ice_turn_servers: turn,
  };
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
    kind: metadata.connectivity.invite_kind,
    scope: metadata.connectivity.scope_id_commitment,
  });
  if (metadata.groupId) query.set("gid", metadata.groupId);
  const groupBootstrap = metadata.connectivity.group_bootstrap;
  if (groupBootstrap) {
    query.set("group_identity", groupBootstrap.group_identity_commitment);
    query.set("role_policy", groupBootstrap.role_admission_policy_commitment);
    query.set("channel_policy", groupBootstrap.channel_policy_commitment);
  }
  const dmBootstrap = metadata.connectivity.dm_bootstrap;
  if (dmBootstrap) {
    query.set("dm_inviter", dmBootstrap.inviter_identity_commitment);
    query.set("dm_contact", dmBootstrap.contact_token_commitment);
    query.set("dm_reply", dmBootstrap.reply_rendezvous_commitment);
  }
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
  const descriptor = params.get("d") ? decodeBase64UrlJson(params.get("d") ?? "") : null;
  const descriptorSignaling = descriptor?.signaling_metadata ?? {};
  const descriptorTrust = descriptorSignaling.trust ?? {};
  const descriptorIce = descriptorSignaling.ice_endpoint_policy ?? {};
  const descriptorBootstrap = descriptor?.bootstrap_metadata ?? {};
  const signalingEndpoint =
    params.get("endpoint") ?? descriptorSignaling.signaling_endpoint ?? "";
  const endpointPolicy = endpointPolicyName(
    params.get("policy") ?? descriptorSignaling.endpoint_policy ?? "",
  );
  const signalingTrustFingerprint =
    params.get("trust_fp") ?? descriptorTrust.signaling_fingerprint ?? "";
  const signalingTrustStatus =
    params.get("trust") ?? descriptorTrust.trust_status ?? "";
  if (
    !signalingEndpoint ||
    !endpointPolicy ||
    !/^[a-fA-F0-9]{64}$/.test(signalingTrustFingerprint) ||
    !signalingTrustStatus
  ) {
    return null;
  }
  const inviteKind =
    params.get("kind") === "dm_contact" || descriptorBootstrap.invite_kind === "dm_contact"
      ? "dm_contact"
      : "group_join";
  const scope =
    params.get("scope") ??
    descriptorBootstrap.scope_id_commitment ??
    hashCommitment("discrypt-legacy-invite-scope-commitment-v1", [inviteKey]);
  const iceStunServers =
    params.getAll("stun").length > 0
      ? params.getAll("stun")
      : Array.isArray(descriptorIce.stun_servers)
        ? descriptorIce.stun_servers
        : [];
  const iceTurnServers =
    params.getAll("turn").length > 0
      ? params.getAll("turn").map((endpoint) => ({
          endpoint,
          credential_declared: true,
          credential_expires_at: null,
        }))
      : Array.isArray(descriptorIce.turn_servers)
        ? descriptorIce.turn_servers.map((server: any) => ({
            endpoint: typeof server === "string" ? server : String(server?.endpoint ?? ""),
            credential_declared: Boolean(
              server?.username ?? server?.credential ?? server?.credential_expires_at,
            ),
            credential_expires_at: server?.credential_expires_at ?? null,
          }))
        : [];
  const connectivity =
    inviteKind === "dm_contact"
      ? {
          ...dmConnectivityPolicy(`dm-${inviteKey}`, scope),
          scope_id_commitment: scope,
          ice_stun_servers: iceStunServers,
          ice_turn_servers: iceTurnServers,
          dm_bootstrap:
            params.get("dm_inviter") && params.get("dm_contact") && params.get("dm_reply")
              ? {
                  inviter_identity_commitment: params.get("dm_inviter") ?? "",
                  contact_token_commitment: params.get("dm_contact") ?? "",
                  reply_rendezvous_commitment: params.get("dm_reply") ?? "",
                }
              : dmConnectivityPolicy(`dm-${inviteKey}`, scope).dm_bootstrap,
        }
      : {
          ...groupConnectivityPolicy(`group-${inviteKey}`),
          scope_id_commitment: scope,
          ice_stun_servers: iceStunServers,
          ice_turn_servers: iceTurnServers,
          group_bootstrap:
            params.get("group_identity") &&
            params.get("role_policy") &&
            params.get("channel_policy")
              ? {
                  group_identity_commitment: params.get("group_identity") ?? "",
                  role_admission_policy_commitment: params.get("role_policy") ?? "",
                  channel_policy_commitment: params.get("channel_policy") ?? "",
                }
              : groupConnectivityPolicy(`group-${inviteKey}`).group_bootstrap,
        };
  if (
    inviteKind === "group_join" &&
    Array.isArray(descriptorBootstrap.signaling_profiles) &&
    descriptorBootstrap.signaling_profiles.length > 0
  ) {
    connectivity.signaling_profiles = descriptorBootstrap.signaling_profiles;
  }
  return {
    inviteKey,
    groupId: params.get("gid"),
    groupName: descriptor ? null : params.get("gname") ?? params.get("name"),
    roomSecretHash:
      params.get("commitment") ?? hexFromByteArray(descriptor?.room_secret_commitment),
    signalingEndpoint,
    signalingTrustFingerprint,
    signalingTrustStatus,
    endpointPolicy,
    iceStunServers,
    iceTurnServers,
    connectivity,
    expiresAt: params.get("exp") ?? descriptor?.expires_at ?? "",
    maxUses: Number(params.get("max") ?? descriptor?.max_uses ?? 1) || 1,
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
  hydrateFallbackState();
  fallbackState.last_command_error = null;
  update(fallbackState);
  const nextState = syncSnapshot(fallbackState);
  persistFallbackState();
  return cloneState(nextState);
}

export function commandErrorToAction(error: CommandErrorView | null): string {
  return error?.recovery_hint ?? "";
}

function localGroupMember(role: GroupRoleView): GroupMemberView {
  const memberId = localUserId(fallbackState);
  return {
    member_id: memberId,
    display_name: fallbackState.profile?.display_name ?? "Local member",
    device_id: fallbackState.profile?.device_name ?? null,
    role,
    status: "offline",
    signer_public_key_hex: null,
    joined_at: new Date().toISOString(),
    last_seen_at: null,
    presence_expires_at: null,
    revoked_at: null,
    revoked_by: null,
  };
}

function ensureFallbackGroupGovernanceDefaults(group: GroupView): void {
  group.members ??= [localGroupMember((group.role as GroupRoleView) || "member")];
  if (!group.members.some((member) => member.member_id === localUserId(fallbackState))) {
    group.members.push(localGroupMember((group.role as GroupRoleView) || "member"));
  }
  group.governance_log ??= [
    {
      event_id: `fallback-governance-${group.group_id}`,
      group_id: group.group_id,
      event_kind: "governance.defaults_restored",
      actor_member_id: localUserId(fallbackState),
      target_member_id: null,
      request_id: null,
      role_before: null,
      role_after: null,
      created_at: new Date().toISOString(),
      summary: "Restored fallback governance roster defaults",
    },
  ];
}

function fallbackLocalRoleForGroup(group: GroupView): GroupRoleView | null {
  ensureFallbackGroupGovernanceDefaults(group);
  const member = group.members?.find(
    (item) => item.member_id === localUserId(fallbackState) && item.status !== "revoked",
  );
  return member?.role ?? null;
}

function governanceEntry(
  group: GroupView,
  eventKind: string,
  targetMemberId: string | null,
  roleBefore: GroupRoleView | null,
  roleAfter: GroupRoleView | null,
  summary: string,
): GroupGovernanceLogEntryView {
  return {
    event_id: `fallback-${eventKind}-${group.governance_log?.length ?? 0}-${Date.now()}`,
    group_id: group.group_id,
    event_kind: eventKind,
    actor_member_id: localUserId(fallbackState),
    target_member_id: targetMemberId,
    request_id: null,
    role_before: roleBefore,
    role_after: roleAfter,
    created_at: new Date().toISOString(),
    summary,
  };
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
      connectivity: applyAppConnectivityDefaults(
        dmConnectivityPolicy(
          dmId,
          participantIdFromFriendCode(fallbackState.snapshot.friend.friend_code),
        ),
        fallbackState.connectivity_defaults,
      ),
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
    const connectivity = applyAppConnectivityDefaults(
      groupConnectivityPolicy(groupId),
      fallbackState.connectivity_defaults,
    );
    const createdAt = new Date().toISOString();
    const role: GroupRoleView = "member";
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
          connectivity: null,
        },
        {
          channel_id: `${groupId}-voice`,
          name: "Voice Lobby",
          kind: "Voice",
          retention_status: "session",
          connectivity: null,
        },
      ],
      members: [initialGroupMember(fallbackState, role, createdAt)],
      role_policy: initialGroupRolePolicy(
        fallbackState,
        "automatic_when_authorized_online",
        createdAt,
      ),
      admission_requests: [],
      governance_log: initialGroupGovernanceLog(
        fallbackState,
        groupId,
        role,
        createdAt,
      ),
      runtime_peers: groupRuntimePeers(connectivity, "member"),
      connectivity,
    });
  }
}

export async function loadAppState(): Promise<AppState> {
  return invokeOrFallback<AppState>("app_state", undefined, () => {
    hydrateFallbackState();
    applyFirstRunStorageE2eState(fallbackState);
    return cloneState(syncSnapshot(fallbackState));
  });
}

export async function exportDiagnosticsLog(): Promise<string> {
  return invokeOrFallback<string>("export_diagnostics_log", undefined, () => {
    const state = cloneState(syncSnapshot(fallbackState));
    return JSON.stringify(
      {
        schema_version: 1,
        generated_at: new Date().toISOString(),
        app_version: "local-dev-fallback",
        transport_policy: {
          message_relay_fallback: "disabled",
          provider_role: "signaling only for SDP/candidates",
          allowed_delivery_paths: ["direct_p2p_data_channel", "explicit_turn_data_channel"],
        },
        lifecycle: state.lifecycle,
        storage_security: state.storage_security,
        runtime_mode: state.runtime_mode,
        active_context: state.active_context,
        transport_status: state.transport_status,
        transport_diagnostics: state.transport_diagnostics,
        last_command_error: state.last_command_error,
        events: state.events,
        group_count: state.groups.length,
        dm_count: state.dms.length,
        message_count: state.messages.length,
        voice_states: state.voice_states,
      },
      null,
      2,
    );
  });
}

export async function loadCompatibilityAppSnapshot(): Promise<AppSnapshot> {
  return invokeOrFallback<AppSnapshot>(
    "app_snapshot",
    undefined,
    () => cloneState(syncSnapshot(fallbackState)).snapshot,
  );
}

export async function configureStorageSecurity(
  request: ConfigureStorageSecurityRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>(
    "configure_storage_security",
    { request },
    () =>
      mutateFallback((state) => {
        state.storage_security = {
          status: "ready",
          mode: request.mode,
          title:
            request.mode === "passphrase_vault"
              ? "Password vault selected"
              : "OS keyring selected",
          detail:
            request.mode === "passphrase_vault"
              ? "The browser fallback cannot enforce production vault unlock; packaged Tauri builds require the password on startup."
              : "The browser fallback cannot access the OS keyring; packaged Tauri builds use the platform keyring.",
          recovery_hint: "Continue to account setup.",
          password_required: false,
          keyring_available: true,
          keyring_detail: "Development builds do not require OS-keyring preflight.",
        };
      }),
  );
}

export async function unlockStorageSecurity(
  request: UnlockStorageSecurityRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>(
    "unlock_storage_security",
    { request },
    () =>
      mutateFallback((state) => {
        state.storage_security = {
          ...state.storage_security,
          status: "ready",
          title: "Storage unlocked",
          detail:
            "The browser fallback accepted the password; packaged Tauri builds decrypt the production vault.",
          recovery_hint: "Continue.",
          password_required: false,
          keyring_available: true,
          keyring_detail: "Development builds do not require OS-keyring preflight.",
        };
      }),
  );
}

export async function loadAppSnapshot(): Promise<AppSnapshot> {
  const state = await loadAppState();
  return state.snapshot;
}

export async function startSignalingSession(
  request: StartSignalingSessionRequest = {},
): Promise<AppState> {
  return invokeOrFallback<AppState>("start_signaling_session", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      pushEvent(
        state,
        "transport.signaling_start_rejected",
        "Tauri IPC unavailable; local fallback cannot start a real signaling transport session",
      );
    }),
  );
}

export async function stopSignalingSession(
  request: StopSignalingSessionRequest = {},
): Promise<AppState> {
  return invokeOrFallback<AppState>("stop_signaling_session", { request }, () =>
    mutateFallback((state) => {
      pushEvent(
        state,
        "transport.signaling_stopped",
        "Local fallback recorded signaling stop; no backend transport session was active",
      );
    }),
  );
}

export async function startTextSession(
  request: StartTextSessionRequest = {},
): Promise<AppState> {
  return invokeOrFallback<AppState>("start_text_session", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      pushEvent(
        state,
        "transport.text_start_rejected",
        "Tauri IPC unavailable; local fallback cannot start a real text transport session",
      );
    }),
  );
}

export async function stopTextSession(
  request: StopTextSessionRequest = {},
): Promise<AppState> {
  return invokeOrFallback<AppState>("stop_text_session", { request }, () =>
    mutateFallback((state) => {
      pushEvent(
        state,
        "transport.text_stopped",
        "Local fallback recorded text transport stop; no backend transport session was active",
      );
    }),
  );
}

export async function attachTextControlTransportRuntime(
  request: AttachTextControlTransportRuntimeRequest = {},
): Promise<AppState> {
  return invokeOrFallback<AppState>(
    "attach_text_control_transport_runtime",
    { request },
    () =>
      mutateFallback((state) => {
        pushCommandError(
          state,
          "transport.runtime_attach_rejected",
          "attach_text_control_transport_runtime",
          "transport_runtime_unavailable",
          "Local fallback web runtime cannot attach a long-lived text/control transport runtime; native Rust/Tauri command path is required",
          "Run the native app and attach the backend runtime before claiming delivery",
        );
      }),
  );
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
          const connectivity = groupConnectivityPolicy(groupId);
          const createdAt = new Date().toISOString();
          const role: GroupRoleView = "member";
          state.groups.push({
            group_id: groupId,
            name,
            role: "member",
            channels: defaultGroupChannels(),
            members: [initialGroupMember(state, role, createdAt)],
            role_policy: initialGroupRolePolicy(
              state,
              "automatic_when_authorized_online",
              createdAt,
            ),
            admission_requests: [],
            governance_log: initialGroupGovernanceLog(
              state,
              groupId,
              role,
              createdAt,
            ),
            runtime_peers: groupRuntimePeers(connectivity, "member"),
            connectivity,
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
      hydrateFallbackState();
      const verified =
        request.friend_id === fallbackState.snapshot.friend.friend_code &&
        request.provided === fallbackState.snapshot.friend.safety_number;
      fallbackState.snapshot.friend.verified = verified;
      persistFallbackState();
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
  const normalized = normalizePreferences(request);
  return invokeOrFallback<AppState>("save_preferences", { request: normalized }, () =>
    mutateFallback((state) => {
      state.preferences = normalized;
      pushEvent(state, "preferences.saved", "Theme/template preferences saved");
    }),
  );
}

function normalizePreferences(request: SavePreferencesRequest): SavePreferencesRequest {
  const themeIds = discryptUiConfig.themes.map((theme) => theme.id);
  const templateIds = discryptUiConfig.templates.map((template) => template.id);
  return {
    theme_id: themeIds.includes(request.theme_id as never)
      ? request.theme_id
      : discryptUiConfig.activeTheme,
    template_id: templateIds.includes(request.template_id as never)
      ? request.template_id
      : discryptUiConfig.activeTemplate,
  };
}

export async function setConnectivityPolicy(
  request: SetConnectivityPolicyRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("set_connectivity_policy", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      try {
        if (request.scope_kind === "app" || request.scope_kind === "defaults") {
          state.connectivity_defaults = normalizeConnectivityPolicyOverride(
            state.connectivity_defaults,
            request,
          );
          pushEvent(state, "connectivity.policy_saved", "Updated app signaling/ICE defaults");
          return;
        }
        if (request.scope_kind === "dm") {
          const dmId = request.dm_id ?? state.active_context?.dm_id ?? null;
          const dm = state.dms.find((item) => item.dm_id === dmId);
          if (!dm) throw new Error("Requested DM does not exist");
          const policy = normalizeConnectivityPolicyOverride(
            dm.connectivity ?? dmConnectivityPolicy(dm.dm_id, dm.participant_id),
            request,
          );
          dm.runtime_peers = dmRuntimePeers(policy, "inviter");
          dm.connectivity = policy;
          pushEvent(state, "connectivity.policy_saved", `Updated connectivity for DM ${dm.display_name}`);
          return;
        }
        const groupId = request.group_id ?? state.active_context?.group_id ?? null;
        const group = state.groups.find((item) => item.group_id === groupId);
        if (!group) throw new Error("Requested group does not exist");
        if (request.scope_kind === "channel") {
          const channelId = request.channel_id ?? state.active_context?.channel_id ?? null;
          const channel = group.channels.find((item) => item.channel_id === channelId);
          if (!channel) throw new Error("Requested channel does not exist");
          const base =
            channel.connectivity ??
            ({
              ...(group.connectivity ?? groupConnectivityPolicy(group.group_id)),
              scope_id_commitment: hashCommitment("discrypt-channel-scope-commitment-v1", [
                channel.channel_id,
              ]),
              privacy_label:
                "Channel signaling topics are derived commitments; channel names and room secrets are not exposed",
            } satisfies ConnectivityPolicyView);
          channel.connectivity = normalizeConnectivityPolicyOverride(base, request);
          pushEvent(state, "connectivity.policy_saved", `Updated connectivity for channel ${channel.name}`);
          return;
        }
        const policy = normalizeConnectivityPolicyOverride(
          group.connectivity ?? groupConnectivityPolicy(group.group_id),
          request,
        );
        group.runtime_peers = groupRuntimePeers(policy, group.role);
        group.connectivity = policy;
        pushEvent(state, "connectivity.policy_saved", `Updated connectivity for group ${group.name}`);
      } catch (error) {
        pushCommandError(
          state,
          "connectivity.rejected",
          "set_connectivity_policy",
          "invalid_connectivity_policy",
          error instanceof Error ? error.message : "Invalid connectivity policy",
          "Pick a supported adapter and valid STUN/TURN endpoints before saving",
        );
      }
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
        const participantId = slugify(displayName);
        const connectivity = applyAppConnectivityDefaults(
          dmConnectivityPolicy(dmId, participantId),
          state.connectivity_defaults,
        );
        state.dms.push({
          dm_id: dmId,
          participant_id: participantId,
          display_name: displayName,
          local_only_copy:
            "Local DM; remote delivery is not claimed until backend proof is available",
          runtime_peers: dmRuntimePeers(connectivity, "inviter"),
          connectivity,
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
        let connectivity = groupConnectivityPolicy(groupId, request);
        if (!requestHasConnectivityOverrides(request)) {
          connectivity = applyAppConnectivityDefaults(
            connectivity,
            state.connectivity_defaults,
          );
        }
        const createdAt = new Date().toISOString();
        const role: GroupRoleView = "owner";
        state.groups.push({
          group_id: groupId,
          name,
          role: "owner",
          channels: defaultGroupChannels(),
          members: [initialGroupMember(state, role, createdAt)],
          role_policy: initialGroupRolePolicy(
            state,
            request.admission_mode,
            createdAt,
          ),
          admission_requests: [],
          governance_log: initialGroupGovernanceLog(
            state,
            groupId,
            role,
            createdAt,
          ),
          runtime_peers: groupRuntimePeers(connectivity, "owner"),
          connectivity,
        });
        ensureGroupGovernance(state, state.groups[state.groups.length - 1], "owner");
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
      const name = inviteGroupNameFromMetadata(
        inviteCode,
        request.group_name,
        parsedInvite,
      );
      const groupId = parsedInvite?.groupId || `group-${slugify(name)}`;
      if (!state.groups.some((group) => group.group_id === groupId)) {
        const connectivity =
          parsedInvite?.connectivity ?? groupConnectivityPolicy(groupId);
        const createdAt = new Date().toISOString();
        const role: GroupRoleView = "member";
        const localMember = initialGroupMember(state, role, createdAt);
        localMember.status = "pending";
        state.groups.push({
          group_id: groupId,
          name,
          role: "member",
          channels: defaultGroupChannels(),
          members: [localMember],
          role_policy: initialGroupRolePolicy(
            state,
            "automatic_when_authorized_online",
            createdAt,
          ),
          admission_requests: [],
          governance_log: initialGroupGovernanceLog(
            state,
            groupId,
            role,
            createdAt,
          ),
          runtime_peers: groupRuntimePeers(connectivity, "member"),
          connectivity,
        });
        ensureGroupGovernance(state, state.groups[state.groups.length - 1], "member");
      }
      ensureGroupGovernanceDefaults(state);
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
          dm_id: null,
          connectivity_schema_version: parsedInvite.connectivity.connectivity_schema_version,
          invite_kind: parsedInvite.connectivity.invite_kind,
          scope_id_commitment: parsedInvite.connectivity.scope_id_commitment,
          signaling_profiles: parsedInvite.connectivity.signaling_profiles,
          privacy_label: parsedInvite.connectivity.privacy_label,
          dm_bootstrap: parsedInvite.connectivity.dm_bootstrap,
          group_bootstrap: parsedInvite.connectivity.group_bootstrap,
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
        "group.admission_requested",
        `Requested admission to ${name} via ${request.invite_code}`,
      );
    }),
  );
}

export async function setGroupAdmissionMode(
  request: SetGroupAdmissionModeRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("set_group_admission_mode", { request }, () =>
    mutateFallback((state) => {
      const group = findGovernedGroup(state, request.group_id);
      if (!canModerateAdmissions(state, group)) {
        pushCommandError(
          state,
          "governance.unauthorized",
          "set_group_admission_mode",
          "forbidden",
          "Only owners or staff can change admission mode",
          "Ask an owner or staff member to update group admission policy",
        );
        return;
      }
      const previousMode = group.role_policy?.admission_mode ?? "manual_approval";
      group.role_policy = {
        admission_mode: normalizedAdmissionMode(request.admission_mode),
        policy_epoch: (group.role_policy?.policy_epoch ?? 1) + 1,
        updated_by: localMemberId(state),
        updated_at: isoNow(),
      };
      governanceLog(state, group, {
        event_kind: "admission.mode_changed",
        actor_member_id: localMemberId(state),
        target_member_id: null,
        request_id: null,
        role_before: previousMode,
        role_after: group.role_policy.admission_mode,
        summary: `Admission mode changed to ${group.role_policy.admission_mode}`,
      });
    }),
  );
}

export async function approveGroupAdmissionRequest(
  request: ApproveGroupAdmissionRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("approve_group_admission_request", { request }, () =>
    mutateFallback((state) => {
      const group = findGovernedGroup(state, request.group_id);
      if (!canModerateAdmissions(state, group)) {
        pushCommandError(state, "governance.unauthorized", "approve_group_admission_request", "forbidden", "Only owners or staff can approve admission requests", "Ask an owner or staff member to review the request");
        return;
      }
      const admission = group.admission_requests?.find((item) => item.request_id === request.request_id);
      if (!admission) throw new Error("Admission request does not exist");
      admission.status = "approved";
      admission.decided_by = localMemberId(state);
      admission.decided_at = isoNow();
      admission.decision_reason = null;
      group.members ??= [];
      if (!group.members.some((member) => member.member_id === admission.member_identity)) {
        group.members.push({
          member_id: admission.member_identity,
          display_name: admission.display_name,
          device_id: admission.device_name ?? null,
          role: "member",
          status: "unknown",
          signer_public_key_hex: admission.signer_public_key_hex,
          joined_at: admission.decided_at,
          last_seen_at: null,
          presence_expires_at: null,
          revoked_at: null,
          revoked_by: null,
        });
      }
      governanceLog(state, group, {
        event_kind: "admission.approved",
        actor_member_id: localMemberId(state),
        target_member_id: admission.member_identity,
        request_id: admission.request_id,
        role_before: null,
        role_after: "member",
        summary: `Approved ${admission.display_name} for admission`,
      });
    }),
  );
}

export async function refuseGroupAdmissionRequest(
  request: RefuseGroupAdmissionRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("refuse_group_admission_request", { request }, () =>
    mutateFallback((state) => {
      const group = findGovernedGroup(state, request.group_id);
      if (!canModerateAdmissions(state, group)) {
        pushCommandError(state, "governance.unauthorized", "refuse_group_admission_request", "forbidden", "Only owners or staff can refuse admission requests", "Ask an owner or staff member to review the request");
        return;
      }
      const admission = group.admission_requests?.find((item) => item.request_id === request.request_id);
      if (!admission) throw new Error("Admission request does not exist");
      admission.status = "refused";
      admission.decided_by = localMemberId(state);
      admission.decided_at = isoNow();
      admission.decision_reason = request.reason ?? null;
      governanceLog(state, group, {
        event_kind: "admission.refused",
        actor_member_id: localMemberId(state),
        target_member_id: admission.member_identity,
        request_id: admission.request_id,
        role_before: null,
        role_after: null,
        summary: `Refused ${admission.display_name} admission`,
      });
    }),
  );
}

export async function promoteGroupMemberToStaff(
  request: PromoteGroupMemberRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("promote_group_member_to_staff", { request }, () =>
    mutateFallback((state) => {
      const group = findGovernedGroup(state, request.group_id);
      if (!canPromoteMembers(state, group)) {
        pushCommandError(state, "governance.unauthorized", "promote_group_member_to_staff", "forbidden", "Only owners can promote staff", "Ask the group owner to promote this member");
        return;
      }
      const member = group.members?.find((item) => item.member_id === request.member_id);
      if (!member) throw new Error("Group member does not exist");
      const before = member.role;
      if (before === "owner") return;
      member.role = "staff";
      governanceLog(state, group, {
        event_kind: "member.role_changed",
        actor_member_id: localMemberId(state),
        target_member_id: member.member_id,
        request_id: null,
        role_before: before,
        role_after: "staff",
        summary: `Promoted ${member.display_name} to staff`,
      });
    }),
  );
}

export async function demoteGroupStaffToMember(
  request: DemoteGroupStaffRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("demote_group_staff_to_member", { request }, () =>
    mutateFallback((state) => {
      const group = findGovernedGroup(state, request.group_id);
      if (!canPromoteMembers(state, group)) {
        pushCommandError(state, "governance.unauthorized", "demote_group_staff_to_member", "forbidden", "Only owners can demote staff", "Ask the group owner to change this role");
        return;
      }
      const member = group.members?.find((item) => item.member_id === request.member_id);
      if (!member) throw new Error("Group member does not exist");
      const before = member.role;
      if (before !== "staff") return;
      member.role = "member";
      governanceLog(state, group, {
        event_kind: "member.role_changed",
        actor_member_id: localMemberId(state),
        target_member_id: member.member_id,
        request_id: null,
        role_before: before,
        role_after: "member",
        summary: `Demoted ${member.display_name} to member`,
      });
    }),
  );
}

export async function revokeGroupMemberAccess(
  request: RevokeGroupMemberAccessRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("revoke_group_member_access", { request }, () =>
    mutateFallback((state) => {
      const group = findGovernedGroup(state, request.group_id);
      const member = group.members?.find((item) => item.member_id === request.member_id);
      if (!member) throw new Error("Group member does not exist");
      if (!canRevokeMember(state, group, member)) {
        pushCommandError(state, "governance.unauthorized", "revoke_group_member_access", "forbidden", "This role cannot revoke the selected member", "Owners can revoke staff or members; staff can revoke members only");
        return;
      }
      member.status = "revoked";
      member.revoked_at = isoNow();
      member.revoked_by = localMemberId(state);
      governanceLog(state, group, {
        event_kind: "member.revoked",
        actor_member_id: localMemberId(state),
        target_member_id: member.member_id,
        request_id: null,
        role_before: member.role,
        role_after: member.role,
        summary: `Revoked ${member.display_name} access`,
      });
    }),
  );
}

export async function publishGroupPresence(
  request: PublishGroupPresenceRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("publish_group_presence", { request }, () =>
    mutateFallback((state) => {
      const group = findGovernedGroup(state, request.group_id);
      const member = group.members?.find((item) => item.member_id === localMemberId(state));
      if (!member || member.status === "revoked") return;
      const now = isoNow();
      member.status = request.status ?? "online";
      member.last_seen_at = now;
      member.presence_expires_at = new Date(Date.now() + Math.max(30, request.ttl_seconds ?? 300) * 1000).toISOString();
      pushEvent(state, "group.presence", `Published ${request.status ?? "online"} presence for ${group.name}`);
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

export async function setActiveChannel(
  request: SetActiveChannelRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("set_active_channel", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const group = state.groups.find((g) => g.group_id === request.group_id);
      const channel = group?.channels.find(
        (c) => c.channel_id === request.channel_id,
      );
      if (!channel) {
        pushCommandError(
          state,
          "channel.focus_missing",
          "set_active_channel",
          "channel_not_found",
          "Requested channel does not exist in the group",
          "Select a channel that belongs to the active group",
        );
        return;
      }
      const kind = channel.kind === "Voice" ? "voice_channel" : "text_channel";
      state.active_context = {
        kind,
        group_id: request.group_id,
        channel_id: request.channel_id,
        dm_id: null,
      };
      pushEvent(state, "channel.focused", `Focused channel ${channel.name}`);
    }),
  );
}

export async function setActiveDm(
  request: SetActiveDmRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("set_active_dm", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const dm = state.dms.find((d) => d.dm_id === request.dm_id);
      if (!dm) {
        pushCommandError(
          state,
          "dm.focus_missing",
          "set_active_dm",
          "dm_not_found",
          "Requested DM does not exist",
          "Select a DM that already exists",
        );
        return;
      }
      state.active_context = {
        kind: "dm",
        group_id: null,
        channel_id: null,
        dm_id: request.dm_id,
      };
      pushEvent(state, "dm.focused", `Focused DM ${request.dm_id}`);
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
      const connectivity = group?.connectivity ?? groupConnectivityPolicy(groupId);
      const inviteKey =
        crypto.randomUUID?.() ?? `local-${state.invites.length + 1}`;
      const roomSecretHash = stableHash(
        `${groupId}:${inviteKey}:${state.invites.length}`,
      );
      const expires = request.expires || fallbackState.snapshot.invite.expires;
      const maxUse = request.max_use || fallbackState.snapshot.invite.max_use;
      const passwordGate = request.password_gate?.trim();
      const expiresAt = inviteExpirationHorizon(expires);
      const signalingEndpoint = defaultSignalingEndpoint(connectivity);
      const signalingTrustFingerprint = stableHash(
        `external-signaling-endpoint-fingerprint-v1:${signalingEndpoint}`,
      );
      const endpointPolicy = endpointPolicyForSignalingEndpoint(signalingEndpoint);
      const trustStatus =
        "signed endpoint fingerprint; verify before MLS Welcome";
      const inviteMetadata: ParsedInviteMetadata = {
        inviteKey,
        groupId,
        groupName: group?.name ?? null,
        signalingEndpoint,
        endpointPolicy,
        signalingTrustFingerprint,
        signalingTrustStatus: trustStatus,
        iceStunServers: connectivity.ice_stun_servers,
        iceTurnServers: connectivity.ice_turn_servers,
        roomSecretHash,
        connectivity,
        expiresAt,
        maxUses: parseMaxUses(maxUse),
      };
      state.invites.push({
        invite_id: `invite-${inviteKey}`,
        invite_key: inviteKey,
        group_id: groupId,
        dm_id: null,
        connectivity_schema_version: connectivity.connectivity_schema_version,
        invite_kind: connectivity.invite_kind,
        scope_id_commitment: connectivity.scope_id_commitment,
        signaling_profiles: connectivity.signaling_profiles,
        privacy_label: connectivity.privacy_label,
        dm_bootstrap: connectivity.dm_bootstrap,
        group_bootstrap: connectivity.group_bootstrap,
        admission_snapshot: inviteAdmissionSnapshot(groupId, connectivity),
        revocation_policy: inviteRevocationPolicy(groupId),
        password_policy: passwordGate ? invitePasswordPolicy(groupId) : null,
        code: productionInviteLink(inviteMetadata),
        room_secret_hash: roomSecretHash,
        signaling_endpoint: signalingEndpoint,
        signaling_trust_fingerprint: signalingTrustFingerprint,
        signaling_trust_status: trustStatus,
        endpoint_policy: endpointPolicy,
        ice_stun_servers: connectivity.ice_stun_servers,
        ice_turn_servers: connectivity.ice_turn_servers,
        expires,
        expires_at: expiresAt,
        max_use: maxUse,
        uses: 0,
        revoked: false,
        admission_copy: passwordGate
          ? "Password-gated admission requested; final admission still requires an authorized MLS Welcome/add and no password verifier is embedded in the invite"
          : "Final admission still requires an authorized MLS Welcome/add; the room-secret link alone is insufficient",
      });
      pushEvent(
        state,
        "invite.created",
        `Invite created for ${group?.name ?? "group"}`,
      );
    }),
  );
}

export async function createDmInvite(
  request: CreateDmInviteRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("create_dm_invite", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const dmId =
        request.dm_id ?? state.active_context?.dm_id ?? state.dms[0]?.dm_id;
      if (!dmId) {
        pushCommandError(
          state,
          "invite.rejected",
          "create_dm_invite",
          "dm_not_found",
          "No DM contact exists for invite creation",
          "Start or select a DM before creating a contact invite",
        );
        return;
      }
      const dm = state.dms.find((item) => item.dm_id === dmId);
      if (!dm) {
        pushCommandError(
          state,
          "invite.rejected",
          "create_dm_invite",
          "dm_not_found",
          "Requested DM contact does not exist",
          "Pick a contact from the DM list before creating an invite",
        );
        return;
      }
      const connectivity =
        dm.connectivity ?? dmConnectivityPolicy(dm.dm_id, dm.participant_id);
      const inviteKey =
        crypto.randomUUID?.() ?? `dm-local-${state.invites.length + 1}`;
      const roomSecretHash = stableHash(
        `${dm.dm_id}:${inviteKey}:${state.invites.length}`,
      );
      const expires = request.expires || fallbackState.snapshot.invite.expires;
      const maxUse = request.max_use || fallbackState.snapshot.invite.max_use;
      const expiresAt = inviteExpirationHorizon(expires);
      const signalingEndpoint = defaultSignalingEndpoint(connectivity);
      const signalingTrustFingerprint = stableHash(
        `external-signaling-endpoint-fingerprint-v1:${signalingEndpoint}`,
      );
      const endpointPolicy = endpointPolicyForSignalingEndpoint(signalingEndpoint);
      const trustStatus = "signed endpoint fingerprint; verify before DM accept";
      const inviteMetadata: ParsedInviteMetadata = {
        inviteKey,
        signalingEndpoint,
        endpointPolicy,
        signalingTrustFingerprint,
        signalingTrustStatus: trustStatus,
        iceStunServers: connectivity.ice_stun_servers,
        iceTurnServers: connectivity.ice_turn_servers,
        roomSecretHash,
        connectivity,
        expiresAt,
        maxUses: parseMaxUses(maxUse),
      };
      state.invites.push({
        invite_id: `invite-${inviteKey}`,
        invite_key: inviteKey,
        group_id: "",
        dm_id: dm.dm_id,
        connectivity_schema_version: connectivity.connectivity_schema_version,
        invite_kind: connectivity.invite_kind,
        scope_id_commitment: connectivity.scope_id_commitment,
        signaling_profiles: connectivity.signaling_profiles,
        privacy_label: connectivity.privacy_label,
        dm_bootstrap: connectivity.dm_bootstrap,
        group_bootstrap: connectivity.group_bootstrap,
        code: productionInviteLink(inviteMetadata),
        room_secret_hash: roomSecretHash,
        signaling_endpoint: signalingEndpoint,
        signaling_trust_fingerprint: signalingTrustFingerprint,
        signaling_trust_status: trustStatus,
        endpoint_policy: endpointPolicy,
        ice_stun_servers: connectivity.ice_stun_servers,
        ice_turn_servers: connectivity.ice_turn_servers,
        expires,
        expires_at: expiresAt,
        max_use: maxUse,
        uses: 0,
        revoked: false,
        admission_copy:
          "Final DM acceptance still requires a sealed reply rendezvous and verified contact identity; the link alone is insufficient",
      });
      pushEvent(
        state,
        "invite.dm_created",
        `DM contact invite created for ${dm.display_name}`,
      );
    }),
  );
}

export async function acceptDmInvite(
  request: AcceptDmInviteRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("accept_dm_invite", { request }, () =>
    mutateFallback((state) => {
      ensureFallbackReady();
      const inviteCode = request.invite_code.trim();
      const parsedInvite = parseInviteMetadata(inviteCode);
      if (!parsedInvite) {
        pushCommandError(
          state,
          "invite.rejected",
          "accept_dm_invite",
          "invite_parse_failed",
          "DM contact invite metadata could not be parsed",
          "Paste a signed DM contact invite descriptor before accepting",
        );
        return;
      }
      if (parsedInvite.connectivity.invite_kind !== "dm_contact") {
        pushCommandError(
          state,
          "invite.rejected",
          "accept_dm_invite",
          "invite_kind_mismatch",
          "Invite is not a DM contact invite",
          "Use group join for group invites or request a DM contact invite",
        );
        return;
      }
      const displayName = request.display_name?.trim() || "DM contact";
      const participantId = hashCommitment(
        "discrypt-accepted-dm-participant-id-v1",
        [parsedInvite.connectivity.scope_id_commitment],
      );
      const existing = state.dms.find(
        (dm) =>
          dm.connectivity?.scope_id_commitment ===
          parsedInvite.connectivity.scope_id_commitment,
      );
      const dmId = existing?.dm_id ?? `dm-${slugify(parsedInvite.inviteKey)}`;
      if (!existing) {
        state.dms.push({
          dm_id: dmId,
          participant_id: participantId,
          display_name: displayName,
          local_only_copy:
            "DM contact opened from signed invite metadata; remote delivery is not claimed until backend receipt proof",
          runtime_peers: dmRuntimePeers(parsedInvite.connectivity, "reply"),
          connectivity: parsedInvite.connectivity,
        });
      }
      state.active_context = {
        kind: "dm",
        group_id: null,
        channel_id: null,
        dm_id: dmId,
      };
      state.invites.push({
        invite_id: `invite-${parsedInvite.inviteKey}`,
        invite_key: parsedInvite.inviteKey,
        group_id: "",
        dm_id: dmId,
        connectivity_schema_version:
          parsedInvite.connectivity.connectivity_schema_version,
        invite_kind: parsedInvite.connectivity.invite_kind,
        scope_id_commitment: parsedInvite.connectivity.scope_id_commitment,
        signaling_profiles: parsedInvite.connectivity.signaling_profiles,
        privacy_label: parsedInvite.connectivity.privacy_label,
        dm_bootstrap: parsedInvite.connectivity.dm_bootstrap,
        group_bootstrap: parsedInvite.connectivity.group_bootstrap,
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
          "Parsed DM contact invite metadata; final acceptance still requires sealed reply rendezvous/contact verification",
      });
      pushEvent(state, "dm.invite_accepted", `Opened DM contact ${displayName}`);
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
          connectivity: null,
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

export async function publishVoiceSignalingMessage(
  request: PublishVoiceSignalingMessageRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>(
    "publish_voice_signaling_message",
    { request },
    () =>
      mutateFallback((state) => {
        pushCommandError(
          state,
          "voice.signal_rejected",
          "publish_voice_signaling_message",
          "voice_signal_queue_failed",
          "Local fallback web runtime cannot persist provider-signaled sealed voice envelopes; native Rust/Tauri command path is required",
          "Run the native app with provider-derived runtime peers before queueing voice signaling",
        );
      }),
  );
}

export async function takePendingVoiceSignalingMessages(
  request: TakePendingVoiceSignalingMessagesRequest = {},
): Promise<TakePendingVoiceSignalingMessagesResponse> {
  return invokeOrFallback<TakePendingVoiceSignalingMessagesResponse>(
    "take_pending_voice_signaling_messages",
    { request },
    () => {
      const state = mutateFallback((draft) => {
        pushCommandError(
          draft,
          "voice.signal_take_rejected",
          "take_pending_voice_signaling_messages",
          "voice_signal_inbox_unavailable",
          "Local fallback web runtime cannot drain backend voice signaling; native Rust/Tauri command path is required",
          "Run the native app and process provider-signaled sealed voice envelopes from backend state",
        );
      });
      return { state, messages: [] };
    },
  );
}

export async function startNativeVoiceMediaSession(
  request: StartNativeVoiceMediaSessionRequest,
): Promise<StartNativeVoiceMediaSessionResponse> {
  return invokeOrFallback<StartNativeVoiceMediaSessionResponse>(
    "start_native_voice_media_session",
    { request },
    () => {
      const state = mutateFallback((draft) => {
        pushCommandError(
          draft,
          "voice.native_media_rejected",
          "start_native_voice_media_session",
          "native_voice_media_unavailable",
          "Local fallback web runtime cannot start native Rust voice media; Tauri backend is required",
          "Run the native Tauri app before claiming native Rust voice media proof",
        );
      });
      return { state, native_media: null };
    },
  );
}

export async function acceptNativeVoiceMediaFrame(
  request: AcceptNativeVoiceMediaFrameRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("accept_native_voice_media_frame", { request }, () =>
    mutateFallback((state) => {
      pushCommandError(
        state,
        "voice.native_media_rejected",
        "accept_native_voice_media_frame",
        "native_voice_media_unavailable",
        "Local fallback web runtime cannot accept native Rust voice media; Tauri backend is required",
        "Run the native Tauri app before claiming native Rust voice media proof",
      );
    }),
  );
}

export async function acceptNativeVoiceMediaSignal(
  request: AcceptNativeVoiceMediaSignalRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("accept_native_voice_media_signal", { request }, () =>
    mutateFallback((state) => {
      pushCommandError(
        state,
        "voice.native_media_rejected",
        "accept_native_voice_media_signal",
        "native_voice_media_unavailable",
        "Local fallback web runtime cannot accept native Rust voice media signals; Tauri backend is required",
        "Run the native Tauri app before claiming native Rust voice media proof",
      );
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
        media_runtime: captureAllowed
          ? {
              runtime_id: `voice-runtime:${request.channel_id}`,
              boundary: "webview-local-capture",
              local_capture_active: true,
              remote_transport_active: false,
              remote_audio: [],
              fail_closed_reason:
                "Remote audio transport remains disabled until backend media-route evidence attaches",
              status_copy:
                "Local capture is active; remote playback remains disabled until backend media-route evidence attaches",
            }
          : {
              runtime_id: `voice-runtime:${request.channel_id}:fail-closed`,
              boundary: "fail-closed",
              local_capture_active: false,
              remote_transport_active: false,
              remote_audio: [],
              fail_closed_reason:
                "Microphone permission/input device required before joining voice",
              status_copy:
                "No local capture or remote playback route is active because voice permission was denied",
            },
        signaling: {
          ...inactiveVoiceSignalingState,
          session_id: `voice-${request.channel_id}`,
          status_copy: captureAllowed
            ? "Voice signaling waits for provider-derived peer ids before SDP/ICE exchange"
            : "Voice signaling did not start because capture permission/device gates failed",
        },
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
          ? "Local capture permission and device selection are ready; remote media transport remains gated until backend media-route evidence exists; speaking indicators wait for media audio-level/VAD events"
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
        pushEvent(state, "voice.joined", "Joined backend voice session after local media permission");
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
        "Not joined; backend voice controls are idle";
      state.voice_session.route_copy =
        "Voice media runtime stopped; no local capture or remote playback route is active";
      state.voice_session.media_runtime = {
        runtime_id: `voice-runtime:${request.session_id}`,
        boundary: "stopped",
        local_capture_active: false,
        remote_transport_active: false,
        remote_audio: [],
        fail_closed_reason: "",
        status_copy:
          "Voice media runtime stopped by leave; local tracks and remote playback are inactive",
      };
      state.voice_session.signaling = {
        ...inactiveVoiceSignalingState,
        session_id: state.voice_session.session_id,
        role: "stopped",
        status_copy: "Voice signaling stopped by leave; pending inbound SDP/ICE was cleared",
      };
      const localId = localUserId(state);
      state.voice_session.participants = state.voice_session.participants
        .filter((participant) => participant.id === localId || participant.role === "you")
        .map((participant) => ({
          ...participant,
          speaking: false,
        }));
      pushEvent(state, "voice.left", "Left backend voice session and cleared remote media attachments");
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

export async function updateVoiceActivity(
  request: UpdateVoiceActivityRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("update_voice_activity", { request }, () =>
    mutateFallback((state) => {
      if (
        !state.voice_session ||
        state.voice_session.session_id !== request.session_id
      ) {
        pushCommandError(
          state,
          "voice.activity_rejected",
          "update_voice_activity",
          "voice_session_not_found",
          state.voice_session
            ? "Voice activity request did not match active session"
            : "No active voice session for microphone activity",
          state.voice_session
            ? "Join the active voice session before sending microphone activity"
            : "Join a voice channel before sending microphone activity",
        );
        return;
      }
      if (!state.voice_session.joined) {
        pushCommandError(
          state,
          "voice.activity_rejected",
          "update_voice_activity",
          "voice_not_joined",
          "Voice activity was ignored because the voice session is not joined",
          "Join a voice channel before sending microphone activity",
        );
        return;
      }
      const selfMuted = state.voice_session.self_muted;
      const speaking =
        !selfMuted && (request.rms_i16 >= 512 || request.peak_i16 >= 2048);
      const localId = localUserId(state);
      let localParticipantFound = false;
      state.voice_session.participants = state.voice_session.participants.map(
        (participant) => {
          if (participant.id !== localId) return participant;
          localParticipantFound = true;
          return { ...participant, speaking, muted: selfMuted };
        },
      );
      if (!localParticipantFound) {
        state.voice_session.participants.push({
          id: localId,
          name: "You",
          role: "you",
          speaking,
          muted: selfMuted,
          volume: 82,
        });
      }
      if (!state.voice_session.media_runtime.remote_transport_active) {
        state.voice_session.route_copy =
          "Local capture permission, device selection, and microphone level evidence are active; remote media transport remains gated until backend media-route evidence exists";
      }
      state.voice_session.status_copy = selfMuted
        ? `Local microphone level observed at ${request.captured_at_ms} ms (rms ${request.rms_i16}, peak ${request.peak_i16}) but self-mute suppresses speaking state`
        : speaking
          ? `Local speaking indicator is driven by real microphone level evidence at ${request.captured_at_ms} ms (rms ${request.rms_i16}, peak ${request.peak_i16}); remote media transport remains gated until backend media-route evidence exists`
          : `Local microphone level observed below speaking threshold at ${request.captured_at_ms} ms (rms ${request.rms_i16}, peak ${request.peak_i16}); remote media transport remains gated until backend media-route evidence exists`;
      pushEvent(
        state,
        "voice.activity",
        `Local microphone activity ${speaking ? "speaking" : "silent"}`,
      );
    }),
  );
}

export async function attachVoiceRemoteMedia(
  request: AttachVoiceRemoteMediaRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("attach_voice_remote_media", { request }, () =>
    mutateFallback((state) => {
      if (
        !state.voice_session ||
        state.voice_session.session_id !== request.session_id
      ) {
        pushCommandError(
          state,
          "voice.remote_media_rejected",
          "attach_voice_remote_media",
          "voice_session_not_found",
          state.voice_session
            ? "Remote media evidence did not match the active voice session"
            : "No active voice session for remote media evidence",
          "Join voice before attaching remote playback",
        );
        return;
      }
      if (!state.voice_session.joined) {
        pushCommandError(
          state,
          "voice.remote_media_rejected",
          "attach_voice_remote_media",
          "voice_not_joined",
          "Remote media evidence was ignored because the voice session is not joined",
          "Join voice before attaching remote playback",
        );
        return;
      }
      const fields = [
        request.participant_id,
        request.remote_peer_id,
        request.stream_id,
        request.audio_track_id,
        request.playback_element_id,
      ];
      if (
        request.participant_id === localUserId(state) ||
        fields.some((field) => !field.trim()) ||
        request.local_audio_tracks_sent <= 0 ||
        request.received_audio_frames <= 0
      ) {
        pushCommandError(
          state,
          "voice.remote_media_rejected",
          "attach_voice_remote_media",
          "voice_remote_media_evidence_invalid",
          "Remote audio requires a non-local peer, a sent local audio track, and received remote audio frame evidence",
          "Attach only backend media-route evidence from a real WebRTC remote audio track",
        );
        return;
      }
      const existing = state.voice_session.participants.find(
        (participant) => participant.id === request.participant_id,
      );
      if (existing) {
        state.voice_session.participants = state.voice_session.participants.map(
          (participant) =>
            participant.id === request.participant_id
              ? {
                  ...participant,
                  name: request.participant_name,
                  role: "remote",
                  speaking: Boolean(request.speaking),
                  muted: false,
                }
              : participant,
        );
      } else {
        state.voice_session.participants.push({
          id: request.participant_id,
          name: request.participant_name,
          role: "remote",
          speaking: Boolean(request.speaking),
          muted: false,
          volume: 82,
        });
      }
      state.voice_session.media_runtime.remote_audio = [
        ...state.voice_session.media_runtime.remote_audio.filter(
          (track) => track.participant_id !== request.participant_id,
        ),
        {
          participant_id: request.participant_id,
          remote_peer_id: request.remote_peer_id,
          stream_id: request.stream_id,
          audio_track_id: request.audio_track_id,
          playback_element_id: request.playback_element_id,
          local_audio_tracks_sent: request.local_audio_tracks_sent,
          received_audio_frames: request.received_audio_frames,
          attached_at_ms: request.attached_at_ms,
        },
      ];
      state.voice_session.media_runtime = {
        ...state.voice_session.media_runtime,
        boundary: "webview-backend-state-audio",
        local_capture_active: true,
        remote_transport_active: true,
        fail_closed_reason: "",
        status_copy: `Backend media-route evidence attached remote WebRTC audio for ${request.participant_name} after sending ${request.local_audio_tracks_sent} local audio track(s) and receiving ${request.received_audio_frames} audio frame(s)`,
      };
      state.voice_session.route_copy =
        "Backend media-route evidence attached real WebRTC remote audio playback; remote participants and volume controls are shown only for admitted remote tracks";
      state.voice_session.status_copy = state.voice_session.media_runtime.status_copy;
      pushEvent(
        state,
        "voice.remote_media_attached",
        `Remote audio route proof attached for ${request.participant_name} via ${request.remote_peer_id}`,
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
      const targetParticipant = state.voice_session.participants.find(
        (participant) => participant.id === request.participant_id,
      );
      if (!targetParticipant) {
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
      if (targetParticipant.id === localUserId(state) || targetParticipant.role !== "remote") {
        pushCommandError(
          state,
          "voice.volume_rejected",
          "set_speaker_volume",
          "voice_volume_local_participant",
          "Speaker volume applies only to backend-admitted remote audio participants",
          "Wait for remote media evidence before changing per-peer volume",
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
        state_key: request.transport_proof
          ? "transport_probe_unavailable"
          : "sent_local",
        state_label: request.transport_proof
          ? "Transport proof unavailable"
          : "Sent locally",
        state_detail: request.transport_proof
          ? "Local fallback web runtime cannot run the Rust/Tauri provider-signaled WebRTC transport proof; native command path is required"
          : "Message is in the local encrypted author log; peer receipt requires backend-state proof",
        peer_receipt: null,
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


export async function applyTextDeliveryReceipt(
  request: ApplyTextDeliveryReceiptRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>(
    "apply_text_delivery_receipt",
    { request },
    () =>
      mutateFallback((state) => {
        pushCommandError(
          state,
          "message.receipt_rejected",
          "apply_text_delivery_receipt",
          "receipt_verification_unavailable",
          "Fallback web runtime cannot verify signed peer receipts; native Rust/Tauri command path is required",
          "Run the native app to verify peer receipt signatures",
        );
      }),
  );
}

export async function receiveTextDeliveryEnvelope(
  request: ReceiveTextDeliveryEnvelopeRequest,
): Promise<ReceiveTextDeliveryEnvelopeResponse> {
  return invokeOrFallback<ReceiveTextDeliveryEnvelopeResponse>(
    "receive_text_delivery_envelope",
    { request },
    () => {
      const state = mutateFallback((draft) => {
        pushCommandError(
          draft,
          "message.envelope_rejected",
          "receive_text_delivery_envelope",
          "text_envelope_verification_unavailable",
          "Local fallback web runtime cannot verify signed encrypted peer envelopes or generate receipts; native Rust/Tauri command path is required",
          "Run the native app to verify peer envelopes and generate delivery receipts",
        );
      });
      return {
        state,
        receipt: null,
        recipient_verifying_key_hex: null,
      };
    },
  );
}

export async function listPendingTextControlFrames(
  request: ListPendingTextControlFramesRequest = {},
): Promise<ListPendingTextControlFramesResponse> {
  return invokeOrFallback<ListPendingTextControlFramesResponse>(
    "list_pending_text_control_frames",
    { request },
    () => {
      const state = mutateFallback((draft) => {
        pushCommandError(
          draft,
          "message.outbox_unavailable",
          "list_pending_text_control_frames",
          "text_control_outbox_unavailable",
          "Local fallback web runtime cannot expose persisted signed text/control frames; native Rust/Tauri command path is required",
          "Run the native app to drive the transport session loop",
        );
      });
      return { state, frames: [] };
    },
  );
}

export async function pumpTextControlTransportOnce(
  request: ListPendingTextControlFramesRequest = {},
): Promise<TextControlTransportPumpReportView> {
  return invokeOrFallback<TextControlTransportPumpReportView>(
    "pump_text_control_transport_once",
    { request },
    () => {
      mutateFallback((state) => {
        pushCommandError(
          state,
          "message.transport_pump_unavailable",
          "pump_text_control_transport_once",
          "text_control_transport_runtime_unavailable",
          "Local fallback web runtime cannot pump signed text/control frames through the Rust transport runtime",
          "Run the native app with an active text transport session",
        );
      });
      return {
        pending_before: 0,
        frames_sent: 0,
        response_frames_received: 0,
        receipts_applied: 0,
        failures: [
          "Local fallback web runtime cannot pump signed text/control frames through the Rust transport runtime",
        ],
        metrics: {
          schema_version: 1,
          label: "fallback-text-control-runtime",
          attached_channels: 0,
          open: false,
          frames_sent: 0,
          frames_received: 0,
          bytes_sent: 0,
          bytes_received: 0,
          last_state: "unavailable",
        },
      };
    },
  );
}

export async function markTextControlFrameSent(
  request: MarkTextControlFrameSentRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>(
    "mark_text_control_frame_sent",
    { request },
    () =>
      mutateFallback((state) => {
        pushCommandError(
          state,
          "message.outbox_mark_rejected",
          "mark_text_control_frame_sent",
          "text_control_outbox_unavailable",
          "Local fallback web runtime cannot mark persisted text/control frames sent; native Rust/Tauri command path is required",
          "Run the native app so transport-session send state is persisted by the backend",
        );
      }),
  );
}

export async function handleTextControlFrame(
  request: HandleTextControlFrameRequest,
): Promise<HandleTextControlFrameResponse> {
  return invokeOrFallback<HandleTextControlFrameResponse>(
    "handle_text_control_frame",
    { request },
    () => {
      const state = mutateFallback((draft) => {
        pushCommandError(
          draft,
          "message.control_frame_rejected",
          "handle_text_control_frame",
          "text_control_frame_unavailable",
          "Local fallback web runtime cannot verify signed text/control frames or generate receipt response frames; native Rust/Tauri command path is required",
          "Run the native app to process peer text/control frames",
        );
      });
      return {
        state,
        response_frame: null,
      };
    },
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
      fallbackState.security_copy.metadata.includes("does not claim anonymity") &&
      fallbackState.security_copy.malicious_member.includes("not metadata anonymity") &&
      fallbackState.security_copy.sybil_resistance.includes(
        "do not solve Sybil attacks without a central identity",
      ),
  }));
}

export async function resetAppState(
  request: ResetAppStateRequest,
): Promise<AppState> {
  return invokeOrFallback<AppState>("reset_app_state", { request }, () => {
    hydrateFallbackState();
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
    const nextState = syncSnapshot(fallbackState);
    persistFallbackState();
    return cloneState(nextState);
  });
}

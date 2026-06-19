import React, { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  discryptUiConfig,
  ThemeId,
} from "./app-config";
import {
  AppEventStreamView,
  AppMessageView,
  AppSnapshot,
  AppState,
  ChannelKind,
  ChannelStateView,
  ConnectivityPolicyView,
  DirectConversationView,
  GroupAdmissionModeView,
  GroupAdmissionRequestView,
  GroupMemberView,
  GroupRoleView,
  GroupView,
  InviteView,
  JoinProgressStepView,
  RuntimeModeView,
  SignalingAdapterKind,
  SetConnectivityPolicyRequest,
  TextStateView,
  TransportDiagnosticsView,
  TransportStatusView,
  VoiceMediaRuntimeView,
  VoiceParticipantView,
  VoiceRemoteAudioView,
  VoiceSessionView,
  VoiceStateView,
  RESET_APP_CONFIRMATION_PHRASE,
  commandErrorToAction,
  configureStorageSecurity,
  exportDiagnosticsLog,
  defaultSignalingEndpointForAdapter,
  createChannel as createChannelCommand,
  createGroup,
  createInvite,
  createDmInvite,
  createUser,
  demoteGroupStaffToMember,
  joinGroup,
  acceptDmInvite,
  joinVoice,
  leaveVoice,
  loadAppState,
  pollAppEvents,
  promoteGroupMemberToStaff,
  recoverUser,
  refuseGroupAdmissionRequest,
  resetAppState,
  revokeGroupMemberAccess,
  savePreferences,
  sendMessage,
  verifySafetyNumber,
  setConnectivityPolicy,
  setGroupAdmissionMode,
  setActiveGroup,
  setActiveChannel,
  setActiveDm,
  setSelfMute,
  updateVoiceActivity,
  attachVoiceRemoteMedia,
  startSignalingSession,
  startTextSession,
  attachTextControlTransportRuntime,
  pumpTextControlTransportOnce,
  publishGroupPresence,
  startDm,
  approveGroupAdmissionRequest,
  unlockStorageSecurity,
} from "./commands";
import {
  startNativeRustVoiceMediaSession,
  startWebViewVoiceMediaSession,
  VoiceMediaSessionHandle,
} from "./voice-media";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Select, SelectItem } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import "./styles.css";

type Workflow =
  | "setup"
  | "dm"
  | "join"
  | "create-group"
  | "channel"
  | "voice"
  | "admission_requests";
type OverlayKind =
  | "launcher"
  | "create-group"
  | "group-invite"
  | "group-config"
  | "settings"
  | "diagnostics";
type ContextMenuPoint = {
  x: number;
  y: number;
};
type SharedContextMenuItem = {
  id: string;
  label: string;
  icon?: React.ReactNode;
  description?: string;
  danger?: boolean;
  disabled?: boolean;
  onSelect?: () => void;
};
type VoiceParticipant = VoiceParticipantView;
type StorageSetupChoice = "keyring" | "passphrase_vault";
type CommandNotification = {
  id: string;
  title: string;
  message: string;
  createdAt: string;
};
const APP_EVENT_FALLBACK_POLL_MS = 5_000;
const APP_EVENT_HEALTH_RESYNC_MS = 10_000;
const diagnosticsUiEnabled =
  import.meta.env.VITE_DISCRYPT_SHOW_DIAGNOSTICS === "1";
type VoiceDeviceAccess = {
  stream: MediaStream | null;
  microphone_permission: "granted" | "denied" | "prompt" | "unknown";
  input_device_id: string | null;
  input_device_label: string | null;
  output_device_id: string | null;
  output_device_label: string | null;
  available_input_devices: VoiceDeviceOption[];
  available_output_devices: VoiceDeviceOption[];
  activity_rms_i16: number | null;
  activity_peak_i16: number | null;
  activity_captured_at_ms: number | null;
};

type VoiceDeviceOption = {
  device_id: string;
  label: string;
};

type VoiceActivitySample = Pick<
  VoiceDeviceAccess,
  "activity_rms_i16" | "activity_peak_i16" | "activity_captured_at_ms"
>;

type VoiceActivityReading = {
  activity_rms_i16: number;
  activity_peak_i16: number;
  activity_captured_at_ms: number;
};

type StopVoiceActivityCapture = () => void;

const G012_WEBDRIVER_VOICE_HARNESS_KEY =
  "discrypt:g012:webdriver-voice-harness";

function g012WebDriverVoiceHarnessEnabled(): boolean {
  const automationWindow = window as typeof window & {
    __discryptG012ForceNativeRustVoice?: unknown;
    __discryptG012WebDriverVoiceEvidence?: unknown;
  };
  if (
    automationWindow.__discryptG012ForceNativeRustVoice !== undefined ||
    automationWindow.__discryptG012WebDriverVoiceEvidence !== undefined
  ) {
    return true;
  }
  try {
    return (
      window.localStorage?.getItem(G012_WEBDRIVER_VOICE_HARNESS_KEY) === "1"
    );
  } catch {
    return false;
  }
}

function generatedAutomationVoiceDeviceAccess(
  selectedInputDeviceId?: string,
  stream: MediaStream | null = null,
  activity: VoiceActivitySample = {
    activity_rms_i16: 1150,
    activity_peak_i16: 4096,
    activity_captured_at_ms: Date.now(),
  },
): VoiceDeviceAccess {
  const selectedGeneratedDeviceId = selectedInputDeviceId?.trim()
    ? selectedInputDeviceId
    : "g012-generated-audio-input";
  const availableInputDevices = [
    {
      device_id: selectedGeneratedDeviceId,
      label: "Generated audio input",
    },
    ...(selectedGeneratedDeviceId === "g012-generated-audio-input"
      ? []
      : [
          {
            device_id: "g012-generated-audio-input",
            label: "Generated audio input",
          },
        ]),
  ];

  return {
    stream,
    microphone_permission: "granted",
    input_device_id: selectedGeneratedDeviceId,
    input_device_label: "Generated audio input",
    output_device_id: "default",
    output_device_label: "System default speaker",
    available_input_devices: availableInputDevices,
    available_output_devices: [
      {
        device_id: "default",
        label: "System default speaker",
      },
    ],
    ...activity,
  };
}

async function requestGeneratedAutomationVoiceAccess(
  selectedInputDeviceId?: string,
): Promise<VoiceDeviceAccess | null> {
  if (!g012WebDriverVoiceHarnessEnabled()) return null;
  const audioWindow = window as Window &
    typeof globalThis & { webkitAudioContext?: typeof AudioContext };
  const AudioContextCtor =
    window.AudioContext ?? audioWindow.webkitAudioContext;
  if (!AudioContextCtor) {
    return generatedAutomationVoiceDeviceAccess(selectedInputDeviceId);
  }

  try {
    const context = new AudioContextCtor();
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    const destination = context.createMediaStreamDestination();
    oscillator.frequency.value = 440;
    gain.gain.value = 0.035;
    oscillator.connect(gain);
    gain.connect(destination);
    oscillator.start();
    await context.resume().catch(() => undefined);

    const stream = destination.stream;
    const track = stream.getAudioTracks()[0];
    if (!track) {
      await context.close().catch(() => undefined);
      return generatedAutomationVoiceDeviceAccess(selectedInputDeviceId);
    }
    const stopTrack = track.stop.bind(track);
    track.stop = () => {
      try {
        oscillator.stop();
      } catch {
        // Already stopped.
      }
      void context.close().catch(() => undefined);
      stopTrack();
    };

    const activity = await measureLocalVoiceActivity(stream).catch(() => ({
      activity_rms_i16: 1150,
      activity_peak_i16: 4096,
      activity_captured_at_ms: Date.now(),
    }));

    return generatedAutomationVoiceDeviceAccess(
      selectedInputDeviceId,
      stream,
      activity,
    );
  } catch {
    return generatedAutomationVoiceDeviceAccess(selectedInputDeviceId);
  }
}

const inactiveVoiceMediaRuntime: VoiceMediaRuntimeView = {
  runtime_id: "voice-runtime:not-started",
  boundary: "not-started",
  local_capture_active: false,
  remote_transport_active: false,
  remote_audio: [],
  fail_closed_reason: "No backend media-runtime evidence has been returned.",
  status_copy: "No capture or remote playback route is active.",
};

function isLocalVoiceParticipant(
  participant: VoiceParticipant,
  localUserId: string | null,
): boolean {
  return participant.id === localUserId || participant.role === "you";
}

function isUsableMediaStream(value: unknown): value is MediaStream {
  if (!value || typeof value !== "object") return false;
  if (typeof MediaStream !== "undefined" && value instanceof MediaStream)
    return true;
  const candidate = value as { getAudioTracks?: unknown; getTracks?: unknown };
  return (
    typeof candidate.getAudioTracks === "function" ||
    typeof candidate.getTracks === "function"
  );
}

function remoteAudioSource(
  participant: VoiceParticipant,
  mediaRuntime: VoiceMediaRuntimeView,
): string | null {
  return (
    mediaRuntime.remote_audio_streams?.find(
      (stream) => stream.participant_id === participant.id,
    )?.src ??
    participant.remote_audio_src ??
    participant.media_stream_url ??
    null
  );
}

function asThemeId(value: string): ThemeId {
  return discryptUiConfig.themes.some((theme) => theme.id === value)
    ? (value as ThemeId)
    : discryptUiConfig.activeTheme;
}

function stableUiHash(input: string): string {
  let hash = 0x811c9dc5;
  for (const char of input) {
    hash ^= char.charCodeAt(0);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

function runtimePeerIdFromCommitment(
  label: string,
  commitment: string,
): string {
  return `peer-${stableUiHash(`${label}:${commitment}`)}`;
}

function textRuntimePeerDefaults(state: AppState): {
  local: string;
  remote: string;
} {
  const scope =
    state.active_context?.dm_id ??
    state.active_context?.group_id ??
    state.active_context?.channel_id ??
    state.invites.at(-1)?.invite_key ??
    "active-scope";
  const activeDm = state.active_context?.dm_id
    ? state.dms.find((dm) => dm.dm_id === state.active_context?.dm_id)
    : state.dms[0];
  const activeGroup = state.active_context?.group_id
    ? state.groups.find(
        (group) => group.group_id === state.active_context?.group_id,
      )
    : state.groups[0];
  const activeInvite = state.active_context?.dm_id
    ? state.invites
        .slice()
        .reverse()
        .find((invite) => invite.dm_id === state.active_context?.dm_id)
    : state.active_context?.group_id
      ? state.invites
          .slice()
          .reverse()
          .find((invite) => invite.group_id === state.active_context?.group_id)
      : state.invites.at(-1);
  const dmRuntimePeers = activeDm?.runtime_peers ?? [];
  const backendLocalDmPeer = dmRuntimePeers.find((peer) => peer.is_local);
  const backendRemoteDmPeer = dmRuntimePeers.find((peer) => !peer.is_local);
  if (backendLocalDmPeer && backendRemoteDmPeer) {
    return {
      local: backendLocalDmPeer.peer_id,
      remote: backendRemoteDmPeer.peer_id,
    };
  }

  const groupRuntimePeers = activeGroup?.runtime_peers ?? [];
  const backendLocalGroupPeer = groupRuntimePeers.find((peer) => peer.is_local);
  const backendRemoteGroupPeer = groupRuntimePeers.find(
    (peer) => !peer.is_local,
  );
  if (backendLocalGroupPeer && backendRemoteGroupPeer) {
    return {
      local: backendLocalGroupPeer.peer_id,
      remote: backendRemoteGroupPeer.peer_id,
    };
  }

  const dmBootstrap =
    activeDm?.connectivity?.dm_bootstrap ?? activeInvite?.dm_bootstrap ?? null;
  if (dmBootstrap) {
    const inviterPeer = runtimePeerIdFromCommitment(
      "dm-inviter-runtime-peer",
      dmBootstrap.inviter_identity_commitment,
    );
    const replyPeer = runtimePeerIdFromCommitment(
      "dm-reply-runtime-peer",
      dmBootstrap.reply_rendezvous_commitment,
    );
    const openedFromInvite = state.events.some(
      (event) => event.kind === "dm.invite_accepted",
    );
    return openedFromInvite
      ? { local: replyPeer, remote: inviterPeer }
      : { local: inviterPeer, remote: replyPeer };
  }
  const groupBootstrap =
    activeGroup?.connectivity?.group_bootstrap ??
    activeInvite?.group_bootstrap ??
    null;
  if (groupBootstrap) {
    const ownerPeer = runtimePeerIdFromCommitment(
      "group-owner-runtime-peer",
      groupBootstrap.group_identity_commitment,
    );
    const memberPeer = runtimePeerIdFromCommitment(
      "group-member-runtime-peer",
      `${groupBootstrap.role_admission_policy_commitment}:${groupBootstrap.channel_policy_commitment}`,
    );
    const joinedFromInvite =
      activeGroup?.role !== "owner" ||
      state.events.some((event) => event.kind === "group.joined");
    return joinedFromInvite
      ? { local: memberPeer, remote: ownerPeer }
      : { local: ownerPeer, remote: memberPeer };
  }
  const remoteSeed =
    activeDm?.participant_id ??
    activeInvite?.invite_key ??
    state.active_context?.group_id ??
    "remote-peer";
  return {
    local: `peer-${stableUiHash(`local:${state.profile?.user_id ?? "local-profile-pending"}:${scope}`)}`,
    remote: `peer-${stableUiHash(`remote:${remoteSeed}:${scope}`)}`,
  };
}

function activeScopeLabelForState(state: AppState): string {
  return (
    state.active_context?.dm_id ??
    state.active_context?.group_id ??
    state.active_context?.channel_id ??
    "active-scope"
  );
}

function voiceConnectivityForState(state: AppState) {
  const group = getActiveGroup(state);
  const voiceChannel = getActiveVoiceChannel(state, group);
  return (
    voiceChannel?.connectivity ??
    group?.connectivity ??
    state.connectivity_defaults ??
    null
  );
}

function textRuntimeRole(state: AppState): "offerer" | "answerer" {
  const activeDm = state.active_context?.dm_id
    ? state.dms.find((dm) => dm.dm_id === state.active_context?.dm_id)
    : state.dms[0];
  const localDmPeer = activeDm?.runtime_peers?.find((peer) => peer.is_local);
  if (localDmPeer) {
    return localDmPeer.role === "reply" ? "answerer" : "offerer";
  }

  const activeGroup = state.active_context?.group_id
    ? state.groups.find(
        (group) => group.group_id === state.active_context?.group_id,
      )
    : state.groups[0];
  const localGroupPeer = activeGroup?.runtime_peers?.find(
    (peer) => peer.is_local,
  );
  if (localGroupPeer) {
    return localGroupPeer.role === "member" ? "answerer" : "offerer";
  }

  if (state.events.some((event) => event.kind === "dm.invite_accepted")) {
    return "answerer";
  }
  if (activeGroup?.role && activeGroup.role !== "owner") {
    return "answerer";
  }
  if (state.events.some((event) => event.kind === "group.joined")) {
    return "answerer";
  }
  return "offerer";
}

function emptyVoiceDeviceAccess(
  microphonePermission: VoiceDeviceAccess["microphone_permission"],
): VoiceDeviceAccess {
  return {
    stream: null,
    microphone_permission: microphonePermission,
    input_device_id: null,
    input_device_label: null,
    output_device_id: null,
    output_device_label: null,
    available_input_devices: [],
    available_output_devices: [],
    activity_rms_i16: null,
    activity_peak_i16: null,
    activity_captured_at_ms: null,
  };
}

function voiceDeviceOptions(
  devices: MediaDeviceInfo[],
  kind: MediaDeviceKind,
  fallbackLabel: string,
): VoiceDeviceOption[] {
  const inputs = devices.filter((device) => device.kind === kind);
  const seen = new Set<string>();
  return inputs
    .map((device, index) => ({
      device_id: device.deviceId || `${kind}-${index + 1}`,
      label: device.label || `${fallbackLabel} ${index + 1}`,
    }))
    .filter((device) => {
      if (seen.has(device.device_id)) return false;
      seen.add(device.device_id);
      return true;
    });
}

function voiceInputDeviceOptions(devices: MediaDeviceInfo[]): VoiceDeviceOption[] {
  return voiceDeviceOptions(devices, "audioinput", "Microphone");
}

function voiceOutputDeviceOptions(devices: MediaDeviceInfo[]): VoiceDeviceOption[] {
  return voiceDeviceOptions(devices, "audiooutput", "Speaker");
}

async function enumerateVoiceInputDevices(
  requestPermission = false,
): Promise<VoiceDeviceOption[]> {
  if (g012WebDriverVoiceHarnessEnabled()) {
    return [
      {
        device_id: "g012-generated-audio-input",
        label: "Generated audio input",
      },
    ];
  }
  if (!navigator.mediaDevices?.enumerateDevices) return [];
  let stream: MediaStream | null = null;
  try {
    if (requestPermission && navigator.mediaDevices.getUserMedia) {
      stream = await navigator.mediaDevices.getUserMedia({
        audio: true,
        video: false,
      });
    }
    const devices = await navigator.mediaDevices.enumerateDevices();
    return voiceInputDeviceOptions(devices);
  } finally {
    stopMediaStream(stream);
  }
}

async function measureLocalVoiceActivity(
  stream: MediaStream,
): Promise<
  Pick<
    VoiceDeviceAccess,
    "activity_rms_i16" | "activity_peak_i16" | "activity_captured_at_ms"
  >
> {
  const audioWindow = window as Window &
    typeof globalThis & { webkitAudioContext?: typeof AudioContext };
  const AudioContextCtor =
    window.AudioContext ?? audioWindow.webkitAudioContext;
  if (!AudioContextCtor) {
    return {
      activity_rms_i16: null,
      activity_peak_i16: null,
      activity_captured_at_ms: null,
    };
  }
  const context = new AudioContextCtor();
  try {
    if (context.state === "suspended") {
      await context.resume().catch(() => undefined);
    }
    const source = context.createMediaStreamSource(stream);
    const analyser = context.createAnalyser();
    analyser.fftSize = 1024;
    source.connect(analyser);
    await new Promise((resolve) => window.setTimeout(resolve, 140));
    const buffer = new Uint8Array(analyser.fftSize);
    analyser.getByteTimeDomainData(buffer);
    let squareSum = 0;
    let peak = 0;
    for (const sample of buffer) {
      const centered = Math.abs(sample - 128) / 128;
      squareSum += centered * centered;
      peak = Math.max(peak, centered);
    }
    const rms = Math.sqrt(squareSum / buffer.length);
    return {
      activity_rms_i16: Math.round(Math.min(1, rms) * 32767),
      activity_peak_i16: Math.round(Math.min(1, peak) * 32767),
      activity_captured_at_ms: Date.now(),
    };
  } finally {
    await context.close().catch(() => undefined);
  }
}

function startLocalVoiceActivityCapture(
  stream: MediaStream,
  onSample: (sample: VoiceActivityReading) => void,
): StopVoiceActivityCapture | null {
  const audioWindow = window as Window &
    typeof globalThis & { webkitAudioContext?: typeof AudioContext };
  const AudioContextCtor =
    window.AudioContext ?? audioWindow.webkitAudioContext;
  if (!AudioContextCtor) return null;

  const context = new AudioContextCtor();
  const source = context.createMediaStreamSource(stream);
  const analyser = context.createAnalyser();
  analyser.fftSize = 1024;
  source.connect(analyser);

  const buffer = new Uint8Array(analyser.fftSize);
  let stopped = false;
  let timer: number | null = null;

  const sample = () => {
    analyser.getByteTimeDomainData(buffer);
    let squareSum = 0;
    let peak = 0;
    for (const frame of buffer) {
      const centered = Math.abs(frame - 128) / 128;
      squareSum += centered * centered;
      peak = Math.max(peak, centered);
    }
    const rms = Math.sqrt(squareSum / buffer.length);
    onSample({
      activity_rms_i16: Math.round(Math.min(1, rms) * 32767),
      activity_peak_i16: Math.round(Math.min(1, peak) * 32767),
      activity_captured_at_ms: Date.now(),
    });
  };

  const schedule = () => {
    if (stopped) return;
    timer = window.setTimeout(() => {
      if (stopped) return;
      sample();
      schedule();
    }, 750);
  };

  void context
    .resume()
    .catch(() => undefined)
    .finally(schedule);

  return () => {
    stopped = true;
    if (timer !== null) window.clearTimeout(timer);
    source.disconnect?.();
    analyser.disconnect?.();
    void context.close().catch(() => undefined);
  };
}

async function requestVoiceDeviceAccess(
  selectedInputDeviceId?: string,
  selectedOutputDeviceId?: string,
): Promise<VoiceDeviceAccess> {
  const generatedAutomationAccess =
    await requestGeneratedAutomationVoiceAccess(selectedInputDeviceId);
  if (generatedAutomationAccess) return generatedAutomationAccess;

  if (!navigator.mediaDevices?.getUserMedia) {
    if (window.__TAURI__?.core?.invoke) {
      return {
        stream: null,
        microphone_permission: "granted",
        input_device_id: "native-rust-default-capture",
        input_device_label: "Native Rust capture source",
        output_device_id: "native-rust-default-playback",
        output_device_label: "Native Rust playback sink",
        available_input_devices: [
          {
            device_id: "native-rust-default-capture",
            label: "Native Rust capture source",
          },
        ],
        available_output_devices: [
          {
            device_id: "native-rust-default-playback",
            label: "Native Rust playback sink",
          },
        ],
        activity_rms_i16: null,
        activity_peak_i16: null,
        activity_captured_at_ms: null,
      };
    }
    return emptyVoiceDeviceAccess("denied");
  }

  let stream: MediaStream | null = null;
  try {
    const requestedDevice =
      selectedInputDeviceId && selectedInputDeviceId !== "default"
        ? selectedInputDeviceId
        : null;
    stream = await navigator.mediaDevices.getUserMedia({
      audio: requestedDevice ? { deviceId: { exact: requestedDevice } } : true,
      video: false,
    });
    const devices = await navigator.mediaDevices.enumerateDevices();
    const availableInputs = voiceInputDeviceOptions(devices);
    const availableOutputs = voiceOutputDeviceOptions(devices);
    const input =
      (requestedDevice
        ? devices.find(
            (device) =>
              device.kind === "audioinput" &&
              device.deviceId === requestedDevice,
          )
        : null) ??
      devices.find(
        (device) => device.kind === "audioinput" && device.deviceId,
      ) ??
      devices.find((device) => device.kind === "audioinput");
    const requestedOutput =
      selectedOutputDeviceId && selectedOutputDeviceId !== "default"
        ? selectedOutputDeviceId
        : null;
    const output =
      (requestedOutput
        ? devices.find(
            (device) =>
              device.kind === "audiooutput" &&
              device.deviceId === requestedOutput,
          )
        : null) ??
      devices.find(
        (device) => device.kind === "audiooutput" && device.deviceId,
      ) ??
      devices.find((device) => device.kind === "audiooutput");
    const activity = await measureLocalVoiceActivity(stream).catch(() => ({
      activity_rms_i16: null,
      activity_peak_i16: null,
      activity_captured_at_ms: null,
    }));
    return {
      stream,
      microphone_permission: "granted",
      input_device_id: input?.deviceId || "default",
      input_device_label: input?.label || "Default microphone",
      output_device_id: output?.deviceId || "default",
      output_device_label: output?.label || "Default speaker",
      available_input_devices: availableInputs,
      available_output_devices: availableOutputs,
      ...activity,
    };
  } catch {
    stopMediaStream(stream);
    return emptyVoiceDeviceAccess("denied");
  }
}

function stopMediaStream(stream: MediaStream | null) {
  stream?.getTracks().forEach((track) => track.stop());
}

function localAudioTracks(stream: MediaStream | null): MediaStreamTrack[] {
  if (!stream) return [];
  if (typeof stream.getAudioTracks === "function") {
    return stream.getAudioTracks();
  }
  return stream.getTracks().filter((track) => track.kind === "audio");
}

function audioTracksFromStream(stream: MediaStream | null) {
  if (!stream) return [];
  const audioTracks = stream.getAudioTracks?.();
  return audioTracks?.length ? audioTracks : stream.getTracks();
}

function parseEndpointList(value: string): string[] {
  return value
    .split(/[\n,]/)
    .map((endpoint) => endpoint.trim())
    .filter(Boolean);
}

function parseTurnEndpointList(value: string) {
  return parseEndpointList(value).map((endpoint) => ({
    endpoint,
    credential_declared: true,
    credential_expires_at: null,
  }));
}

function turnCredentialGateCopy(policy: ConnectivityPolicyView | null): string {
  const turnServers = policy?.ice_turn_servers ?? [];
  const configured = turnServers.length;
  const credentialed = turnServers.filter(
    (server) => server.credential_declared,
  ).length;
  if (configured === 0) {
    return "No TURN relay is configured. If backend route checks report TURN required, voice/text transport must fail closed instead of claiming a connection.";
  }
  if (credentialed === 0) {
    return `${configured} redacted TURN endpoint${configured === 1 ? " is" : "s are"} configured without declared credentials; relay success remains blocked until credentialed backend route evidence exists.`;
  }
  const expiring = turnServers.filter(
    (server) => server.credential_expires_at,
  ).length;
  const expiryCopy = expiring
    ? ` ${expiring} credential${expiring === 1 ? " has" : "s have"} an expiry marker.`
    : " Credentials are declared but not displayed in UI.";
  return `${credentialed}/${configured} redacted TURN endpoint${configured === 1 ? "" : "s"} credential-gated for relay fallback; relay success is shown only after backend route proof.${expiryCopy}`;
}

function providerFallbackCopy(
  diagnostics: TransportDiagnosticsView | null | undefined,
): string {
  const attempts = diagnostics?.adapter_fallback_attempts ?? [];
  if (!attempts.length) {
    return "No provider fallback attempt has been reported by backend diagnostics; retry/backoff remains unclaimed in this UI state.";
  }
  const selected = attempts.find((attempt) => attempt.selected);
  const attempted = attempts.filter((attempt) => attempt.attempted).length;
  const failed = attempts.filter(
    (attempt) =>
      attempt.attempted &&
      !attempt.selected &&
      /fail|unavailable|error|timeout|denied/i.test(
        `${attempt.readiness} ${attempt.failure_class}`,
      ),
  ).length;
  return selected
    ? `Provider fallback selected ${selected.kind} after ${attempted} backend attempt${attempted === 1 ? "" : "s"}; recovery is shown only from backend diagnostics.`
    : `${attempted} provider fallback attempt${attempted === 1 ? "" : "s"} reported, ${failed} failed/unavailable; retry/backoff stays visible as degraded until backend selects a healthy adapter.`;
}

function turnRequiredCopy(
  diagnostics: TransportDiagnosticsView | null | undefined,
  policy: ConnectivityPolicyView | null,
): string {
  const turnState = diagnostics?.turn_required ?? "not-proven";
  const normalized = turnState.toLowerCase();
  const turnServers = policy?.ice_turn_servers ?? [];
  const credentialed = turnServers.filter(
    (server) => server.credential_declared,
  ).length;
  const required =
    /required|needed|must|relay-only/.test(normalized) &&
    !/not|none|false|unproven/.test(normalized);
  if (!required) {
    return `TURN-required state: ${turnState}. ${turnCredentialGateCopy(policy)}`;
  }
  if (!turnServers.length) {
    return `TURN-required state: ${turnState}; no TURN endpoint is configured, so the app must fail closed and avoid claiming remote connectivity.`;
  }
  if (!credentialed) {
    return `TURN-required state: ${turnState}; configured TURN endpoints have no declared credentials, so relay use remains blocked until backend proves credentialed relay success.`;
  }
  return `TURN-required state: ${turnState}; ${credentialed} credentialed redacted TURN endpoint${credentialed === 1 ? "" : "s"} can be tried, but success still requires backend route proof.`;
}

function Icon({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex h-4 w-4 items-center justify-center leading-none",
        className,
      )}
    >
      {children}
    </span>
  );
}

function isKeyboardContextMenu(event: React.KeyboardEvent<HTMLElement>): boolean {
  return event.key === "ContextMenu" || (event.shiftKey && event.key === "F10");
}

function contextMenuPointFromElement(element: HTMLElement): ContextMenuPoint {
  const rect = element.getBoundingClientRect();
  return {
    x: Math.min(rect.left + 16, Math.max(12, window.innerWidth - 240)),
    y: Math.min(rect.top + Math.min(rect.height, 36), Math.max(12, window.innerHeight - 160)),
  };
}

function clampContextMenuPoint(point: ContextMenuPoint): ContextMenuPoint {
  return {
    x: Math.min(point.x, Math.max(12, window.innerWidth - 240)),
    y: Math.min(point.y, Math.max(12, window.innerHeight - 160)),
  };
}

function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() =>
    typeof window !== "undefined" && "matchMedia" in window
      ? window.matchMedia(query).matches
      : false,
  );

  useEffect(() => {
    if (typeof window === "undefined" || !("matchMedia" in window)) {
      setMatches(false);
      return;
    }
    const mediaQuery = window.matchMedia(query);
    const update = () => setMatches(mediaQuery.matches);
    update();
    mediaQuery.addEventListener?.("change", update);
    return () => mediaQuery.removeEventListener?.("change", update);
  }, [query]);

  return matches;
}

function App() {
  const [commandState, setCommandState] = useState<AppState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [verifyMessage, setVerifyMessage] = useState<string | null>(null);
  const [workflow, setWorkflow] = useState<Workflow>("setup");
  const [draftChannel, setDraftChannel] = useState("general");
  const [draftMessage, setDraftMessage] = useState("");
  const [draftGroup, setDraftGroup] = useState("");
  const [draftAdmissionMode, setDraftAdmissionMode] =
    useState<GroupAdmissionModeView>("manual_approval");
  const [draftConfigAdmissionMode, setDraftConfigAdmissionMode] =
    useState<GroupAdmissionModeView>("manual_approval");
  const [membersPanelOpen, setMembersPanelOpen] = useState(true);
  const [memberActionInFlight, setMemberActionInFlight] = useState<string | null>(null);
  const [draftSignalingAdapter, setDraftSignalingAdapter] =
    useState<SignalingAdapterKind>("mqtt");
  const [draftSignalingEndpoint, setDraftSignalingEndpoint] = useState(
    "mqtts://broker.emqx.io:8883",
  );
  const [draftIceStunServers, setDraftIceStunServers] = useState(
    "stun:stun.l.google.com:19302",
  );
  const [draftIceTurnServers, setDraftIceTurnServers] = useState("");
  const [draftInvite, setDraftInvite] = useState("");
  const [draftJoinName, setDraftJoinName] = useState("");
  const [inviteExpiryDays, setInviteExpiryDays] = useState("7");
  const [inviteMaxUses, setInviteMaxUses] = useState("5");
  const [inviteRevocationState, setInviteRevocationState] =
    useState("active_revocable");
  const [invitePasswordEnabled, setInvitePasswordEnabled] = useState(false);
  const [invitePassword, setInvitePassword] = useState("");
  const [draftDisplayName, setDraftDisplayName] = useState("");
  const [draftDeviceName, setDraftDeviceName] = useState("");
  const [draftRecoveryCode, setDraftRecoveryCode] = useState("");
  const [storagePassword, setStoragePassword] = useState("");
  const [storagePasswordConfirm, setStoragePasswordConfirm] = useState("");
  const [selectedStorageMode, setSelectedStorageMode] =
    useState<StorageSetupChoice | null>(null);
  const [draftDmName, setDraftDmName] = useState("");
  const [resetPhrase, setResetPhrase] = useState("");
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const showDesktopSidebar = useMediaQuery("(min-width: 1024px)");
  const [activeOverlay, setActiveOverlay] = useState<OverlayKind | null>(null);
  const [overlayClosing, setOverlayClosing] = useState(false);
  const [overlayGroupId, setOverlayGroupId] = useState<string | null>(null);
  const [visibleInviteId, setVisibleInviteId] = useState<string | null>(null);
  const [inlineTextDraft, setInlineTextDraft] = useState<string | null>(null);
  const [inlineVoiceDraft, setInlineVoiceDraft] = useState<string | null>(null);
  const [groupContextMenu, setGroupContextMenu] = useState<{
    groupId: string;
    x: number;
    y: number;
  } | null>(null);
  const [commandNotifications, setCommandNotifications] = useState<
    CommandNotification[]
  >([]);
  const [lastTextChannelId, setLastTextChannelId] = useState<string | null>(
    null,
  );
  const [messageTransportProof, setMessageTransportProof] = useState(false);
  const [localVoiceSpeaking, setLocalVoiceSpeaking] = useState(false);
  const [voiceInputDevices, setVoiceInputDevices] = useState<
    VoiceDeviceOption[]
  >([]);
  const [selectedVoiceInputId, setSelectedVoiceInputId] = useState("default");
  const [voiceOutputDevices, setVoiceOutputDevices] = useState<
    VoiceDeviceOption[]
  >([]);
  const [selectedVoiceOutputId, setSelectedVoiceOutputId] = useState("default");
  const [voiceDeviceStatus, setVoiceDeviceStatus] = useState<string | null>(
    null,
  );
  const [localMicGain, setLocalMicGain] = useState(100);
  const [appOutputVolume, setAppOutputVolume] = useState(100);
  const [voiceRemoteStreams, setVoiceRemoteStreams] = useState<
    Record<string, MediaStream>
  >({});
  const eventCursorRef = useRef(0);
  const commandStateRef = useRef<AppState | null>(null);
  const textRuntimeSyncInFlightRef = useRef(false);
  const groupPresenceInFlightRef = useRef(false);
  const voiceCaptureRef = useRef<MediaStream | null>(null);
  const voiceMediaSessionRef = useRef<VoiceMediaSessionHandle | null>(null);
  const stopVoiceActivityCaptureRef = useRef<StopVoiceActivityCapture | null>(
    null,
  );

  function cleanupVoiceMediaSession() {
    voiceMediaSessionRef.current?.close();
    voiceMediaSessionRef.current = null;
    setVoiceRemoteStreams({});
  }

  function stopLocalVoiceCapture() {
    voiceMediaSessionRef.current?.close();
    voiceMediaSessionRef.current = null;
    setVoiceRemoteStreams({});
    stopVoiceActivityCaptureRef.current?.();
    stopVoiceActivityCaptureRef.current = null;
    stopMediaStream(voiceCaptureRef.current);
    voiceCaptureRef.current = null;
    setLocalVoiceSpeaking(false);
  }

  useEffect(() => {
    commandStateRef.current = commandState;
  }, [commandState]);

  useEffect(() => {
    voiceMediaSessionRef.current?.setInputGain?.(localMicGain);
  }, [localMicGain]);

  function updateEventCursor(nextCursor: number) {
    const cursor = Math.max(eventCursorRef.current, nextCursor);
    eventCursorRef.current = cursor;
  }

  function reportCommandError(message: string, title = "Command error") {
    const notification: CommandNotification = {
      id: `command-error-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      title,
      message,
      createdAt: new Date().toLocaleTimeString(),
    };
    setCommandError(message);
    setCommandNotifications((current) => [notification, ...current].slice(0, 6));
  }

  function dismissCommandNotification(id: string) {
    setCommandNotifications((current) =>
      current.filter((notification) => notification.id !== id),
    );
  }

  function finishClosingOverlay(closedOverlay: OverlayKind | null) {
    if (closedOverlay === "group-invite" || closedOverlay === "launcher") {
      setVisibleInviteId(null);
      setDraftInvite("");
    }
    if (closedOverlay === "group-config" || closedOverlay === "group-invite") {
      setOverlayGroupId(null);
    }
    setActiveOverlay(null);
    setOverlayClosing(false);
  }

  function closeOverlay() {
    if (!activeOverlay || overlayClosing) return;
    const closing = activeOverlay;
    setOverlayClosing(true);
    window.setTimeout(() => finishClosingOverlay(closing), 160);
  }

  function openLauncherOverlay() {
    setOverlayClosing(false);
    setVisibleInviteId(null);
    setDraftInvite("");
    setActiveOverlay("launcher");
  }

  function hydrateConnectivityDrafts(policy: ConnectivityPolicyView | null) {
    const profile = policy?.signaling_profiles[0];
    if (profile?.adapter_kind) {
      setDraftSignalingAdapter(profile.adapter_kind as SignalingAdapterKind);
    }
    if (profile?.endpoints?.[0]) {
      setDraftSignalingEndpoint(profile.endpoints[0]);
    }
    setDraftIceStunServers((policy?.ice_stun_servers ?? []).join(", "));
    setDraftIceTurnServers(
      (policy?.ice_turn_servers ?? [])
        .map((server) => server.endpoint)
        .join(", "),
    );
  }

  function chooseSignalingAdapter(value: string) {
    const adapter = value as SignalingAdapterKind;
    setDraftSignalingAdapter(adapter);
    setDraftSignalingEndpoint(defaultSignalingEndpointForAdapter(adapter));
  }

  function openGroupInviteOverlay(groupId: string) {
    setOverlayClosing(false);
    setOverlayGroupId(groupId);
    setVisibleInviteId(null);
    setDraftInvite("");
    setActiveOverlay("group-invite");
  }

  function openGroupConfigOverlay(groupId: string) {
    const group = commandState?.groups.find((candidate) => candidate.group_id === groupId);
    setOverlayClosing(false);
    setOverlayGroupId(groupId);
    hydrateConnectivityDrafts(group?.connectivity ?? commandState?.connectivity_defaults ?? null);
    setDraftConfigAdmissionMode(group?.role_policy?.admission_mode ?? "manual_approval");
    setActiveOverlay("group-config");
  }

  useEffect(() => {
    let mounted = true;
    loadAppState()
      .then(async (loaded) => {
        if (!mounted) return;
        let initialState = loaded;
        if (loaded.voice_session?.joined) {
          stopLocalVoiceCapture();
          const sessionId = loaded.voice_session.session_id;
          try {
            initialState = await leaveVoice({ session_id: sessionId });
          } catch (error) {
            reportCommandError(
              error instanceof Error
                ? error.message
                : "Unable to clear stale voice session on startup.",
              "Voice state",
            );
            initialState = {
              ...loaded,
              active_context:
                loaded.active_context?.kind === "voice_channel"
                  ? null
                  : loaded.active_context,
              voice_session: null,
            };
          }
          if (!mounted) return;
        }
        setCommandState(initialState);
        updateEventCursor(initialState.event_cursor);
        if (
          initialState.groups.length > 0 &&
          initialState.lifecycle !== "first_run"
        ) {
          setWorkflow("channel");
        }
      })
      .catch(
        (error: unknown) =>
          mounted &&
          setLoadError(
            error instanceof Error
              ? error.message
              : "Unable to load app command state",
          ),
      );
    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => () => stopLocalVoiceCapture(), []);

  useEffect(() => {
    let cancelled = false;
    void enumerateVoiceInputDevices(false)
      .then((devices) => {
        if (cancelled || devices.length === 0) return;
        setVoiceInputDevices(devices);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!commandState?.voice_session?.joined) {
      stopLocalVoiceCapture();
    }
  }, [commandState?.voice_session?.joined]);

  useEffect(() => {
    const enabled = !Boolean(commandState?.voice_session?.self_muted);
    localAudioTracks(voiceCaptureRef.current).forEach((track) => {
      track.enabled = enabled;
    });
    voiceMediaSessionRef.current?.setMuted(!enabled);
    if (!enabled) setLocalVoiceSpeaking(false);
  }, [commandState?.voice_session?.self_muted]);

  useEffect(() => {
    if (!commandState || commandState.runtime_mode.mode !== "native") {
      return;
    }

    const tauriListen = window.__TAURI__?.event?.listen;
    let cancelled = false;
    let eventFallbackPoll: number | null = null;
    let eventHealthResync: number | null = null;
    let unlistenAppEvent: (() => void) | null = null;
    const fallbackPollMs = tauriListen ? 30000 : 5000;

    const refreshCommandState = (stream: AppEventStreamView) => {
      if (stream.events.length === 0) {
        updateEventCursor(stream.next_cursor);
        return;
      }
      void loadAppState()
        .then((refreshed) => {
          if (!cancelled) {
            setCommandState(refreshed);
            updateEventCursor(
              Math.max(stream.next_cursor, refreshed.event_cursor),
            );
          }
        })
        .catch(() => undefined);
    };

    const pollAppEventFallback = () => {
      void pollAppEvents({ after: eventCursorRef.current, limit: 32 })
        .then((stream) => {
          if (!cancelled) {
            refreshCommandState(stream);
          }
        })
        .catch(() => undefined);
    };

    const startFallbackPolling = () => {
      if (eventFallbackPoll !== null) return;
      eventFallbackPoll = window.setInterval(
        pollAppEventFallback,
        fallbackPollMs,
      );
    };

    if (tauriListen) {
      void tauriListen<AppEventStreamView>("app_event", (event) => {
        refreshCommandState(event.payload);
      })
        .then((unlisten) => {
          if (cancelled) {
            unlisten();
            return;
          }
          unlistenAppEvent = unlisten;
        })
        .catch(startFallbackPolling);
      startFallbackPolling();
      eventHealthResync = window.setInterval(
        pollAppEventFallback,
        APP_EVENT_HEALTH_RESYNC_MS,
      );
    } else {
      startFallbackPolling();
    }

    return () => {
      cancelled = true;
      unlistenAppEvent?.();
      if (eventFallbackPoll !== null) {
        window.clearInterval(eventFallbackPoll);
      }
      if (eventHealthResync !== null) {
        window.clearInterval(eventHealthResync);
      }
    };
  }, [commandState?.runtime_mode.mode]);

  async function applyCommand(
    command: Promise<AppState>,
    success?: (state: AppState) => void,
  ): Promise<AppState | null> {
    try {
      setCommandError(null);
      const nextState = await command;
      setCommandState(nextState);
      if (nextState.last_command_error) {
        const action = commandErrorToAction(nextState.last_command_error);
        reportCommandError(
          action
            ? `${nextState.last_command_error.message} — ${action}`
            : nextState.last_command_error.message,
          nextState.last_command_error.command,
        );
      }
      success?.(nextState);
      return nextState;
    } catch (error: unknown) {
      reportCommandError(
        error instanceof Error ? error.message : "Command failed",
        "Command failed",
      );
      return null;
    }
  }

  async function probeSelectedAdapter() {
    const scopeLabel =
      commandState?.active_context?.dm_id ??
      commandState?.active_context?.group_id ??
      commandState?.active_context?.channel_id ??
      "active-scope";
    await applyCommand(
      startSignalingSession({
        scope_label: scopeLabel,
        adapter_probe: true,
        data_channel_probe: false,
        adapter_kind: null,
      }),
    );
  }

  function activeScopeLabel() {
    return commandState
      ? activeScopeLabelForState(commandState)
      : "active-scope";
  }

  async function probeSelectedDataChannel() {
    await applyCommand(
      startSignalingSession({
        scope_label: activeScopeLabel(),
        adapter_probe: false,
        data_channel_probe: true,
        adapter_kind: null,
      }),
    );
  }

  async function startTextTransportProof() {
    await applyCommand(
      startTextSession({
        scope_label: activeScopeLabel(),
        data_channel_probe: true,
        adapter_kind: null,
      }),
    );
  }

  async function attachTextRuntime(
    stateOverride?: AppState,
  ): Promise<AppState | null> {
    const runtimeState = stateOverride ?? commandState;
    if (!runtimeState) return null;
    // Runtime role and peer ids are derived inside the Rust app-service from
    // signed invite/connectivity state; the UI never supplies manual pairing ids.
    return applyCommand(
      attachTextControlTransportRuntime({
        session_id: null,
        derive_from_state: true,
      }),
    );
  }

  async function refreshVoiceInputDevices(requestPermission = true) {
    try {
      setVoiceDeviceStatus(
        requestPermission
          ? "Requesting microphone access and refreshing devices…"
          : "Refreshing microphone devices…",
      );
      const devices = await enumerateVoiceInputDevices(requestPermission);
      setVoiceInputDevices(devices);
      if (navigator.mediaDevices?.enumerateDevices) {
        const allDevices = await navigator.mediaDevices.enumerateDevices();
        const outputDevices = voiceOutputDeviceOptions(allDevices);
        setVoiceOutputDevices(outputDevices);
        if (
          selectedVoiceOutputId !== "default" &&
          !outputDevices.some(
            (device) => device.device_id === selectedVoiceOutputId,
          )
        ) {
          setSelectedVoiceOutputId("default");
        }
      }
      setVoiceDeviceStatus(
        devices.length > 0
          ? `Found ${devices.length} microphone${devices.length === 1 ? "" : "s"}.`
          : "No microphone devices were reported by the Linux audio stack.",
      );
      if (
        selectedVoiceInputId !== "default" &&
        !devices.some((device) => device.device_id === selectedVoiceInputId)
      ) {
        setSelectedVoiceInputId("default");
      }
    } catch (error) {
      setVoiceDeviceStatus(
        error instanceof Error
          ? `Microphone refresh failed: ${error.message}`
          : "Microphone refresh failed.",
      );
    }
  }

  async function ensureTextRuntimeForActiveScope(
    stateForScope: AppState,
    reportFailures = true,
  ): Promise<AppState | null> {
    // Backend-derived runtime peers come from invite/connectivity state, not user-entered pairing fields.
    if (!window.__TAURI__?.core?.invoke) return stateForScope;
    const scopeLabel = activeScopeLabelForState(stateForScope);
    let started: AppState;
    try {
      started = await startTextSession({
        scope_label: scopeLabel,
        data_channel_probe: false,
        adapter_kind: null,
      });
    } catch (error) {
      const message =
        error instanceof Error ? error.message : String(error ?? "");
      if (reportFailures) {
        reportCommandError(message || "Text runtime did not start.", "start_text_session");
      }
      return stateForScope;
    }
    setCommandState(started);
    if (started.last_command_error) {
      const action = commandErrorToAction(started.last_command_error);
      if (reportFailures) {
        reportCommandError(
          action
            ? `${started.last_command_error.message} — ${action}`
            : started.last_command_error.message,
          started.last_command_error.command,
        );
      }
      return started;
    }
    const attached = await attachTextRuntime(started).catch((error: unknown) => {
      const message =
        error instanceof Error ? error.message : String(error ?? "");
      if (reportFailures) {
        reportCommandError(
          message || "Text runtime did not attach.",
          "attach_text_control_transport_runtime",
        );
      }
      return null;
    });
    const report = await pumpTextControlTransportOnce({
      target: null,
      limit: 8,
      operation_timeout_ms: 5_000,
    });
    if (
      reportFailures &&
      attached?.last_command_error &&
      report.frames_sent === 0 &&
      report.response_frames_received === 0
    ) {
      const action = commandErrorToAction(attached.last_command_error);
      reportCommandError(
        action
          ? `${attached.last_command_error.message} — ${action}`
          : attached.last_command_error.message,
        attached.last_command_error.command,
      );
    }
    if (reportFailures && report.failures.length > 0) {
      reportCommandError(
        report.failures[0],
        "pump_text_control_transport_once",
      );
    }
    const refreshed = await loadAppState().catch(() => null);
    if (refreshed) {
      setCommandState(refreshed);
      return refreshed;
    }
    return attached ?? started;
  }

  async function syncTextRuntimeForState(
    stateForScope: AppState,
    reportFailures = false,
  ): Promise<AppState | null> {
    if (textRuntimeSyncInFlightRef.current || !window.__TAURI__?.core?.invoke) {
      return stateForScope;
    }
    textRuntimeSyncInFlightRef.current = true;
    try {
      const synced = await ensureTextRuntimeForActiveScope(stateForScope, reportFailures);
      return synced;
    } catch (error) {
      if (reportFailures) {
        reportCommandError(
          error instanceof Error ? error.message : String(error ?? "Text runtime sync failed."),
          "text_runtime_sync",
        );
      }
      return stateForScope;
    } finally {
      textRuntimeSyncInFlightRef.current = false;
    }
  }

  useEffect(() => {
    const activeGroupId = commandState?.active_context?.group_id ?? null;
    if (
      !commandState ||
      !activeGroupId ||
      commandState.lifecycle === "first_run" ||
      commandState.storage_security.status !== "ready" ||
      commandState.runtime_mode.mode !== "native" ||
      !window.__TAURI__?.core?.invoke
    ) {
      return;
    }

    let cancelled = false;
    const publishAndPump = async () => {
      const latestState = commandStateRef.current ?? commandState;
      const latestGroupId = latestState.active_context?.group_id ?? activeGroupId;
      if (cancelled || groupPresenceInFlightRef.current || !latestGroupId) return;
      groupPresenceInFlightRef.current = true;
      try {
        const localStatus = latestState.groups
          .find((group) => group.group_id === latestGroupId)
          ?.members?.find((member) => member.member_id === latestState.profile?.user_id)
          ?.status;
        if (localStatus === "pending") {
          await syncTextRuntimeForState(latestState, false);
          return;
        }
        const presenceState = await publishGroupPresence({
          group_id: latestGroupId,
          member_id: null,
          status: "online",
          ttl_seconds: 120,
        });
        if (cancelled) return;
        setCommandState(presenceState);
        commandStateRef.current = presenceState;
        await syncTextRuntimeForState(presenceState, false);
      } catch (_error) {
      } finally {
        groupPresenceInFlightRef.current = false;
      }
    };

    void publishAndPump();
    const intervalId = window.setInterval(publishAndPump, 30_000);
    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, [
    commandState?.active_context?.group_id,
    commandState?.lifecycle,
    commandState?.runtime_mode.mode,
    commandState?.storage_security.status,
    commandState?.profile?.user_id,
  ]);

  if (loadError) {
    return (
      <main className="grid min-h-dvh place-items-center bg-[hsl(var(--background))] p-6 text-red-200">
        <span role="alert">discrypt command surface failed: {loadError}</span>
      </main>
    );
  }
  if (!commandState) {
    return (
      <main className="grid min-h-dvh place-items-center bg-[hsl(var(--background))] p-6 text-[hsl(var(--foreground))]">
        <span role="status">Loading discrypt…</span>
      </main>
    );
  }

  const appState = commandState;
  const currentSnapshot = appState.snapshot;
  const activeGroup = getActiveGroup(appState);
  const hasActiveGroup = Boolean(activeGroup);
  const overlayGroup =
    appState.groups.find((group) => group.group_id === overlayGroupId) ??
    activeGroup;
  const activeTextChannel = getActiveTextChannel(
    appState,
    activeGroup,
    lastTextChannelId,
  );
  const activeVoiceChannel = getActiveVoiceChannel(appState, activeGroup);
  const textChannels =
    activeGroup?.channels.filter((channel) => channel.kind === "Text") ?? [];
  const voiceChannels =
    activeGroup?.channels.filter((channel) => channel.kind === "Voice") ?? [];
  const activeDm = getActiveDm(appState);
  const activeConnectivity =
    activeTextChannel?.connectivity ??
    activeVoiceChannel?.connectivity ??
    activeGroup?.connectivity ??
    activeDm?.connectivity ??
    appState.connectivity_defaults;
  const groupLabel = activeGroup?.name ?? "Local profile";
  const topBarScopeLabel = workflow === "dm" ? "Direct messages" : groupLabel;
  const activePane = activePaneSummary(
    workflow,
    groupLabel,
    activeTextChannel,
    activeVoiceChannel,
    activeDm,
  );
  const groupMembers = normalizedGroupMembers(activeGroup, appState);
  const localGroupRole = localGroupRoleForUi(activeGroup, appState);
  const localMemberStatus = activeGroup?.members?.find(
    (member) => member.member_id === appState.profile?.user_id,
  )?.status;
  const localAdmissionPending = localMemberStatus === "pending";
  const canReviewAdmissions = ["owner", "staff"].includes(localGroupRole);
  const pendingAdmissionRequests = (activeGroup?.admission_requests ?? []).filter(
    (request) => request.status === "pending",
  );
  const backendVoiceParticipants = appState.voice_session?.participants ?? [];
  const voiceJoined = appState.voice_session?.joined ?? false;
  const selfMuted =
    appState.voice_session?.self_muted ??
    backendVoiceParticipants.find(
      (participant) => participant.id === appState.profile?.user_id,
    )?.muted ??
    false;
  const localVoiceParticipant: VoiceParticipant | null =
    voiceJoined &&
    appState.profile &&
    voiceCaptureRef.current &&
    !backendVoiceParticipants.some(
      (participant) => participant.id === appState.profile?.user_id,
    )
      ? {
          id: appState.profile.user_id,
          name: "You",
          role: "you",
          speaking: localVoiceSpeaking && !selfMuted,
          muted: selfMuted,
          volume: 82,
        }
      : null;
  const remoteStreamParticipants: VoiceParticipant[] = Object.keys(
    voiceRemoteStreams,
  )
    .filter(
      (participantId) =>
        !backendVoiceParticipants.some(
          (participant) => participant.id === participantId,
        ),
    )
    .map((participantId) => ({
      id: participantId,
      name: `Remote ${participantId}`,
      role: "remote",
      speaking: true,
      muted: false,
      volume: 82,
    }));
  const participants = [
    ...(localVoiceParticipant ? [localVoiceParticipant] : []),
    ...backendVoiceParticipants,
    ...remoteStreamParticipants,
  ];


  const activeTheme =
    discryptUiConfig.themes.find(
      (theme) => theme.id === appState.preferences.theme_id,
    ) ?? discryptUiConfig.themes[0];
  const themeStyle = {
    ...activeTheme.cssVars,
    "--shell-grid":
      workflow === "dm" || workflow === "setup" || !hasActiveGroup
        ? "72px minmax(0,1fr)"
        : membersPanelOpen
          ? "72px 300px minmax(0,1fr) 320px"
          : "72px 300px minmax(0,1fr)",
    "--shell-grid-inspector":
      workflow === "dm" || workflow === "setup" || !hasActiveGroup
        ? "72px minmax(0,1fr) 280px"
        : "72px 300px minmax(0,1fr) 280px",
    "--shell-font-size": "16px",
    "--shell-panel-radius": "1rem",
  } as React.CSSProperties & Record<`--${string}`, string>;
  const showInspector =
    diagnosticsUiEnabled && inspectorOpen && workflow !== "setup";

  async function configureFirstRunStorage(): Promise<boolean> {
    if (appState.storage_security.status === "ready") return true;
    if (!selectedStorageMode) {
      reportCommandError(
        "Choose OS keyring or Discrypt password vault before account setup.",
        "configure_storage_security",
      );
      return false;
    }
    if (selectedStorageMode === "passphrase_vault") {
      if (storagePassword.length < 12) {
        reportCommandError(
          "Discrypt storage password must be at least 12 characters.",
          "configure_storage_security",
        );
        return false;
      }
      if (storagePassword !== storagePasswordConfirm) {
        reportCommandError(
          "Storage passwords do not match.",
          "configure_storage_security",
        );
        return false;
      }
    }
    const configured = await applyCommand(
      configureStorageSecurity({
        mode: selectedStorageMode,
        passphrase:
          selectedStorageMode === "passphrase_vault" ? storagePassword : null,
      }),
    );
    if (
      !configured ||
      configured.last_command_error ||
      configured.storage_security.status !== "ready"
    ) {
      return false;
    }
    return true;
  }

  async function confirmSafetyNumber() {
    try {
      const result = await verifySafetyNumber({
        friend_id: currentSnapshot.friend.friend_code,
        provided: currentSnapshot.friend.safety_number,
      });
      setVerifyMessage(result.message);
    } catch (error) {
      reportCommandError(
        error instanceof Error
          ? error.message
          : "Unable to verify the current DM safety number.",
        "verify_safety_number",
      );
    }
  }

  async function createCommandUser() {
    if (!(await configureFirstRunStorage())) return;
    void applyCommand(
      createUser({
        display_name: draftDisplayName,
        device_name: draftDeviceName,
      }),
      () => {
        setStoragePassword("");
        setStoragePasswordConfirm("");
        setWorkflow("setup");
      },
    );
  }

  async function recoverCommandUser() {
    if (!(await configureFirstRunStorage())) return;
    void applyCommand(
      recoverUser({
        display_name: draftDisplayName,
        device_name: draftDeviceName,
        recovery_code: draftRecoveryCode,
        recovery_room_memberships: ["Recovered Private Lab"],
        recovered_device_count: 2,
        use_sealed_account_backup: true,
      }),
      () => {
        setStoragePassword("");
        setStoragePasswordConfirm("");
        setWorkflow("setup");
      },
    );
  }

  function createCommandGroup() {
    void applyCommand(
      createGroup({
        name: draftGroup,
        retention: currentSnapshot.retention.selected,
        admission_mode: draftAdmissionMode,
        adapter_kind: draftSignalingAdapter,
        signaling_endpoint: draftSignalingEndpoint,
        ice_stun_servers: parseEndpointList(draftIceStunServers),
        ice_turn_servers: parseTurnEndpointList(draftIceTurnServers),
      }),
      (state) => {
        const group = getActiveGroup(state);
        setDraftGroup(group?.name ?? draftGroup);
        setDraftConfigAdmissionMode(group?.role_policy?.admission_mode ?? draftAdmissionMode);
        setWorkflow("channel");
        setActiveOverlay(null);
      },
    );
  }

  function saveConnectivityPolicy(
    scopeKind: SetConnectivityPolicyRequest["scope_kind"],
  ) {
    const activeChannel = activeTextChannel ?? activeVoiceChannel ?? null;
    void applyCommand(
      setConnectivityPolicy({
        scope_kind: scopeKind,
        group_id: activeGroup?.group_id ?? null,
        channel_id: activeChannel?.channel_id ?? null,
        dm_id: activeDm?.dm_id ?? null,
        adapter_kind: draftSignalingAdapter,
        signaling_endpoint: draftSignalingEndpoint,
        ice_stun_servers: parseEndpointList(draftIceStunServers),
        ice_turn_servers: parseTurnEndpointList(draftIceTurnServers),
      }),
    );
  }

  async function saveGroupConfiguration(groupId: string) {
    const connectivityState = await applyCommand(
      setConnectivityPolicy({
        scope_kind: "group",
        group_id: groupId,
        channel_id: null,
        dm_id: null,
        adapter_kind: draftSignalingAdapter,
        signaling_endpoint: draftSignalingEndpoint,
        ice_stun_servers: parseEndpointList(draftIceStunServers),
        ice_turn_servers: parseTurnEndpointList(draftIceTurnServers),
      }),
    );
    if (!connectivityState?.last_command_error) {
      void applyCommand(
        setGroupAdmissionMode({
          group_id: groupId,
          admission_mode: draftConfigAdmissionMode,
        }),
        () => setActiveOverlay(null),
      );
    }
  }

  function reviewPendingAdmissions() {
    setWorkflow("admission_requests");
  }

  function runMemberAction(actionId: string, command: Promise<AppState>) {
    setMemberActionInFlight(actionId);
    void applyCommand(command, () => setMemberActionInFlight(null)).finally(() =>
      setMemberActionInFlight(null),
    );
  }

  function approveAdmission(request: GroupAdmissionRequestView) {
    if (!activeGroup) return;
    runMemberAction(
      `approve:${request.request_id}`,
      approveGroupAdmissionRequest({
        group_id: activeGroup.group_id,
        request_id: request.request_id,
      }),
    );
  }

  function refuseAdmission(request: GroupAdmissionRequestView) {
    if (!activeGroup) return;
    runMemberAction(
      `refuse:${request.request_id}`,
      refuseGroupAdmissionRequest({
        group_id: activeGroup.group_id,
        request_id: request.request_id,
        reason: "Refused from member panel review",
      }),
    );
  }

  function promoteMember(member: GroupMemberView) {
    if (!activeGroup) return;
    runMemberAction(
      `promote:${member.member_id}`,
      promoteGroupMemberToStaff({
        group_id: activeGroup.group_id,
        member_id: member.member_id,
      }),
    );
  }

  function demoteMember(member: GroupMemberView) {
    if (!activeGroup) return;
    runMemberAction(
      `demote:${member.member_id}`,
      demoteGroupStaffToMember({
        group_id: activeGroup.group_id,
        member_id: member.member_id,
      }),
    );
  }

  function revokeMember(member: GroupMemberView) {
    if (!activeGroup) return;
    if (!window.confirm(`Revoke ${member.display_name} access to ${activeGroup.name}?`)) return;
    runMemberAction(
      `revoke:${member.member_id}`,
      revokeGroupMemberAccess({
        group_id: activeGroup.group_id,
        member_id: member.member_id,
        reason: "Revoked from member panel",
      }),
    );
  }

  function joinCommandGroup() {
    void applyCommand(
      joinGroup({
        invite_code: draftInvite,
        group_name: draftJoinName || null,
      }),
      (state) => {
        const group = getActiveGroup(state);
        setDraftJoinName(group?.name ?? draftJoinName);
        setDraftInvite("");
        setVisibleInviteId(null);
        setWorkflow("channel");
        setActiveOverlay(null);
        void syncTextRuntimeForState(state, true);
      },
    );
  }

  function startCommandDm() {
    void applyCommand(startDm({ display_name: draftDmName }), () => {
      setWorkflow("dm");
      setActiveOverlay(null);
    });
  }

  function focusCommandGroup(groupId: string) {
    void applyCommand(setActiveGroup({ group_id: groupId }), () =>
      setWorkflow("channel"),
    );
  }

  function focusCommandChannel(channelId: string, kind: ChannelKind) {
    if (!activeGroup) return;
    const targetWorkflow: Workflow =
      kind === "Voice" && !showDesktopSidebar ? "voice" : "channel";
    if (kind === "Text") setLastTextChannelId(channelId);
    void applyCommand(
      setActiveChannel({
        group_id: activeGroup.group_id,
        channel_id: channelId,
      }),
      (nextState) => {
        setWorkflow(targetWorkflow);
        if (kind === "Voice") {
          const nextGroup =
            nextState.groups.find(
              (group) => group.group_id === activeGroup.group_id,
            ) ?? activeGroup;
          const voiceChannel =
            nextGroup.channels.find(
              (channel) =>
                channel.channel_id === channelId && channel.kind === "Voice",
            ) ?? null;
          void toggleVoiceJoin(true, voiceChannel, nextState, targetWorkflow);
        }
      },
    );
  }

  function focusCommandDm(dmId: string) {
    void applyCommand(setActiveDm({ dm_id: dmId }), () => {
      setWorkflow("dm");
    });
  }

  function createCommandChannel(kind: ChannelKind = "Text") {
    if (!activeGroup) {
      reportCommandError("Create or join a group before adding a channel.");
      return;
    }
    const name =
      draftChannel.trim().replace(/^#/, "") ||
      (kind === "Text" ? "general" : "Voice Lobby");
    void applyCommand(
      createChannelCommand({
        group_id: activeGroup.group_id,
        name,
        kind,
        retention_status:
          kind === "Voice" ? "session" : currentSnapshot.retention.selected,
      }),
      (nextState) => {
        if (kind === "Text") {
          const nextGroup = getActiveGroup(nextState);
          const nextText = getActiveTextChannel(nextState, nextGroup, null);
          setLastTextChannelId(nextText?.channel_id ?? null);
        }
        setWorkflow(kind === "Voice" && !showDesktopSidebar ? "voice" : "channel");
        setActiveOverlay(null);
      },
    );
  }

  function commitInlineChannel(kind: ChannelKind, rawName: string) {
    const name = rawName.trim().replace(/^#/, "");
    if (!name) {
      if (kind === "Text") setInlineTextDraft(null);
      if (kind === "Voice") setInlineVoiceDraft(null);
      return;
    }
    if (!activeGroup) {
      reportCommandError("Create or join a group before adding a channel.");
      return;
    }
    void applyCommand(
      createChannelCommand({
        group_id: activeGroup.group_id,
        name,
        kind,
        retention_status:
          kind === "Voice" ? "session" : currentSnapshot.retention.selected,
      }),
      (nextState) => {
        if (kind === "Text") {
          const nextGroup = getActiveGroup(nextState);
          const nextText = getActiveTextChannel(nextState, nextGroup, null);
          setLastTextChannelId(nextText?.channel_id ?? null);
          setInlineTextDraft(null);
          setWorkflow("channel");
        } else {
          setInlineVoiceDraft(null);
          setWorkflow(showDesktopSidebar ? "channel" : "voice");
        }
      },
    );
  }

  function sendCommandMessage() {
    const body = draftMessage.trim();
    if (!body) return;
    if (!activeGroup || !activeTextChannel) {
      reportCommandError("Create a group text channel before sending a message.");
      return;
    }
    const runtimeState = commandState;
    const target = {
      kind: "channel" as const,
      dm_id: null,
      group_id: activeGroup.group_id,
      channel_id: activeTextChannel.channel_id,
    };
    const requestTransportProof = messageTransportProof;
    void (async () => {
      let latestState = runtimeState;
      if (runtimeState && window.__TAURI__?.core?.invoke) {
        latestState = (await syncTextRuntimeForState(runtimeState, true)) ?? runtimeState;
      }
      await applyCommand(
        sendMessage({
          target,
          body,
          transport_proof: requestTransportProof,
          adapter_kind: null,
        }),
        async (state) => {
          setDraftMessage("");
          if (latestState && window.__TAURI__?.core?.invoke) {
            await syncTextRuntimeForState(state, false);
          }
        },
      );
    })();
  }

  function sendCommandDm() {
    const body = draftMessage.trim();
    if (!body || !activeDm) return;
    const runtimeState = commandState;
    const target = {
      kind: "dm" as const,
      dm_id: activeDm.dm_id,
      group_id: null,
      channel_id: null,
    };
    const requestTransportProof = messageTransportProof;
    void (async () => {
      let latestState = runtimeState;
      if (runtimeState && window.__TAURI__?.core?.invoke) {
        latestState = (await syncTextRuntimeForState(runtimeState, true)) ?? runtimeState;
      }
      await applyCommand(
        sendMessage({
          target,
          body,
          transport_proof: requestTransportProof,
          adapter_kind: null,
        }),
        async (state) => {
          setDraftMessage("");
          if (latestState && window.__TAURI__?.core?.invoke) {
            await syncTextRuntimeForState(state, false);
          }
        },
      );
    })();
  }

  function createCommandInvite() {
    if (!activeGroup) {
      reportCommandError("Create or join a group before creating an invite.");
      return;
    }
    createCommandInviteForGroup(activeGroup.group_id);
  }

  function createCommandInviteForGroup(groupId: string) {
    const expiresLabel = `${inviteExpiryDays.trim() || "7"} days`;
    const maxUseLabel = `${inviteMaxUses.trim() || "5"} uses`;
    void applyCommand(
      createInvite({
        group_id: groupId,
        expires: expiresLabel,
        max_use: maxUseLabel,
        revocation_state: inviteRevocationState,
        password_gate: invitePasswordEnabled ? invitePassword : null,
      }),
      (state) => {
        const invite = state.invites.at(-1);
        if (invite) {
          setDraftInvite(invite.code);
          setVisibleInviteId(invite.invite_id);
        }
        setOverlayGroupId(groupId);
        setActiveOverlay("group-invite");
      },
    );
  }

  function createCommandDmInvite() {
    if (!activeDm) {
      reportCommandError("Start or select a DM before creating a contact invite.");
      return;
    }
    void applyCommand(
      createDmInvite({
        dm_id: activeDm.dm_id,
        expires: currentSnapshot.invite.expires,
        max_use: currentSnapshot.invite.max_use,
      }),
      (state) => {
        const invite = state.invites.at(-1);
        if (invite) {
          setDraftInvite(invite.code);
          setVisibleInviteId(invite.invite_id);
        }
        setActiveOverlay("launcher");
      },
    );
  }

  function acceptCommandDmInvite() {
    void applyCommand(
      acceptDmInvite({
        invite_code: draftInvite,
        display_name: draftJoinName || null,
      }),
      () => {
        setDraftInvite("");
        setVisibleInviteId(null);
        setWorkflow("dm");
        setActiveOverlay(null);
      },
    );
  }


  function toggleSelfMute(checked: boolean) {
    const sessionId = appState.voice_session?.session_id;
    if (!sessionId) {
      reportCommandError("Join a voice channel before muting.");
      return;
    }
    localAudioTracks(voiceCaptureRef.current).forEach((track) => {
      track.enabled = !checked;
    });
    voiceMediaSessionRef.current?.setMuted(checked);
    void applyCommand(setSelfMute({ session_id: sessionId, muted: checked }));
  }

  async function toggleVoiceJoin(
    joined: boolean,
    voiceChannelOverride: ChannelStateView | null = null,
    stateOverride: AppState | null = null,
    workflowAfterUpdate: Workflow = "channel",
  ) {
    const runtimeState = stateOverride ?? appState;
    const runtimeGroup =
      (activeGroup
        ? (runtimeState.groups.find(
            (group) => group.group_id === activeGroup.group_id,
          ) ?? activeGroup)
        : getActiveGroup(runtimeState)) ?? null;
    if (joined) {
      if (!runtimeGroup) {
        reportCommandError("Create or join a group before joining voice.");
        return;
      }
      let voiceChannel =
        voiceChannelOverride ?? getActiveVoiceChannel(runtimeState, runtimeGroup);
      if (!voiceChannel) {
        const withVoice = await createChannelCommand({
          group_id: runtimeGroup.group_id,
          name: "Voice Lobby",
          kind: "Voice",
          retention_status: "session",
        });
        setCommandState(withVoice);
        voiceChannel = getActiveVoiceChannel(
          withVoice,
          withVoice.groups.find(
            (group) => group.group_id === runtimeGroup.group_id,
          ) ?? null,
        );
      }
      if (!voiceChannel) {
        reportCommandError("Voice channel creation did not return a channel.");
        return;
      }
      stopLocalVoiceCapture();
      const voiceAccess = await requestVoiceDeviceAccess(
        selectedVoiceInputId,
        selectedVoiceOutputId,
      );
      if (voiceAccess.available_input_devices.length > 0) {
        setVoiceInputDevices(voiceAccess.available_input_devices);
        const selectedStillAvailable =
          selectedVoiceInputId === "default" ||
          voiceAccess.available_input_devices.some(
            (device) => device.device_id === selectedVoiceInputId,
          );
        if (!selectedStillAvailable) {
          setSelectedVoiceInputId("default");
        }
      }
      if (voiceAccess.available_output_devices.length > 0) {
        setVoiceOutputDevices(voiceAccess.available_output_devices);
        const selectedOutputStillAvailable =
          selectedVoiceOutputId === "default" ||
          voiceAccess.available_output_devices.some(
            (device) => device.device_id === selectedVoiceOutputId,
          );
        if (!selectedOutputStillAvailable) {
          setSelectedVoiceOutputId("default");
        }
      }
      const joinedState = await joinVoice({
        group_id: runtimeGroup.group_id,
        channel_id: voiceChannel.channel_id,
        microphone_permission: voiceAccess.microphone_permission,
        input_device_id: voiceAccess.input_device_id,
        input_device_label: voiceAccess.input_device_label,
        output_device_id: voiceAccess.output_device_id,
        output_device_label: voiceAccess.output_device_label,
      });
      setCommandState(joinedState);
      setWorkflow(workflowAfterUpdate);
      voiceCaptureRef.current = voiceAccess.stream;
      if (joinedState.voice_session?.self_muted) {
        localAudioTracks(voiceCaptureRef.current).forEach((track) => {
          track.enabled = false;
        });
      }
      if (joinedState.last_command_error) {
        const action = commandErrorToAction(joinedState.last_command_error);
        reportCommandError(
          action
            ? `${joinedState.last_command_error.message} — ${action}`
            : joinedState.last_command_error.message,
          joinedState.last_command_error.command,
        );
        stopLocalVoiceCapture();
        return;
      }
      const sessionId = joinedState.voice_session?.session_id;
      if (sessionId) {
        if (voiceAccess.stream) {
          stopVoiceActivityCaptureRef.current = startLocalVoiceActivityCapture(
            voiceAccess.stream,
            (sample) => {
              const trackEnabled = localAudioTracks(voiceAccess.stream).some(
                (track) => track.enabled,
              );
              setLocalVoiceSpeaking(
                trackEnabled &&
                  (sample.activity_rms_i16 >= 512 ||
                    sample.activity_peak_i16 >= 2048),
              );
              void applyCommand(
                updateVoiceActivity({
                  session_id: sessionId,
                  rms_i16: sample.activity_rms_i16,
                  peak_i16: sample.activity_peak_i16,
                  captured_at_ms: sample.activity_captured_at_ms,
                }),
              );
            },
          );
          if (
            voiceAccess.activity_rms_i16 !== null &&
            voiceAccess.activity_peak_i16 !== null &&
            voiceAccess.activity_captured_at_ms !== null
          ) {
            void applyCommand(
              updateVoiceActivity({
                session_id: sessionId,
                rms_i16: voiceAccess.activity_rms_i16,
                peak_i16: voiceAccess.activity_peak_i16,
                captured_at_ms: voiceAccess.activity_captured_at_ms,
              }),
            );
          }
        }
        const mediaState = joinedState;
        const voiceSession = mediaState.voice_session?.joined
          ? mediaState.voice_session
          : joinedState.voice_session;
        const voicePeers = textRuntimePeerDefaults(mediaState);
        if (voiceSession) {
          const forceNativeRustVoice = Boolean(
            (
              window as typeof window & {
                __discryptG012ForceNativeRustVoice?: boolean;
              }
            ).__discryptG012ForceNativeRustVoice ||
            window.localStorage?.getItem(
              "discrypt:g012:force-native-rust-voice",
            ) === "1",
          );
          const canUseWebViewRtc = Boolean(
            !forceNativeRustVoice &&
            voiceAccess.stream &&
            typeof RTCPeerConnection !== "undefined" &&
            localAudioTracks(voiceAccess.stream).length > 0,
          );
          if (canUseWebViewRtc && voiceAccess.stream) {
            voiceMediaSessionRef.current = startWebViewVoiceMediaSession({
              session: voiceSession,
              localStream: voiceAccess.stream,
              inputGain: localMicGain,
              localPeerId: voicePeers.local,
              remotePeerId: voicePeers.remote,
              role: textRuntimeRole(mediaState),
              connectivity: voiceConnectivityForState(mediaState),
              onRemoteTrack: (track) => {
                if (isUsableMediaStream(track.stream)) {
                  setVoiceRemoteStreams((current) => ({
                    ...current,
                    [track.participant_id]: track.stream,
                  }));
                }
              },
              onRemoteMedia: (evidence) => {
                if (isUsableMediaStream(evidence.stream)) {
                  setVoiceRemoteStreams((current) => ({
                    ...current,
                    [evidence.participant_id]: evidence.stream,
                  }));
                }
                void applyCommand(
                  attachVoiceRemoteMedia({
                    session_id: voiceSession.session_id,
                    participant_id: evidence.participant_id,
                    participant_name: evidence.participant_name,
                    remote_peer_id: evidence.remote_peer_id,
                    stream_id: evidence.stream_id,
                    audio_track_id: evidence.audio_track_id,
                    playback_element_id: evidence.playback_element_id,
                    local_audio_tracks_sent: evidence.local_audio_tracks_sent,
                    received_audio_frames: evidence.received_audio_frames,
                    speaking: evidence.speaking,
                    attached_at_ms: evidence.attached_at_ms,
                  }),
                );
              },
              onStatus: (status) => reportCommandError(status, "Voice media"),
            });
          } else {
            voiceMediaSessionRef.current = startNativeRustVoiceMediaSession({
              session: voiceSession,
              localPeerId: voicePeers.local,
              remotePeerId: voicePeers.remote,
              role: textRuntimeRole(mediaState),
              connectivity: voiceConnectivityForState(mediaState),
              onState: (state) => setCommandState(state as AppState),
              onStatus: (status) => {
                if (!/proof generated|proof received/i.test(status)) {
                  reportCommandError(status, "Voice media");
                }
              },
            });
          }
        }
      }
      return;
    }
    const sessionId = appState.voice_session?.session_id;
    if (!sessionId) return;
    stopLocalVoiceCapture();
    void applyCommand(leaveVoice({ session_id: sessionId }), () =>
      setWorkflow(workflowAfterUpdate),
    );
  }

  function chooseTheme(nextTheme: ThemeId) {
    void applyCommand(
      savePreferences({
        theme_id: nextTheme,
        template_id: discryptUiConfig.activeTemplate,
      }),
    );
  }

  function resetCommandState() {
    void applyCommand(resetAppState({ confirmation: resetPhrase }), (state) => {
      if (!state.last_command_error) {
        setResetPhrase("");
        setWorkflow("setup");
      }
    });
  }

  function chooseKeyringStorage() {
    void applyCommand(configureStorageSecurity({ mode: "keyring" }), (state) => {
      if (!state.last_command_error && state.storage_security.status === "ready") {
        setStoragePassword("");
        setStoragePasswordConfirm("");
      }
    });
  }

  function setupPasswordStorage() {
    if (storagePassword !== storagePasswordConfirm) {
      reportCommandError(
        "Storage passwords do not match.",
        "configure_storage_security",
      );
      return;
    }
    void applyCommand(
      configureStorageSecurity({
        mode: "passphrase_vault",
        passphrase: storagePassword,
      }),
      (state) => {
        if (!state.last_command_error && state.storage_security.status === "ready") {
          setStoragePassword("");
          setStoragePasswordConfirm("");
        }
      },
    );
  }

  function unlockPasswordStorage() {
    void applyCommand(
      unlockStorageSecurity({ passphrase: storagePassword }),
      (state) => {
        if (!state.last_command_error && state.storage_security.status === "ready") {
          setStoragePassword("");
          setStoragePasswordConfirm("");
        }
      },
    );
  }

  const storageCommandError =
    commandError ??
    (appState.last_command_error
      ? `${appState.last_command_error.message} — ${appState.last_command_error.recovery_hint}`
      : null);
  const storageErrorIsSecurityScoped =
    appState.last_command_error?.command === "app_persistence" ||
    appState.last_command_error?.command === "storage_security";
  const storageModeRequiresPanel =
    appState.storage_security.mode !== "development_store" &&
    appState.storage_security.mode !== "unknown";
  const existingVaultNeedsUnlock =
    storageModeRequiresPanel &&
    (appState.storage_security.status === "locked" ||
      (appState.storage_security.status === "error" &&
        (appState.storage_security.mode === "passphrase_vault" ||
          storageErrorIsSecurityScoped)));
  if (
    existingVaultNeedsUnlock ||
    (storageModeRequiresPanel &&
      appState.lifecycle !== "first_run" &&
      appState.storage_security.status !== "ready")
  ) {
    return (
      <StorageSecurityPanel
        themeStyle={themeStyle}
        storage={appState.storage_security}
        password={storagePassword}
        setPassword={setStoragePassword}
        passwordConfirm={storagePasswordConfirm}
        setPasswordConfirm={setStoragePasswordConfirm}
        commandError={storageCommandError}
        onUseKeyring={chooseKeyringStorage}
        onSetupPassword={setupPasswordStorage}
        onUnlockPassword={unlockPasswordStorage}
      />
    );
  }

  if (appState.lifecycle === "first_run") {
    return (
      <FirstRunPanel
        themeStyle={themeStyle}
        storage={appState.storage_security}
        selectedStorageMode={selectedStorageMode}
        setSelectedStorageMode={(mode) => {
          setSelectedStorageMode(mode);
          setCommandError(null);
        }}
        storagePassword={storagePassword}
        setStoragePassword={setStoragePassword}
        storagePasswordConfirm={storagePasswordConfirm}
        setStoragePasswordConfirm={setStoragePasswordConfirm}
        displayName={draftDisplayName}
        setDisplayName={setDraftDisplayName}
        deviceName={draftDeviceName}
        setDeviceName={setDraftDeviceName}
        recoveryCode={draftRecoveryCode}
        setRecoveryCode={setDraftRecoveryCode}
        commandError={storageCommandError}
        onCreate={createCommandUser}
        onRecover={recoverCommandUser}
      />
    );
  }

  return (
    <main
      data-testid="app-shell"
      style={themeStyle}
      className={cn(
        "grid h-dvh min-h-0 overflow-hidden bg-[hsl(var(--background))] text-[hsl(var(--foreground))]",
        showInspector
          ? "grid-cols-1 md:grid-cols-[72px_minmax(0,1fr)] lg:grid-cols-[var(--shell-grid-inspector)]"
          : "grid-cols-1 md:grid-cols-[72px_minmax(0,1fr)] lg:grid-cols-[var(--shell-grid)]",
      )}
    >
      <ServerRail
        groups={appState.groups}
        dms={appState.dms}
        workflow={workflow}
        activeGroup={activeGroup}
        activeDm={activeDm}
        themeLabel={activeTheme.label}
        onSelectGroup={focusCommandGroup}
        onSelectDm={focusCommandDm}
        onOpenLauncher={openLauncherOverlay}
        onOpenSettings={() => setActiveOverlay("settings")}
        onGroupContextMenu={(groupId, x, y) =>
          setGroupContextMenu({ groupId, x, y })
        }
      />
      {showDesktopSidebar && workflow !== "dm" && workflow !== "setup" && hasActiveGroup ? (
        <ChannelSidebar
          groupLabel={groupLabel}
          role={localGroupRole || activeGroup?.role || "local profile"}
          pendingAdmissionCount={canReviewAdmissions ? pendingAdmissionRequests.length : 0}
          textChannels={textChannels}
          voiceChannels={voiceChannels}
          activeChannelId={activeTextChannel?.channel_id ?? null}
          activeVoiceChannelId={activeVoiceChannel?.channel_id ?? null}
          selectedWorkflow={workflow}
          inlineTextDraft={inlineTextDraft}
          setInlineTextDraft={setInlineTextDraft}
          inlineVoiceDraft={inlineVoiceDraft}
          setInlineVoiceDraft={setInlineVoiceDraft}
          onCommitInlineChannel={commitInlineChannel}
          onSelectTextChannel={(channelId) =>
            focusCommandChannel(channelId, "Text")
          }
          onSelectVoiceChannel={(channelId) =>
            focusCommandChannel(channelId, "Voice")
          }
          onReviewPendingAdmissions={reviewPendingAdmissions}
          voiceJoined={voiceJoined}
          participants={participants}
          localUserId={appState.profile?.user_id ?? null}
          selfMuted={selfMuted}
          connectivity={voiceConnectivityForState(appState)}
          voiceSession={appState.voice_session}
          remoteAudio={appState.voice_session?.media_runtime.remote_audio ?? []}
          remoteStreams={voiceRemoteStreams}
          appOutputVolume={appOutputVolume}
          selectedOutputDeviceId={selectedVoiceOutputId}
          localMicGain={localMicGain}
          onAppOutputVolumeChange={setAppOutputVolume}
          onLocalMicGainChange={setLocalMicGain}
          onToggleSelfMute={toggleSelfMute}
          onLeaveVoice={() => void toggleVoiceJoin(false)}
        />
      ) : null}
      <section
        aria-label="Main chat pane"
        data-testid="main-chat-pane"
        className="flex h-full min-h-0 min-w-0 flex-col bg-[radial-gradient(circle_at_80%_0%,hsl(var(--primary)/0.10),transparent_34rem)]"
      >
        <TopBar
          groupLabel={topBarScopeLabel}
          activeTitle={activePane.title}
          activeSubtitle={activePane.subtitle}
          membersPanelOpen={membersPanelOpen}
          onToggleMembers={() => setMembersPanelOpen((open) => !open)}
          membersPanelAvailable={
            workflow !== "setup" && workflow !== "dm" && hasActiveGroup
          }
          onOpenDiagnostics={() => {
            setInspectorOpen(true);
            setActiveOverlay("diagnostics");
          }}
          inspectorOpen={diagnosticsUiEnabled && inspectorOpen}
          diagnosticsEnabled={diagnosticsUiEnabled}
        />
        <div
          data-testid="main-chat-content"
          className="min-h-0 flex-1 overflow-hidden px-4 pb-24 pt-4 md:px-6 lg:pb-4"
        >
          <div className="flex h-full min-h-0 w-full flex-col">
            {workflow === "setup" ? (
              <SetupPanel
                onCreateGroup={() => setActiveOverlay("create-group")}
                onJoinInvite={openLauncherOverlay}
              />
            ) : workflow === "dm" ? (
              <DmPanel
                activeDm={activeDm}
                messages={appState.messages}
                textStateLegend={appState.text_state_legend}
                draftDmName={draftDmName}
                setDraftDmName={setDraftDmName}
                draftMessage={draftMessage}
                setDraftMessage={setDraftMessage}
                onStartDm={startCommandDm}
                onSendDm={sendCommandDm}
                transportProof={messageTransportProof}
                setTransportProof={setMessageTransportProof}
                diagnosticsEnabled={diagnosticsUiEnabled}
              />
            ) : workflow === "voice" && !showDesktopSidebar ? (
              <MobileVoicePanel
                group={activeGroup}
                voiceChannels={voiceChannels}
                activeVoiceChannelId={activeVoiceChannel?.channel_id ?? null}
                voiceJoined={voiceJoined}
                participants={participants}
                localUserId={appState.profile?.user_id ?? null}
                selfMuted={selfMuted}
                connectivity={voiceConnectivityForState(appState)}
                voiceSession={appState.voice_session}
                remoteAudio={
                  appState.voice_session?.media_runtime.remote_audio ?? []
                }
                remoteStreams={voiceRemoteStreams}
                appOutputVolume={appOutputVolume}
                selectedOutputDeviceId={selectedVoiceOutputId}
                localMicGain={localMicGain}
                onSelectVoiceChannel={(channelId) =>
                  focusCommandChannel(channelId, "Voice")
                }
                onOpenCreateChannel={() => setInlineVoiceDraft("")}
                onAppOutputVolumeChange={setAppOutputVolume}
                onLocalMicGainChange={setLocalMicGain}
                onToggleSelfMute={toggleSelfMute}
                onLeaveVoice={() =>
                  void toggleVoiceJoin(false, null, null, "voice")
                }
              />
            ) : workflow === "admission_requests" ? (
              <AdmissionRequestsPanel
                group={activeGroup}
                localRole={localGroupRole}
                requests={activeGroup?.admission_requests ?? []}
                onApprove={approveAdmission}
                onRefuse={refuseAdmission}
                actionInFlight={memberActionInFlight}
              />
            ) : (
              <ChannelPanel
                group={activeGroup}
                activeChannel={activeTextChannel}
                messages={appState.messages}
                textStateLegend={appState.text_state_legend}
                draftMessage={draftMessage}
                setDraftMessage={setDraftMessage}
                onOpenCreateChannel={() => setInlineTextDraft("")}
                onSendMessage={sendCommandMessage}
                transportProof={messageTransportProof}
                setTransportProof={setMessageTransportProof}
                diagnosticsEnabled={diagnosticsUiEnabled}
                admissionPending={localAdmissionPending}
              />
            )}
          </div>
        </div>
      </section>
      {workflow !== "dm" && workflow !== "setup" && membersPanelOpen && hasActiveGroup ? (
        <MemberPanel
          group={activeGroup}
          members={groupMembers}
          localRole={localGroupRole}
          open={membersPanelOpen}
          pendingCount={pendingAdmissionRequests.length}
          onReviewPendingAdmissions={reviewPendingAdmissions}
          onPromote={promoteMember}
          onDemote={demoteMember}
          onRevoke={revokeMember}
          actionInFlight={memberActionInFlight}
        />
      ) : null}
      <MobileWorkflowNav workflow={workflow} setWorkflow={setWorkflow} />
      <Button
        type="button"
        variant="outline"
        aria-label="Add group or direct message"
        title="Add group or direct message"
        onClick={openLauncherOverlay}
        className="fixed bottom-24 left-4 z-40 grid h-12 w-12 place-items-center rounded-2xl border-emerald-300/35 bg-emerald-400/14 text-xl font-semibold text-emerald-100 shadow-2xl shadow-black/35 md:hidden"
      >
        <Icon>+</Icon>
      </Button>
      {showInspector ? (
        <InspectorRail
          snapshot={currentSnapshot}
          appState={appState}
          participants={participants}
          themeLabel={activeTheme.label}
          resetPhrase={resetPhrase}
          setResetPhrase={setResetPhrase}
          onResetState={resetCommandState}
          runtimePeers={textRuntimePeerDefaults(appState)}
          runtimeRole={textRuntimeRole(appState)}
          onProbeAdapter={probeSelectedAdapter}
          onProbeDataChannel={probeSelectedDataChannel}
          onStartTextTransport={startTextTransportProof}
          onAttachRuntime={attachTextRuntime}
        />
      ) : null}
      <GroupContextMenu
        menu={groupContextMenu}
        groups={appState.groups}
        onClose={() => setGroupContextMenu(null)}
        onCreateInvite={(groupId) => {
          setGroupContextMenu(null);
          openGroupInviteOverlay(groupId);
        }}
        onOpenConfig={(groupId) => {
          setGroupContextMenu(null);
          openGroupConfigOverlay(groupId);
        }}
      />
      <WorkspaceOverlay
        overlay={activeOverlay}
        closing={overlayClosing}
        onClose={closeOverlay}
      >
        {activeOverlay === "create-group" ? (
          <CreateGroupPanel
            snapshot={currentSnapshot}
            groupName={draftGroup}
            setGroupName={setDraftGroup}
            signalingAdapter={draftSignalingAdapter}
            setSignalingAdapter={chooseSignalingAdapter}
            signalingEndpoint={draftSignalingEndpoint}
            setSignalingEndpoint={setDraftSignalingEndpoint}
            iceStunServers={draftIceStunServers}
            setIceStunServers={setDraftIceStunServers}
            iceTurnServers={draftIceTurnServers}
            setIceTurnServers={setDraftIceTurnServers}
            admissionMode={draftAdmissionMode}
            setAdmissionMode={setDraftAdmissionMode}
            onCreate={createCommandGroup}
          />
        ) : null}
        {activeOverlay === "launcher" ? (
          <LauncherPanel
            inviteValue={draftInvite}
            setInviteValue={setDraftInvite}
            groupName={draftJoinName}
            setGroupName={setDraftJoinName}
            contactName={draftDmName}
            setContactName={setDraftDmName}
            latestInvite={
              visibleInviteId
                ? (appState.invites.find(
                    (invite) => invite.invite_id === visibleInviteId,
                  ) ?? null)
                : null
            }
            joinProgress={appState.join_progress}
            onJoin={joinCommandGroup}
            onAcceptDmInvite={acceptCommandDmInvite}
            onStartDm={startCommandDm}
            onCreateDmInvite={createCommandDmInvite}
            canCreateDmInvite={Boolean(activeDm)}
            onCreateGroup={() => {
              setOverlayClosing(false);
              setActiveOverlay("create-group");
            }}
          />
        ) : null}
        {activeOverlay === "group-invite" ? (
          <GroupInvitePanel
            group={overlayGroup}
            latestInvite={
              visibleInviteId
                ? (appState.invites.find(
                    (invite) => invite.invite_id === visibleInviteId,
                  ) ?? null)
                : null
            }
            expiryDays={inviteExpiryDays}
            setExpiryDays={setInviteExpiryDays}
            maxUses={inviteMaxUses}
            setMaxUses={setInviteMaxUses}
            revocationState={inviteRevocationState}
            setRevocationState={setInviteRevocationState}
            passwordEnabled={invitePasswordEnabled}
            setPasswordEnabled={setInvitePasswordEnabled}
            password={invitePassword}
            setPassword={setInvitePassword}
            onCreateInvite={() => {
              if (overlayGroup) createCommandInviteForGroup(overlayGroup.group_id);
            }}
          />
        ) : null}
        {activeOverlay === "group-config" ? (
          <GroupConfigPanel
            group={overlayGroup}
            signalingAdapter={draftSignalingAdapter}
            setSignalingAdapter={chooseSignalingAdapter}
            signalingEndpoint={draftSignalingEndpoint}
            setSignalingEndpoint={setDraftSignalingEndpoint}
            iceStunServers={draftIceStunServers}
            setIceStunServers={setDraftIceStunServers}
            iceTurnServers={draftIceTurnServers}
            setIceTurnServers={setDraftIceTurnServers}
            admissionMode={draftConfigAdmissionMode}
            setAdmissionMode={setDraftConfigAdmissionMode}
            onSave={() => {
              if (overlayGroup) void saveGroupConfiguration(overlayGroup.group_id);
            }}
          />
        ) : null}
        {activeOverlay === "settings" ? (
          <div className="grid gap-4">
            <AppearanceSettings
              themeId={asThemeId(activeTheme.id)}
              onThemeChange={chooseTheme}
            />
            <AudioSettingsPanel
              inputDevices={voiceInputDevices}
              outputDevices={voiceOutputDevices}
              selectedInputDeviceId={selectedVoiceInputId}
              selectedOutputDeviceId={selectedVoiceOutputId}
              voiceDeviceStatus={voiceDeviceStatus}
              localMicGain={localMicGain}
              appOutputVolume={appOutputVolume}
              onSelectInputDevice={setSelectedVoiceInputId}
              onSelectOutputDevice={setSelectedVoiceOutputId}
              onRefreshDevices={() => void refreshVoiceInputDevices(true)}
              onLocalMicGainChange={setLocalMicGain}
              onAppOutputVolumeChange={setAppOutputVolume}
            />
            <ConnectivitySettingsPanel
              policy={activeConnectivity}
              signalingAdapter={draftSignalingAdapter}
              setSignalingAdapter={chooseSignalingAdapter}
              signalingEndpoint={draftSignalingEndpoint}
              setSignalingEndpoint={setDraftSignalingEndpoint}
              iceStunServers={draftIceStunServers}
              setIceStunServers={setDraftIceStunServers}
              iceTurnServers={draftIceTurnServers}
              setIceTurnServers={setDraftIceTurnServers}
              onSaveAppDefaults={() => saveConnectivityPolicy("app")}
              onSaveGroup={
                activeGroup ? () => saveConnectivityPolicy("group") : null
              }
              onSaveChannel={
                activeTextChannel || activeVoiceChannel
                  ? () => saveConnectivityPolicy("channel")
                  : null
              }
              onSaveDm={activeDm ? () => saveConnectivityPolicy("dm") : null}
            />
          </div>
        ) : null}
        {activeOverlay === "diagnostics" ? (
          <DiagnosticsSheet
            snapshot={currentSnapshot}
            appState={appState}
            participants={participants}
            themeLabel={activeTheme.label}
            verifyMessage={verifyMessage}
            onVerifySafetyNumber={confirmSafetyNumber}
          />
        ) : null}
      </WorkspaceOverlay>
      <CommandNotificationStack
        notifications={commandNotifications}
        onDismiss={dismissCommandNotification}
      />
    </main>
  );
}

function getActiveGroup(state: AppState): GroupView | null {
  const activeId = state.active_context?.group_id;
  if (activeId)
    return (
      state.groups.find((group) => group.group_id === activeId) ??
      state.groups[0] ??
      null
    );
  return state.groups[0] ?? null;
}

function activePaneSummary(
  workflow: Workflow,
  groupLabel: string,
  activeTextChannel: ChannelStateView | null,
  activeVoiceChannel: ChannelStateView | null,
  activeDm: DirectConversationView | null,
): { title: string; subtitle: string } {
  switch (workflow) {
    case "dm":
      return {
        title: activeDm?.display_name ?? "Direct messages",
        subtitle:
          activeDm?.local_only_copy ??
          "Start or select a private conversation.",
      };
    case "channel":
      return {
        title: activeTextChannel
          ? activeTextChannel.name.replace(/^#/, "")
          : "Text channels",
        subtitle: activeTextChannel
          ? `${groupLabel} · ${activeTextChannel.retention_status}`
          : "Create or select a text channel.",
      };
    case "admission_requests":
      return {
        title: "Pending admission requests",
        subtitle: `${groupLabel} · owner/staff review queue`,
      };
    case "voice":
      return {
        title: activeVoiceChannel?.name ?? "Voice rooms",
        subtitle: activeVoiceChannel
          ? `${groupLabel} · voice room`
          : "Create or select a voice room.",
      };
    case "join":
      return {
        title: "Invites",
        subtitle: "Create or paste signed invite descriptors.",
      };
    case "create-group":
      return {
        title: "Create group",
        subtitle: "Persist a group, default text channel, and voice room.",
      };
    case "setup":
    default:
      return {
        title: "Getting started",
        subtitle: "Create a group or open an invite to begin.",
      };
  }
}

function isPresenceOnline(member: GroupMemberView): boolean {
  if (member.status === "revoked") return false;
  if (member.status === "offline" || member.status === "unknown") return false;
  if (!member.presence_expires_at) return member.status === "online";
  return Date.parse(member.presence_expires_at) > Date.now();
}

function normalizedGroupMembers(
  group: GroupView | null,
  state: AppState,
): GroupMemberView[] {
  if (!group) return [];
  const members = group.members ?? [];
  if (members.length > 0) return members;
  const now = new Date().toISOString();
  return [
    {
      member_id: state.profile?.user_id ?? "local-profile-pending",
      display_name: state.profile?.display_name ?? "You",
      device_id: state.profile?.device_name ?? null,
      role: (group.role as GroupRoleView) || "member",
      status: "online",
      signer_public_key_hex: null,
      joined_at: now,
      last_seen_at: now,
      presence_expires_at: new Date(Date.now() + 300_000).toISOString(),
      revoked_at: null,
      revoked_by: null,
    },
  ];
}

function localGroupRoleForUi(
  group: GroupView | null,
  state: AppState,
): string {
  if (!group) return "local profile";
  const localMember = (group.members ?? []).find(
    (member) => member.member_id === state.profile?.user_id,
  );
  return localMember?.role ?? group.role ?? "member";
}

function roleRank(role: string): number {
  if (role === "owner") return 0;
  if (role === "staff") return 1;
  return 2;
}

function canPromoteFromUi(localRole: string, member: GroupMemberView): boolean {
  return localRole === "owner" && member.role === "member" && member.status !== "revoked";
}

function canDemoteFromUi(localRole: string, member: GroupMemberView): boolean {
  return localRole === "owner" && member.role === "staff" && member.status !== "revoked";
}

function canRevokeFromUi(localRole: string, member: GroupMemberView): boolean {
  if (member.status === "revoked" || member.role === "owner") return false;
  if (localRole === "owner") return true;
  return localRole === "staff" && member.role === "member";
}

function getActiveTextChannel(
  state: AppState,
  group: GroupView | null,
  preferredChannelId: string | null = null,
): ChannelStateView | null {
  if (!group) return null;
  const activeId =
    state.active_context?.kind === "text_channel"
      ? state.active_context.channel_id
      : null;
  return (
    (preferredChannelId
      ? group.channels.find(
          (channel) =>
            channel.channel_id === preferredChannelId &&
            channel.kind === "Text",
        )
      : null) ??
    (activeId
      ? group.channels.find(
          (channel) =>
            channel.channel_id === activeId && channel.kind === "Text",
        )
      : null) ??
    group.channels.find((channel) => channel.kind === "Text") ??
    null
  );
}

function getActiveVoiceChannel(
  state: AppState,
  group: GroupView | null,
): ChannelStateView | null {
  if (!group) return null;
  const activeId =
    state.active_context?.kind === "voice_channel"
      ? state.active_context.channel_id
      : null;
  return (
    (activeId
      ? group.channels.find(
          (channel) =>
            channel.channel_id === activeId && channel.kind === "Voice",
        )
      : null) ??
    group.channels.find((channel) => channel.kind === "Voice") ??
    null
  );
}

function getActiveDm(state: AppState): DirectConversationView | null {
  const activeDmId = state.active_context?.dm_id ?? state.dms[0]?.dm_id ?? null;
  return activeDmId
    ? (state.dms.find((dm) => dm.dm_id === activeDmId) ?? state.dms[0] ?? null)
    : (state.dms[0] ?? null);
}

function StorageSecurityPanel({
  themeStyle,
  storage,
  password,
  setPassword,
  passwordConfirm,
  setPasswordConfirm,
  commandError,
  onUseKeyring,
  onSetupPassword,
  onUnlockPassword,
}: {
  themeStyle: React.CSSProperties;
  storage: AppState["storage_security"];
  password: string;
  setPassword: (value: string) => void;
  passwordConfirm: string;
  setPasswordConfirm: (value: string) => void;
  commandError: string | null;
  onUseKeyring: () => void;
  onSetupPassword: () => void;
  onUnlockPassword: () => void;
}) {
  const locked = storage.status === "locked" || storage.password_required;
  const error = storage.status === "error";
  return (
    <main
      style={themeStyle}
      className="min-h-dvh bg-[radial-gradient(circle_at_20%_10%,hsl(var(--primary)/0.12),transparent_24rem),hsl(var(--background))] p-4 text-[hsl(var(--foreground))] md:p-8"
    >
      <div className="mx-auto grid min-h-[calc(100dvh-2rem)] w-full max-w-5xl place-items-center md:min-h-[calc(100dvh-4rem)]">
        <Card className="w-full overflow-hidden border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.94)] shadow-2xl shadow-black/30">
          <CardHeader className="border-b border-[hsl(var(--border))] bg-[linear-gradient(135deg,hsl(var(--secondary)/0.48),transparent)] p-6 lg:p-8">
            <Badge variant={error ? "warning" : "secondary"} className="w-fit">
              local storage security
            </Badge>
            <CardTitle className="max-w-3xl text-3xl leading-tight md:text-4xl">
              {storage.title}
            </CardTitle>
            <CardDescription className="max-w-3xl text-base leading-7">
              {storage.detail}
            </CardDescription>
          </CardHeader>
          <CardContent className="grid gap-5 p-6 lg:grid-cols-[0.95fr_1.05fr] lg:p-8">
            {commandError ? (
              <p className="rounded-xl border border-red-300/30 bg-red-300/10 p-3 text-sm text-red-100 lg:col-span-2">
                Action failed: {commandError}
              </p>
            ) : null}
            <div className="grid gap-3 rounded-2xl border border-[hsl(var(--border))] bg-black/10 p-4 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
              <h2 className="text-base font-semibold text-[hsl(var(--foreground))]">
                Why this happens before account setup
              </h2>
              <p>
                Discrypt encrypts local state before it stores your identity,
                groups, message envelopes, and voice preferences. If the app
                cannot unlock that storage, it must stop instead of creating a
                replacement vault or keyring entry over existing data.
              </p>
              <p>
                OS keyrings are smoother, but trust the logged-in operating
                system session. A Discrypt password vault is worse UX because
                you must type the password on every startup, but it provides a
                separate app-level secret.
              </p>
              <p className="rounded-xl border border-amber-300/25 bg-amber-300/10 p-3 text-amber-100">
                No storage restore flow exists yet for a lost password, broken
                keyring, or moved vault. Discrypt preserves existing unreadable
                state and leaves recovery/migration on the roadmap.
              </p>
            </div>
            {locked || storage.mode === "passphrase_vault" ? (
              <div className="flex flex-col gap-4 rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.36)] p-4">
                <div>
                  <h2 className="text-lg font-semibold">
                    Unlock password vault
                  </h2>
                  <p className="text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                    Enter the password used for this local Discrypt vault.
                  </p>
                </div>
                <Label className="grid gap-2">
                  Storage password
                  <Input
                    type="password"
                    value={password}
                    autoComplete="current-password"
                    onChange={(event) => setPassword(event.target.value)}
                  />
                </Label>
                <Button
                  className="mt-auto"
                  disabled={password.length < 12}
                  onClick={onUnlockPassword}
                >
                  Unlock storage
                </Button>
              </div>
            ) : (
              <div className="grid gap-4">
                <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.36)] p-4">
                  <h2 className="text-lg font-semibold">Use OS keyring</h2>
                  <p className="mt-2 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                    Best UX. The desktop keyring protects Discrypt's wrapping
                    key and may unlock with your login session.
                  </p>
                  <Button className="mt-4 w-full" onClick={onUseKeyring}>
                    Use keyring if available
                  </Button>
                </div>
                <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.36)] p-4">
                  <h2 className="text-lg font-semibold">
                    Use Discrypt password vault
                  </h2>
                  <p className="mt-2 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                    Stronger separation from the OS keyring. Discrypt will ask
                    for this password on every startup.
                  </p>
                  <div className="mt-4 grid gap-3">
                    <Label className="grid gap-2">
                      Storage password
                      <Input
                        type="password"
                        value={password}
                        autoComplete="new-password"
                        onChange={(event) => setPassword(event.target.value)}
                      />
                    </Label>
                    <Label className="grid gap-2">
                      Confirm password
                      <Input
                        type="password"
                        value={passwordConfirm}
                        autoComplete="new-password"
                        onChange={(event) =>
                          setPasswordConfirm(event.target.value)
                        }
                      />
                    </Label>
                    <Button
                      variant="outline"
                      disabled={
                        password.length < 12 || password !== passwordConfirm
                      }
                      onClick={onSetupPassword}
                    >
                      Set password vault
                    </Button>
                  </div>
                </div>
              </div>
            )}
            <p className="text-sm text-[hsl(var(--muted-foreground))] lg:col-span-2">
              {storage.recovery_hint}
            </p>
          </CardContent>
        </Card>
      </div>
    </main>
  );
}

function PasswordInput({
  value,
  onChange,
  placeholder,
  autoComplete,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  autoComplete: string;
}) {
  const [visible, setVisible] = useState(false);
  return (
    <div className="relative">
      <Input
        className="pr-12"
        type={visible ? "text" : "password"}
        value={value}
        placeholder={placeholder}
        autoComplete={autoComplete}
        onChange={(event) => onChange(event.target.value)}
      />
      <Button
        type="button"
        variant="ghost"
        size="icon"
        aria-label={visible ? "Hide password" : "Show password"}
        className="absolute right-2 top-1/2 h-8 w-8 -translate-y-1/2 rounded-md"
        onClick={() => setVisible((current) => !current)}
      >
        {visible ? (
          <svg viewBox="0 0 24 24" aria-hidden="true" className="h-4 w-4">
            <path
              fill="currentColor"
              d="M2.7 3.3 1.3 4.7l4 4C4 9.7 2.9 10.9 2 12c2.5 3.3 5.8 5 10 5 1.6 0 3.1-.3 4.4-.9l2.9 2.9 1.4-1.4-18-18ZM12 15c-1.7 0-3-1.3-3-3 0-.4.1-.8.2-1.1l3.9 3.9c-.3.1-.7.2-1.1.2Zm0-8c1.7 0 3 1.3 3 3 0 .4-.1.8-.2 1.1l3.5 3.5c1.4-.8 2.6-1.9 3.7-3.3C19.5 8 16.2 6.3 12 6.3c-.9 0-1.8.1-2.6.3l2 2c.2 0 .4-.1.6-.1Z"
            />
          </svg>
        ) : (
          <svg viewBox="0 0 24 24" aria-hidden="true" className="h-4 w-4">
            <path
              fill="currentColor"
              d="M12 5c4.2 0 7.5 2.3 10 7-2.5 4.7-5.8 7-10 7S4.5 16.7 2 12c2.5-4.7 5.8-7 10-7Zm0 2C9 7 6.5 8.4 4.4 12 6.5 15.6 9 17 12 17s5.5-1.4 7.6-5C17.5 8.4 15 7 12 7Zm0 2.5A2.5 2.5 0 1 1 12 14.5 2.5 2.5 0 0 1 12 9.5Z"
            />
          </svg>
        )}
      </Button>
    </div>
  );
}

function FirstRunPanel({
  themeStyle,
  storage,
  selectedStorageMode,
  setSelectedStorageMode,
  storagePassword,
  setStoragePassword,
  storagePasswordConfirm,
  setStoragePasswordConfirm,
  displayName,
  setDisplayName,
  deviceName,
  setDeviceName,
  recoveryCode,
  setRecoveryCode,
  commandError,
  onCreate,
  onRecover,
}: {
  themeStyle: React.CSSProperties;
  storage: AppState["storage_security"];
  selectedStorageMode: StorageSetupChoice | null;
  setSelectedStorageMode: (value: StorageSetupChoice) => void;
  storagePassword: string;
  setStoragePassword: (value: string) => void;
  storagePasswordConfirm: string;
  setStoragePasswordConfirm: (value: string) => void;
  displayName: string;
  setDisplayName: (value: string) => void;
  deviceName: string;
  setDeviceName: (value: string) => void;
  recoveryCode: string;
  setRecoveryCode: (value: string) => void;
  commandError: string | null;
  onCreate: () => void | Promise<void>;
  onRecover: () => void | Promise<void>;
}) {
  const storageSetupRequired = storage.status !== "ready";
  const vaultSelected = selectedStorageMode === "passphrase_vault";
  const passwordLongEnough = storagePassword.length >= 12;
  const passwordsMatch = storagePassword === storagePasswordConfirm;
  const storageReadyForSubmit =
    !storageSetupRequired ||
    selectedStorageMode === "keyring" ||
    (vaultSelected && passwordLongEnough && passwordsMatch);
  const passwordMessage = !vaultSelected
    ? null
    : !passwordLongEnough
      ? `Use at least 12 characters (${storagePassword.length}/12).`
      : !passwordsMatch
        ? "Passwords do not match yet."
        : "Password vault will be created when you create or recover the account.";
  return (
    <main
      style={themeStyle}
      className="min-h-dvh bg-[radial-gradient(circle_at_20%_10%,hsl(var(--primary)/0.12),transparent_24rem),hsl(var(--background))] p-4 text-[hsl(var(--foreground))] md:p-8"
    >
      <div className="mx-auto grid min-h-[calc(100dvh-2rem)] w-full max-w-5xl place-items-center md:min-h-[calc(100dvh-4rem)]">
        <Card className="w-full overflow-hidden border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.9)] shadow-2xl shadow-black/30">
          <CardHeader className="border-b border-[hsl(var(--border))] bg-[linear-gradient(135deg,hsl(var(--secondary)/0.48),transparent)] p-6 lg:p-8">
            <Badge variant="secondary" className="w-fit">
              first run
            </Badge>
            <CardTitle className="max-w-3xl text-3xl leading-tight md:text-4xl">
              Set up your local discrypt profile
            </CardTitle>
            <CardDescription className="max-w-3xl text-base leading-7">
              Choose how this device protects local data, then create or recover
              your profile.
            </CardDescription>
          </CardHeader>
          <CardContent className="grid gap-5 p-6 lg:p-8">
            {commandError ? (
              <p className="rounded-xl border border-red-300/30 bg-red-300/10 p-3 text-sm text-red-100">
                Action failed: {commandError}
              </p>
            ) : null}
            {storageSetupRequired ? (
              <section
                data-testid="first-run-storage"
                className="grid gap-4 rounded-2xl border border-[hsl(var(--border))] bg-black/10 p-4"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <h2 className="text-lg font-semibold">
                      Local storage protection
                    </h2>
                    <p className="mt-1 max-w-3xl text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                      Discrypt will not create your account until local storage
                      can be protected. Existing unreadable state is preserved;
                      choosing a mode here stays reversible until you submit the
                      account form.
                    </p>
                  </div>
                  <Badge
                    variant={storage.keyring_available ? "secondary" : "warning"}
                    className="w-fit"
                  >
                    {storage.keyring_available
                      ? "keyring preflight passed"
                      : "keyring needs attention"}
                  </Badge>
                </div>
                <fieldset className="grid gap-3">
                  <legend className="sr-only">Storage mode</legend>
                  <div
                    data-testid="first-run-storage-mode-options"
                    className="grid gap-3 lg:grid-cols-2"
                  >
                  <Button
                    type="button"
                    variant="outline"
                    aria-pressed={selectedStorageMode === "keyring"}
                    className={cn(
                      "h-auto min-h-40 w-full flex-col items-stretch justify-start whitespace-normal rounded-2xl border p-4 text-left transition hover:border-[hsl(var(--primary))] hover:bg-[hsl(var(--secondary)/0.42)]",
                      selectedStorageMode === "keyring"
                        ? "border-[hsl(var(--primary))] bg-[hsl(var(--primary)/0.12)]"
                        : "border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.28)]",
                    )}
                    onClick={() => setSelectedStorageMode("keyring")}
                  >
                    <div className="flex items-center justify-between gap-3">
                      <h3 className="font-semibold">Use OS keyring</h3>
                      <Badge variant="secondary">best UX</Badge>
                    </div>
                    <p className="mt-2 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                      The desktop keyring protects Discrypt's storage key and may
                      unlock with your login session. It trusts your OS keyring
                      boundary.
                    </p>
                    <p className="mt-3 rounded-xl border border-[hsl(var(--border))] bg-black/10 p-2 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
                      {storage.keyring_detail}
                    </p>
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    className={cn(
                      "h-auto min-h-36 w-full flex-col items-stretch justify-start whitespace-normal rounded-2xl border p-4 text-left transition hover:border-[hsl(var(--primary))] hover:bg-[hsl(var(--secondary)/0.42)]",
                      selectedStorageMode === "passphrase_vault"
                        ? "border-[hsl(var(--primary))] bg-[hsl(var(--primary)/0.12)]"
                        : "border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.28)]",
                    )}
                    onClick={() => setSelectedStorageMode("passphrase_vault")}
                  >
                    <div className="flex items-center justify-between gap-3">
                      <h3 className="font-semibold">
                        Use Discrypt password vault
                      </h3>
                      <Badge variant="secondary">stronger separation</Badge>
                    </div>
                    <p className="mt-2 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                      A password-derived vault protects local storage without
                      relying on the OS keyring. You must enter it every time the
                      app starts.
                    </p>
                    <p className="mt-3 rounded-xl border border-amber-300/25 bg-amber-300/10 p-2 text-xs leading-5 text-amber-100">
                      No storage restore exists yet for a lost password;
                      existing unreadable state is preserved.
                    </p>
                  </Button>
                  </div>
                </fieldset>
                {vaultSelected ? (
                  <div className="grid gap-3 rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.28)] p-4 md:grid-cols-2">
                    <div className="md:col-span-2">
                      <h3 className="text-base font-semibold">
                        Password vault credentials
                      </h3>
                      <p className="mt-1 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                        These credentials protect local storage for this device.
                        They are checked when you submit either account form.
                      </p>
                    </div>
                    <Label className="grid gap-2">
                      Storage password
                      <PasswordInput
                        value={storagePassword}
                        autoComplete="new-password"
                        placeholder="At least 12 characters"
                        onChange={setStoragePassword}
                      />
                    </Label>
                    <Label className="grid gap-2">
                      Confirm storage password
                      <PasswordInput
                        value={storagePasswordConfirm}
                        autoComplete="new-password"
                        placeholder="Repeat storage password"
                        onChange={setStoragePasswordConfirm}
                      />
                    </Label>
                    <p
                      className={cn(
                        "text-sm md:col-span-2",
                        passwordLongEnough && passwordsMatch
                          ? "text-emerald-200"
                          : "text-amber-100",
                      )}
                    >
                      {passwordMessage}
                    </p>
                  </div>
                ) : null}
              </section>
            ) : null}
            <section
              data-testid="first-run-account-forms"
              className="grid gap-4 lg:grid-cols-2"
            >
              <div className="flex min-h-72 flex-col rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4">
                <div className="mb-4">
                  <h2 className="text-lg font-semibold">New local user</h2>
                  <p className="text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                    Best for first machine setup.
                  </p>
                </div>
                <Label className="grid gap-2">
                  Display name
                  <Input
                    value={displayName}
                    placeholder="Display name"
                    onChange={(event) => setDisplayName(event.target.value)}
                  />
                </Label>
                <Label className="mt-4 grid gap-2">
                  Device name
                  <Input
                    value={deviceName}
                    placeholder="This device"
                    onChange={(event) => setDeviceName(event.target.value)}
                  />
                </Label>
                <Button
                  className="mt-auto w-full"
                  onClick={onCreate}
                  disabled={
                    !displayName.trim() ||
                    !deviceName.trim() ||
                    !storageReadyForSubmit
                  }
                >
                  Create new user
                </Button>
              </div>
              <div className="flex min-h-72 flex-col rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4">
                <div className="mb-4">
                  <h2 className="text-lg font-semibold">Existing user</h2>
                  <p className="text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                    Recover profile continuity on this device.
                  </p>
                </div>
                <Label className="grid gap-2">
                  Display name
                  <Input
                    value={displayName}
                    placeholder="Display name"
                    onChange={(event) => setDisplayName(event.target.value)}
                  />
                </Label>
                <Label className="mt-4 grid gap-2">
                  Device name
                  <Input
                    value={deviceName}
                    placeholder="This device"
                    onChange={(event) => setDeviceName(event.target.value)}
                  />
                </Label>
                <Label className="mt-4 grid gap-2">
                  Recovery phrase/code
                  <Input
                    value={recoveryCode}
                    placeholder="Recovery phrase or code"
                    onChange={(event) => setRecoveryCode(event.target.value)}
                  />
                </Label>
                <p className="mt-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                  Use the recovery material generated for your existing profile.
                </p>
                <Button
                  variant="outline"
                  className="mt-auto w-full"
                  onClick={onRecover}
                  disabled={
                    !displayName.trim() ||
                    !deviceName.trim() ||
                    !recoveryCode.trim() ||
                    !storageReadyForSubmit
                  }
                >
                  Recover existing user
                </Button>
              </div>
            </section>
          </CardContent>
        </Card>
      </div>
    </main>
  );
}

function ServerRail({
  groups,
  dms,
  workflow,
  activeGroup,
  activeDm,
  themeLabel,
  onSelectGroup,
  onSelectDm,
  onOpenLauncher,
  onOpenSettings,
  onGroupContextMenu,
}: {
  groups: GroupView[];
  dms: DirectConversationView[];
  workflow: Workflow;
  activeGroup: GroupView | null;
  activeDm: DirectConversationView | null;
  themeLabel: string;
  onSelectGroup: (groupId: string) => void;
  onSelectDm: (dmId: string) => void;
  onOpenLauncher: () => void;
  onOpenSettings: () => void;
  onGroupContextMenu: (groupId: string, x: number, y: number) => void;
}) {
  return (
    <aside
      aria-label="Server rail"
      className="hidden h-dvh border-r border-[hsl(var(--border))] bg-black/25 p-3 md:flex md:flex-col md:items-center md:gap-3"
    >
      <div
        className="grid h-11 w-11 place-items-center rounded-2xl bg-[hsl(var(--primary))] font-black text-[hsl(var(--primary-foreground))] shadow-sm shadow-black/30"
        title="discrypt home"
      >
        d
      </div>
      <div className="h-px w-9 rounded-full bg-[hsl(var(--border))]" />
      <div className="flex min-h-0 flex-1 flex-col items-center gap-3 overflow-y-auto pb-2">
        {(groups.length
          ? groups
          : [
              {
                group_id: "local",
                name: "Local",
                role: "local profile",
                channels: [],
              },
            ]
        ).map((group) => {
          const active =
            workflow !== "dm" && group.group_id === activeGroup?.group_id;
          return (
            <Button
              key={group.group_id}
              variant="outline"
              size="icon"
              title={`${group.name} · right-click for group actions`}
              aria-label={`Open ${group.name} group`}
              aria-current={active ? "page" : undefined}
              onClick={() => onSelectGroup(group.group_id)}
              onContextMenu={(event) => {
                event.preventDefault();
                if (group.group_id !== "local") {
                  onGroupContextMenu(
                    group.group_id,
                    event.clientX,
                    event.clientY,
                  );
                }
              }}
              onKeyDown={(event) => {
                if (
                  !isKeyboardContextMenu(event) ||
                  group.group_id === "local"
                ) {
                  return;
                }
                event.preventDefault();
                const point = contextMenuPointFromElement(event.currentTarget);
                onGroupContextMenu(group.group_id, point.x, point.y);
              }}
              disabled={group.group_id === "local"}
              className={cn(
                "h-11 w-11 shrink-0 rounded-2xl text-xs font-bold shadow-sm shadow-black/20 transition-transform hover:-translate-y-0.5 disabled:cursor-default disabled:opacity-70 disabled:hover:translate-y-0",
                active
                  ? "border-[hsl(var(--primary)/0.65)] bg-[hsl(var(--accent))] text-[hsl(var(--foreground))]"
                  : "border-[hsl(var(--border))] text-[hsl(var(--muted-foreground))]",
              )}
            >
              {group.name.slice(0, 2).toUpperCase()}
            </Button>
          );
        })}
        {dms.length ? <div className="h-px w-9 rounded-full bg-[hsl(var(--border))]" /> : null}
        {dms.map((dm) => {
          const active = workflow === "dm" && dm.dm_id === activeDm?.dm_id;
          return (
            <Button
              key={dm.dm_id}
              variant="outline"
              size="icon"
              title={dm.display_name}
              aria-label={`Open ${dm.display_name} direct message`}
              aria-current={active ? "page" : undefined}
              onClick={() => onSelectDm(dm.dm_id)}
              className={cn(
                "h-11 w-11 shrink-0 rounded-full text-xs font-bold shadow-sm shadow-black/20 transition-transform hover:-translate-y-0.5",
                active
                  ? "border-[hsl(var(--primary)/0.65)] bg-[hsl(var(--accent))] text-[hsl(var(--foreground))]"
                  : "border-[hsl(var(--border))] text-[hsl(var(--muted-foreground))]",
              )}
            >
              {dm.display_name.slice(0, 2).toUpperCase()}
            </Button>
          );
        })}
      </div>
      <Button
        type="button"
        variant="outline"
        onClick={onOpenLauncher}
        aria-label="Add group or direct message"
        className="grid h-11 w-11 place-items-center rounded-2xl border-emerald-300/35 bg-emerald-400/12 text-xl font-semibold text-emerald-100 shadow-sm shadow-black/20 transition-transform hover:-translate-y-0.5 hover:bg-emerald-400/18"
        title="Add group or direct message"
      >
        <Icon>+</Icon>
      </Button>
      <Button
        type="button"
        variant="outline"
        onClick={onOpenSettings}
        aria-label="Open rail configuration"
        className="grid h-10 w-10 place-items-center rounded-xl border-[hsl(var(--border))] text-[hsl(var(--muted-foreground))]"
        title={themeLabel}
      >
        <Icon>⚙</Icon>
      </Button>
    </aside>
  );
}

function MobileVoicePanel({
  group,
  voiceChannels,
  activeVoiceChannelId,
  voiceJoined,
  participants,
  localUserId,
  selfMuted,
  connectivity,
  voiceSession,
  remoteAudio,
  remoteStreams,
  appOutputVolume,
  selectedOutputDeviceId,
  localMicGain,
  onSelectVoiceChannel,
  onOpenCreateChannel,
  onAppOutputVolumeChange,
  onLocalMicGainChange,
  onToggleSelfMute,
  onLeaveVoice,
}: {
  group: GroupView | null;
  voiceChannels: ChannelStateView[];
  activeVoiceChannelId: string | null;
  voiceJoined: boolean;
  participants: VoiceParticipant[];
  localUserId: string | null;
  selfMuted: boolean;
  connectivity: ConnectivityPolicyView | null;
  voiceSession: VoiceSessionView | null;
  remoteAudio: VoiceRemoteAudioView[];
  remoteStreams: Record<string, MediaStream>;
  appOutputVolume: number;
  selectedOutputDeviceId: string;
  localMicGain: number;
  onSelectVoiceChannel: (channelId: string) => void;
  onOpenCreateChannel: () => void;
  onAppOutputVolumeChange: (value: number) => void;
  onLocalMicGainChange: (value: number) => void;
  onToggleSelfMute: (muted: boolean) => void;
  onLeaveVoice: () => void;
}) {
  const activeVoiceChannel =
    voiceChannels.find((channel) => channel.channel_id === activeVoiceChannelId) ??
    voiceChannels[0] ??
    null;
  const speaking = participants.filter(
    (participant) => participant.speaking && !participant.muted,
  ).length;

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-2xl border border-[hsl(var(--border)/0.74)] bg-[hsl(var(--card)/0.62)] shadow-none">
      <div className="border-b border-[hsl(var(--border))] px-4 py-3">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="text-xs font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">
              {group?.name ?? "No active group"}
            </p>
            <h2 className="mt-1 text-lg font-semibold tracking-tight">
              Voice rooms
            </h2>
            <p className="mt-1 text-sm text-[hsl(var(--muted-foreground))]">
              Tap a room to join. Text stays available from the Text tab.
            </p>
          </div>
          <Badge variant={voiceJoined ? "success" : "secondary"}>
            {voiceJoined ? "joined" : "idle"}
          </Badge>
        </div>
      </div>
      <div className="grid min-h-0 flex-1 gap-4 overflow-y-auto p-4 pb-28 lg:pb-4">
        <section className="grid gap-2">
          {voiceChannels.length === 0 ? (
            <div className="rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.24)] p-4">
              <p className="font-medium">No voice rooms yet</p>
              <p className="mt-1 text-sm text-[hsl(var(--muted-foreground))]">
                Create a voice room for this group, then tap it to join.
              </p>
            </div>
          ) : null}
          {voiceChannels.map((channel) => {
            const active = activeVoiceChannel?.channel_id === channel.channel_id;
            return (
              <Button
                key={channel.channel_id}
                variant={active ? "secondary" : "ghost"}
                aria-current={active ? "page" : undefined}
                className="h-auto justify-start rounded-xl px-3 py-3 text-left"
                onClick={() => onSelectVoiceChannel(channel.channel_id)}
              >
                <span className="grid gap-1">
                  <span className="font-medium">{channel.name}</span>
                  <span className="text-xs text-[hsl(var(--muted-foreground))]">
                    {voiceJoined && active
                      ? `${participants.length} participant${participants.length === 1 ? "" : "s"} · ${speaking} speaking`
                      : "Tap to join voice"}
                  </span>
                </span>
              </Button>
            );
          })}
          <Button
            variant="outline"
            size="icon"
            className="mt-1"
            aria-label="Add voice channel"
            title="Add voice channel"
            onClick={onOpenCreateChannel}
            disabled={!group}
          >
            <Icon>+</Icon>
          </Button>
        </section>

        {voiceJoined && activeVoiceChannel ? (
          <section
            aria-label={`${activeVoiceChannel.name} participants`}
            className="rounded-xl border border-[hsl(var(--border))] bg-black/15 p-3"
          >
            <div className="mb-2 flex items-center justify-between gap-2">
              <p className="text-sm font-semibold">{activeVoiceChannel.name}</p>
              <Badge variant="success">{speaking} speaking</Badge>
            </div>
            <VoiceParticipantList
              participants={participants}
              localUserId={localUserId}
              remoteAudio={remoteAudio}
              remoteStreams={remoteStreams}
              outputVolume={appOutputVolume}
              outputDeviceId={selectedOutputDeviceId}
            />
          </section>
        ) : null}
      </div>
      <SidebarVoiceStatus
        joined={voiceJoined}
        channelName={activeVoiceChannel?.name ?? null}
        connectivity={connectivity}
        voiceSession={voiceSession}
        selfMuted={selfMuted}
        localMicGain={localMicGain}
        appOutputVolume={appOutputVolume}
        onLocalMicGainChange={onLocalMicGainChange}
        onAppOutputVolumeChange={onAppOutputVolumeChange}
        onToggleSelfMute={onToggleSelfMute}
        onLeaveVoice={onLeaveVoice}
      />
    </div>
  );
}

function ChannelSidebar({
  groupLabel,
  role,
  pendingAdmissionCount,
  textChannels,
  voiceChannels,
  activeChannelId,
  activeVoiceChannelId,
  selectedWorkflow,
  inlineTextDraft,
  setInlineTextDraft,
  inlineVoiceDraft,
  setInlineVoiceDraft,
  onCommitInlineChannel,
  onSelectTextChannel,
  onSelectVoiceChannel,
  onReviewPendingAdmissions,
  voiceJoined,
  participants,
  localUserId,
  selfMuted,
  connectivity,
  voiceSession,
  remoteAudio,
  remoteStreams,
  appOutputVolume,
  selectedOutputDeviceId,
  localMicGain,
  onAppOutputVolumeChange,
  onLocalMicGainChange,
  onToggleSelfMute,
  onLeaveVoice,
}: {
  groupLabel: string;
  role: string;
  pendingAdmissionCount: number;
  textChannels: ChannelStateView[];
  voiceChannels: ChannelStateView[];
  activeChannelId: string | null;
  activeVoiceChannelId: string | null;
  selectedWorkflow: Workflow;
  inlineTextDraft: string | null;
  setInlineTextDraft: (value: string | null) => void;
  inlineVoiceDraft: string | null;
  setInlineVoiceDraft: (value: string | null) => void;
  onCommitInlineChannel: (kind: ChannelKind, rawName: string) => void;
  onSelectTextChannel: (channelId: string) => void;
  onSelectVoiceChannel: (channelId: string) => void;
  onReviewPendingAdmissions: () => void;
  voiceJoined: boolean;
  participants: VoiceParticipant[];
  localUserId: string | null;
  selfMuted: boolean;
  connectivity: ConnectivityPolicyView | null;
  voiceSession: VoiceSessionView | null;
  remoteAudio: VoiceRemoteAudioView[];
  remoteStreams: Record<string, MediaStream>;
  appOutputVolume: number;
  selectedOutputDeviceId: string;
  localMicGain: number;
  onAppOutputVolumeChange: (value: number) => void;
  onLocalMicGainChange: (value: number) => void;
  onToggleSelfMute: (muted: boolean) => void;
  onLeaveVoice: () => void;
}) {
  const speaking = participants.filter(
    (participant) => participant.speaking && !participant.muted,
  ).length;
  const [channelContextMenu, setChannelContextMenu] = useState<
    (ContextMenuPoint & { channel: ChannelStateView }) | null
  >(null);
  const openChannelContextMenu = (
    channel: ChannelStateView,
    point: ContextMenuPoint,
  ) => setChannelContextMenu({ channel, ...point });
  const channelContextMenuItems: SharedContextMenuItem[] = channelContextMenu
    ? [
        {
          id: "open-channel",
          label:
            channelContextMenu.channel.kind === "Voice"
              ? "Join voice room"
              : "Open text channel",
          icon: channelContextMenu.channel.kind === "Voice" ? "🔊" : "#",
          onSelect: () => {
            if (channelContextMenu.channel.kind === "Voice") {
              onSelectVoiceChannel(channelContextMenu.channel.channel_id);
            } else {
              onSelectTextChannel(channelContextMenu.channel.channel_id);
            }
          },
        },
        {
          id: "channel-authority",
          label: "Channel management unavailable",
          icon: "ⓘ",
          description:
            "Rename and destructive actions stay disabled until backend authority is available.",
          disabled: true,
        },
      ]
    : [];
  return (
    <aside
      aria-label="Channel navigation"
      className="hidden h-dvh border-r border-[hsl(var(--border))] bg-[hsl(var(--card)/0.72)] backdrop-blur-xl lg:block"
    >
      <div className="flex h-full flex-col">
        <div className="border-b border-[hsl(var(--border))] p-4">
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <h1 className="truncate text-lg font-semibold tracking-tight">
                {groupLabel}
              </h1>
              <p className="truncate text-xs text-[hsl(var(--muted-foreground))]">
                {role} · workspace
              </p>
            </div>
            <Badge variant={voiceJoined ? "success" : "secondary"}>
              {voiceJoined ? "voice" : "ready"}
            </Badge>
          </div>
        </div>
        <ScrollArea className="min-h-0 flex-1 p-3">
          {pendingAdmissionCount > 0 ? (
            <SidebarButton
              active={selectedWorkflow === "admission_requests"}
              meta={`${pendingAdmissionCount} waiting for owner/staff review`}
              onClick={onReviewPendingAdmissions}
            >
              Pending requests · {pendingAdmissionCount}
            </SidebarButton>
          ) : null}
          <SectionLabel
            actionLabel="Add text channel"
            onAction={() => setInlineTextDraft("")}
          >
            Text channels
          </SectionLabel>
          {textChannels.length === 0 && inlineTextDraft === null ? (
            <p className="px-2 text-xs text-[hsl(var(--muted-foreground))]">
              Use + to add the first text channel.
            </p>
          ) : null}
          {textChannels.map((channel) => (
            <SidebarButton
              key={channel.channel_id}
              active={
                selectedWorkflow === "channel" &&
                activeChannelId === channel.channel_id
              }
              onClick={() => onSelectTextChannel(channel.channel_id)}
              onContextMenu={(event) => {
                event.preventDefault();
                openChannelContextMenu(channel, {
                  x: event.clientX,
                  y: event.clientY,
                });
              }}
              onKeyboardContextMenu={(point) =>
                openChannelContextMenu(channel, point)
              }
              meta={channel.retention_status}
            >
              # {channel.name}
            </SidebarButton>
          ))}
          {inlineTextDraft !== null ? (
            <InlineChannelDraft
              kind="Text"
              value={inlineTextDraft}
              onChange={setInlineTextDraft}
              onCancel={() => setInlineTextDraft(null)}
              onCommit={(value) => onCommitInlineChannel("Text", value)}
            />
          ) : null}
          <SectionLabel
            actionLabel="Add voice channel"
            onAction={() => setInlineVoiceDraft("")}
          >
            Voice rooms
          </SectionLabel>
          {voiceChannels.length === 0 && inlineVoiceDraft === null ? (
            <p className="px-2 text-xs text-[hsl(var(--muted-foreground))]">
              Use + to add the first voice room.
            </p>
          ) : null}
          {voiceChannels.map((channel) => {
            const focusedVoiceRoom =
              voiceJoined && activeVoiceChannelId === channel.channel_id;
            return (
              <div key={channel.channel_id} className="mb-1">
                <SidebarButton
                  active={focusedVoiceRoom}
                  ariaCurrent={focusedVoiceRoom ? "page" : undefined}
                  onClick={() => onSelectVoiceChannel(channel.channel_id)}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    openChannelContextMenu(channel, {
                      x: event.clientX,
                      y: event.clientY,
                    });
                  }}
                  onKeyboardContextMenu={(point) =>
                    openChannelContextMenu(channel, point)
                  }
                  meta={
                    voiceJoined && focusedVoiceRoom
                      ? `${speaking} speaking`
                      : "click to join"
                  }
                >
                  🔊 {channel.name}
                </SidebarButton>
                {voiceJoined && focusedVoiceRoom ? (
                  <VoiceParticipantList
                    participants={participants}
                    localUserId={localUserId}
                    remoteAudio={remoteAudio}
                    remoteStreams={remoteStreams}
                    outputVolume={appOutputVolume}
                    outputDeviceId={selectedOutputDeviceId}
                  />
                ) : null}
              </div>
            );
          })}
          {inlineVoiceDraft !== null ? (
            <InlineChannelDraft
              kind="Voice"
              value={inlineVoiceDraft}
              onChange={setInlineVoiceDraft}
              onCancel={() => setInlineVoiceDraft(null)}
              onCommit={(value) => onCommitInlineChannel("Voice", value)}
            />
          ) : null}
        </ScrollArea>
        {channelContextMenu ? (
          <SharedContextMenu
            ariaLabel={`${channelContextMenu.channel.name} channel actions`}
            position={channelContextMenu}
            items={channelContextMenuItems}
            onClose={() => setChannelContextMenu(null)}
            testId="channel-context-menu"
          />
        ) : null}
        <SidebarVoiceStatus
          joined={voiceJoined}
          channelName={
            voiceChannels.find(
              (channel) => channel.channel_id === activeVoiceChannelId,
            )?.name ?? null
          }
          connectivity={connectivity}
          voiceSession={voiceSession}
          selfMuted={selfMuted}
          localMicGain={localMicGain}
          appOutputVolume={appOutputVolume}
          onLocalMicGainChange={onLocalMicGainChange}
          onAppOutputVolumeChange={onAppOutputVolumeChange}
          onToggleSelfMute={onToggleSelfMute}
          onLeaveVoice={onLeaveVoice}
        />
      </div>
    </aside>
  );
}

function SectionLabel({
  children,
  actionLabel,
  onAction,
}: {
  children: React.ReactNode;
  actionLabel?: string;
  onAction?: () => void;
}) {
  return (
    <div className="mb-2 mt-5 flex items-center justify-between gap-2 px-2">
      <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">
        {children}
      </p>
      {onAction ? (
        <Button
          type="button"
          variant="ghost"
          aria-label={actionLabel}
          title={actionLabel}
          className="grid h-6 w-6 place-items-center rounded-md p-0 text-[hsl(var(--muted-foreground))] hover:bg-[hsl(var(--accent))] hover:text-[hsl(var(--foreground))]"
          onClick={onAction}
        >
          <Icon>+</Icon>
        </Button>
      ) : null}
    </div>
  );
}

function InlineChannelDraft({
  kind,
  value,
  onChange,
  onCancel,
  onCommit,
}: {
  kind: ChannelKind;
  value: string;
  onChange: (value: string) => void;
  onCancel: () => void;
  onCommit: (value: string) => void;
}) {
  const committedRef = useRef(false);
  const commit = () => {
    if (committedRef.current) return;
    committedRef.current = true;
    onCommit(value);
  };
  return (
    <div className="mb-1 rounded-xl border border-[hsl(var(--border))] bg-black/15 p-2">
      <Input
        autoFocus
        aria-label={`${kind} channel name`}
        value={value}
        placeholder={kind === "Text" ? "text-channel-name" : "voice-room-name"}
        onChange={(event) => onChange(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            commit();
          }
          if (event.key === "Escape") {
            committedRef.current = true;
            onCancel();
          }
        }}
        className="h-9"
      />
    </div>
  );
}

function VoiceParticipantList({
  participants,
  localUserId,
  remoteAudio,
  remoteStreams,
  outputVolume,
  outputDeviceId,
}: {
  participants: VoiceParticipant[];
  localUserId: string | null;
  remoteAudio: VoiceRemoteAudioView[];
  remoteStreams: Record<string, MediaStream>;
  outputVolume: number;
  outputDeviceId: string;
}) {
  if (participants.length === 0) {
    return (
      <p className="mb-2 ml-5 px-2 text-xs text-[hsl(var(--muted-foreground))]">
        Waiting for audio activity…
      </p>
    );
  }
  return (
    <div className="mb-2 ml-4 grid gap-1 border-l border-[hsl(var(--border))] pl-2">
      {participants.map((participant) => {
        const local = isLocalVoiceParticipant(participant, localUserId);
        const remoteTrack = remoteAudio.find(
          (track) => track.participant_id === participant.id,
        );
        const remoteStream = local ? null : remoteStreams[participant.id];
        return (
          <div
            key={participant.id}
            data-testid={
              local ? "voice-local-participant" : "voice-remote-participant"
            }
            className={cn(
              "group flex min-w-0 items-center gap-2 rounded-lg px-2 py-1 text-sm text-[hsl(var(--muted-foreground))]",
              participant.speaking &&
                !participant.muted &&
                "bg-emerald-400/10 text-emerald-100",
            )}
            title={
              participant.muted
                ? "Muted"
                : participant.speaking
                  ? "Speaking"
                  : "Listening"
            }
          >
            <span
              className={cn(
                "h-2 w-2 rounded-full bg-[hsl(var(--muted-foreground)/0.45)]",
                participant.speaking &&
                  !participant.muted &&
                  "bg-emerald-300 shadow-[0_0_0_3px_hsl(142_76%_36%/0.24)]",
              )}
              aria-hidden="true"
            />
            <span className="truncate">{participant.name}</span>
            {participant.muted ? (
              <span className="ml-auto text-[10px]" aria-label="muted">
                mute
              </span>
            ) : null}
            {!local && (remoteTrack || remoteStream) ? (
              <RemoteAudioAttachment
                participant={participant}
                src={remoteTrack?.playback_element_id ? null : participant.remote_audio_src}
                stream={remoteStream}
                volumePercent={outputVolume}
                outputDeviceId={outputDeviceId}
              />
            ) : null}
          </div>
        );
      })}
    </div>
  );
}

function SidebarVoiceStatus({
  joined,
  channelName,
  connectivity,
  voiceSession,
  selfMuted,
  localMicGain,
  appOutputVolume,
  onLocalMicGainChange,
  onAppOutputVolumeChange,
  onToggleSelfMute,
  onLeaveVoice,
}: {
  joined: boolean;
  channelName: string | null;
  connectivity: ConnectivityPolicyView | null;
  voiceSession: VoiceSessionView | null;
  selfMuted: boolean;
  localMicGain: number;
  appOutputVolume: number;
  onLocalMicGainChange: (value: number) => void;
  onAppOutputVolumeChange: (value: number) => void;
  onToggleSelfMute: (muted: boolean) => void;
  onLeaveVoice: () => void;
}) {
  const adapter = connectivity?.signaling_profiles[0]?.adapter_kind ?? "provider";
  return (
    <div
      data-testid="voice-sidebar-status"
      className="border-t border-[hsl(var(--border))] bg-black/20 p-3"
    >
      <div className="mb-3 flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-sm font-semibold">
            {joined ? (channelName ?? "Voice") : "Voice idle"}
          </p>
          <p className="truncate text-xs text-[hsl(var(--muted-foreground))]">
            {joined
              ? `${adapter} · ${voiceSession?.status_copy ?? "joined"}`
              : "Click a voice channel to join"}
          </p>
        </div>
        {joined ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label="Leave voice call"
            title="Leave voice call"
            onClick={onLeaveVoice}
            className="h-8 w-8 text-red-200 hover:bg-red-400/10 hover:text-red-100"
          >
            <Icon>🚪</Icon>
          </Button>
        ) : null}
      </div>
      <div className="grid gap-3">
        <label className="grid gap-1 text-xs text-[hsl(var(--muted-foreground))]">
          <span className="flex items-center justify-between gap-2">
            <span>Mic gain</span>
            <span>{localMicGain}%</span>
          </span>
          <Slider
            aria-label="Microphone input volume"
            value={[localMicGain]}
            min={0}
            max={200}
            step={1}
            disabled={!joined}
            onValueChange={(value) => onLocalMicGainChange(value[0] ?? 100)}
          />
        </label>
        <label className="grid gap-1 text-xs text-[hsl(var(--muted-foreground))]">
          <span className="flex items-center justify-between gap-2">
            <span>App output</span>
            <span>{appOutputVolume}%</span>
          </span>
          <Slider
            aria-label="App output volume"
            value={[appOutputVolume]}
            min={0}
            max={100}
            step={1}
            onValueChange={(value) => onAppOutputVolumeChange(value[0] ?? 100)}
          />
        </label>
        <div className="grid grid-cols-2 gap-2">
          <Button
            type="button"
            variant={selfMuted ? "secondary" : "outline"}
            size="sm"
            disabled={!joined}
            onClick={() => onToggleSelfMute(!selfMuted)}
          >
            {selfMuted ? "Unmute" : "Mute"}
          </Button>
          <Badge variant={joined ? "success" : "secondary"} className="justify-center">
            {joined ? "joined" : "idle"}
          </Badge>
        </div>
      </div>
    </div>
  );
}

function CommandNotificationStack({
  notifications,
  onDismiss,
}: {
  notifications: CommandNotification[];
  onDismiss: (id: string) => void;
}) {
  if (notifications.length === 0) return null;
  return (
    <div
      role="region"
      aria-label="Command notifications"
      className="fixed right-4 top-16 z-50 grid w-[min(24rem,calc(100vw-2rem))] gap-3"
    >
      {notifications.map((notification) => (
        <div
          key={notification.id}
          role="alert"
          className="rounded-2xl border border-red-300/35 bg-[hsl(var(--card)/0.96)] p-4 text-[hsl(var(--foreground))] shadow-2xl shadow-black/40 backdrop-blur-xl"
        >
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="text-sm font-semibold text-red-100">
                {notification.title}
              </p>
              <p className="mt-1 text-sm leading-5 text-[hsl(var(--muted-foreground))]">
                {notification.message}
              </p>
              <p className="mt-2 text-[11px] uppercase tracking-[0.14em] text-[hsl(var(--muted-foreground))]">
                Logged to console · {notification.createdAt}
              </p>
            </div>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label="Dismiss command notification"
              className="h-8 w-8 shrink-0"
              onClick={() => onDismiss(notification.id)}
            >
              <Icon>×</Icon>
            </Button>
          </div>
        </div>
      ))}
    </div>
  );
}

function SidebarButton({
  children,
  active,
  meta,
  ariaCurrent,
  onClick,
  onContextMenu,
  onKeyboardContextMenu,
}: {
  children: React.ReactNode;
  active?: boolean;
  meta?: string;
  ariaCurrent?: React.AriaAttributes["aria-current"];
  onClick?: () => void;
  onContextMenu?: (event: React.MouseEvent<HTMLButtonElement>) => void;
  onKeyboardContextMenu?: (point: ContextMenuPoint) => void;
}) {
  return (
    <Button
      variant="ghost"
      aria-current={ariaCurrent}
      onClick={onClick}
      onContextMenu={onContextMenu}
      onKeyDown={(event) => {
        if (!onKeyboardContextMenu || !isKeyboardContextMenu(event)) return;
        event.preventDefault();
        onKeyboardContextMenu(contextMenuPointFromElement(event.currentTarget));
      }}
      className={cn(
        "mb-1 h-auto w-full justify-start whitespace-normal rounded-xl px-3 py-2 text-left text-sm text-[hsl(var(--muted-foreground))]",
        active && "bg-[hsl(var(--accent))] text-[hsl(var(--foreground))]",
      )}
    >
      <span className="grid gap-0.5">
        <span className="font-medium">{children}</span>
        {meta ? (
          <span className="truncate text-[11px] text-[hsl(var(--muted-foreground))]">
            {meta}
          </span>
        ) : null}
      </span>
    </Button>
  );
}

function RuntimeModeBanner({ runtimeMode }: { runtimeMode: RuntimeModeView }) {
  return (
    <section className="mx-4 mt-3 rounded-2xl border border-amber-300/25 bg-amber-300/10 p-3 text-amber-50 md:mx-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <Badge
              variant={
                runtimeMode.production_labels_enabled ? "success" : "warning"
              }
            >
              {runtimeMode.harness_badge}
            </Badge>
            <span className="text-xs font-semibold uppercase tracking-[0.16em] text-amber-50/70">
              runtime mode: {runtimeMode.mode}
            </span>
          </div>
          <p className="mt-2 text-sm leading-6 text-amber-50/85">
            {runtimeMode.disabled_reason}
          </p>
        </div>
        <div className="grid min-w-[16rem] gap-2 sm:grid-cols-3">
          {runtimeMode.services.map((service) => (
            <div
              key={service.key}
              className="rounded-xl border border-amber-300/15 bg-black/15 p-2"
              title={service.detail}
            >
              <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-amber-50/60">
                {service.label}
              </p>
              <p className="mt-1 text-xs text-amber-50/90">{service.status}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

function TopBar({
  groupLabel,
  activeTitle,
  activeSubtitle,
  membersPanelOpen,
  onToggleMembers,
  membersPanelAvailable,
  onOpenDiagnostics,
  inspectorOpen,
  diagnosticsEnabled,
}: {
  groupLabel: string;
  activeTitle: string;
  activeSubtitle: string;
  membersPanelOpen: boolean;
  onToggleMembers: () => void;
  membersPanelAvailable: boolean;
  onOpenDiagnostics: () => void;
  inspectorOpen: boolean;
  diagnosticsEnabled: boolean;
  admissionPending?: boolean;
}) {
  return (
    <header
      aria-label="Workspace topbar"
      className="border-b border-[hsl(var(--border))] bg-[hsl(var(--background)/0.88)] p-3 backdrop-blur-xl md:p-4"
    >
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant="outline">{groupLabel}</Badge>
            <span className="text-xs text-[hsl(var(--muted-foreground))]">
              private workspace
            </span>
          </div>
          <h2 className="mt-1 truncate text-xl font-semibold tracking-tight">
            {activeTitle}
          </h2>
          <p className="line-clamp-1 text-xs text-[hsl(var(--muted-foreground))]">
            {activeSubtitle}
          </p>
        </div>
        <div className="flex shrink-0 items-center justify-end gap-2">
          {membersPanelAvailable ? (
            <Button
              type="button"
              variant="outline"
              size="icon"
              aria-label={membersPanelOpen ? "Close member panel" : "Open member panel"}
              title="Members"
              onClick={onToggleMembers}
              className="h-9 w-9"
            >
              <Icon>👥</Icon>
            </Button>
          ) : null}
          {diagnosticsEnabled ? (
            <Button
              type="button"
              variant={inspectorOpen ? "secondary" : "outline"}
              size="icon"
              aria-label="Open diagnostics"
              title="Diagnostics"
              onClick={onOpenDiagnostics}
              className="h-9 w-9"
            >
              <Icon>⌁</Icon>
            </Button>
          ) : null}
        </div>
      </div>
    </header>
  );
}

function overlayCopy(overlay: OverlayKind | null): {
  title: string;
  description: string;
  align: "center" | "side";
} {
  switch (overlay) {
    case "create-group":
      return {
        title: "Create group",
        description:
          "Start a workspace with a default text channel and voice room.",
        align: "center",
      };
    case "launcher":
      return {
        title: "Add group or direct message",
        description:
          "Paste an invite for a group or DM, or start a new group.",
        align: "center",
      };
    case "group-invite":
      return {
        title: "Create group invite",
        description:
          "Generate a signed invite descriptor for this group.",
        align: "side",
      };
    case "group-config":
      return {
        title: "Group configuration",
        description:
          "Configure this group's signaling profile and ICE policy.",
        align: "side",
      };
    case "settings":
      return {
        title: "Config",
        description:
          "Manage theme, audio devices, volume, signaling, and ICE policy.",
        align: "side",
      };
    case "diagnostics":
      return {
        title: "Diagnostics",
        description:
          "Review runtime, transport, and workspace evidence without changing chat context.",
        align: "side",
      };
    default:
      return {
        title: "Workspace dialog",
        description: "Workspace action",
        align: "center",
      };
  }
}

function WorkspaceOverlay({
  overlay,
  closing,
  onClose,
  children,
}: {
  overlay: OverlayKind | null;
  closing: boolean;
  onClose: () => void;
  children: React.ReactNode;
}) {
  if (!overlay) return null;
  const copy = overlayCopy(overlay);
  const titleId = `workspace-overlay-${overlay}-title`;
  const descriptionId = `workspace-overlay-${overlay}-description`;
  return (
    <Dialog>
      <DialogPortal>
        <div
          data-state={closing ? "closed" : "open"}
          className={cn(
            "fixed inset-0 z-50 grid bg-black/55 p-3 backdrop-blur-sm md:p-6",
            closing
              ? "animate-[discrypt-fade-out_140ms_ease-in_forwards]"
              : "animate-[discrypt-fade-in_160ms_ease-out]",
          )}
        >
          <DialogOverlay
            data-state={closing ? "closed" : "open"}
            className="fixed inset-0"
            aria-hidden="true"
            onClick={onClose}
          />
          <DialogContent
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
            aria-describedby={descriptionId}
            data-state={closing ? "closed" : "open"}
            onEscapeKeyDown={onClose}
            className={cn(
              "relative z-10 max-h-[calc(100dvh-1.5rem)] w-full overflow-y-auto border-[hsl(var(--border)/0.9)] bg-[hsl(var(--popover)/0.96)] p-0 shadow-2xl shadow-black/45 md:max-h-[calc(100dvh-3rem)]",
              copy.align === "side"
                ? cn(
                    "ml-auto max-w-3xl self-stretch",
                    closing
                      ? "animate-[discrypt-slide-out-right_140ms_ease-in_forwards]"
                      : "animate-[discrypt-slide-in-right_180ms_ease-out]",
                  )
                : cn(
                    "max-w-5xl place-self-center",
                    closing
                      ? "animate-[discrypt-scale-out_140ms_ease-in_forwards]"
                      : "animate-[discrypt-scale-in_160ms_ease-out]",
                  ),
            )}
          >
            <DialogHeader className="sticky top-0 z-10 border-b border-[hsl(var(--border))] bg-[hsl(var(--popover)/0.96)] p-4 backdrop-blur md:p-5">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <DialogTitle id={titleId}>{copy.title}</DialogTitle>
                  <DialogDescription id={descriptionId}>
                    {copy.description}
                  </DialogDescription>
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  aria-label={`Close ${copy.title}`}
                  onClick={onClose}
                >
                  ✕
                </Button>
              </div>
            </DialogHeader>
            <div className="p-4 md:p-5">{children}</div>
          </DialogContent>
        </div>
      </DialogPortal>
    </Dialog>
  );
}

function AppearanceSettings({
  themeId,
  onThemeChange,
}: {
  themeId: ThemeId;
  onThemeChange: (id: ThemeId) => void;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Appearance</CardTitle>
        <CardDescription>
          Choose the app theme. Theme tokens drive the shadcn component system so future themes can extend the interface without changing screens.
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-3">
        <Label className="grid gap-2">
          Theme
          <Select aria-label="Theme" value={themeId} onValueChange={(value) => onThemeChange(value as ThemeId)}>
            {discryptUiConfig.themes.map((theme) => (
              <SelectItem key={theme.id} value={theme.id}>
                {theme.label}
              </SelectItem>
            ))}
          </Select>
        </Label>
      </CardContent>
    </Card>
  );
}

function AudioSettingsPanel({
  inputDevices,
  outputDevices,
  selectedInputDeviceId,
  selectedOutputDeviceId,
  voiceDeviceStatus,
  localMicGain,
  appOutputVolume,
  onSelectInputDevice,
  onSelectOutputDevice,
  onRefreshDevices,
  onLocalMicGainChange,
  onAppOutputVolumeChange,
}: {
  inputDevices: VoiceDeviceOption[];
  outputDevices: VoiceDeviceOption[];
  selectedInputDeviceId: string;
  selectedOutputDeviceId: string;
  voiceDeviceStatus: string | null;
  localMicGain: number;
  appOutputVolume: number;
  onSelectInputDevice: (deviceId: string) => void;
  onSelectOutputDevice: (deviceId: string) => void;
  onRefreshDevices: () => void;
  onLocalMicGainChange: (value: number) => void;
  onAppOutputVolumeChange: (value: number) => void;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Audio</CardTitle>
        <CardDescription>
          Select capture and playback devices, then tune app-wide microphone and output levels used by voice and future app sounds.
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-5">
        <div className="grid gap-4 md:grid-cols-2">
          <Label className="grid gap-2">
            Microphone input
            <Select aria-label="Microphone input" data-testid="voice-mic-selector" value={selectedInputDeviceId} onValueChange={onSelectInputDevice}>
              <SelectItem value="default">System default microphone</SelectItem>
              {inputDevices.map((device) => (
                <SelectItem key={device.device_id} value={device.device_id}>
                  {device.label}
                </SelectItem>
              ))}
            </Select>
          </Label>
          <Label className="grid gap-2">
            App output device
            <Select aria-label="App output device" data-testid="voice-output-selector" value={selectedOutputDeviceId} onValueChange={onSelectOutputDevice}>
              <SelectItem value="default">System default output</SelectItem>
              {outputDevices.map((device) => (
                <SelectItem key={device.device_id} value={device.device_id}>
                  {device.label}
                </SelectItem>
              ))}
            </Select>
          </Label>
        </div>
        <Button type="button" variant="outline" aria-label="Refresh audio devices" onClick={onRefreshDevices} className="w-fit">
          Refresh audio devices
        </Button>
        <p className="text-xs leading-5 text-[hsl(var(--muted-foreground))]">
          {voiceDeviceStatus ?? "Refresh may request microphone access so Linux and browser backends can reveal device names."}
        </p>
        <div className="grid gap-4 md:grid-cols-2">
          <label className="grid gap-2 text-sm">
            <span className="flex items-center justify-between gap-2">
              <span>Microphone input volume</span>
              <span className="text-xs text-[hsl(var(--muted-foreground))]">{localMicGain}%</span>
            </span>
            <Slider aria-label="Microphone input volume" value={[localMicGain]} min={0} max={200} step={1} onValueChange={(value) => onLocalMicGainChange(value[0] ?? 100)} />
          </label>
          <label className="grid gap-2 text-sm">
            <span className="flex items-center justify-between gap-2">
              <span>App output volume</span>
              <span className="text-xs text-[hsl(var(--muted-foreground))]">{appOutputVolume}%</span>
            </span>
            <Slider aria-label="App output volume" value={[appOutputVolume]} min={0} max={100} step={1} onValueChange={(value) => onAppOutputVolumeChange(value[0] ?? 100)} />
          </label>
        </div>
      </CardContent>
    </Card>
  );
}

function TransportStatusStrip({
  statuses,
  diagnostics,
  runtimePeers,
  runtimeRole,
  onProbeAdapter,
  onProbeDataChannel,
  onStartTextTransport,
  onAttachRuntime,
}: {
  statuses: TransportStatusView[];
  diagnostics?: TransportDiagnosticsView;
  runtimePeers: { local: string; remote: string };
  runtimeRole: "offerer" | "answerer";
  onProbeAdapter: () => void;
  onProbeDataChannel: () => void;
  onStartTextTransport: () => void;
  onAttachRuntime: () => void;
}) {
  const ordered = statuses.length
    ? statuses
    : [
        {
          label: "signaling",
          status: "waiting-for-invite",
          detail: "Create or paste an invite before signaling can be used",
        },
        {
          label: "ICE",
          status: "waiting-for-signed-invite",
          detail:
            "No ICE server metadata is available until an invite descriptor is present",
        },
      ];
  return (
    <section
      aria-label="Backend-derived transport status"
      className="mx-4 mt-3 rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--card)/0.82)] p-3 shadow-sm shadow-black/20 md:mx-6"
    >
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div>
          <p className="text-sm font-semibold">Transport status</p>
          <p className="text-xs leading-5 text-[hsl(var(--muted-foreground))]">
            Backend-derived state only; route and media claims stay quiet until
            command state provides evidence.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" variant="outline" onClick={onProbeAdapter}>
            Probe adapter
          </Button>
          <Button size="sm" variant="outline" onClick={onProbeDataChannel}>
            Probe data channel
          </Button>
          <Button size="sm" onClick={onStartTextTransport}>
            Start text proof
          </Button>
          <Badge variant="outline">status</Badge>
        </div>
      </div>
      <div className="mb-3 grid gap-2 rounded-xl border border-[hsl(var(--border))] bg-black/15 p-3 md:grid-cols-[1fr_auto]">
        <div className="grid gap-2">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">
              Backend-derived text runtime
            </p>
            <p className="mt-1 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
              Peer ids and role are derived from signed invite/group metadata.
            </p>
          </div>
          <div className="grid gap-2 text-xs md:grid-cols-3">
            <code className="rounded-lg bg-black/25 px-2 py-1 font-mono">
              local {runtimePeers.local}
            </code>
            <code className="rounded-lg bg-black/25 px-2 py-1 font-mono">
              remote {runtimePeers.remote}
            </code>
            <Badge variant="outline">{runtimeRole}</Badge>
          </div>
        </div>
        <div className="flex flex-wrap items-end gap-2">
          <Button size="sm" onClick={onAttachRuntime}>
            Attach text runtime
          </Button>
        </div>
        <p className="text-xs leading-5 text-[hsl(var(--muted-foreground))] md:col-span-2">
          Attachments and route probes are available for diagnostics and release checks.
        </p>
      </div>
      <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-4">
        {ordered.map((item) => (
          <div
            key={item.label}
            className="min-w-0 rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.30)] p-3"
          >
            <div className="flex items-center justify-between gap-2">
              <span className="truncate text-xs font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">
                {item.label}
              </span>
              <Badge variant={transportBadgeVariant(item.status)}>
                {item.status}
              </Badge>
            </div>
            <p className="mt-2 line-clamp-3 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
              {item.detail}
            </p>
          </div>
        ))}
      </div>
      {diagnostics ? (
        <div className="mt-3 grid gap-3 border-t border-[hsl(var(--border))] pt-3 lg:grid-cols-[1.1fr_0.9fr]">
          <div>
            <div className="mb-2 flex items-center justify-between gap-2">
              <p className="text-xs font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">
                Adapter readiness
              </p>
              <Badge
                variant={diagnostics.selected_adapter ? "success" : "secondary"}
              >
                {diagnostics.selected_adapter ?? "none selected"}
              </Badge>
            </div>
            <div className="grid gap-2 sm:grid-cols-2">
              {diagnostics.adapter_boundaries.map((boundary) => (
                <div
                  key={boundary.kind}
                  className="rounded-xl border border-[hsl(var(--border))] bg-black/15 p-3"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate text-xs font-semibold">
                      {boundary.kind}
                    </span>
                    <Badge variant={transportBadgeVariant(boundary.readiness)}>
                      {boundary.readiness}
                    </Badge>
                  </div>
                  <p className="mt-1 text-xs text-[hsl(var(--muted-foreground))]">
                    Feature {boundary.cargo_feature}; failure class{" "}
                    {boundary.failure_class}
                  </p>
                </div>
              ))}
            </div>
          </div>
          <div className="rounded-xl border border-[hsl(var(--border))] bg-black/15 p-3">
            <p className="text-xs font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">
              Route proof
            </p>
            <div className="mt-2 flex flex-wrap gap-2">
              <Badge
                variant={transportBadgeVariant(diagnostics.route_proof_status)}
              >
                {diagnostics.route_proof_status}
              </Badge>
              <Badge variant={transportBadgeVariant(diagnostics.turn_required)}>
                TURN {diagnostics.turn_required}
              </Badge>
            </div>
            <p className="mt-2 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
              {diagnostics.route_proof_detail}
            </p>
            <p className="mt-2 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
              {providerFallbackCopy(diagnostics)}
            </p>
            <div className="mt-3 rounded-lg border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.22)] p-2">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-xs font-semibold uppercase tracking-[0.12em] text-[hsl(var(--muted-foreground))]">
                  Adapter probe
                </span>
                <Badge
                  variant={transportBadgeVariant(
                    diagnostics.adapter_probe_status,
                  )}
                >
                  {diagnostics.adapter_probe_status}
                </Badge>
              </div>
              <p className="mt-2 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
                {diagnostics.adapter_probe_detail}
              </p>
            </div>
            <div className="mt-3 rounded-lg border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.22)] p-2">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-xs font-semibold uppercase tracking-[0.12em] text-[hsl(var(--muted-foreground))]">
                  DataChannel probe
                </span>
                <Badge
                  variant={transportBadgeVariant(
                    diagnostics.data_channel_probe_status,
                  )}
                >
                  {diagnostics.data_channel_probe_status}
                </Badge>
              </div>
              <p className="mt-2 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
                {diagnostics.data_channel_probe_detail}
              </p>
              {diagnostics.data_channel_probe ? (
                <div className="mt-2 grid gap-1 text-xs text-[hsl(var(--muted-foreground))] sm:grid-cols-2">
                  <span>
                    Direct: offerer{" "}
                    {diagnostics.data_channel_probe.offerer_direct_path_ready
                      ? "ready"
                      : "not ready"}{" "}
                    / answerer{" "}
                    {diagnostics.data_channel_probe.answerer_direct_path_ready
                      ? "ready"
                      : "not ready"}
                  </span>
                  <span>
                    TURN: offerer{" "}
                    {diagnostics.data_channel_probe.offerer_turn_fallback_ready
                      ? "ready"
                      : "not ready"}{" "}
                    / answerer{" "}
                    {diagnostics.data_channel_probe.answerer_turn_fallback_ready
                      ? "ready"
                      : "not ready"}
                  </span>
                  <span>
                    TURN servers:{" "}
                    {
                      diagnostics.data_channel_probe
                        .offerer_configured_turn_servers
                    }
                    /
                    {
                      diagnostics.data_channel_probe
                        .answerer_configured_turn_servers
                    }
                  </span>
                  <span>
                    Relay candidates: local{" "}
                    {
                      diagnostics.data_channel_probe
                        .offerer_local_relay_candidates_gathered
                    }
                    /
                    {
                      diagnostics.data_channel_probe
                        .answerer_local_relay_candidates_gathered
                    }{" "}
                    remote{" "}
                    {
                      diagnostics.data_channel_probe
                        .offerer_remote_relay_candidates_applied
                    }
                    /
                    {
                      diagnostics.data_channel_probe
                        .answerer_remote_relay_candidates_applied
                    }
                  </span>
                </div>
              ) : null}
            </div>
            {diagnostics.adapter_fallback_attempts.length ? (
              <div className="mt-3 space-y-1">
                {diagnostics.adapter_fallback_attempts.map((attempt) => (
                  <p
                    key={`${attempt.kind}-${attempt.readiness}`}
                    className="text-xs text-[hsl(var(--muted-foreground))]"
                  >
                    {attempt.selected ? "✓" : attempt.attempted ? "•" : "○"}{" "}
                    {attempt.kind}: {attempt.readiness} ({attempt.failure_class}
                    )
                  </p>
                ))}
              </div>
            ) : null}
          </div>
        </div>
      ) : null}
    </section>
  );
}

function transportBadgeVariant(
  status: string,
): React.ComponentProps<typeof Badge>["variant"] {
  if (
    [
      "configured",
      "signed-endpoint-ready",
      "clear",
      "available",
      "selected",
      "route-proofed",
      "provider-roundtrip-proofed",
      "webrtc-datachannel-proofed",
    ].includes(status)
  ) {
    return "success";
  }
  if (
    [
      "attention",
      "last-command-error",
      "media-gated",
      "provider-roundtrip-failed",
      "webrtc-datachannel-failed",
      "no-healthy-adapter",
      "provider-failed",
      "retrying-fallback",
      "turn-required",
      "credential-gated",
    ].includes(status)
  ) {
    return "warning";
  }
  if (["failed"].includes(status)) {
    return "warning";
  }
  return "secondary";
}

function MobileWorkflowNav({
  workflow,
  setWorkflow,
}: {
  workflow: Workflow;
  setWorkflow: (workflow: Workflow) => void;
}) {
  const items: { id: Workflow; label: string }[] = [
    { id: "setup", label: "Setup" },
    { id: "dm", label: "DMs" },
    { id: "channel", label: "Text" },
    { id: "voice", label: "Voice" },
    { id: "admission_requests", label: "Requests" },
  ];
  return (
    <nav
      className="fixed inset-x-3 bottom-3 z-40 flex gap-2 overflow-x-auto rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--card)/0.94)] p-2 shadow-2xl shadow-black/40 backdrop-blur-xl lg:hidden"
      aria-label="Workspace sections"
    >
      {items.map((item) => (
        <Button
          key={item.id}
          variant={workflow === item.id ? "secondary" : "ghost"}
          size="sm"
          onClick={() => setWorkflow(item.id)}
          aria-current={workflow === item.id ? "page" : undefined}
        >
          {item.label}
        </Button>
      ))}
    </nav>
  );
}

function SetupPanel({
  onCreateGroup,
  onJoinInvite,
}: {
  onCreateGroup: () => void;
  onJoinInvite: () => void;
}) {
  return (
    <div className="grid h-full min-h-0 place-items-center px-2 py-8">
      <Card className="w-full max-w-lg border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.88)] shadow-xl shadow-black/20">
        <CardHeader className="text-center">
          <Badge variant="secondary" className="mx-auto w-fit">
            Local profile ready
          </Badge>
          <CardTitle className="text-2xl tracking-tight md:text-3xl">
            Start a private space
          </CardTitle>
          <CardDescription className="mx-auto max-w-sm text-sm leading-6">
            Create a group or open an invite to begin.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-2">
          <Button className="justify-center" onClick={onCreateGroup}>
            <Icon>+</Icon>Create group
          </Button>
          <Button
            className="justify-center"
            variant="secondary"
            onClick={onJoinInvite}
          >
            Join with invite
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}

function DmPanel({
  activeDm,
  messages,
  textStateLegend,
  draftDmName,
  setDraftDmName,
  draftMessage,
  setDraftMessage,
  onStartDm,
  onSendDm,
  transportProof,
  setTransportProof,
  diagnosticsEnabled,
}: {
  activeDm: DirectConversationView | null;
  messages: AppMessageView[];
  textStateLegend: TextStateView[];
  draftDmName: string;
  setDraftDmName: (value: string) => void;
  draftMessage: string;
  setDraftMessage: (value: string) => void;
  onStartDm: () => void;
  onSendDm: () => void;
  transportProof: boolean;
  setTransportProof: (value: boolean) => void;
  diagnosticsEnabled: boolean;
  composerNotice?: string | null;
}) {
  const visibleMessages = activeDm
    ? messages.filter((message) => message.target.dm_id === activeDm.dm_id)
    : [];
  if (!activeDm) {
    return (
      <Card className="flex h-full min-h-0 flex-col">
        <CardHeader className="border-b border-[hsl(var(--border))]">
          <CardTitle>Direct messages</CardTitle>
          <CardDescription>
            Start a private conversation, then use this space as the message
            timeline.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid flex-1 place-items-center p-6">
          <div className="w-full max-w-md rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.32)] p-5">
            <Label className="grid gap-2">
              Contact name
              <Input
                value={draftDmName}
                placeholder="Display name"
                onChange={(event) => setDraftDmName(event.target.value)}
              />
            </Label>
            <Button
              className="mt-4 w-full"
              onClick={onStartDm}
              disabled={!draftDmName.trim()}
            >
              <Icon>+</Icon>Start DM
            </Button>
          </div>
        </CardContent>
      </Card>
    );
  }
  return (
    <div className="h-full min-h-0">
      <Timeline
        title={activeDm.display_name}
        description="Private conversation"
        messages={visibleMessages}
        textStateLegend={textStateLegend}
        draftMessage={draftMessage}
        setDraftMessage={setDraftMessage}
        sendLabel="Send DM message"
        onSend={onSendDm}
        disabled={!activeDm}
        transportProof={transportProof}
        setTransportProof={setTransportProof}
        diagnosticsEnabled={diagnosticsEnabled}
      />
    </div>
  );
}

function GroupContextMenu({
  menu,
  groups,
  onClose,
  onCreateInvite,
  onOpenConfig,
}: {
  menu: { groupId: string; x: number; y: number } | null;
  groups: GroupView[];
  onClose: () => void;
  onCreateInvite: (groupId: string) => void;
  onOpenConfig: (groupId: string) => void;
}) {
  if (!menu) return null;
  const group = groups.find((candidate) => candidate.group_id === menu.groupId);
  return (
    <SharedContextMenu
      ariaLabel={`${group?.name ?? "Group"} actions`}
      position={menu}
      onClose={onClose}
      testId="group-context-menu"
      items={[
        {
          id: "create-invite",
          label: "Create invite",
          icon: "🔗",
          onSelect: () => onCreateInvite(menu.groupId),
        },
        {
          id: "group-configuration",
          label: "Group configuration",
          icon: "⚙",
          onSelect: () => onOpenConfig(menu.groupId),
        },
      ]}
    />
  );
}

function SharedContextMenu({
  ariaLabel,
  position,
  items,
  onClose,
  testId,
}: {
  ariaLabel: string;
  position: ContextMenuPoint;
  items: SharedContextMenuItem[];
  onClose: () => void;
  testId?: string;
}) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const clamped = clampContextMenuPoint(position);

  useEffect(() => {
    const firstEnabledItem = menuRef.current?.querySelector<HTMLButtonElement>(
      'button[role="menuitem"]:not(:disabled)',
    );
    (firstEnabledItem ?? menuRef.current)?.focus();
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) onClose();
    };
    const onResize = () => onClose();
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("resize", onResize);
    };
  }, [onClose]);

  function focusMenuItem(delta: number) {
    const enabledItems = Array.from(
      menuRef.current?.querySelectorAll<HTMLButtonElement>(
        'button[role="menuitem"]:not(:disabled)',
      ) ?? [],
    );
    if (enabledItems.length === 0) return;
    const currentIndex = enabledItems.findIndex(
      (item) => item === document.activeElement,
    );
    const nextIndex =
      currentIndex === -1
        ? 0
        : (currentIndex + delta + enabledItems.length) % enabledItems.length;
    enabledItems[nextIndex]?.focus();
  }

  return (
    <div
      ref={menuRef}
      role="menu"
      tabIndex={-1}
      aria-label={ariaLabel}
      data-testid={testId}
      className="fixed z-[60] min-w-56 rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--popover)/0.98)] p-1 text-sm text-[hsl(var(--popover-foreground))] shadow-2xl shadow-black/40 outline-none animate-[discrypt-scale-in_120ms_ease-out]"
      style={{ left: clamped.x, top: clamped.y }}
      onClick={(event) => event.stopPropagation()}
      onContextMenu={(event) => event.preventDefault()}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          onClose();
        }
        if (event.key === "ArrowDown") {
          event.preventDefault();
          focusMenuItem(1);
        }
        if (event.key === "ArrowUp") {
          event.preventDefault();
          focusMenuItem(-1);
        }
      }}
    >
      {items.map((item) => (
        <Button
          key={item.id}
          type="button"
          variant="ghost"
          role="menuitem"
          disabled={item.disabled}
          aria-disabled={item.disabled || undefined}
          className={cn(
            "flex h-auto w-full items-start justify-start gap-2 rounded-lg px-3 py-2 text-left hover:bg-[hsl(var(--accent))]",
            item.danger && "text-red-200 hover:text-red-100",
          )}
          onClick={() => {
            if (item.disabled || !item.onSelect) return;
            onClose();
            item.onSelect();
          }}
        >
          {item.icon ? <Icon className="mt-0.5">{item.icon}</Icon> : null}
          <span className="grid min-w-0 gap-0.5">
            <span>{item.label}</span>
            {item.description ? (
              <span className="text-xs leading-4 text-[hsl(var(--muted-foreground))]">
                {item.description}
              </span>
            ) : null}
          </span>
        </Button>
      ))}
    </div>
  );
}

function MemberPanel({
  group,
  members,
  localRole,
  open,
  pendingCount,
  onReviewPendingAdmissions,
  onPromote,
  onDemote,
  onRevoke,
  actionInFlight,
}: {
  group: GroupView | null;
  members: GroupMemberView[];
  localRole: string;
  open: boolean;
  pendingCount: number;
  onReviewPendingAdmissions: () => void;
  onPromote: (member: GroupMemberView) => void;
  onDemote: (member: GroupMemberView) => void;
  onRevoke: (member: GroupMemberView) => void;
  actionInFlight: string | null;
}) {
  const [menu, setMenu] = useState<{
    member: GroupMemberView;
    x: number;
    y: number;
  } | null>(null);

  const sortedMembers = [...members].sort((left, right) => {
    const roleDelta = roleRank(left.role) - roleRank(right.role);
    if (roleDelta !== 0) return roleDelta;
    const onlineDelta = Number(isPresenceOnline(right)) - Number(isPresenceOnline(left));
    if (onlineDelta !== 0) return onlineDelta;
    return left.display_name.localeCompare(right.display_name);
  });
  const section = (role: "owner" | "staff" | "member", title: string) =>
    sortedMembers.filter((member) => member.role === role && member.status !== "revoked");
  const revoked = sortedMembers.filter((member) => member.status === "revoked");

  return (
    <aside
      aria-label="Member panel"
      className={cn(
        "hidden h-dvh border-l border-[hsl(var(--border))] bg-[hsl(var(--card)/0.68)] backdrop-blur-xl transition-[opacity,transform] duration-200 lg:block",
        open ? "translate-x-0 opacity-100" : "pointer-events-none translate-x-full opacity-0",
      )}
    >
      <ScrollArea className="h-full">
        <div className="grid gap-4 p-4">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">
              Members
            </p>
            <h2 className="mt-1 text-lg font-semibold">{group?.name ?? "No group"}</h2>
            <p className="mt-1 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
              Role and presence are read from backend governance state with TTL-backed online status.
            </p>
          </div>
          {pendingCount > 0 && ["owner", "staff"].includes(localRole) ? (
            <Button
              type="button"
              variant="secondary"
              className="justify-between rounded-xl"
              onClick={onReviewPendingAdmissions}
            >
              Pending requests
              <Badge variant="warning">{pendingCount}</Badge>
            </Button>
          ) : null}
          <MemberSection
            title="Owner"
            members={section("owner", "Owner")}
            localRole={localRole}
            onContextMenu={setMenu}
          />
          <MemberSection
            title="Staff"
            members={section("staff", "Staff")}
            localRole={localRole}
            onContextMenu={setMenu}
          />
          <MemberSection
            title="Members"
            members={section("member", "Members")}
            localRole={localRole}
            onContextMenu={setMenu}
          />
          {revoked.length > 0 ? (
            <MemberSection
              title="Revoked"
              members={revoked}
              localRole={localRole}
              onContextMenu={setMenu}
            />
          ) : null}
        </div>
      </ScrollArea>
      <MemberContextMenu
        menu={menu}
        localRole={localRole}
        actionInFlight={actionInFlight}
        onClose={() => setMenu(null)}
        onPromote={onPromote}
        onDemote={onDemote}
        onRevoke={onRevoke}
      />
    </aside>
  );
}

function MemberSection({
  title,
  members,
  localRole,
  onContextMenu,
}: {
  title: string;
  members: GroupMemberView[];
  localRole: string;
  onContextMenu: (menu: {
    member: GroupMemberView;
    x: number;
    y: number;
  }) => void;
}) {
  return (
    <section className="grid gap-2">
      <div className="flex items-center justify-between gap-2">
        <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">
          {title}
        </p>
        <Badge variant="outline">{members.length}</Badge>
      </div>
      {members.length === 0 ? (
        <p className="rounded-xl border border-dashed border-[hsl(var(--border))] p-3 text-xs text-[hsl(var(--muted-foreground))]">
          No {title.toLowerCase()} yet.
        </p>
      ) : (
        members.map((member) => (
          <MemberRow
            key={member.member_id}
            member={member}
            onContextMenu={onContextMenu}
          />
        ))
      )}
    </section>
  );
}

function MemberRow({
  member,
  onContextMenu,
}: {
  member: GroupMemberView;
  onContextMenu: (menu: {
    member: GroupMemberView;
    x: number;
    y: number;
  }) => void;
}) {
  const online = isPresenceOnline(member);
  const status = member.status === "revoked" ? "revoked" : online ? "online" : "offline";
  return (
    <div
      className="group flex min-w-0 items-center gap-3 rounded-xl px-2 py-2 text-sm hover:bg-[hsl(var(--accent)/0.58)]"
      tabIndex={0}
      aria-label={`${member.display_name} member`}
      onContextMenu={(event) => {
        event.preventDefault();
        onContextMenu({ member, x: event.clientX, y: event.clientY });
      }}
      onKeyDown={(event) => {
        if (!isKeyboardContextMenu(event)) return;
        event.preventDefault();
        const point = contextMenuPointFromElement(event.currentTarget);
        onContextMenu({ member, ...point });
      }}
      title="Right-click for member actions"
    >
      <Avatar className="h-8 w-8">
        <AvatarFallback>{member.display_name.slice(0, 2).toUpperCase()}</AvatarFallback>
      </Avatar>
      <div className="min-w-0 flex-1">
        <p className="truncate font-medium">{member.display_name}</p>
        <p className="truncate text-[11px] text-[hsl(var(--muted-foreground))]">
          {member.role} · {status}
        </p>
      </div>
      <span
        aria-label={status}
        className={cn(
          "h-2.5 w-2.5 rounded-full",
          status === "online" && "bg-emerald-300 shadow-[0_0_0_3px_hsl(142_76%_36%/0.20)]",
          status === "offline" && "bg-[hsl(var(--muted-foreground)/0.45)]",
          status === "revoked" && "bg-red-300",
        )}
      />
    </div>
  );
}

function MemberContextMenu({
  menu,
  localRole,
  actionInFlight,
  onClose,
  onPromote,
  onDemote,
  onRevoke,
}: {
  menu: { member: GroupMemberView; x: number; y: number } | null;
  localRole: string;
  actionInFlight: string | null;
  onClose: () => void;
  onPromote: (member: GroupMemberView) => void;
  onDemote: (member: GroupMemberView) => void;
  onRevoke: (member: GroupMemberView) => void;
}) {
  if (!menu) return null;
  const { member } = menu;
  const actions: SharedContextMenuItem[] = [
    canPromoteFromUi(localRole, member)
      ? {
          id: `promote:${member.member_id}`,
          label: "Make staff",
          icon: "↑",
          onSelect: () => onPromote(member),
        }
      : null,
    canDemoteFromUi(localRole, member)
      ? {
          id: `demote:${member.member_id}`,
          label: "Demote to member",
          icon: "↓",
          onSelect: () => onDemote(member),
        }
      : null,
    canRevokeFromUi(localRole, member)
      ? {
          id: `revoke:${member.member_id}`,
          label: "Revoke access",
          icon: "×",
          onSelect: () => onRevoke(member),
          danger: true,
        }
      : null,
  ].filter(Boolean) as SharedContextMenuItem[];
  const items =
    actions.length > 0
      ? actions.map((action) => ({
          ...action,
          disabled: actionInFlight === action.id,
        }))
      : [
          {
            id: "member-authority",
            label: "No member actions available",
            icon: "ⓘ",
            description:
              "Role changes require backend-governed owner or staff authority.",
            disabled: true,
          },
        ];
  return (
    <SharedContextMenu
      ariaLabel={`${member.display_name} member actions`}
      position={menu}
      items={items}
      onClose={onClose}
      testId="member-context-menu"
    />
  );
}

function AdmissionRequestsPanel({
  group,
  localRole,
  requests,
  onApprove,
  onRefuse,
  actionInFlight,
}: {
  group: GroupView | null;
  localRole: string;
  requests: GroupAdmissionRequestView[];
  onApprove: (request: GroupAdmissionRequestView) => void;
  onRefuse: (request: GroupAdmissionRequestView) => void;
  actionInFlight: string | null;
}) {
  const canReview = ["owner", "staff"].includes(localRole);
  const pending = requests.filter((request) => request.status === "pending");
  const history = requests.filter((request) => request.status !== "pending");
  return (
    <Card className="flex h-full min-h-0 flex-col overflow-hidden">
      <CardHeader className="border-b border-[hsl(var(--border))]">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <CardTitle>Pending admission requests</CardTitle>
            <CardDescription>
              {group?.name ?? "Group"} · valid approvals create an OpenMLS Welcome before a request is marked approved.
            </CardDescription>
          </div>
          <Badge variant={pending.length ? "warning" : "secondary"}>
            {pending.length} pending
          </Badge>
        </div>
      </CardHeader>
      <ScrollArea className="min-h-0 flex-1">
        <CardContent className="grid gap-4 p-4">
          {!canReview ? (
            <EmptyState
              title="Owner/staff review required"
              copy="Members can see their own channels, but admission decisions are hidden unless backend role state authorizes review."
            />
          ) : pending.length === 0 ? (
            <EmptyState
              title="No pending requests"
              copy="Manual requests remain here until an owner or staff member approves or refuses them."
            />
          ) : (
            pending.map((request) => (
              <AdmissionRequestCard
                key={request.request_id}
                request={request}
                onApprove={onApprove}
                onRefuse={onRefuse}
                actionInFlight={actionInFlight}
              />
            ))
          )}
          {history.length > 0 ? (
            <section className="grid gap-2">
              <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">
                Decision history
              </p>
              {history.map((request) => (
                <div
                  key={request.request_id}
                  className="rounded-xl border border-[hsl(var(--border))] bg-black/10 p-3 text-sm"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <span className="font-medium">{request.display_name}</span>
                    <Badge variant={request.status === "approved" ? "success" : "secondary"}>
                      {request.status}
                    </Badge>
                  </div>
                  <p className="mt-1 text-xs text-[hsl(var(--muted-foreground))]">
                    decided {request.decided_at ?? "not recorded"} by {request.decided_by ?? "unknown"}
                  </p>
                </div>
              ))}
            </section>
          ) : null}
        </CardContent>
      </ScrollArea>
    </Card>
  );
}

function AdmissionRequestCard({
  request,
  onApprove,
  onRefuse,
  actionInFlight,
}: {
  request: GroupAdmissionRequestView;
  onApprove: (request: GroupAdmissionRequestView) => void;
  onRefuse: (request: GroupAdmissionRequestView) => void;
  actionInFlight: string | null;
}) {
  return (
    <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.24)] p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-base font-semibold">{request.display_name}</p>
          <p className="mt-1 text-sm text-[hsl(var(--muted-foreground))]">
            {request.device_name ?? "Unknown device"} · requested {request.requested_at}
          </p>
        </div>
        <Badge variant="warning">{request.status}</Badge>
      </div>
      <div className="mt-3 grid gap-2 text-xs text-[hsl(var(--muted-foreground))] md:grid-cols-2">
        <InfoRow title="Invite" copy={request.invite_id ?? "invite fingerprint unavailable"} />
        <InfoRow title="Policy epoch" copy={String(request.policy_epoch_at_request)} />
        <InfoRow title="Admission mode at request" copy={request.admission_mode_at_request ?? "not recorded"} />
        <InfoRow title="Key package bytes" copy={`${request.key_package.length} byte(s) stored`} />
      </div>
      <div className="mt-4 flex flex-wrap gap-2">
        <Button
          type="button"
          onClick={() => onApprove(request)}
          disabled={actionInFlight === `approve:${request.request_id}`}
        >
          Approve
        </Button>
        <Button
          type="button"
          variant="secondary"
          onClick={() => onRefuse(request)}
          disabled={actionInFlight === `refuse:${request.request_id}`}
        >
          Refuse
        </Button>
      </div>
    </div>
  );
}

function LauncherPanel({
  inviteValue,
  setInviteValue,
  groupName,
  setGroupName,
  contactName,
  setContactName,
  latestInvite,
  joinProgress,
  onJoin,
  onAcceptDmInvite,
  onStartDm,
  onCreateDmInvite,
  canCreateDmInvite,
  onCreateGroup,
}: {
  inviteValue: string;
  setInviteValue: (value: string) => void;
  groupName: string;
  setGroupName: (value: string) => void;
  contactName: string;
  setContactName: (value: string) => void;
  latestInvite: InviteView | null;
  joinProgress: JoinProgressStepView[];
  onJoin: () => void;
  onAcceptDmInvite: () => void;
  onStartDm: () => void;
  onCreateDmInvite: () => void;
  canCreateDmInvite: boolean;
  onCreateGroup: () => void;
}) {
  return (
    <div className="grid gap-4 py-5">
      <Card>
        <CardHeader>
          <CardTitle>Join a group or direct message</CardTitle>
          <CardDescription>
            Paste the invite URL or code you received, name it locally, then
            open the group or DM.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-4">
          <Label className="grid gap-2">
            Invite URL or code
            <Input
              value={inviteValue}
              placeholder="Paste invite URL or code"
              onChange={(event) => setInviteValue(event.target.value)}
            />
          </Label>
          <Label className="grid gap-2">
            Local label
            <Input
              value={groupName}
              placeholder="Group or contact name"
              onChange={(event) => setGroupName(event.target.value)}
            />
          </Label>
          <div className="flex flex-wrap gap-2">
            <Button onClick={onJoin} disabled={!inviteValue.trim()}>
              Join/open group
            </Button>
            <Button
              variant="secondary"
              onClick={onAcceptDmInvite}
              disabled={!inviteValue.trim()}
            >
              Accept/open DM invite
            </Button>
          </div>
          <JoinProgressCard steps={joinProgress} />
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Start a direct message</CardTitle>
          <CardDescription>
            Open a private conversation with one person.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-4">
          <Label className="grid gap-2">
            Contact name
            <Input
              value={contactName}
              placeholder="Contact name"
              onChange={(event) => setContactName(event.target.value)}
            />
          </Label>
          <Button
            variant="outline"
            onClick={onStartDm}
            disabled={!contactName.trim()}
          >
            <Icon>+</Icon>Start direct message
          </Button>
          <Button
            variant="secondary"
            onClick={onCreateDmInvite}
            disabled={!canCreateDmInvite}
          >
            Create DM invite for current direct message
          </Button>
          {latestInvite ? (
            <InviteDetailCard invite={latestInvite} />
          ) : null}
        </CardContent>
      </Card>
      <Card className="border-[hsl(var(--primary)/0.28)] bg-[hsl(var(--primary)/0.08)]">
        <CardHeader>
          <CardTitle>Create a new group</CardTitle>
          <CardDescription>
            Start a private workspace with its own channels, signaling policy,
            invites, and voice rooms.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button className="w-full justify-center" onClick={onCreateGroup}>
            <Icon>+</Icon>Create a new group
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}

function GroupInvitePanel({
  group,
  latestInvite,
  expiryDays,
  setExpiryDays,
  maxUses,
  setMaxUses,
  revocationState,
  setRevocationState,
  passwordEnabled,
  setPasswordEnabled,
  password,
  setPassword,
  onCreateInvite,
}: {
  group: GroupView | null;
  latestInvite: InviteView | null;
  expiryDays: string;
  setExpiryDays: (value: string) => void;
  maxUses: string;
  setMaxUses: (value: string) => void;
  revocationState: string;
  setRevocationState: (value: string) => void;
  passwordEnabled: boolean;
  setPasswordEnabled: (value: boolean) => void;
  password: string;
  setPassword: (value: string) => void;
  onCreateInvite: () => void;
}) {
  const selectedProfile = group?.connectivity?.signaling_profiles[0] ?? null;
  const passwordReady = !passwordEnabled || password.trim().length >= 8;
  const adapterSnapshot = selectedProfile
    ? `${selectedProfile.adapter_kind} · ${selectedProfile.endpoints[0] ?? "endpoint missing"}`
    : "No signaling profile selected";
  return (
    <div className="grid gap-4 py-5">
      <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.22)] p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <p className="text-sm font-semibold">{group?.name ?? "No group selected"}</p>
            <p className="mt-1 text-sm text-[hsl(var(--muted-foreground))]">
              {selectedProfile
                ? `${selectedProfile.adapter_kind} · ${selectedProfile.endpoints[0] ?? "endpoint missing"}`
                : "Select a group with a configured signaling profile."}
            </p>
          </div>
          <Badge variant={selectedProfile ? "success" : "warning"}>
            {selectedProfile ? "signaling configured" : "not ready"}
          </Badge>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Create invite</CardTitle>
          <CardDescription>
            Configure expiry, max uses, and admission options before generating the share link.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-4">
          <div className="grid gap-4 md:grid-cols-2">
            <Label className="grid gap-2">
              Expires after
              <Select value={expiryDays} onValueChange={setExpiryDays} aria-label="Invite expiry">
                <SelectItem value="1">1 day</SelectItem>
                <SelectItem value="7">7 days</SelectItem>
                <SelectItem value="30">30 days</SelectItem>
                <SelectItem value="90">90 days</SelectItem>
              </Select>
            </Label>
            <Label className="grid gap-2">
              Maximum uses
              <Input
                inputMode="numeric"
                min={1}
                max={100}
                value={maxUses}
                onChange={(event) => setMaxUses(event.target.value.replace(/[^0-9]/g, ""))}
                placeholder="5"
              />
            </Label>
          </div>

          <div className="grid gap-4 md:grid-cols-2">
            <Label className="grid gap-2">
              Revocation state
              <Select
                value={revocationState}
                onValueChange={setRevocationState}
                aria-label="Invite revocation state"
              >
                <SelectItem value="active_revocable">
                  Active, owner-revocable
                </SelectItem>
              </Select>
            </Label>
            <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.18)] p-4">
              <p className="text-sm font-medium">Adapter snapshot</p>
              <p
                className="mt-1 truncate text-sm text-[hsl(var(--muted-foreground))]"
                title={adapterSnapshot}
              >
                {adapterSnapshot}
              </p>
              <p className="mt-2 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
                This signed snapshot is copied from the selected group policy when the invite is issued.
              </p>
            </div>
          </div>

          <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.18)] p-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <p className="text-sm font-medium">Password admission</p>
                <p className="mt-1 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
                  Optional admission password. The raw password is never embedded in the pasted invite.
                </p>
              </div>
              <Switch
                checked={passwordEnabled}
                onCheckedChange={setPasswordEnabled}
                aria-label="Require invite password"
              />
            </div>
            {passwordEnabled ? (
              <Label className="mt-4 grid gap-2">
                Invite password
                <Input
                  type="password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                  placeholder="At least 8 characters"
                />
              </Label>
            ) : null}
          </div>

          <Button onClick={onCreateInvite} disabled={!group || !passwordReady || !maxUses.trim()}>
            Create invite for {group?.name ?? "selected group"}
          </Button>
          {latestInvite ? (
            <InviteDetailCard invite={latestInvite} />
          ) : (
            <div className="rounded-2xl border border-dashed border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.22)] p-5 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
              No invite generated in this modal yet.
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function GroupConfigPanel({
  group,
  signalingAdapter,
  setSignalingAdapter,
  signalingEndpoint,
  setSignalingEndpoint,
  iceStunServers,
  setIceStunServers,
  iceTurnServers,
  setIceTurnServers,
  admissionMode,
  setAdmissionMode,
  onSave,
}: {
  group: GroupView | null;
  signalingAdapter: SignalingAdapterKind;
  setSignalingAdapter: (value: string) => void;
  signalingEndpoint: string;
  setSignalingEndpoint: (value: string) => void;
  iceStunServers: string;
  setIceStunServers: (value: string) => void;
  iceTurnServers: string;
  setIceTurnServers: (value: string) => void;
  admissionMode: GroupAdmissionModeView;
  setAdmissionMode: (value: GroupAdmissionModeView) => void;
  onSave: () => void;
}) {
  return (
    <div className="grid gap-4 py-5">
      <Card>
        <CardHeader>
          <CardTitle>{group?.name ?? "Group"} connectivity</CardTitle>
          <CardDescription>
            Group-level defaults are used for text, voice, and invites until a
            narrower channel policy overrides them.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-4">
          <Label className="grid gap-2">
            Signaling adapter
            <Select
              value={signalingAdapter}
              onValueChange={setSignalingAdapter}
            >
              <SelectItem value="nostr">Nostr relay</SelectItem>
              <SelectItem value="mqtt">MQTT broker</SelectItem>
              <SelectItem value="ipfs_pubsub">IPFS/libp2p pubsub</SelectItem>
              <SelectItem value="discrypt_quic_rendezvous">
                Discrypt QUIC rendezvous
              </SelectItem>
            </Select>
          </Label>
          <Label className="grid gap-2">
            Signaling endpoint
            <Input
              value={signalingEndpoint}
              placeholder="wss://relay.example or mqtt://broker.example"
              onChange={(event) => setSignalingEndpoint(event.target.value)}
            />
          </Label>
          <div className="grid gap-4 md:grid-cols-2">
            <Label className="grid gap-2">
              STUN servers
              <Input
                value={iceStunServers}
                placeholder="stun:stun.l.google.com:19302"
                onChange={(event) => setIceStunServers(event.target.value)}
              />
            </Label>
            <Label className="grid gap-2">
              TURN servers
              <Input
                value={iceTurnServers}
                placeholder="turn:host?user=name&credential=secret"
                onChange={(event) => setIceTurnServers(event.target.value)}
              />
            </Label>
          </div>
          <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.22)] p-4">
            <Label className="grid gap-2">
              Invite admission
              <Select
                aria-label="Group admission mode"
                value={admissionMode}
                onValueChange={(value) => setAdmissionMode(value as GroupAdmissionModeView)}
              >
                <SelectItem value="manual_approval">Manual approval</SelectItem>
                <SelectItem value="automatic_when_authorized_online">
                  Automatic when owner/staff is online
                </SelectItem>
              </Select>
            </Label>
            <p className="mt-2 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
              Existing manual pending requests stay pending after switching to automatic mode.
            </p>
          </div>
          <Button onClick={onSave} disabled={!group}>
            Save group configuration
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}

function JoinProgressCard({ steps }: { steps: JoinProgressStepView[] }) {
  const visibleSteps = steps.length
    ? steps
    : [
        {
          key: "invite_parsed",
          label: "Invite parsed",
          status: "waiting-for-invite",
          detail: "Paste an invite before receiver-side join progress can start",
        },
        {
          key: "rendezvous",
          label: "Rendezvous link",
          status: "blocked",
          detail:
            "Progress updates when an authenticated rendezvous exchange is available.",
        },
      ];
  return (
    <Card className="border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.22)] shadow-none">
      <CardHeader className="pb-3">
        <CardTitle className="text-base">Group join progress</CardTitle>
        <CardDescription>
          Receiver-side invite parsing, authorization, Welcome, MLS, and route
          stages are evidence-gated by command state.
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-3">
        {visibleSteps.map((step, index) => (
          <div
            key={step.key}
            className="grid grid-cols-[28px_minmax(0,1fr)] gap-3 rounded-xl border border-[hsl(var(--border))] bg-black/10 p-3"
          >
            <div
              className={cn(
                "grid h-7 w-7 place-items-center rounded-full border text-xs font-semibold",
                step.status === "complete"
                  ? "border-emerald-300/40 bg-emerald-300/15 text-emerald-100"
                  : "border-[hsl(var(--border))] bg-[hsl(var(--secondary))] text-[hsl(var(--muted-foreground))]",
              )}
            >
              {step.status === "complete" ? "✓" : index + 1}
            </div>
            <div className="min-w-0">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <p className="font-medium">{step.label}</p>
                <Badge variant={joinProgressBadgeVariant(step.status)}>
                  {step.status}
                </Badge>
              </div>
              <p className="mt-1 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                {step.detail}
              </p>
            </div>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}

function joinProgressBadgeVariant(
  status: string,
): React.ComponentProps<typeof Badge>["variant"] {
  if (status === "complete" || status === "local-group-open") return "success";
  if (status.startsWith("waiting") || status.startsWith("pending")) {
    return "warning";
  }
  return "secondary";
}

function InviteDetailCard({ invite }: { invite: InviteView }) {
  const [copied, setCopied] = useState(false);
  const maxUsesNumber = Number(invite.max_use.match(/\d+/)?.[0] ?? 0);
  const remainingUses = maxUsesNumber
    ? Math.max(0, maxUsesNumber - invite.uses)
    : null;
  const selectedProfile = invite.signaling_profiles[0] ?? null;
  const adapterSnapshot = selectedProfile
    ? `${selectedProfile.adapter_kind} · ${selectedProfile.endpoints[0] ?? "endpoint missing"}`
    : invite.signaling_endpoint || invite.invite_kind;
  const revocationState = invite.revoked
    ? "Revoked locally"
    : invite.revocation_policy?.revocable
      ? "Active, owner-revocable"
      : "Active";
  const passwordGate = invite.password_policy?.required
    ? `${invite.password_policy.protocol}; offline verifier not embedded`
    : "Not required";
  const admissionSnapshot = invite.admission_snapshot
    ? `${invite.admission_snapshot.admission_mode}; Welcome required`
    : "Authorized MLS Welcome required";
  const copyInvite = async () => {
    await navigator.clipboard?.writeText(invite.code);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };
  return (
    <Card className="border-emerald-300/25 bg-emerald-300/8 text-emerald-50">
      <CardHeader className="gap-3 pb-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <CardTitle className="text-base text-emerald-50">
              Invite ready
            </CardTitle>
            <CardDescription className="text-emerald-50/75">
              Share this link with the person joining this group.
            </CardDescription>
          </div>
          <div className="flex flex-wrap gap-2">
            <Badge variant={invite.revoked ? "warning" : "success"}>
              {invite.revoked ? "revoked" : "usable"}
            </Badge>
            <Badge variant="secondary">uses {invite.uses}</Badge>
          </div>
        </div>
        <div className="grid gap-2 rounded-xl border border-emerald-300/20 bg-black/20 p-3">
          <textarea
            readOnly
            value={invite.code}
            onFocus={(event) => event.currentTarget.select()}
            onClick={(event) => event.currentTarget.select()}
            aria-label="Invite link"
            data-testid="invite-link"
            className="h-20 resize-none overflow-auto bg-transparent font-mono text-xs leading-5 text-emerald-50/90 outline-none"
          />
          <div className="flex items-center justify-between gap-3">
            <span className="truncate text-xs text-emerald-50/70">
              Click the field to select the full invite.
            </span>
            <Button type="button" size="sm" variant="secondary" onClick={copyInvite}>
              {copied ? "Copied" : "Copy invite"}
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="grid gap-3">
        <div className="grid gap-3 md:grid-cols-2">
          <InviteFact
            label="Adapter"
            value={selectedProfile?.adapter_kind ?? invite.invite_kind}
          />
          <InviteFact
            label="Adapter snapshot"
            value={adapterSnapshot}
          />
          <InviteFact
            label="Signaling endpoint"
            value={invite.signaling_endpoint || selectedProfile?.endpoints[0] || "not provided"}
          />
          <InviteFact
            label="Endpoint policy"
            value={invite.endpoint_policy || "unknown"}
          />
          <InviteFact
            label="Expires"
            value={invite.expires_at || invite.expires || "not provided"}
          />
          <InviteFact label="Max uses" value={invite.max_use} />
          <InviteFact
            label="Remaining local uses"
            value={remainingUses === null ? "not parsed" : String(remainingUses)}
          />
          <InviteFact label="Revocation state" value={revocationState} />
          <InviteFact label="Password gate" value={passwordGate} />
          <InviteFact
            label="Admission"
            value={invite.admission_copy || "Authorized MLS welcome required"}
          />
          <InviteFact
            label="Admission snapshot"
            value={admissionSnapshot}
          />
          <InviteFact
            label="STUN/TURN"
            value={`${invite.ice_stun_servers.length} STUN · ${invite.ice_turn_servers.length} TURN`}
          />
        </div>
      </CardContent>
    </Card>
  );
}

function InviteFact({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="min-w-0 rounded-xl border border-emerald-300/15 bg-black/15 p-3">
      <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-emerald-50/60">
        {label}
      </p>
      <p
        className={cn(
          "mt-1 break-words text-sm leading-6 text-emerald-50/90",
          mono && "font-mono text-xs",
        )}
      >
        {value}
      </p>
    </div>
  );
}

function CreateGroupPanel({
  snapshot,
  groupName,
  setGroupName,
  signalingAdapter,
  setSignalingAdapter,
  signalingEndpoint,
  setSignalingEndpoint,
  iceStunServers,
  setIceStunServers,
  iceTurnServers,
  setIceTurnServers,
  admissionMode,
  setAdmissionMode,
  onCreate,
}: {
  snapshot: AppSnapshot;
  groupName: string;
  setGroupName: (value: string) => void;
  signalingAdapter: SignalingAdapterKind;
  setSignalingAdapter: (value: string) => void;
  signalingEndpoint: string;
  setSignalingEndpoint: (value: string) => void;
  iceStunServers: string;
  setIceStunServers: (value: string) => void;
  iceTurnServers: string;
  setIceTurnServers: (value: string) => void;
  admissionMode: GroupAdmissionModeView;
  setAdmissionMode: (value: GroupAdmissionModeView) => void;
  onCreate: () => void;
}) {
  return (
    <div className="grid gap-4 py-5 xl:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]">
      <Card>
        <CardHeader>
          <CardTitle>Create a group</CardTitle>
          <CardDescription>
            Creates a persisted group with default text and voice rooms so the
            workspace is immediately usable.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Label className="grid gap-2">
            Group name
            <Input
              value={groupName}
              onChange={(event) => setGroupName(event.target.value)}
            />
          </Label>
          <div className="mt-4 rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.24)] p-4">
            <Label className="grid gap-2">
              Invite admission
              <Select
                aria-label="Invite admission mode"
                value={admissionMode}
                onValueChange={(value) => setAdmissionMode(value as GroupAdmissionModeView)}
              >
                <SelectItem value="manual_approval">Manual approval</SelectItem>
                <SelectItem value="automatic_when_authorized_online">
                  Automatic when owner/staff is online
                </SelectItem>
              </Select>
            </Label>
            <p className="mt-2 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
              Manual is safest for private groups. Switching modes later will not retroactively approve pending manual requests.
            </p>
          </div>
          <div className="mt-4 grid gap-3">
            <Label className="grid gap-2">
              Signaling adapter
              <Select
                aria-label="Signaling adapter"
                value={signalingAdapter}
                onValueChange={setSignalingAdapter}
              >
                <SelectItem value="nostr">Nostr relay</SelectItem>
                <SelectItem value="mqtt">MQTT broker</SelectItem>
                <SelectItem value="ipfs_pubsub">IPFS/libp2p pubsub</SelectItem>
                <SelectItem value="discrypt_quic_rendezvous">
                  Discrypt rendezvous
                </SelectItem>
              </Select>
            </Label>
            <Label className="grid gap-2">
              Signaling endpoint
              <Input
                aria-label="Signaling endpoint"
                value={signalingEndpoint}
                onChange={(event) => setSignalingEndpoint(event.target.value)}
              />
            </Label>
            <Label className="grid gap-2">
              STUN servers
              <Input
                aria-label="STUN servers"
                value={iceStunServers}
                onChange={(event) => setIceStunServers(event.target.value)}
              />
            </Label>
            <Label className="grid gap-2">
              TURN servers (redacted credentials)
              <Input
                aria-label="TURN servers"
                value={iceTurnServers}
                placeholder="turns:turn.example.com:5349"
                onChange={(event) => setIceTurnServers(event.target.value)}
              />
            </Label>
          </div>
          <Button
            className="mt-5 w-full"
            onClick={onCreate}
            disabled={!groupName.trim()}
          >
            Create group
          </Button>
        </CardContent>
      </Card>
      <div className="grid gap-3">
        <InfoRow
          title="Default text channel"
          copy="#general is created for messages."
        />
        <InfoRow
          title="Default voice room"
          copy="Voice Lobby is created with the group and becomes active when you join."
        />
        <InfoRow
          title="Retention warning"
          copy={snapshot.retention.unlimited_warning}
        />
      </div>
    </div>
  );
}

function ConnectivitySettingsPanel({
  policy,
  signalingAdapter,
  setSignalingAdapter,
  signalingEndpoint,
  setSignalingEndpoint,
  iceStunServers,
  setIceStunServers,
  iceTurnServers,
  setIceTurnServers,
  onSaveAppDefaults,
  onSaveGroup,
  onSaveChannel,
  onSaveDm,
  showAdvancedStatus = false,
}: {
  policy: ConnectivityPolicyView;
  signalingAdapter: SignalingAdapterKind;
  setSignalingAdapter: (value: string) => void;
  signalingEndpoint: string;
  setSignalingEndpoint: (value: string) => void;
  iceStunServers: string;
  setIceStunServers: (value: string) => void;
  iceTurnServers: string;
  setIceTurnServers: (value: string) => void;
  onSaveAppDefaults: () => void;
  onSaveGroup: (() => void) | null;
  onSaveChannel: (() => void) | null;
  onSaveDm: (() => void) | null;
  showAdvancedStatus?: boolean;
}) {
  const currentProfile = policy.signaling_profiles[0];
  return (
    <Card className="mt-4">
      <CardHeader>
        <CardTitle>Signaling and ICE settings</CardTitle>
        <CardDescription>
          Configure production provider discovery and NAT traversal without
          exposing room names, raw SDP, ICE secrets, or manual peer pairing.
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(280px,0.8fr)]">
        <div className="grid gap-3">
          <Label className="grid gap-2">
            Signaling adapter
            <Select
              aria-label="Provider adapter override"
              value={signalingAdapter}
              onValueChange={setSignalingAdapter}
            >
              <SelectItem value="nostr">Nostr relay</SelectItem>
              <SelectItem value="mqtt">MQTT broker</SelectItem>
              <SelectItem value="ipfs_pubsub">IPFS/libp2p pubsub</SelectItem>
              <SelectItem value="discrypt_quic_rendezvous">
                Discrypt QUIC rendezvous
              </SelectItem>
            </Select>
          </Label>
          <Label className="grid gap-2">
            Signaling endpoint
            <Input
              aria-label="Provider endpoint override"
              value={signalingEndpoint}
              onChange={(event) => setSignalingEndpoint(event.target.value)}
            />
          </Label>
          <Label className="grid gap-2">
            STUN servers
            <Input
              aria-label="Provider STUN overrides"
              value={iceStunServers}
              onChange={(event) => setIceStunServers(event.target.value)}
            />
          </Label>
          <Label className="grid gap-2">
            TURN servers
            <Input
              aria-label="Provider TURN overrides"
              value={iceTurnServers}
              placeholder="turns:turn.example.com:5349"
              onChange={(event) => setIceTurnServers(event.target.value)}
            />
          </Label>
          <div className="flex flex-wrap gap-2">
            <Button size="sm" onClick={onSaveAppDefaults}>
              Save as app defaults
            </Button>
            {onSaveGroup ? (
              <Button size="sm" variant="outline" onClick={onSaveGroup}>
                Save for group
              </Button>
            ) : null}
            {onSaveChannel ? (
              <Button size="sm" variant="outline" onClick={onSaveChannel}>
                Save for channel
              </Button>
            ) : null}
            {onSaveDm ? (
              <Button size="sm" variant="outline" onClick={onSaveDm}>
                Save for DM
              </Button>
            ) : null}
          </div>
        </div>
        <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.32)] p-4 text-sm">
          <p className="font-semibold">Active signed policy</p>
          <dl className="mt-3 grid gap-2 text-xs text-[hsl(var(--muted-foreground))]">
            <div>
              <dt className="uppercase tracking-[0.14em]">Scope</dt>
              <dd className="break-all font-mono">
                {policy.scope_id_commitment}
              </dd>
            </div>
            <div>
              <dt className="uppercase tracking-[0.14em]">Adapter</dt>
              <dd>{currentProfile?.adapter_kind ?? "not configured"}</dd>
            </div>
            <div>
              <dt className="uppercase tracking-[0.14em]">Endpoint</dt>
              <dd className="break-all">
                {currentProfile?.endpoints[0] ?? "none"}
              </dd>
            </div>
            <div>
              <dt className="uppercase tracking-[0.14em]">ICE</dt>
              <dd>
                {policy.ice_stun_servers.length} STUN /{" "}
                {policy.ice_turn_servers.length} TURN endpoint(s)
              </dd>
            </div>
          </dl>
          <p className="mt-3 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
            {policy.privacy_label}
          </p>
          {showAdvancedStatus ? (
            <div className="mt-3 grid gap-2 rounded-xl border border-[hsl(var(--border))] bg-black/15 p-3 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
              <p className="font-semibold text-[hsl(var(--foreground))]">
                TURN credential gate
              </p>
              <p>{turnCredentialGateCopy(policy)}</p>
              <p>
                Provider fallback states come from backend diagnostics and stay
                separate from the default production controls.
              </p>
            </div>
          ) : null}
        </div>
      </CardContent>
    </Card>
  );
}

function ChannelPanel({
  group,
  activeChannel,
  messages,
  textStateLegend,
  draftMessage,
  setDraftMessage,
  onOpenCreateChannel,
  onSendMessage,
  transportProof,
  setTransportProof,
  diagnosticsEnabled,
  admissionPending = false,
}: {
  group: GroupView | null;
  activeChannel: ChannelStateView | null;
  messages: AppMessageView[];
  textStateLegend: TextStateView[];
  draftMessage: string;
  setDraftMessage: (value: string) => void;
  onOpenCreateChannel: () => void;
  onSendMessage: () => void;
  transportProof: boolean;
  setTransportProof: (value: boolean) => void;
  diagnosticsEnabled: boolean;
  admissionPending?: boolean;
}) {
  const visibleMessages = activeChannel
    ? messages.filter(
        (message) => message.target.channel_id === activeChannel.channel_id,
      )
    : [];
  if (!activeChannel) {
    return (
      <Card className="flex h-full min-h-0 flex-col">
        <CardHeader className="border-b border-[hsl(var(--border))]">
          <CardTitle>{group ? "No text channel selected" : "Choose a group"}</CardTitle>
          <CardDescription>
            {group
              ? "Use the + next to Text channels in the sidebar, or pick an existing channel."
              : "Create or join a group to start a private workspace."}
          </CardDescription>
        </CardHeader>
        <CardContent className="grid flex-1 place-items-center p-6">
          <EmptyState
            title={group ? "Create a channel" : "No active group"}
            copy={
              group
                ? "The main chat stays reserved for conversation; channel setup lives in the sidebar."
                : "Use the server rail + button to create or join a group."
            }
          />
        </CardContent>
      </Card>
    );
  }
  return (
    <div className="h-full min-h-0">
      <Timeline
        title={
          activeChannel.name.startsWith("#")
            ? activeChannel.name
            : `# ${activeChannel.name}`
        }
        description={group ? group.name : "Private workspace"}
        messages={visibleMessages}
        textStateLegend={textStateLegend}
        draftMessage={draftMessage}
        setDraftMessage={setDraftMessage}
        sendLabel="Send message"
        onSend={onSendMessage}
        disabled={admissionPending}
        composerNotice={admissionPending ? "Waiting for owner/staff approval before protected messages can be sent." : null}
        transportProof={transportProof}
        setTransportProof={setTransportProof}
        diagnosticsEnabled={diagnosticsEnabled}
      />
    </div>
  );
}

function Timeline({
  title,
  description,
  messages,
  textStateLegend,
  draftMessage,
  setDraftMessage,
  sendLabel,
  onSend,
  disabled,
  transportProof,
  setTransportProof,
  diagnosticsEnabled,
  composerNotice = null,
}: {
  title: string;
  description: string;
  messages: AppMessageView[];
  textStateLegend: TextStateView[];
  draftMessage: string;
  setDraftMessage: (value: string) => void;
  sendLabel: string;
  onSend: () => void;
  disabled?: boolean;
  transportProof: boolean;
  setTransportProof: (value: boolean) => void;
  diagnosticsEnabled: boolean;
  composerNotice?: string | null;
}) {
  return (
    <Card
      data-testid="message-timeline"
      className="flex h-full min-h-0 flex-col overflow-hidden border-[hsl(var(--border)/0.74)] bg-[hsl(var(--card)/0.54)] shadow-none"
    >
      <CardHeader className="border-b border-[hsl(var(--border))] px-4 py-3">
        <CardTitle className="text-lg">{title}</CardTitle>
        <CardDescription className="line-clamp-1">{description}</CardDescription>
      </CardHeader>
      {diagnosticsEnabled ? <TextStateLegend states={textStateLegend} /> : null}
      <ScrollArea data-testid="message-scroll" className="min-h-0 flex-1">
        <div className="py-3">
          {messages.length === 0 ? (
            <div className="px-4">
              <EmptyState title="No messages yet" copy="Send the first local message. It will persist through reloads." />
            </div>
          ) : (
            messages.map((message) => (
              <MessageRow key={message.message_id} message={message} diagnosticsEnabled={diagnosticsEnabled} />
            ))
          )}
        </div>
      </ScrollArea>
      <div className="border-t border-[hsl(var(--border))] p-3">
        <div className="flex items-center gap-2 rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.36)] px-3 py-2">
          <Icon>+</Icon>
          <Input
            aria-label="Message"
            value={draftMessage}
            onChange={(event) => setDraftMessage(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey && draftMessage.trim()) {
                event.preventDefault();
                onSend();
              }
            }}
            placeholder="Send a message"
            disabled={disabled}
            className="border-0 bg-transparent px-0 shadow-none focus-visible:ring-0"
          />
          <Button type="button" size="icon" aria-label={sendLabel} title={sendLabel} onClick={onSend} disabled={disabled || !draftMessage.trim()} className="h-9 w-9 rounded-xl">
            <Icon>➤</Icon>
          </Button>
        </div>
        {composerNotice ? (
          <p className="mt-2 text-xs text-[hsl(var(--muted-foreground))]">{composerNotice}</p>
        ) : null}
        {diagnosticsEnabled ? (
          <div className="mt-2 flex justify-end">
            <Label className="flex items-center gap-2 text-xs text-[hsl(var(--muted-foreground))]">
              <Switch checked={transportProof} onCheckedChange={setTransportProof} disabled={disabled} />
              Verify backend-state provider-signaled WebRTC transport for this send
            </Label>
          </div>
        ) : null}
      </div>
    </Card>
  );
}

function TextStateLegend({ states }: { states: TextStateView[] }) {
  if (states.length === 0) return null;
  return (
    <div className="border-b border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.18)] p-3">
      <div
        className="flex gap-2 overflow-x-auto pb-1"
        aria-label="Text message states"
      >
        {states.map((state) => (
          <div
            key={state.key}
            className="min-w-44 rounded-xl border border-[hsl(var(--border))] bg-black/10 p-2"
          >
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs font-semibold">{state.label}</span>
              <Badge variant={messageStateBadgeVariant(state.key)}>
                {state.status}
              </Badge>
            </div>
            <p className="mt-1 line-clamp-2 text-[11px] leading-4 text-[hsl(var(--muted-foreground))]">
              {state.detail}
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}

function messageStateBadgeVariant(
  stateKey: string,
): React.ComponentProps<typeof Badge>["variant"] {
  if (["sent_local", "received", "transport_probe_verified"].includes(stateKey))
    return "success";
  if (
    [
      "pending",
      "locked",
      "peer_receipt",
      "transport_probe_unavailable",
    ].includes(stateKey)
  )
    return "warning";
  if (["failed", "shredded", "transport_probe_failed"].includes(stateKey))
    return "secondary";
  return "outline";
}

function messageStateIcon(stateKey: string): string {
  if (["failed", "shredded", "transport_probe_failed"].includes(stateKey)) return "!";
  if (["pending", "locked", "peer_receipt", "transport_probe_unavailable"].includes(stateKey)) return "…";
  return "✓";
}

function messageStateTone(stateKey: string): string {
  if (["failed", "shredded", "transport_probe_failed"].includes(stateKey))
    return "border-[hsl(var(--destructive)/0.58)] bg-[hsl(var(--destructive)/0.12)] text-[hsl(var(--destructive-foreground))]";
  if (["pending", "locked", "transport_probe_unavailable"].includes(stateKey))
    return "border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.58)] text-[hsl(var(--muted-foreground))]";
  if (stateKey === "peer_receipt")
    return "border-[hsl(var(--primary)/0.5)] bg-[hsl(var(--primary)/0.12)] text-[hsl(var(--accent-foreground))]";
  return "border-[hsl(var(--primary)/0.42)] bg-[hsl(var(--primary)/0.08)] text-[hsl(var(--accent-foreground))]";
}

function MessageRow({
  message,
  diagnosticsEnabled,
}: {
  message: AppMessageView;
  diagnosticsEnabled: boolean;
}) {
  const statusTitle = `${message.state_label}: ${message.state_detail || message.status}`;
  return (
    <article
      data-testid="message-row"
      data-message-state={message.state_key}
      className="group grid grid-cols-[2.25rem_minmax(0,1fr)_auto] gap-3 px-4 py-2.5 transition-colors hover:bg-[hsl(var(--secondary)/0.32)]"
    >
      <Avatar className="mt-0.5 h-9 w-9">
        <AvatarFallback>{message.author.slice(0, 2).toUpperCase()}</AvatarFallback>
      </Avatar>
      <div className="min-w-0">
        <div className="flex flex-wrap items-baseline gap-2">
          <span className="font-medium text-[hsl(var(--foreground))]">{message.author}</span>
          <time className="text-[11px] text-[hsl(var(--muted-foreground))]">{message.sent_at}</time>
        </div>
        <p className="whitespace-pre-wrap break-words text-sm leading-6 text-[hsl(var(--foreground)/0.92)]">{message.body}</p>
        {diagnosticsEnabled ? (
          <p className="mt-1 text-[11px] leading-5 text-[hsl(var(--muted-foreground))]">{message.state_detail}</p>
        ) : null}
      </div>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            data-testid="message-delivery-status"
            className={cn(
              "mt-1 grid h-7 w-7 place-items-center rounded-full border text-xs font-semibold opacity-75 transition-opacity focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--card))] group-hover:opacity-100",
              messageStateTone(message.state_key),
            )}
            aria-label={statusTitle}
          >
            {messageStateIcon(message.state_key)}
          </Button>
        </TooltipTrigger>
        <TooltipContent side="left">{statusTitle}</TooltipContent>
      </Tooltip>
    </article>
  );
}

function RemoteAudioAttachment({
  participant,
  src,
  stream,
  volumePercent,
  outputDeviceId,
}: {
  participant: VoiceParticipant;
  src?: string | null;
  stream?: MediaStream | null;
  volumePercent?: number;
  outputDeviceId?: string | null;
}) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const volume = volumePercent ?? participant.volume;
  useEffect(() => {
    if (audioRef.current) {
      audioRef.current.volume = Math.max(0, Math.min(1, volume / 100));
    }
  }, [volume]);
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;
    const playbackStream = isUsableMediaStream(stream) ? stream : null;
    audio.srcObject = playbackStream;
    return () => {
      if (audio.srcObject === playbackStream) {
        audio.srcObject = null;
      }
    };
  }, [stream]);
  useEffect(() => {
    const audio = audioRef.current as
      | (HTMLAudioElement & { setSinkId?: (sinkId: string) => Promise<void> })
      | null;
    if (!audio?.setSinkId || !outputDeviceId) return;
    const sinkId = outputDeviceId === "default" ? "" : outputDeviceId;
    void audio.setSinkId(sinkId).catch((error: unknown) => {
      void error;
    });
  }, [outputDeviceId]);
  return (
    <audio
      ref={audioRef}
      aria-label={`${participant.name} remote audio`}
      data-testid="voice-remote-audio"
      autoPlay
      playsInline
      src={isUsableMediaStream(stream) ? undefined : (src ?? undefined)}
    />
  );
}

function DiagnosticsSheet({
  snapshot,
  appState,
  participants,
  themeLabel,
  verifyMessage,
  onVerifySafetyNumber,
}: {
  snapshot: AppSnapshot;
  appState: AppState;
  participants: VoiceParticipant[];
  themeLabel: string;
  verifyMessage: string | null;
  onVerifySafetyNumber: () => void;
}) {
  const latestEvents = appState.events.slice(-6).reverse();
  const speaking = participants.filter(
    (participant) => participant.speaking && !participant.muted,
  ).length;
  const [exportStatus, setExportStatus] = useState<string>("");
  const copyDiagnostics = async () => {
    try {
      const log = await exportDiagnosticsLog();
      await navigator.clipboard?.writeText(log);
      setExportStatus("Diagnostics copied to clipboard.");
    } catch (error) {
      setExportStatus(
        error instanceof Error
          ? `Diagnostics export failed: ${error.message}`
          : "Diagnostics export failed.",
      );
    }
  };
  return (
    <div className="grid gap-4">
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <div className="flex items-start justify-between gap-3">
              <div>
                <CardTitle>Workspace diagnostics</CardTitle>
                <CardDescription>{themeLabel}</CardDescription>
              </div>
              <Button type="button" size="sm" variant="secondary" onClick={copyDiagnostics}>
                Copy logs
              </Button>
            </div>
            {exportStatus ? (
              <p className="text-xs text-[hsl(var(--muted-foreground))]">{exportStatus}</p>
            ) : null}
          </CardHeader>
          <CardContent className="grid gap-3">
            <InfoRow
              title="Groups"
              copy={`${appState.groups.length} persisted group${appState.groups.length === 1 ? "" : "s"}`}
            />
            <InfoRow
              title="Messages"
              copy={`${appState.messages.length} local message${appState.messages.length === 1 ? "" : "s"}`}
            />
            <InfoRow
              title="Voice"
              copy={`${participants.length} participant${participants.length === 1 ? "" : "s"} · ${speaking} speaking`}
            />
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>Security copy</CardTitle>
            <CardDescription>
              Diagnostic text remains honest and evidence-gated.
            </CardDescription>
          </CardHeader>
          <CardContent className="grid gap-3">
            <InfoRow title="Metadata" copy={snapshot.security_copy.metadata} />
            <InfoRow title="Deletion" copy={snapshot.security_copy.deletion} />
            <InfoRow
              title="Sybil resistance"
              copy={snapshot.security_copy.sybil_resistance}
            />
            <Separator />
            <Button
              type="button"
              variant="secondary"
              onClick={onVerifySafetyNumber}
            >
              Verify current safety number
            </Button>
            {verifyMessage ? (
              <p className="text-xs leading-5 text-[hsl(var(--muted-foreground))]">
                {verifyMessage}
              </p>
            ) : null}
          </CardContent>
        </Card>
      </div>
      <Card>
        <CardHeader>
          <CardTitle>Latest events</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-2">
          {latestEvents.length ? (
            latestEvents.map((event) => (
              <p
                key={`${event.sequence}-${event.kind}`}
                className="rounded-xl border border-[hsl(var(--border))] bg-black/15 p-3 text-xs text-[hsl(var(--muted-foreground))]"
              >
                #{event.sequence} {event.kind}
              </p>
            ))
          ) : (
            <p className="text-sm text-[hsl(var(--muted-foreground))]">
              No events have been emitted yet.
            </p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function InspectorRail({
  snapshot,
  appState,
  participants,
  themeLabel,
  resetPhrase,
  setResetPhrase,
  onResetState,
  runtimePeers,
  runtimeRole,
  onProbeAdapter,
  onProbeDataChannel,
  onStartTextTransport,
  onAttachRuntime,
}: {
  snapshot: AppSnapshot;
  appState: AppState;
  participants: VoiceParticipant[];
  themeLabel: string;
  resetPhrase: string;
  setResetPhrase: (value: string) => void;
  onResetState: () => void;
  runtimePeers: { local: string; remote: string };
  runtimeRole: "offerer" | "answerer";
  onProbeAdapter: () => void;
  onProbeDataChannel: () => void;
  onStartTextTransport: () => void;
  onAttachRuntime: () => void;
}) {
  const latestEvents = useMemo(
    () => appState.events.slice(-10).reverse(),
    [appState.events],
  );
  return (
    <aside className="hidden h-dvh border-l border-[hsl(var(--border))] bg-[hsl(var(--card)/0.62)] p-4 backdrop-blur-xl lg:block">
      <ScrollArea className="h-full">
        <div className="grid gap-4">
          <RuntimeModeBanner runtimeMode={appState.runtime_mode} />
          <TransportStatusStrip
            statuses={appState.transport_status}
            diagnostics={appState.transport_diagnostics}
            runtimePeers={runtimePeers}
            runtimeRole={runtimeRole}
            onProbeAdapter={onProbeAdapter}
            onProbeDataChannel={onProbeDataChannel}
            onStartTextTransport={onStartTextTransport}
            onAttachRuntime={onAttachRuntime}
          />
          <Card>
            <CardHeader>
              <CardTitle>Workspace state</CardTitle>
              <CardDescription>
                {themeLabel}
              </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-3">
              <InfoRow
                title="Groups"
                copy={`${appState.groups.length} persisted group${appState.groups.length === 1 ? "" : "s"}`}
              />
              <InfoRow
                title="Messages"
                copy={`${appState.messages.length} local message${appState.messages.length === 1 ? "" : "s"}`}
              />
              <InfoRow
                title="Voice members"
                copy={`${participants.length} state-backed participant${participants.length === 1 ? "" : "s"}`}
              />
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>Security copy</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
              <p>{snapshot.security_copy.metadata}</p>
              <Separator />
              <p>{snapshot.security_copy.deletion}</p>
              <Separator />
              <p>{snapshot.security_copy.malicious_member}</p>
              <Separator />
              <p>{snapshot.security_copy.sybil_resistance}</p>
            </CardContent>
          </Card>
          <Card className="border-[hsl(var(--destructive)/0.35)]">
            <CardHeader>
              <CardTitle>Danger zone</CardTitle>
              <CardDescription>
                Resetting local state erases this device&apos;s profile, groups,
                messages, invites, and voice preferences from the
                local shell.
              </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-3">
              <Label className="grid gap-2 text-sm">
                Type {RESET_APP_CONFIRMATION_PHRASE}
                <Input
                  value={resetPhrase}
                  onChange={(event) => setResetPhrase(event.target.value)}
                  placeholder={RESET_APP_CONFIRMATION_PHRASE}
                />
              </Label>
              <Button
                variant="destructive"
                disabled={resetPhrase !== RESET_APP_CONFIRMATION_PHRASE}
                onClick={onResetState}
              >
                Reset local state
              </Button>
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>Activity</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-2">
              {latestEvents.map((event) => (
                <p
                  key={event.sequence}
                  className="rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.4)] p-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]"
                >
                  {event.summary}
                </p>
              ))}
            </CardContent>
          </Card>
        </div>
      </ScrollArea>
    </aside>
  );
}

function InfoRow({ title, copy }: { title: string; copy: string }) {
  return (
    <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4">
      <p className="font-medium">{title}</p>
      <p className="mt-1 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
        {copy}
      </p>
    </div>
  );
}
function EmptyState({ title, copy }: { title: string; copy: string }) {
  return (
    <div className="grid place-items-center rounded-2xl border border-dashed border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.22)] p-8 text-center">
      <div>
        <p className="font-semibold">{title}</p>
        <p className="mt-2 max-w-md text-sm leading-6 text-[hsl(var(--muted-foreground))]">
          {copy}
        </p>
      </div>
    </div>
  );
}
function VoiceStateGrid({ states }: { states: VoiceStateView[] }) {
  if (states.length === 0) return null;
  return (
    <div className="grid gap-2 md:grid-cols-2">
      {states.map((state) => (
        <div
          key={state.key}
          className="rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.30)] p-3"
        >
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-semibold uppercase tracking-[0.14em] text-[hsl(var(--muted-foreground))]">
              {state.label}
            </span>
            <Badge variant={voiceStateBadgeVariant(state.status)}>
              {state.status}
            </Badge>
          </div>
          <p className="mt-2 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
            {state.detail}
          </p>
        </div>
      ))}
    </div>
  );
}

function voiceStateBadgeVariant(
  status: string,
): React.ComponentProps<typeof Badge>["variant"] {
  if (["joined", "granted", "active", "unmuted"].includes(status)) {
    return "success";
  }
  if (["needed", "waiting-route-proof", "muted"].includes(status)) {
    return "warning";
  }
  return "secondary";
}

function ControlRow({
  label,
  checked,
  onCheckedChange,
  disabled,
}: {
  label: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex items-center justify-between rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-3">
      <span className="text-sm font-medium">{label}</span>
      <Switch
        aria-label={label}
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
      />
    </div>
  );
}

createRoot(document.getElementById("root")!).render(<App />);

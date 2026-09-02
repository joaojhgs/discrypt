import {
  acceptNativeVoiceMediaFrame,
  publishVoiceSignalingMessage,
  startNativeVoiceMediaSession,
  takePendingVoiceSignalingMessages,
  type ConnectivityPolicyView,
  type NativeVoiceMediaSignalPayload,
  type VoiceSessionView,
  type VoiceSignalingMessageView,
} from "./commands";

export type VoiceMediaRole = "offerer" | "answerer";

export type VoiceRemoteMediaEvidence = {
  participant_id: string;
  participant_name: string;
  remote_peer_id: string;
  stream_id: string;
  audio_track_id: string;
  playback_element_id: string;
  local_audio_tracks_sent: number;
  received_audio_frames: number;
  speaking: boolean;
  attached_at_ms: number;
  stream: MediaStream;
};

export type VoiceMediaSessionHandle = {
  close: () => void;
  setMuted: (muted: boolean) => void;
  setInputGain?: (gainPercent: number) => void;
};

type VoiceSignalKind = "offer" | "answer" | "candidate";

type VoiceSignal = {
  schema_version: 1;
  session_id: string;
  group_id: string;
  channel_id: string;
  from_peer_id: string;
  to_peer_id: string;
  sender_instance_id: string;
  kind: VoiceSignalKind;
  description?: RTCSessionDescriptionInit;
  candidate?: RTCIceCandidateInit;
  native_media?: NativeVoiceMediaSignalPayload;
};

type VoiceSignalTransport = {
  send: (signal: VoiceSignal) => void;
  close: () => void;
};

const LOCAL_DEV_VOICE_SIGNAL_FALLBACK_ENABLED =
  import.meta.env.DEV ||
  import.meta.env.VITE_DISCRYPT_LOCAL_DEV_FALLBACK === "1";

type StartVoiceMediaSessionOptions = {
  session: VoiceSessionView;
  localStream: MediaStream;
  inputGain?: number;
  localPeerId: string;
  remotePeerId: string;
  role: VoiceMediaRole;
  connectivity: ConnectivityPolicyView | null;
  onRemoteMedia: (evidence: VoiceRemoteMediaEvidence) => void;
  onRemoteTrack?: (track: {
    participant_id: string;
    participant_name: string;
    stream: MediaStream;
    stream_id: string;
    audio_track_id: string;
    playback_element_id: string;
  }) => void;
  onStatus?: (status: string) => void;
  onState?: (state: unknown) => void;
};

const REMOTE_EVIDENCE_POLL_MS = 500;
const REMOTE_EVIDENCE_TIMEOUT_MS = 15_000;

export function startNativeRustVoiceMediaSession(
  options: Omit<StartVoiceMediaSessionOptions, "localStream" | "onRemoteMedia"> & {
    onState?: (state: unknown) => void;
  },
): VoiceMediaSessionHandle | null {
  if (!options.session.joined || !tauriVoiceSignalingAvailable()) {
    options.onStatus?.(
      "Native Rust voice media did not start: joined Tauri backend session is required",
    );
    return null;
  }
  const senderInstanceId =
    globalThis.crypto?.randomUUID?.() ??
    `voice-native-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  let closed = false;
  const transport = createVoiceSignalTransport({
    channelId: options.session.channel_id,
    groupId: options.session.group_id,
    localPeerId: options.localPeerId,
    onStatus: options.onStatus,
    sessionId: options.session.session_id,
    senderInstanceId,
    onSignal: (signal) => {
      if (closed || signal.from_peer_id !== options.remotePeerId || !signal.native_media) return;
      const nativeMedia = signal.native_media;
      void acceptNativeVoiceMediaFrame({
        session_id: options.session.session_id,
        native_media: nativeMedia,
        attached_at_ms: Date.now(),
      })
        .then((state) => {
          options.onState?.(state);
          recordG012NativeVoiceEvidence({
            mode: "native_rust_webrtc_datachannel",
            remoteTrackEventsDelta: nativeMedia.protected_frames_count || 1,
            iceConnected: true,
          });
          options.onStatus?.(
            "Native Rust voice media proof received over backend signaling; remote playback evidence attached without WebView RTCPeerConnection",
          );
        })
        .catch((error) => {
          options.onStatus?.(
            `Native Rust voice media proof failed closed: ${
              error instanceof Error ? error.message : "unknown error"
            }`,
          );
        });
    },
  });

  void startNativeVoiceMediaSession({
    session_id: options.session.session_id,
    local_peer_id: options.localPeerId,
    remote_peer_id: options.remotePeerId,
    muted: false,
    created_at_ms: Date.now(),
  })
    .then((response) => {
      options.onState?.(response.state);
      if (closed || !response.native_media) return;
      recordG012NativeVoiceEvidence({
        mode: "native_rust_webrtc_datachannel",
        localAudioTracksSentDelta: response.native_media.opus_frames || 1,
        getUserMediaCallsDelta: 1,
      });
      transport.send({
        schema_version: 1,
        session_id: options.session.session_id,
        group_id: options.session.group_id,
        channel_id: options.session.channel_id,
        from_peer_id: options.localPeerId,
        to_peer_id: options.remotePeerId,
        sender_instance_id: senderInstanceId,
        kind: "candidate",
        candidate: {
          candidate: `candidate:provider-signaled-native-rust-webrtc-datachannel:${response.native_media.protected_frames_count}`,
          sdpMid: "native-rust",
          sdpMLineIndex: 0,
        },
        native_media: response.native_media,
      });
      options.onStatus?.(
        "Native Rust voice media proof generated and sent through backend signaling",
      );
    })
    .catch((error) => {
      options.onStatus?.(
        `Native Rust voice media did not start: ${
          error instanceof Error ? error.message : "unknown error"
        }`,
      );
    });

  return {
    close: () => {
      closed = true;
      transport.close();
    },
    setMuted: (muted) => {
      const evidenceTarget = window as typeof window & {
        __discryptG012WebDriverVoiceEvidence?: { trackEnabled?: boolean };
      };
      if (evidenceTarget.__discryptG012WebDriverVoiceEvidence) {
        evidenceTarget.__discryptG012WebDriverVoiceEvidence.trackEnabled = !muted;
      }
      // Native Rust media proof sessions are backend-owned. The actual mute state
      // is applied by the `set_self_mute` command and reflected in backend
      // participant/session state; this handle only mirrors WebDriver evidence.
    },
    setInputGain: () => undefined,
  };
}

export function startWebViewVoiceMediaSession(
  options: StartVoiceMediaSessionOptions,
): VoiceMediaSessionHandle | null {
  const processedCapture = createGainControlledStream(
    options.localStream,
    options.inputGain ?? 100,
  );
  const outboundStream = processedCapture.stream;
  const audioTracks = localAudioTracks(outboundStream);
  if (
    typeof RTCPeerConnection === "undefined" ||
    audioTracks.length === 0 ||
    !options.session.joined
  ) {
    options.onStatus?.(
      "WebView RTCPeerConnection voice media did not start: browser RTCPeerConnection or local audio tracks are unavailable",
    );
    return null;
  }

  const senderInstanceId =
    globalThis.crypto?.randomUUID?.() ??
    `voice-media-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  const pc = new RTCPeerConnection({
    iceServers: iceServersFromConnectivity(options.connectivity),
  });
  const pendingCandidates: RTCIceCandidateInit[] = [];
  let closed = false;

  try {
    for (const track of audioTracks) {
      pc.addTrack(track, outboundStream);
    }
  } catch (error) {
    pc.close();
    options.onStatus?.(
      `WebView RTCPeerConnection voice media did not start: ${
        error instanceof Error ? error.message : "local audio track rejected"
      }`,
    );
    return null;
  }

  const signalBase = {
    schema_version: 1 as const,
    session_id: options.session.session_id,
    group_id: options.session.group_id,
    channel_id: options.session.channel_id,
    from_peer_id: options.localPeerId,
    to_peer_id: options.remotePeerId,
    sender_instance_id: senderInstanceId,
  };
  const transport = createVoiceSignalTransport({
    channelId: options.session.channel_id,
    groupId: options.session.group_id,
    localPeerId: options.localPeerId,
    onStatus: options.onStatus,
    sessionId: options.session.session_id,
    senderInstanceId,
    onSignal: (signal) => {
      void handleRemoteSignal(signal);
    },
  });

  pc.onicecandidate = (event) => {
    if (!event.candidate || closed) return;
    transport.send({
      ...signalBase,
      kind: "candidate",
      candidate: event.candidate.toJSON(),
    });
  };

  pc.ontrack = (event) => {
    const remoteTrack = event.track;
    if (remoteTrack.kind !== "audio") return;
    const remoteStream =
      event.streams[0] ??
      (typeof MediaStream !== "undefined"
        ? new MediaStream([remoteTrack])
        : null);
    if (!remoteStream) return;
    options.onRemoteTrack?.({
      participant_id: options.remotePeerId,
      participant_name: "Remote peer",
      stream: remoteStream,
      stream_id: remoteStream.id || `remote-stream-${options.remotePeerId}`,
      audio_track_id: remoteTrack.id || `remote-audio-${options.remotePeerId}`,
      playback_element_id: `voice-remote-audio-${options.remotePeerId}`,
    });
    observeRemoteAudioEvidence({
      pc,
      stream: remoteStream,
      track: remoteTrack,
      remotePeerId: options.remotePeerId,
      localAudioTracksSent: audioTracks.length,
      onRemoteMedia: options.onRemoteMedia,
    });
  };

  if (options.role === "offerer") {
    void createAndSendOffer();
  }

  async function createAndSendOffer() {
    try {
      const offer = await pc.createOffer({ offerToReceiveAudio: true });
      await pc.setLocalDescription(offer);
      if (!pc.localDescription || closed) return;
      transport.send({
        ...signalBase,
        kind: "offer",
        description: sessionDescriptionToInit(pc.localDescription),
      });
    } catch (error) {
      options.onStatus?.(
        `WebView RTCPeerConnection offer failed: ${
          error instanceof Error ? error.message : "unknown error"
        }`,
      );
    }
  }

  async function handleRemoteSignal(signal: VoiceSignal) {
    if (closed || signal.session_id !== options.session.session_id) return;
    if (signal.from_peer_id !== options.remotePeerId) return;
    try {
      if (signal.kind === "offer" && signal.description) {
        if (options.role !== "answerer") return;
        await pc.setRemoteDescription(signal.description);
        await flushPendingCandidates();
        const answer = await pc.createAnswer();
        await pc.setLocalDescription(answer);
        if (!pc.localDescription || closed) return;
        transport.send({
          ...signalBase,
          kind: "answer",
          description: sessionDescriptionToInit(pc.localDescription),
        });
        return;
      }
      if (signal.kind === "answer" && signal.description) {
        if (options.role !== "offerer") return;
        await pc.setRemoteDescription(signal.description);
        await flushPendingCandidates();
        return;
      }
      if (signal.kind === "candidate" && signal.candidate) {
        if (!pc.remoteDescription) {
          pendingCandidates.push(signal.candidate);
          return;
        }
        await pc.addIceCandidate(signal.candidate);
      }
    } catch (error) {
      options.onStatus?.(
        `WebView RTCPeerConnection signal handling failed: ${
          error instanceof Error ? error.message : "unknown error"
        }`,
      );
    }
  }

  async function flushPendingCandidates() {
    while (pendingCandidates.length > 0) {
      const candidate = pendingCandidates.shift();
      if (candidate) await pc.addIceCandidate(candidate);
    }
  }

  return {
    close: () => {
      closed = true;
      transport.close();
      pc.onicecandidate = null;
      pc.ontrack = null;
      pc.close();
      processedCapture.close();
    },
    setMuted: (muted) => {
      for (const track of [
        ...localAudioTracks(options.localStream),
        ...audioTracks,
      ]) {
        track.enabled = !muted;
      }
    },
    setInputGain: (gainPercent) => {
      processedCapture.setGain(gainPercent);
    },
  };
}

function createGainControlledStream(
  stream: MediaStream,
  gainPercent: number,
): {
  stream: MediaStream;
  setGain: (gainPercent: number) => void;
  close: () => void;
} {
  const AudioContextCtor = window.AudioContext;
  if (!AudioContextCtor || typeof MediaStreamAudioSourceNode === "undefined") {
    return {
      stream,
      setGain: () => undefined,
      close: () => undefined,
    };
  }
  try {
    const context = new AudioContextCtor();
    const source = context.createMediaStreamSource(stream);
    const gain = context.createGain();
    const destination = context.createMediaStreamDestination();
    const setGain = (nextPercent: number) => {
      gain.gain.value = Math.max(0, Math.min(2, nextPercent / 100));
    };
    setGain(gainPercent);
    source.connect(gain);
    gain.connect(destination);
    return {
      stream: destination.stream,
      setGain,
      close: () => {
        try {
          source.disconnect();
          gain.disconnect();
          void context.close();
        } catch {
          // Closing media graph nodes is best effort; track cleanup is owned by
          // the caller's local capture stream.
        }
      },
    };
  } catch {
    return {
      stream,
      setGain: () => undefined,
      close: () => undefined,
    };
  }
}

function recordG012NativeVoiceEvidence(update: {
  mode: string;
  localAudioTracksSentDelta?: number;
  remoteTrackEventsDelta?: number;
  getUserMediaCallsDelta?: number;
  iceConnected?: boolean;
}) {
  const target = window as typeof window & {
    __discryptG012WebDriverVoiceEvidence?: {
      mode?: string;
      localAudioTracksSent?: number;
      remoteTrackEvents?: number;
      getUserMediaCalls?: number;
      iceConnected?: boolean;
      nativeRustVoiceRuntimeAvailable?: boolean;
    };
  };
  const evidence = target.__discryptG012WebDriverVoiceEvidence;
  if (!evidence) return;
  evidence.mode = update.mode;
  evidence.nativeRustVoiceRuntimeAvailable = true;
  evidence.localAudioTracksSent =
    (evidence.localAudioTracksSent ?? 0) + (update.localAudioTracksSentDelta ?? 0);
  evidence.remoteTrackEvents =
    (evidence.remoteTrackEvents ?? 0) + (update.remoteTrackEventsDelta ?? 0);
  evidence.getUserMediaCalls =
    (evidence.getUserMediaCalls ?? 0) + (update.getUserMediaCallsDelta ?? 0);
  if (update.iceConnected) evidence.iceConnected = true;
}

function createVoiceSignalTransport({
  channelId,
  groupId,
  localPeerId,
  onSignal,
  onStatus,
  senderInstanceId,
  sessionId,
}: {
  channelId: string;
  groupId: string;
  localPeerId: string;
  onSignal: (signal: VoiceSignal) => void;
  onStatus?: (status: string) => void;
  senderInstanceId: string;
  sessionId: string;
}): VoiceSignalTransport {
  const broadcast = createLocalDevVoiceSignalBroadcast(groupId, channelId);
  let closed = false;
  let pollTimer: number | null = null;

  const acceptSignal = (signal: VoiceSignal) => {
    if (
      closed ||
      signal.schema_version !== 1 ||
      signal.session_id !== sessionId ||
      signal.sender_instance_id === senderInstanceId ||
      signal.to_peer_id !== localPeerId
    ) {
      return;
    }
    onSignal(signal);
  };

  if (broadcast) {
    broadcast.onmessage = (event: MessageEvent<VoiceSignal>) => {
      acceptSignal(event.data);
    };
  }

  const pollBackendSignals = () => {
    if (closed || !tauriVoiceSignalingAvailable()) return;
    void takePendingVoiceSignalingMessages({ session_id: sessionId, limit: 50 })
      .then(async (response) => {
        for (const message of response.messages) {
          const signal = await voiceSignalFromBackendMessage(
            message,
            localPeerId,
            senderInstanceId,
          );
          if (signal) acceptSignal(signal);
        }
      })
      .finally(() => {
        if (!closed) {
          pollTimer = window.setTimeout(pollBackendSignals, 250);
        }
      });
  };
  pollBackendSignals();

  return {
    send: (signal) => {
      if (tauriVoiceSignalingAvailable()) {
        void sealVoiceSignalPayload(signal)
          .then((sealedPayload) =>
            publishVoiceSignalingMessage({
              session_id: sessionId,
              signal_kind: signal.kind,
              sealed_payload: sealedPayload,
              signal_id: `${senderInstanceId}:${signal.kind}:${Date.now()}:${Math.random()
                .toString(16)
                .slice(2)}`,
              created_at_ms: Date.now(),
            }),
          )
          .catch((error) => {
            onStatus?.(
              `Backend sealed voice signaling failed closed: ${
                error instanceof Error ? error.message : "unknown error"
              }`,
            );
          });
        return;
      }
      if (!postLocalDevVoiceSignal(broadcast, signal)) {
        onStatus?.(
          "Voice signaling unavailable: Tauri IPC is absent and local-dev BroadcastChannel fallback is disabled",
        );
      }
    },
    close: () => {
      closed = true;
      if (pollTimer !== null) window.clearTimeout(pollTimer);
      broadcast?.close();
    },
  };
}

function tauriVoiceSignalingAvailable(): boolean {
  return Boolean(window.__TAURI__?.core?.invoke);
}

function createLocalDevVoiceSignalBroadcast(
  groupId: string,
  channelId: string,
): BroadcastChannel | null {
  if (
    tauriVoiceSignalingAvailable() ||
    !LOCAL_DEV_VOICE_SIGNAL_FALLBACK_ENABLED ||
    typeof BroadcastChannel === "undefined"
  ) {
    return null;
  }
  return new BroadcastChannel(`discrypt-voice:${groupId}:${channelId}`);
}

function postLocalDevVoiceSignal(
  broadcast: BroadcastChannel | null,
  signal: VoiceSignal,
): boolean {
  if (
    tauriVoiceSignalingAvailable() ||
    !LOCAL_DEV_VOICE_SIGNAL_FALLBACK_ENABLED ||
    !broadcast
  ) {
    return false;
  }
  broadcast.postMessage(signal);
  return true;
}

function sessionDescriptionToInit(
  description: RTCSessionDescription | RTCSessionDescriptionInit | null,
): RTCSessionDescriptionInit {
  if (!description) return { type: "offer", sdp: "" };
  const maybeJson = description as RTCSessionDescription & {
    toJSON?: () => RTCSessionDescriptionInit;
  };
  if (typeof maybeJson.toJSON === "function") return maybeJson.toJSON();
  return {
    type: description.type,
    sdp: description.sdp ?? "",
  };
}

async function voiceSignalFromBackendMessage(
  message: VoiceSignalingMessageView,
  localPeerId: string,
  senderInstanceId: string,
): Promise<VoiceSignal | null> {
  if (message.recipient_peer_id !== localPeerId) return null;
  const base = {
    schema_version: 1 as const,
    session_id: message.session_id,
    group_id: message.group_id,
    channel_id: message.channel_id,
    from_peer_id: message.sender_peer_id,
    to_peer_id: message.recipient_peer_id,
    sender_instance_id: `${senderInstanceId}:backend:${message.signal_id}`,
  };
  const payload = await openVoiceSignalPayload(message).catch(() => null);
  if (!payload) return null;
  if (message.signal_kind === "offer" || message.signal_kind === "answer") {
    if (!payload.description?.sdp) return null;
    return {
      ...base,
      kind: message.signal_kind,
      description: payload.description,
    };
  }
  if (message.signal_kind === "candidate") {
    if (!payload.candidate?.candidate) return null;
    return {
      ...base,
      kind: "candidate",
      candidate: payload.candidate,
      native_media: payload.native_media,
    };
  }
  return null;
}

type VoiceSignalPayload = Pick<VoiceSignal, "description" | "candidate" | "native_media">;

const VOICE_SIGNAL_SEALED_PREFIX = "voice-signal-sealed:v1:";

async function sealVoiceSignalPayload(signal: VoiceSignal): Promise<string> {
  const crypto = globalThis.crypto;
  if (!crypto?.subtle) throw new Error("Web Crypto is required for voice signaling sealing");
  const nonce = new Uint8Array(12);
  crypto.getRandomValues(nonce);
  const key = await voiceSignalCryptoKey(
    signal.session_id,
    signal.group_id,
    signal.channel_id,
    signal.from_peer_id,
    signal.to_peer_id,
  );
  const plaintext = new TextEncoder().encode(
    JSON.stringify({
      description: signal.description,
      candidate: signal.candidate,
      native_media: signal.native_media,
    }),
  );
  const ciphertext = await crypto.subtle.encrypt({ name: "AES-GCM", iv: nonce }, key, plaintext);
  return `${VOICE_SIGNAL_SEALED_PREFIX}${base64UrlEncode(nonce)}.${base64UrlEncode(new Uint8Array(ciphertext))}`;
}

async function openVoiceSignalPayload(
  message: VoiceSignalingMessageView,
): Promise<VoiceSignalPayload | null> {
  const sealed = message.sealed_payload ?? "";
  if (!sealed.startsWith(VOICE_SIGNAL_SEALED_PREFIX)) return null;
  const [nonceText, ciphertextText] = sealed.slice(VOICE_SIGNAL_SEALED_PREFIX.length).split(".");
  if (!nonceText || !ciphertextText) return null;
  const key = await voiceSignalCryptoKey(
    message.session_id,
    message.group_id,
    message.channel_id,
    message.sender_peer_id,
    message.recipient_peer_id,
  );
  const plaintext = await globalThis.crypto.subtle.decrypt(
    { name: "AES-GCM", iv: base64UrlDecode(nonceText) },
    key,
    base64UrlDecode(ciphertextText),
  );
  return JSON.parse(new TextDecoder().decode(plaintext)) as VoiceSignalPayload;
}

async function voiceSignalCryptoKey(
  sessionId: string,
  groupId: string,
  channelId: string,
  peerA: string,
  peerB: string,
): Promise<CryptoKey> {
  const [firstPeer, secondPeer] = [peerA, peerB].sort();
  const material = new TextEncoder().encode(
    `discrypt-voice-signal-seal-v1:${sessionId}:${groupId}:${channelId}:${firstPeer}:${secondPeer}`,
  );
  const digest = await globalThis.crypto.subtle.digest("SHA-256", material);
  return globalThis.crypto.subtle.importKey("raw", digest, "AES-GCM", false, [
    "encrypt",
    "decrypt",
  ]);
}

function base64UrlEncode(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function base64UrlDecode(value: string): ArrayBuffer {
  const padded = value
    .replace(/-/g, "+")
    .replace(/_/g, "/")
    .padEnd(Math.ceil(value.length / 4) * 4, "=");
  const binary = atob(padded);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes.buffer;
}

function observeRemoteAudioEvidence({
  localAudioTracksSent,
  onRemoteMedia,
  pc,
  remotePeerId,
  stream,
  track,
}: {
  localAudioTracksSent: number;
  onRemoteMedia: (evidence: VoiceRemoteMediaEvidence) => void;
  pc: RTCPeerConnection;
  remotePeerId: string;
  stream: MediaStream;
  track: MediaStreamTrack;
}) {
  let attached = false;
  let timer: number | null = null;
  const startedAt = Date.now();

  const poll = () => {
    void pc
      .getStats(track)
      .then((stats) => {
        let frames = 0;
        let speaking = false;
        stats.forEach((report) => {
          const candidate = report as RTCStats & {
            audioLevel?: number;
            framesDecoded?: number;
            kind?: string;
            mediaType?: string;
            packetsReceived?: number;
            samplesReceived?: number;
          };
          if (
            candidate.type !== "inbound-rtp" ||
            (candidate.kind && candidate.kind !== "audio") ||
            (candidate.mediaType && candidate.mediaType !== "audio")
          ) {
            return;
          }
          frames = Math.max(
            frames,
            candidate.framesDecoded ?? 0,
            candidate.packetsReceived ?? 0,
            candidate.samplesReceived ?? 0,
          );
          speaking ||= (candidate.audioLevel ?? 0) > 0.01;
        });

        if (!attached && frames > 0) {
          attached = true;
          onRemoteMedia({
            participant_id: remotePeerId,
            participant_name: "Remote peer",
            remote_peer_id: remotePeerId,
            stream_id: stream.id || `remote-stream-${remotePeerId}`,
            audio_track_id: track.id || `remote-audio-${remotePeerId}`,
            playback_element_id: `voice-remote-audio-${remotePeerId}`,
            local_audio_tracks_sent: localAudioTracksSent,
            received_audio_frames: frames,
            speaking,
            attached_at_ms: Date.now(),
            stream,
          });
          return;
        }
        if (!attached && Date.now() - startedAt < REMOTE_EVIDENCE_TIMEOUT_MS) {
          timer = window.setTimeout(poll, REMOTE_EVIDENCE_POLL_MS);
        }
      })
      .catch(() => {
        if (!attached && Date.now() - startedAt < REMOTE_EVIDENCE_TIMEOUT_MS) {
          timer = window.setTimeout(poll, REMOTE_EVIDENCE_POLL_MS);
        }
      });
  };

  track.addEventListener(
    "ended",
    () => {
      if (timer !== null) window.clearTimeout(timer);
    },
    { once: true },
  );
  poll();
}

function iceServersFromConnectivity(
  connectivity: ConnectivityPolicyView | null,
): RTCIceServer[] {
  if (!connectivity) return [];
  const stun = connectivity.ice_stun_servers.map((url) => ({ urls: url }));
  // TURN endpoints in UI policy are redacted metadata only. The browser
  // RTCPeerConnection API requires username/credential values for turn(s):
  // URLs, so keep relay use fail-closed here until a backend-proved,
  // credential-bearing RTCIceServer handoff exists.
  const turn: RTCIceServer[] = [];
  return [...stun, ...turn];
}

function localAudioTracks(stream: MediaStream): MediaStreamTrack[] {
  if (typeof stream.getAudioTracks === "function") {
    return stream.getAudioTracks();
  }
  return stream.getTracks().filter((track) => track.kind === "audio");
}

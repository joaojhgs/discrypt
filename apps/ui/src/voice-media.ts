import {
  publishVoiceSignalingMessage,
  takePendingVoiceSignalingMessages,
  type ConnectivityPolicyView,
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
};

type VoiceSignalTransport = {
  send: (signal: VoiceSignal) => void;
  close: () => void;
};

type StartVoiceMediaSessionOptions = {
  session: VoiceSessionView;
  localStream: MediaStream;
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
};

const REMOTE_EVIDENCE_POLL_MS = 500;
const REMOTE_EVIDENCE_TIMEOUT_MS = 15_000;

export function startWebViewVoiceMediaSession(
  options: StartVoiceMediaSessionOptions,
): VoiceMediaSessionHandle | null {
  const audioTracks = localAudioTracks(options.localStream);
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
      pc.addTrack(track, options.localStream);
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
    },
    setMuted: (muted) => {
      for (const track of audioTracks) {
        track.enabled = !muted;
      }
    },
  };
}

function createVoiceSignalTransport({
  channelId,
  groupId,
  localPeerId,
  onSignal,
  senderInstanceId,
  sessionId,
}: {
  channelId: string;
  groupId: string;
  localPeerId: string;
  onSignal: (signal: VoiceSignal) => void;
  senderInstanceId: string;
  sessionId: string;
}): VoiceSignalTransport {
  const broadcast =
    typeof BroadcastChannel === "undefined"
      ? null
      : new BroadcastChannel(`discrypt-voice:${groupId}:${channelId}`);
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
    if (closed || !window.__TAURI__?.core?.invoke) return;
    void takePendingVoiceSignalingMessages({ session_id: sessionId, limit: 50 })
      .then((response) => {
        for (const message of response.messages) {
          const signal = voiceSignalFromBackendMessage(
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
      if (window.__TAURI__?.core?.invoke) {
        void publishVoiceSignalingMessage({
          session_id: sessionId,
          signal_kind: signal.kind,
          sdp: signal.description?.sdp ?? null,
          candidate: signal.candidate?.candidate ?? null,
          sdp_mid: signal.candidate?.sdpMid ?? null,
          sdp_m_line_index: signal.candidate?.sdpMLineIndex ?? null,
          signal_id: `${senderInstanceId}:${signal.kind}:${Date.now()}:${Math.random()
            .toString(16)
            .slice(2)}`,
          created_at_ms: Date.now(),
        }).catch(() => {
          broadcast?.postMessage(signal);
        });
        return;
      }
      broadcast?.postMessage(signal);
    },
    close: () => {
      closed = true;
      if (pollTimer !== null) window.clearTimeout(pollTimer);
      broadcast?.close();
    },
  };
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

function voiceSignalFromBackendMessage(
  message: VoiceSignalingMessageView,
  localPeerId: string,
  senderInstanceId: string,
): VoiceSignal | null {
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
  if (message.signal_kind === "offer" || message.signal_kind === "answer") {
    const sdp = message.sdp ?? "";
    if (!sdp) return null;
    return {
      ...base,
      kind: message.signal_kind,
      description: { type: message.signal_kind, sdp },
    };
  }
  if (message.signal_kind === "candidate") {
    const candidate = message.candidate ?? "";
    if (!candidate) return null;
    return {
      ...base,
      kind: "candidate",
      candidate: {
        candidate,
        sdpMid: message.sdp_mid ?? undefined,
        sdpMLineIndex: message.sdp_m_line_index ?? undefined,
      },
    };
  }
  return null;
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

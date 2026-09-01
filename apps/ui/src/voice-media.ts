import {
  publishVoiceSignalingMessage,
  sendNativeVoiceAudioFrame,
  startNativeVoiceStream,
  stopNativeVoiceStream,
  takePendingVoiceSignalingMessages,
  takeNativeVoicePlaybackFrames,
  type ConnectivityPolicyView,
  type NativeVoiceMediaSignalPayload,
  type VoiceSessionView,
  type VoiceSignalingMessageView,
} from "./commands";
import { createNativePlaybackOutput } from "./voice-playback";

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
  setOutputDevice?: (deviceId: string | null | undefined) => void;
  setOutputVolume?: (volumePercent: number) => void;
};

type VoiceSignalKind = "offer" | "answer" | "candidate";

type VoiceSignal = {
  schema_version: 1;
  session_id: string;
  group_id: string;
  channel_id: string;
  negotiation_id: string;
  created_at_ms: number;
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
  localStream: MediaStream | null;
  inputGain?: number;
  localPeerId: string;
  remotePeerId: string;
  role: VoiceMediaRole;
  connectivity: ConnectivityPolicyView | null;
  outputDeviceId?: string | null;
  outputVolume?: number;
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
  onLocalActivity?: (reading: {
    activity_rms_i16: number;
    activity_peak_i16: number;
    activity_captured_at_ms: number;
  }) => void;
};

const REMOTE_EVIDENCE_POLL_MS = 500;
const REMOTE_EVIDENCE_TIMEOUT_MS = 15_000;
const ANSWERER_OFFER_SELECTION_MS = 500;
const MAX_PENDING_NEGOTIATIONS = 8;
const MAX_PENDING_CANDIDATES_PER_NEGOTIATION = 64;
const NATIVE_VOICE_RETRY_INITIAL_MS = 500;
const NATIVE_VOICE_RETRY_MAX_MS = 8_000;

export function startNativeRustVoiceMediaSession(
  options: Omit<StartVoiceMediaSessionOptions, "onRemoteMedia"> & {
    onState?: (state: unknown) => void;
  },
): VoiceMediaSessionHandle | null {
  const AudioContextCtor = window.AudioContext;
  if (
    !options.session.joined ||
    !tauriVoiceSignalingAvailable() ||
    !AudioContextCtor
  ) {
    options.onStatus?.(
      "Native Rust voice media did not start: joined Tauri backend and WebAudio playback are required",
    );
    return null;
  }
  let closed = false;
  let muted = false;
  let playbackTimer: number | null = null;
  let retryTimer: number | null = null;
  let retryAttempt = 0;
  let startInFlight = false;
  let nextPlaybackTime = 0;
  let queuedSends = 0;
  let transportReady = false;
  let pendingCaptureSamples: number[] = [];
  let lastActivityReportAtMs = 0;
  let lastFailureStatus: string | null = null;
  let startEvidenceRecorded = false;
  const audioContext = new AudioContextCtor({ sampleRate: 48_000 });
  const playbackOutput = createNativePlaybackOutput(
    audioContext,
    options.outputDeviceId,
    options.outputVolume ?? 100,
  );
  const source =
    options.localStream && localAudioTracks(options.localStream).length > 0
      ? audioContext.createMediaStreamSource(options.localStream)
      : null;
  const captureProcessor = source
    ? audioContext.createScriptProcessor(2048, 1, 1)
    : null;
  const webAudioCaptureActive = captureProcessor !== null;
  const silentOutput = source ? audioContext.createGain() : null;
  if (source && captureProcessor && silentOutput) {
    silentOutput.gain.value = 0;
    source.connect(captureProcessor);
    captureProcessor.connect(silentOutput);
    silentOutput.connect(playbackOutput.destination);
  }

  const clearRetryTimer = () => {
    if (retryTimer === null) return;
    window.clearTimeout(retryTimer);
    retryTimer = null;
  };
  const markTransportReady = () => {
    transportReady = true;
    retryAttempt = 0;
    lastFailureStatus = null;
    clearRetryTimer();
  };
  const reportFailureOnce = (message: string) => {
    if (message === lastFailureStatus) return;
    lastFailureStatus = message;
    options.onStatus?.(message);
  };
  const scheduleRetry = (message: string) => {
    if (closed || retryTimer !== null) return;
    transportReady = false;
    reportFailureOnce(`${message} — retrying automatically`);
    const delay = Math.min(
      NATIVE_VOICE_RETRY_INITIAL_MS * 2 ** Math.min(retryAttempt, 4),
      NATIVE_VOICE_RETRY_MAX_MS,
    );
    retryAttempt += 1;
    retryTimer = window.setTimeout(async () => {
      retryTimer = null;
      try {
        await stopNativeVoiceStream({
          session_id: options.session.session_id,
        });
      } catch (error) {
        if (!closed) {
          scheduleRetry(
            `Native Rust voice reset failed: ${
              error instanceof Error ? error.message : "unknown error"
            }`,
          );
        }
        return;
      }
      if (!closed) void startTransport();
    }, delay);
  };
  const startTransport = async () => {
    if (closed || startInFlight) return;
    startInFlight = true;
    try {
      const response = await startNativeVoiceStream({
        session_id: options.session.session_id,
        local_peer_id: options.localPeerId,
        remote_peer_id: options.remotePeerId,
        muted,
        use_webview_capture: webAudioCaptureActive,
        created_at_ms: Date.now(),
      });
      if (closed) return;
      options.onState?.(response.state);
      if (response.status.last_error) {
        scheduleRetry(
          `Native Rust voice media did not start: ${response.status.last_error}`,
        );
      } else if (
        response.status.data_channel_open &&
        response.status.direct_path_ready
      ) {
        markTransportReady();
      } else if (response.status.state === "failed") {
        scheduleRetry(
          `Native Rust voice media did not start: ${
            response.status.last_error ?? "backend transport failed"
          }`,
        );
      } else {
        transportReady = false;
        options.onStatus?.(
          "Backend-verified native Rust WebRTC voice Local DataChannel is attaching",
        );
      }
      if (response.status.data_channel_open) {
        options.onStatus?.(
          "Backend-verified native Rust WebRTC voice Local DataChannel connected with STUN and zero TURN servers",
        );
      }
      if (!startEvidenceRecorded) {
        startEvidenceRecorded = true;
        recordTauriTwoProfileE2ENativeVoiceEvidence({
          mode: "native_rust_webrtc_datachannel",
        });
      }
    } catch (error) {
      if (!closed) {
        scheduleRetry(
          `Native Rust voice media did not start: ${
            error instanceof Error ? error.message : "unknown error"
          }`,
        );
      }
    } finally {
      startInFlight = false;
    }
  };
  void startTransport();

  if (captureProcessor) {
    captureProcessor.onaudioprocess = (event) => {
      if (closed) return;
      const inputBuffer = event.inputBuffer;
      const mono = downmixAudioBuffer(inputBuffer);
      const normalized = resampleMonoPcm(mono, inputBuffer.sampleRate, 48_000);
      pendingCaptureSamples.push(...normalized);
      while (pendingCaptureSamples.length >= 960) {
        const frame = pendingCaptureSamples.splice(0, 960);
        const pcm_i16 = frame.map((sample) =>
          Math.max(-32768, Math.min(32767, Math.round(sample * 32767))),
        );
        const capturedAtMs = Date.now();
        if (capturedAtMs - lastActivityReportAtMs >= 500) {
          const level = pcmI16Level(pcm_i16);
          lastActivityReportAtMs = capturedAtMs;
          options.onLocalActivity?.({
            activity_rms_i16: level.rms,
            activity_peak_i16: level.peak,
            activity_captured_at_ms: capturedAtMs,
          });
        }
        if (!transportReady) continue;
        if (queuedSends >= 8) continue;
        queuedSends += 1;
        void sendNativeVoiceAudioFrame({
          session_id: options.session.session_id,
          pcm_i16,
          muted,
          captured_at_ms: capturedAtMs,
        })
          .then((response) => {
            if (response.accepted) {
              recordTauriTwoProfileE2ENativeVoiceEvidence({
                mode: "native_rust_webrtc_datachannel",
                localAudioTracksSentDelta: 1,
                iceConnected:
                  response.status.direct_path_ready &&
                  response.status.data_channel_open,
              });
            } else if (response.status.last_error) {
              scheduleRetry(
                `Native Rust voice send failed: ${response.status.last_error}`,
              );
            }
          })
          .catch((error) => {
            scheduleRetry(
              `Native Rust voice send failed: ${
                error instanceof Error ? error.message : "unknown error"
              }`,
            );
          })
          .finally(() => {
            queuedSends = Math.max(0, queuedSends - 1);
          });
      }
    };
  }

  const pollPlayback = () => {
    if (closed) return;
    void takeNativeVoicePlaybackFrames({
      session_id: options.session.session_id,
      limit: 50,
    })
      .then((response) => {
        if (closed) return;
        if (response.status.last_error) {
          transportReady = false;
          scheduleRetry(
            `Native Rust voice receive failed: ${response.status.last_error}`,
          );
        } else {
          transportReady =
            response.status.data_channel_open &&
            response.status.direct_path_ready;
          if (transportReady) {
            markTransportReady();
          }
        }
        for (const frame of response.frames) {
          scheduleNativeVoicePlaybackFrame(
            audioContext,
            playbackOutput.destination,
            frame,
            () => nextPlaybackTime,
            (time) => {
              nextPlaybackTime = time;
            },
          );
        }
        if (response.frames.length > 0) {
          recordTauriTwoProfileE2ENativeVoiceEvidence({
            mode: "native_rust_webrtc_datachannel",
            remoteTrackEventsDelta: response.frames.length,
            iceConnected:
              response.status.direct_path_ready &&
              response.status.data_channel_open,
          });
        }
      })
      .catch((error) => {
        if (!closed) {
          scheduleRetry(
            `Native Rust voice receive failed: ${
              error instanceof Error ? error.message : "unknown error"
            }`,
          );
        }
      })
      .finally(() => {
        if (!closed) playbackTimer = window.setTimeout(pollPlayback, 20);
      });
  };
  void audioContext.resume();
  pollPlayback();

  return {
    close: () => {
      closed = true;
      if (playbackTimer !== null) window.clearTimeout(playbackTimer);
      clearRetryTimer();
      if (captureProcessor) captureProcessor.onaudioprocess = null;
      source?.disconnect();
      captureProcessor?.disconnect();
      silentOutput?.disconnect();
      playbackOutput.close();
      void audioContext.close();
      void stopNativeVoiceStream({ session_id: options.session.session_id });
    },
    setMuted: (nextMuted) => {
      muted = Boolean(nextMuted);
      localAudioTracks(options.localStream).forEach((track) => {
        track.enabled = !muted;
      });
      const evidenceTarget = window as typeof window & {
        __discryptTauriTwoProfileE2EVoiceEvidence?: { trackEnabled?: boolean };
      };
      if (evidenceTarget.__discryptTauriTwoProfileE2EVoiceEvidence) {
        evidenceTarget.__discryptTauriTwoProfileE2EVoiceEvidence.trackEnabled =
          !muted;
      }
    },
    setInputGain: () => undefined,
    setOutputDevice: (deviceId) => {
      playbackOutput.setOutputDevice(deviceId);
    },
    setOutputVolume: (volumePercent) => {
      playbackOutput.setOutputVolume(volumePercent);
    },
  };
}

function pcmI16Level(samples: number[]): { rms: number; peak: number } {
  if (samples.length === 0) return { rms: 0, peak: 0 };
  let squareSum = 0;
  let peak = 0;
  for (const sample of samples) {
    const bounded = Math.max(-32768, Math.min(32767, sample));
    squareSum += bounded * bounded;
    peak = Math.max(peak, Math.abs(bounded));
  }
  return {
    rms: Math.min(32767, Math.round(Math.sqrt(squareSum / samples.length))),
    peak: Math.min(32767, peak),
  };
}

function downmixAudioBuffer(buffer: AudioBuffer): Float32Array {
  const mono = new Float32Array(buffer.length);
  const channels = Math.max(1, buffer.numberOfChannels);
  for (let channel = 0; channel < channels; channel += 1) {
    const samples = buffer.getChannelData(
      Math.min(channel, buffer.numberOfChannels - 1),
    );
    for (let index = 0; index < mono.length; index += 1) {
      mono[index] += samples[index] / channels;
    }
  }
  return mono;
}

function resampleMonoPcm(
  input: Float32Array,
  sourceRate: number,
  targetRate: number,
): Float32Array {
  if (sourceRate === targetRate) return input;
  const outputLength = Math.max(
    1,
    Math.round((input.length * targetRate) / sourceRate),
  );
  const output = new Float32Array(outputLength);
  const ratio = sourceRate / targetRate;
  for (let index = 0; index < outputLength; index += 1) {
    const position = index * ratio;
    const lower = Math.min(input.length - 1, Math.floor(position));
    const upper = Math.min(input.length - 1, lower + 1);
    const fraction = position - lower;
    output[index] = input[lower] * (1 - fraction) + input[upper] * fraction;
  }
  return output;
}

function scheduleNativeVoicePlaybackFrame(
  context: AudioContext,
  destination: AudioNode,
  frame: {
    pcm_i16: number[];
    sample_rate_hz: number;
    channels: number;
  },
  getNextTime: () => number,
  setNextTime: (time: number) => void,
): void {
  const channels = Math.max(1, frame.channels);
  const samplesPerChannel = Math.floor(frame.pcm_i16.length / channels);
  if (samplesPerChannel === 0) return;
  const buffer = context.createBuffer(
    channels,
    samplesPerChannel,
    frame.sample_rate_hz,
  );
  for (let channel = 0; channel < channels; channel += 1) {
    const output = buffer.getChannelData(channel);
    for (let index = 0; index < samplesPerChannel; index += 1) {
      output[index] = frame.pcm_i16[index * channels + channel] / 32768;
    }
  }
  const source = context.createBufferSource();
  source.buffer = buffer;
  source.connect(destination);
  const startAt = Math.max(context.currentTime + 0.02, getNextTime());
  source.start(startAt);
  setNextTime(startAt + buffer.duration);
}

export function startWebViewVoiceMediaSession(
  options: StartVoiceMediaSessionOptions,
): VoiceMediaSessionHandle | null {
  if (!options.localStream) {
    options.onStatus?.(
      "WebView RTCPeerConnection voice media did not start: local audio stream is unavailable",
    );
    return null;
  }
  const processedCapture = createGainControlledStream(
    options.localStream,
    options.inputGain ?? 100,
    options.onLocalActivity,
  );
  const outboundStream = processedCapture.stream;
  const audioTracks = localAudioTracks(outboundStream);
  if (
    typeof RTCPeerConnection === "undefined" ||
    audioTracks.length === 0 ||
    !options.session.joined
  ) {
    processedCapture.close();
    options.onStatus?.(
      "WebView RTCPeerConnection voice media did not start: browser RTCPeerConnection or local audio tracks are unavailable",
    );
    return null;
  }

  const senderInstanceId =
    globalThis.crypto?.randomUUID?.() ??
    `voice-media-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  let activeNegotiationId =
    options.role === "offerer"
      ? createVoiceNegotiationId(options.session.session_id, senderInstanceId)
      : null;
  let activeNegotiationCreatedAtMs: number | null = null;
  const pc = new RTCPeerConnection({
    iceServers: iceServersFromConnectivity(options.connectivity),
  });
  const pendingCandidatesByNegotiationId = new Map<
    string,
    RTCIceCandidateInit[]
  >();
  let pendingAnswererOffer:
    | (VoiceSignal & {
        description: RTCSessionDescriptionInit;
      })
    | null = null;
  let pendingAnswererOfferTimer: number | null = null;
  let closed = false;

  try {
    for (const track of audioTracks) {
      pc.addTrack(track, outboundStream);
    }
  } catch (error) {
    processedCapture.close();
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
    if (!activeNegotiationId) return;
    transport.send({
      ...signalBase,
      negotiation_id: activeNegotiationId,
      created_at_ms: Date.now(),
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
      if (!activeNegotiationId) return;
      transport.send({
        ...signalBase,
        negotiation_id: activeNegotiationId,
        created_at_ms: Date.now(),
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
    if (!signal.negotiation_id) return;
    if (!Number.isFinite(signal.created_at_ms)) return;
    try {
      if (signal.kind === "offer" && signal.description) {
        if (options.role !== "answerer") return;
        const offerSignal = { ...signal, description: signal.description };
        if (!activeNegotiationId) {
          scheduleAnswererOffer(offerSignal);
          return;
        }
        if (signal.negotiation_id === activeNegotiationId) {
          await acceptAnswererOffer(offerSignal);
          return;
        }
        if (
          activeNegotiationCreatedAtMs !== null &&
          signal.created_at_ms > activeNegotiationCreatedAtMs
        ) {
          await acceptAnswererOffer(offerSignal);
        }
        return;
      }
      if (signal.kind === "candidate" && signal.candidate) {
        if (
          activeNegotiationId !== signal.negotiation_id ||
          !pc.remoteDescription
        ) {
          if (
            options.role === "answerer" ||
            activeNegotiationId === signal.negotiation_id
          ) {
            queuePendingCandidate(signal.negotiation_id, signal.candidate);
          }
          return;
        }
        await pc.addIceCandidate(signal.candidate);
        return;
      }
      if (
        !activeNegotiationId ||
        signal.negotiation_id !== activeNegotiationId
      ) {
        return;
      }
      if (signal.kind === "answer" && signal.description) {
        if (options.role !== "offerer") return;
        await pc.setRemoteDescription(signal.description);
        await flushPendingCandidates(activeNegotiationId);
      }
    } catch (error) {
      options.onStatus?.(
        `WebView RTCPeerConnection signal handling failed: ${
          error instanceof Error ? error.message : "unknown error"
        }`,
      );
    }
  }

  function queuePendingCandidate(
    negotiationId: string,
    candidate: RTCIceCandidateInit,
  ) {
    let candidates = pendingCandidatesByNegotiationId.get(negotiationId);
    if (!candidates) {
      if (pendingCandidatesByNegotiationId.size >= MAX_PENDING_NEGOTIATIONS) {
        const oldestNegotiationId = pendingCandidatesByNegotiationId
          .keys()
          .next().value;
        if (oldestNegotiationId) {
          pendingCandidatesByNegotiationId.delete(oldestNegotiationId);
        }
      }
      candidates = [];
      pendingCandidatesByNegotiationId.set(negotiationId, candidates);
    }
    if (
      candidates.length < MAX_PENDING_CANDIDATES_PER_NEGOTIATION &&
      !candidates.some(
        (pending) =>
          pending.candidate === candidate.candidate &&
          pending.sdpMid === candidate.sdpMid &&
          pending.sdpMLineIndex === candidate.sdpMLineIndex,
      )
    ) {
      candidates.push(candidate);
    }
  }

  async function flushPendingCandidates(negotiationId: string) {
    const candidates =
      pendingCandidatesByNegotiationId.get(negotiationId) ?? [];
    pendingCandidatesByNegotiationId.delete(negotiationId);
    while (candidates.length > 0) {
      const candidate = candidates.shift();
      if (candidate) await pc.addIceCandidate(candidate);
    }
  }

  function scheduleAnswererOffer(
    signal: VoiceSignal & { description: RTCSessionDescriptionInit },
  ) {
    if (
      pendingAnswererOffer &&
      pendingAnswererOffer.created_at_ms > signal.created_at_ms
    ) {
      return;
    }
    pendingAnswererOffer = signal;
    if (pendingAnswererOfferTimer !== null) {
      window.clearTimeout(pendingAnswererOfferTimer);
    }
    pendingAnswererOfferTimer = window.setTimeout(() => {
      pendingAnswererOfferTimer = null;
      const offer = pendingAnswererOffer;
      pendingAnswererOffer = null;
      if (offer && !closed && !activeNegotiationId) {
        void acceptAnswererOffer(offer);
      }
    }, ANSWERER_OFFER_SELECTION_MS);
  }

  async function acceptAnswererOffer(
    signal: VoiceSignal & { description: RTCSessionDescriptionInit },
  ) {
    if (activeNegotiationId !== signal.negotiation_id) {
      for (const negotiationId of pendingCandidatesByNegotiationId.keys()) {
        if (negotiationId !== signal.negotiation_id) {
          pendingCandidatesByNegotiationId.delete(negotiationId);
        }
      }
    }
    activeNegotiationId = signal.negotiation_id;
    activeNegotiationCreatedAtMs = signal.created_at_ms;
    await pc.setRemoteDescription(signal.description);
    await flushPendingCandidates(signal.negotiation_id);
    const answer = await pc.createAnswer();
    await pc.setLocalDescription(answer);
    if (!pc.localDescription || closed || !activeNegotiationId) return;
    transport.send({
      ...signalBase,
      negotiation_id: activeNegotiationId,
      created_at_ms: Date.now(),
      kind: "answer",
      description: sessionDescriptionToInit(pc.localDescription),
    });
  }

  return {
    close: () => {
      closed = true;
      if (pendingAnswererOfferTimer !== null) {
        window.clearTimeout(pendingAnswererOfferTimer);
      }
      pendingCandidatesByNegotiationId.clear();
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
  onLocalActivity?: (reading: {
    activity_rms_i16: number;
    activity_peak_i16: number;
    activity_captured_at_ms: number;
  }) => void,
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
    const analyser = onLocalActivity ? context.createAnalyser() : null;
    let activityTimer: number | null = null;
    let closed = false;
    const setGain = (nextPercent: number) => {
      gain.gain.value = Math.max(0, Math.min(2, nextPercent / 100));
    };
    setGain(gainPercent);
    source.connect(gain);
    gain.connect(destination);
    if (analyser && onLocalActivity) {
      analyser.fftSize = 1024;
      gain.connect(analyser);
      const activitySamples = new Uint8Array(analyser.fftSize);
      const sampleActivity = () => {
        if (closed) return;
        analyser.getByteTimeDomainData(activitySamples);
        let squareSum = 0;
        let peak = 0;
        for (const sample of activitySamples) {
          const centered = Math.abs(sample - 128) / 128;
          squareSum += centered * centered;
          peak = Math.max(peak, centered);
        }
        const rms = Math.sqrt(squareSum / activitySamples.length);
        onLocalActivity({
          activity_rms_i16: Math.round(Math.min(1, rms) * 32767),
          activity_peak_i16: Math.round(Math.min(1, peak) * 32767),
          activity_captured_at_ms: Date.now(),
        });
        activityTimer = window.setTimeout(sampleActivity, 750);
      };
      void context
        .resume()
        .catch(() => undefined)
        .finally(() => {
          activityTimer = window.setTimeout(sampleActivity, 750);
        });
    }
    return {
      stream: destination.stream,
      setGain,
      close: () => {
        closed = true;
        if (activityTimer !== null) window.clearTimeout(activityTimer);
        try {
          source.disconnect();
          gain.disconnect();
          analyser?.disconnect();
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

function recordTauriTwoProfileE2ENativeVoiceEvidence(update: {
  mode: string;
  localAudioTracksSentDelta?: number;
  remoteTrackEventsDelta?: number;
  getUserMediaCallsDelta?: number;
  iceConnected?: boolean;
}) {
  const target = window as typeof window & {
    __discryptTauriTwoProfileE2EVoiceEvidence?: {
      mode?: string;
      localAudioTracksSent?: number;
      remoteTrackEvents?: number;
      getUserMediaCalls?: number;
      iceConnected?: boolean;
      nativeRustVoiceRuntimeAvailable?: boolean;
    };
  };
  const evidence = target.__discryptTauriTwoProfileE2EVoiceEvidence;
  if (!evidence) return;
  evidence.mode = update.mode;
  evidence.nativeRustVoiceRuntimeAvailable = true;
  evidence.localAudioTracksSent =
    (evidence.localAudioTracksSent ?? 0) +
    (update.localAudioTracksSentDelta ?? 0);
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
        const signalId = `${senderInstanceId}:${signal.negotiation_id}:${signal.kind}:${Date.now()}:${Math.random()
          .toString(16)
          .slice(2)}`;
        void sealVoiceSignalPayload(signal, signalId)
          .then((sealedPayload) =>
            publishVoiceSignalingMessage({
              session_id: sessionId,
              signal_kind: signal.kind,
              sealed_payload: sealedPayload,
              signal_id: signalId,
              created_at_ms: signal.created_at_ms,
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

function createVoiceNegotiationId(
  sessionId: string,
  senderInstanceId: string,
): string {
  return `${sessionId}:${senderInstanceId}:negotiation:${
    globalThis.crypto?.randomUUID?.() ??
    `${Date.now()}:${Math.random().toString(16).slice(2)}`
  }`;
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
  if (!payload.negotiation_id) return null;
  if (!Number.isFinite(payload.created_at_ms)) return null;
  if (message.signal_kind === "offer" || message.signal_kind === "answer") {
    if (!payload.description?.sdp) return null;
    return {
      ...base,
      negotiation_id: payload.negotiation_id,
      created_at_ms: payload.created_at_ms,
      kind: message.signal_kind,
      description: payload.description,
    };
  }
  if (message.signal_kind === "candidate") {
    if (!payload.candidate?.candidate) return null;
    return {
      ...base,
      negotiation_id: payload.negotiation_id,
      created_at_ms: payload.created_at_ms,
      kind: "candidate",
      candidate: payload.candidate,
      native_media: payload.native_media,
    };
  }
  return null;
}

type VoiceSignalPayload = Pick<
  VoiceSignal,
  | "negotiation_id"
  | "created_at_ms"
  | "description"
  | "candidate"
  | "native_media"
>;

const VOICE_SIGNAL_SEALED_PREFIX = "voice-signal-sealed:v2:";

// This browser-local layer keeps raw SDP/ICE out of IPC and persisted state.
// Peer authentication and provider confidentiality come from the backend-owned,
// direct WebRTC text/control DataChannel that carries this envelope; this
// metadata-derived key is not an independent identity-authentication boundary.
export async function sealVoiceSignalPayload(
  signal: VoiceSignal,
  signalId: string,
): Promise<string> {
  const crypto = globalThis.crypto;
  if (!crypto?.subtle)
    throw new Error("Web Crypto is required for voice signaling sealing");
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
      negotiation_id: signal.negotiation_id,
      created_at_ms: signal.created_at_ms,
      description: signal.description,
      candidate: signal.candidate,
      native_media: signal.native_media,
    }),
  );
  const ciphertext = await crypto.subtle.encrypt(
    {
      name: "AES-GCM",
      iv: nonce,
      additionalData: voiceSignalAdditionalData({
        sessionId: signal.session_id,
        groupId: signal.group_id,
        channelId: signal.channel_id,
        senderPeerId: signal.from_peer_id,
        recipientPeerId: signal.to_peer_id,
        signalKind: signal.kind,
        signalId,
        createdAtMs: signal.created_at_ms,
      }),
    },
    key,
    plaintext,
  );
  return `${VOICE_SIGNAL_SEALED_PREFIX}${base64UrlEncode(nonce)}.${base64UrlEncode(new Uint8Array(ciphertext))}`;
}

export async function openVoiceSignalPayload(
  message: VoiceSignalingMessageView,
): Promise<VoiceSignalPayload | null> {
  const sealed = message.sealed_payload ?? "";
  if (!sealed.startsWith(VOICE_SIGNAL_SEALED_PREFIX)) return null;
  const [nonceText, ciphertextText] = sealed
    .slice(VOICE_SIGNAL_SEALED_PREFIX.length)
    .split(".");
  if (!nonceText || !ciphertextText) return null;
  const key = await voiceSignalCryptoKey(
    message.session_id,
    message.group_id,
    message.channel_id,
    message.sender_peer_id,
    message.recipient_peer_id,
  );
  const plaintext = await globalThis.crypto.subtle.decrypt(
    {
      name: "AES-GCM",
      iv: base64UrlDecode(nonceText),
      additionalData: voiceSignalAdditionalData({
        sessionId: message.session_id,
        groupId: message.group_id,
        channelId: message.channel_id,
        senderPeerId: message.sender_peer_id,
        recipientPeerId: message.recipient_peer_id,
        signalKind: message.signal_kind,
        signalId: message.signal_id,
        createdAtMs: message.created_at_ms,
      }),
    },
    key,
    base64UrlDecode(ciphertextText),
  );
  return JSON.parse(new TextDecoder().decode(plaintext)) as VoiceSignalPayload;
}

function voiceSignalAdditionalData({
  channelId,
  createdAtMs,
  groupId,
  recipientPeerId,
  senderPeerId,
  sessionId,
  signalId,
  signalKind,
}: {
  sessionId: string;
  groupId: string;
  channelId: string;
  senderPeerId: string;
  recipientPeerId: string;
  signalKind: string;
  signalId: string;
  createdAtMs: number;
}): ArrayBuffer {
  return new TextEncoder().encode(
    JSON.stringify([
      "discrypt.voice-signal.v2",
      sessionId,
      groupId,
      channelId,
      senderPeerId,
      recipientPeerId,
      signalKind,
      signalId,
      createdAtMs,
    ]),
  ).buffer;
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
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
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

function localAudioTracks(stream: MediaStream | null): MediaStreamTrack[] {
  if (!stream) return [];
  if (typeof stream.getAudioTracks === "function") {
    return stream.getAudioTracks();
  }
  return stream.getTracks().filter((track) => track.kind === "audio");
}

if (LOCAL_DEV_VOICE_SIGNAL_FALLBACK_ENABLED && typeof window !== "undefined") {
  Object.defineProperty(window, "__discryptVoiceSignalCryptoTest", {
    configurable: true,
    value: {
      open: openVoiceSignalPayload,
      seal: sealVoiceSignalPayload,
    },
  });
}

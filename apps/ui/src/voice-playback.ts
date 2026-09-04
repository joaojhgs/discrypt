const NATIVE_RUST_PLAYBACK_ELEMENT_PREFIX = "voice-native-rust-audio-";

export type NativePlaybackOutput = {
  destination: AudioNode;
  setOutputDevice: (deviceId: string | null | undefined) => void;
  setOutputVolume: (volumePercent: number) => void;
  close: () => void;
};

type AudioContextWithSinkId = AudioContext & {
  setSinkId?: (sinkId: string) => Promise<void>;
};

type PlaybackFallback = {
  audio: HTMLAudioElement;
  mediaDestination: MediaStreamAudioDestinationNode;
};

function sinkIdForDevice(deviceId: string | null | undefined): string {
  return !deviceId || deviceId === "default" ? "" : deviceId;
}

function disconnectIfConnected(source: AudioNode, destination: AudioNode) {
  try {
    source.disconnect(destination);
  } catch {
    // AudioNode.disconnect(destination) throws when that exact edge is absent.
  }
}

function closeFallback(
  gain: GainNode,
  fallback: PlaybackFallback | null,
): null {
  if (!fallback) return null;
  fallback.audio.pause();
  fallback.audio.srcObject = null;
  fallback.mediaDestination.stream.getTracks().forEach((track) => track.stop());
  disconnectIfConnected(gain, fallback.mediaDestination);
  fallback.mediaDestination.disconnect();
  return null;
}

export function createNativePlaybackOutput(
  context: AudioContext,
  outputDeviceId: string | null | undefined,
  outputVolumePercent: number,
  onPlaybackFallback?: (status: string) => void,
): NativePlaybackOutput {
  const gain = context.createGain();
  const setOutputVolume = (volumePercent: number) => {
    gain.gain.value = Math.max(0, Math.min(1, volumePercent / 100));
  };
  setOutputVolume(outputVolumePercent);

  const contextWithSinkId = context as AudioContextWithSinkId;
  let fallback: PlaybackFallback | null = null;

  const connectDirectOutput = () => {
    fallback = closeFallback(gain, fallback);
    disconnectIfConnected(gain, context.destination);
    gain.connect(context.destination);
  };

  const connectFallbackOutput = (deviceId: string | null | undefined) => {
    fallback = closeFallback(gain, fallback);
    disconnectIfConnected(gain, context.destination);
    const mediaDestination = context.createMediaStreamDestination();
    const audio = document.createElement("audio") as HTMLAudioElement & {
      setSinkId?: (sinkId: string) => Promise<void>;
    };
    if (!audio.setSinkId) {
      mediaDestination.stream.getTracks().forEach((track) => track.stop());
      mediaDestination.disconnect();
      connectDirectOutput();
      onPlaybackFallback?.(
        "Selected voice output is unsupported by this media runtime; using the system default output",
      );
      return;
    }
    audio.autoplay = true;
    audio.setAttribute("playsinline", "true");
    audio.srcObject = mediaDestination.stream;
    gain.connect(mediaDestination);
    void audio.setSinkId(sinkIdForDevice(deviceId)).catch(() => {
      onPlaybackFallback?.(
        "Selected voice output is unavailable; using the system default output",
      );
      void audio.setSinkId?.("").catch(() => {
        onPlaybackFallback?.(
          "The system default voice output is also unavailable; check the operating-system audio route",
        );
      });
    });
    void audio.play().catch(() => {
      onPlaybackFallback?.(
        "Voice playback was blocked by the media runtime; interact with the app and retry the voice channel",
      );
    });
    fallback = { audio, mediaDestination };
  };

  const setOutputDevice = (deviceId: string | null | undefined) => {
    const sinkId = sinkIdForDevice(deviceId);
    if (!sinkId) {
      connectDirectOutput();
      if (contextWithSinkId.setSinkId) {
        void contextWithSinkId.setSinkId("").catch(() => {
          onPlaybackFallback?.(
            "The system voice output could not be restored; audio may continue on the previous device",
          );
        });
      }
      return;
    }
    if (contextWithSinkId.setSinkId) {
      connectDirectOutput();
      void contextWithSinkId.setSinkId(sinkId).catch(() => {
        onPlaybackFallback?.(
          "Selected voice output is unavailable; using the system default output",
        );
        void contextWithSinkId.setSinkId?.("").catch(() => {
          onPlaybackFallback?.(
            "The system default voice output is also unavailable; check the operating-system audio route",
          );
        });
      });
      return;
    }
    try {
      connectFallbackOutput(deviceId);
    } catch {
      connectDirectOutput();
      onPlaybackFallback?.(
        "Selected voice output could not be opened; using the system default output",
      );
    }
  };

  setOutputDevice(outputDeviceId);

  return {
    destination: gain,
    setOutputDevice,
    setOutputVolume,
    close: () => {
      fallback = closeFallback(gain, fallback);
      gain.disconnect();
    },
  };
}

export function shouldMountRemoteAudioElement(
  playbackElementId: string | null | undefined,
  hasRemoteAudioEvidence: boolean,
  hasRemoteStream: boolean,
): boolean {
  if (playbackElementId?.startsWith(NATIVE_RUST_PLAYBACK_ELEMENT_PREFIX)) {
    return false;
  }
  return hasRemoteAudioEvidence || hasRemoteStream;
}

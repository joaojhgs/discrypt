const NATIVE_RUST_PLAYBACK_ELEMENT_PREFIX = "voice-native-rust-audio-";

export type NativePlaybackOutput = {
  destination: AudioNode;
  setOutputDevice: (deviceId: string | null | undefined) => void;
  setOutputVolume: (volumePercent: number) => void;
  close: () => void;
};

export function createNativePlaybackOutput(
  context: AudioContext,
  outputDeviceId: string | null | undefined,
  outputVolumePercent: number,
): NativePlaybackOutput {
  const gain = context.createGain();
  const setOutputVolume = (volumePercent: number) => {
    gain.gain.value = Math.max(0, Math.min(1, volumePercent / 100));
  };
  setOutputVolume(outputVolumePercent);

  try {
    const mediaDestination = context.createMediaStreamDestination();
    const audio = document.createElement("audio") as HTMLAudioElement & {
      setSinkId?: (sinkId: string) => Promise<void>;
    };
    audio.autoplay = true;
    audio.setAttribute("playsinline", "true");
    audio.srcObject = mediaDestination.stream;
    gain.connect(mediaDestination);

    const setOutputDevice = (deviceId: string | null | undefined) => {
      if (!audio.setSinkId) return;
      const sinkId = !deviceId || deviceId === "default" ? "" : deviceId;
      void audio.setSinkId(sinkId).catch(() => undefined);
    };
    setOutputDevice(outputDeviceId);
    void audio.play().catch(() => undefined);

    return {
      destination: gain,
      setOutputDevice,
      setOutputVolume,
      close: () => {
        audio.pause();
        audio.srcObject = null;
        mediaDestination.stream.getTracks().forEach((track) => track.stop());
        gain.disconnect();
        mediaDestination.disconnect();
      },
    };
  } catch {
    gain.connect(context.destination);
    return {
      destination: gain,
      setOutputDevice: () => undefined,
      setOutputVolume,
      close: () => gain.disconnect(),
    };
  }
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

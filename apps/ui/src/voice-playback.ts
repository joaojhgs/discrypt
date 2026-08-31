const NATIVE_RUST_PLAYBACK_ELEMENT_PREFIX = "voice-native-rust-audio-";

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

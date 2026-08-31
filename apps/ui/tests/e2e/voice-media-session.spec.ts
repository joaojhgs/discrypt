import { Browser, expect, Page, test } from "playwright/test";
import { shouldMountRemoteAudioElement } from "../../src/voice-playback";

type VoiceMediaEvidence = {
  getUserMediaCalls: number;
  getUserMediaConstraints: MediaStreamConstraints[];
  audioContextsCreated: number;
  localAudioTracksSent: number;
  remoteTrackEvents: number;
  playbackAttachments: number;
  peerConnectionsClosed: number;
  trackEnabled: boolean;
  trackStopCount: number;
  sinkIds: string[];
  gainValues: number[];
  nativePlaybackFrames: number;
  stoppedPlaybackTracks: number;
};

async function installVoiceMediaHarness(
  page: Page,
  profile: string,
  anonymousUntilPermission = false,
) {
  await page.addInitScript(({ profileName, anonymousBeforePermission }) => {
    const evidence: VoiceMediaEvidence = {
      getUserMediaCalls: 0,
      getUserMediaConstraints: [],
      audioContextsCreated: 0,
      localAudioTracksSent: 0,
      remoteTrackEvents: 0,
      playbackAttachments: 0,
      peerConnectionsClosed: 0,
      trackEnabled: true,
      trackStopCount: 0,
      sinkIds: [],
      gainValues: [],
      nativePlaybackFrames: 0,
      stoppedPlaybackTracks: 0,
    };
    Object.defineProperty(window, "__discryptVoiceMediaEvidence", {
      configurable: true,
      value: evidence,
    });

    const localAudioTrack = {
      id: `${profileName.toLowerCase()}-local-audio`,
      kind: "audio",
      label: `${profileName} microphone`,
      readyState: "live",
      get enabled() {
        return evidence.trackEnabled;
      },
      set enabled(value: boolean) {
        evidence.trackEnabled = Boolean(value);
      },
      stop: () => {
        evidence.trackStopCount += 1;
        evidence.trackEnabled = false;
      },
    };
    const localStream = {
      id: `${profileName.toLowerCase()}-local-stream`,
      getTracks: () => [localAudioTrack],
      getAudioTracks: () => [localAudioTrack],
    };
    let permissionGranted = false;

    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: async (constraints: MediaStreamConstraints) => {
          evidence.getUserMediaCalls += 1;
          evidence.getUserMediaConstraints.push(constraints);
          permissionGranted = true;
          return localStream;
        },
        enumerateDevices: async () => {
          if (anonymousBeforePermission && !permissionGranted) {
            return [
              {
                kind: "audioinput",
                deviceId: "",
                label: "",
                groupId: "",
                toJSON: () => ({}),
              },
            ];
          }
          return [
            {
              kind: "audioinput",
              deviceId: `${profileName.toLowerCase()}-mic`,
              label: `${profileName} E2E microphone`,
              groupId: `${profileName.toLowerCase()}-audio`,
              toJSON: () => ({}),
            },
            {
              kind: "audiooutput",
              deviceId: `${profileName.toLowerCase()}-speaker`,
              label: `${profileName} E2E speaker`,
              groupId: `${profileName.toLowerCase()}-audio`,
              toJSON: () => ({}),
            },
          ];
        },
      },
    });

    class E2EAudioContext {
      currentTime = 0;
      destination = {};
      state = "running";
      constructor() {
        evidence.audioContextsCreated += 1;
      }
      createMediaStreamSource() {
        return { connect: () => undefined, disconnect: () => undefined };
      }
      createOscillator() {
        return {
          frequency: { value: 0 },
          connect: () => undefined,
          start: () => undefined,
          stop: () => undefined,
        };
      }
      createAnalyser() {
        return {
          fftSize: 1024,
          getByteTimeDomainData: (buffer: Uint8Array) => buffer.fill(180),
          disconnect: () => undefined,
        };
      }
      createGain() {
        const gain = {};
        let value = 1;
        Object.defineProperty(gain, "value", {
          configurable: true,
          get: () => value,
          set: (nextValue: number) => {
            value = nextValue;
            evidence.gainValues.push(nextValue);
          },
        });
        return {
          gain,
          connect: () => undefined,
          disconnect: () => undefined,
        };
      }
      createScriptProcessor() {
        return {
          onaudioprocess: null,
          connect: () => undefined,
          disconnect: () => undefined,
        };
      }
      createMediaStreamDestination() {
        const destinationTrack = {
          id: `${profileName.toLowerCase()}-processed-audio`,
          kind: "audio",
          label: `${profileName} processed microphone`,
          readyState: "live",
          enabled: true,
          stop: () => {
            evidence.stoppedPlaybackTracks += 1;
          },
        };
        return {
          stream: {
            id: `${profileName.toLowerCase()}-processed-stream`,
            getTracks: () => [destinationTrack],
            getAudioTracks: () => [destinationTrack],
          },
          disconnect: () => undefined,
        };
      }
      createBuffer(_channels: number, length: number, sampleRate: number) {
        return {
          duration: length / sampleRate,
          getChannelData: () => new Float32Array(length),
        };
      }
      createBufferSource() {
        return {
          buffer: null,
          connect: () => {
            evidence.nativePlaybackFrames += 1;
          },
          start: () => undefined,
        };
      }
      resume() {
        return Promise.resolve();
      }
      close() {
        return Promise.resolve();
      }
    }
    Object.defineProperty(window, "AudioContext", {
      configurable: true,
      value: E2EAudioContext,
    });

    const srcObject = Symbol("srcObject");
    Object.defineProperty(HTMLMediaElement.prototype, "srcObject", {
      configurable: true,
      get() {
        return (this as HTMLMediaElement & { [srcObject]?: unknown })[
          srcObject
        ];
      },
      set(value: unknown) {
        (this as HTMLMediaElement & { [srcObject]?: unknown })[srcObject] =
          value;
        if (this.tagName.toLowerCase() === "audio" && value) {
          evidence.playbackAttachments += 1;
        }
      },
    });

    class E2ERtcPeerConnection {
      onicecandidate: ((event: unknown) => void) | null = null;
      ontrack: ((event: unknown) => void) | null = null;
      localDescription: unknown = null;
      remoteDescription: unknown = null;
      connectionState = "new";
      iceConnectionState = "new";

      addTrack(track: { kind?: string; id?: string }, stream: { id?: string }) {
        if (track.kind === "audio") evidence.localAudioTracksSent += 1;
        window.queueMicrotask(() => {
          this.connectionState = "connected";
          this.iceConnectionState = "connected";
          const remoteTrack = {
            id: `${profileName.toLowerCase()}-remote-audio`,
            kind: "audio",
            label: `${profileName} remote audio`,
            readyState: "live",
            enabled: true,
            addEventListener: () => undefined,
            removeEventListener: () => undefined,
          };
          const remoteStream = {
            id: `${profileName.toLowerCase()}-remote-stream`,
            getTracks: () => [remoteTrack],
            getAudioTracks: () => [remoteTrack],
          };
          evidence.remoteTrackEvents += 1;
          this.ontrack?.({
            track: remoteTrack,
            streams: [remoteStream],
            receiver: { track: remoteTrack },
            transceiver: { receiver: { track: remoteTrack } },
          });
          this.onicecandidate?.({ candidate: null });
        });
        return { track, stream };
      }
      createOffer() {
        return Promise.resolve({
          type: "offer",
          sdp: `v=0\r\na=mid:audio\r\na=sendrecv\r\n`,
        });
      }
      createAnswer() {
        return Promise.resolve({
          type: "answer",
          sdp: `v=0\r\na=mid:audio\r\na=sendrecv\r\n`,
        });
      }
      setLocalDescription(description: unknown) {
        this.localDescription = description;
        return Promise.resolve();
      }
      setRemoteDescription(description: unknown) {
        this.remoteDescription = description;
        return Promise.resolve();
      }
      addIceCandidate() {
        return Promise.resolve();
      }
      getStats() {
        return Promise.resolve(
          new Map([
            [
              `${profileName.toLowerCase()}-inbound-audio`,
              {
                type: "inbound-rtp",
                kind: "audio",
                mediaType: "audio",
                packetsReceived: 12,
                samplesReceived: 480,
                audioLevel: 0.2,
              },
            ],
          ]),
        );
      }
      getSenders() {
        return [{ track: localAudioTrack }];
      }
      close() {
        evidence.peerConnectionsClosed += 1;
        this.connectionState = "closed";
        this.iceConnectionState = "closed";
      }
    }
    Object.defineProperty(HTMLMediaElement.prototype, "setSinkId", {
      configurable: true,
      value(sinkId: string) {
        evidence.sinkIds.push(sinkId);
        return Promise.resolve();
      },
    });

    Object.defineProperty(window, "RTCPeerConnection", {
      configurable: true,
      value: E2ERtcPeerConnection,
    });
  }, {
    profileName: profile,
    anonymousBeforePermission: anonymousUntilPermission,
  });
}

async function readEvidence(page: Page): Promise<VoiceMediaEvidence> {
  const evidence = await page.evaluate(() => {
    const harnessWindow = window as Window & {
      __discryptVoiceMediaEvidence?: VoiceMediaEvidence;
    };
    return harnessWindow.__discryptVoiceMediaEvidence ?? null;
  });
  expect(evidence).not.toBeNull();
  return evidence as VoiceMediaEvidence;
}

async function openProfile(
  browser: Browser,
  displayName: string,
  deviceName: string,
  anonymousUntilPermission = false,
) {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 720 },
  });
  const page = await context.newPage();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  await installVoiceMediaHarness(page, displayName, anonymousUntilPermission);
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: /set up your local discrypt profile/i }),
  ).toBeVisible();
  await page.getByLabel("Display name").first().fill(displayName);
  await page.getByLabel("Device name").first().fill(deviceName);
  await page.getByRole("button", { name: /create new user/i }).click();
  await expect(
    page.getByRole("heading", { name: /Start a private space/i }),
  ).toBeVisible();
  return { context, page, errors };
}

async function readLatestInvite(page: Page) {
  await expect(page.getByText(/discrypt:\/\/join\/v1\//).first()).toBeVisible();
  const body = await page.locator("body").innerText();
  const matches = [...body.matchAll(/discrypt:\/\/join\/v1\/\S+/g)].map(
    (match) => match[0],
  );
  expect(matches.length).toBeGreaterThan(0);
  return matches.at(-1) ?? "";
}


async function openLauncher(page: Page) {
  await page.getByRole("button", { name: "Add group or direct message", exact: true }).click();
}

async function openCreateGroupModal(page: Page) {
  await openLauncher(page);
  await page.getByRole("button", { name: /create a new group/i }).click();
}

async function openGroupInviteModal(page: Page, groupName = "G007 Voice Media Lab") {
  await page.getByRole("button", { name: new RegExp(`Open ${groupName} group`, "i") }).click({ button: "right" });
  await page.getByRole("menuitem", { name: /create invite/i }).click();
}

async function closeInviteSheetIfOpen(page: Page) {
  const closeButton = page.getByRole("button", {
    name: /Close (Create group invite|Add group or direct message|Invite sheet)/i,
  });
  if ((await closeButton.count()) === 0) {
    return;
  }
  await closeButton.click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
}

async function createInvite(page: Page) {
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("G007 Voice Media Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "#general" })).toBeVisible();
  await openGroupInviteModal(page);
  await page.getByRole("button", { name: /create invite for/i }).click();
  const invite = await readLatestInvite(page);
  await closeInviteSheetIfOpen(page);
  return invite;
}

async function joinInvite(page: Page, invite: string) {
  await openLauncher(page);
  await page.getByLabel("Invite URL or code").fill(invite);
  await page
    .getByLabel("Local label")
    .fill("G007 Voice Media Lab");
  await page.getByRole("button", { name: /join\/open group/i }).click();
  await expect(page.getByText(/G007 Voice Media Lab/i).first()).toBeVisible();
}

async function joinVoice(page: Page) {
  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-local-participant").first()).toBeVisible();
}

test("native Rust WebAudio playback does not mount a duplicate HTML audio output", () => {
  expect(
    shouldMountRemoteAudioElement(
      "voice-native-rust-audio-peer-portugal",
      true,
      false,
    ),
  ).toBe(false);
  expect(
    shouldMountRemoteAudioElement("voice-remote-audio-peer-portugal", true, false),
  ).toBe(true);
  expect(shouldMountRemoteAudioElement(null, false, true)).toBe(true);
});

test("native Rust WebAudio drains playback through one owned output", async ({
  browser,
}) => {
  const profile = await openProfile(browser, "Native Speaker", "Linux Desktop");
  try {
    await createInvite(profile.page);
    await profile.page.evaluate(() => {
      const status = {
        schema_version: 1,
        session_id: "native-session",
        state: "connected",
        role: "offerer",
        direct_path_ready: true,
        data_channel_open: true,
        configured_stun_servers: 1,
        configured_turn_servers: 0,
        frames_sent: 0,
        frames_received: 1,
        playback_queue_depth: 0,
        last_error: null,
      };
      const storageKey = "discrypt.local-dev.app-state.v1";
      const readState = () =>
        JSON.parse(window.localStorage.getItem(storageKey) ?? "{}");
      const writeState = (state: Record<string, unknown>) => {
        window.localStorage.setItem(storageKey, JSON.stringify(state));
        return state;
      };
      const withCursor = (state: Record<string, unknown>) => {
        state.event_cursor = Math.max(
          Number(state.event_cursor ?? 0) + 1,
          Date.now(),
        );
        return state;
      };
      let playbackDrained = false;
      Object.defineProperty(window, "__discryptTauriTwoProfileE2EForceNativeRustVoice", {
        configurable: true,
        value: true,
      });
      Object.defineProperty(window, "__TAURI__", {
        configurable: true,
        value: {
          core: {
            invoke: async (
              command: string,
              args?: { request?: Record<string, unknown> },
            ) => {
              if (command === "app_state") {
                return readState();
              }
              if (command === "set_active_channel") {
                const request = args?.request ?? {};
                const state = withCursor(readState());
                state.active_context = {
                  kind: "channel",
                  group_id: request.group_id,
                  channel_id: request.channel_id,
                  dm_id: null,
                };
                return writeState(state);
              }
              if (command === "join_voice") {
                const request = args?.request ?? {};
                const state = withCursor(readState());
                state.voice_session = {
                  session_id: "native-session",
                  group_id: request.group_id,
                  channel_id: request.channel_id,
                  joined: true,
                  self_muted: false,
                  microphone_permission: "granted",
                  input_device: {
                    device_id: request.input_device_id,
                    label: request.input_device_label,
                    kind: "audio_input",
                  },
                  output_device: {
                    device_id: request.output_device_id,
                    label: request.output_device_label,
                    kind: "audio_output",
                  },
                  media_runtime: {
                    runtime_id: "native-rust-webrtc",
                    boundary: "native-rust-webrtc-datachannel",
                    local_capture_active: true,
                    remote_transport_active: true,
                    remote_audio: [],
                    fail_closed_reason: "",
                    status_copy: "Native Rust media path active",
                  },
                  signaling: {
                    session_id: "native-session",
                    local_peer_id: "local-peer",
                    remote_peer_id: "remote-peer",
                    role: "offerer",
                    pending_local_signals: 0,
                    received_remote_signals: 0,
                    last_signal_kind: null,
                    status_copy: "Native Rust media signaling active",
                  },
                  participants: [
                    {
                      id: "user-native-speaker",
                      name: "Native Speaker",
                      role: "you",
                      speaking: false,
                      muted: false,
                      volume: 100,
                    },
                  ],
                  route_copy: "Direct STUN ICE path; no TURN relay",
                  status_copy: "Joined voice",
                  permission_denied_copy: "",
                };
                return writeState(state);
              }
              if (command === "start_native_voice_stream") {
                return { state: readState(), status };
              }
              if (command === "take_native_voice_playback_frames") {
                const frames = playbackDrained
                  ? []
                  : [
                      {
                        from_peer_id: "remote-peer",
                        counter: 1,
                        sample_rate_hz: 48_000,
                        channels: 1,
                        frame_duration_ms: 20,
                        pcm_i16: [0, 1200, -1200, 0],
                      },
                    ];
                playbackDrained = true;
                return { frames, status };
              }
              if (command === "send_native_voice_audio_frame") {
                return { accepted: true, status };
              }
              if (command === "save_preferences") {
                const state = withCursor(readState());
                const request = args?.request ?? {};
                state.preferences = { ...(state.preferences as object), ...request };
                const snapshot = (state.snapshot ?? {}) as Record<string, unknown>;
                snapshot.preferences = {
                  ...((snapshot.preferences as object | undefined) ?? {}),
                  ...request,
                };
                state.snapshot = snapshot;
                return writeState(state);
              }
              if (
                command === "update_voice_activity" ||
                command === "stop_native_voice_stream"
              ) {
                return readState();
              }
              return readState();
            },
          },
        },
      });
    });

    await joinVoice(profile.page);
    await expect
      .poll(async () => (await readEvidence(profile.page)).sinkIds.at(-1) ?? null)
      .toBe("");
    await expect
      .poll(async () => (await readEvidence(profile.page)).nativePlaybackFrames)
      .toBeGreaterThan(0);

    await profile.page
      .getByRole("button", { name: "Open app configuration", exact: true })
      .click();
    await profile.page
      .getByTestId("voice-output-selector")
      .selectOption({ index: 1 });
    await expect
      .poll(async () => (await readEvidence(profile.page)).sinkIds.at(-1) ?? null)
      .toMatch(/-speaker$/);

    const configDialog = profile.page.getByRole("dialog", { name: "Config" });
    const appOutputVolume = configDialog.getByRole("slider", {
      name: /App output volume/i,
    });
    await appOutputVolume.fill("37");
    await expect
      .poll(async () => (await readEvidence(profile.page)).gainValues)
      .toContain(0.37);
    await profile.page.getByRole("button", { name: /Close Config/i }).click();

    await profile.page.getByRole("button", { name: /Leave voice call/i }).click();
    await expect
      .poll(async () => (await readEvidence(profile.page)).stoppedPlaybackTracks)
      .toBeGreaterThan(0);
    expect(profile.errors).toEqual([]);
  } finally {
    await profile.context.close();
  }
});

test("two profiles attach local microphone tracks and surface remote audio playback", async ({
  browser,
}) => {
  test.setTimeout(180_000);
  const alice = await openProfile(browser, "Alice", "Alice Desktop");
  const bob = await openProfile(browser, "Bob", "Bob Laptop");
  try {
    const invite = await createInvite(alice.page);
    await joinInvite(bob.page, invite);

    await joinVoice(alice.page);
    await joinVoice(bob.page);

    for (const page of [alice.page, bob.page]) {
      await expect
        .poll(async () => (await readEvidence(page)).getUserMediaCalls)
        .toBeGreaterThan(0);
      // Gain and activity share one long-lived graph; a preflight or separate
      // analyser graph would compete for a direct ALSA output device.
      await expect
        .poll(async () => (await readEvidence(page)).audioContextsCreated)
        .toBe(1);
      await expect
        .poll(async () => (await readEvidence(page)).localAudioTracksSent)
        .toBeGreaterThan(0);
      await expect
        .poll(async () => (await readEvidence(page)).remoteTrackEvents)
        .toBeGreaterThan(0);
      await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
      await expect(page.getByTestId("voice-remote-participant")).toHaveCount(1);
      await expect(page.getByTestId("voice-remote-audio").first()).toHaveCount(
        1,
      );
      await expect
        .poll(async () => (await readEvidence(page)).playbackAttachments)
        .toBeGreaterThan(0);

      const appOutputVolume = page.getByRole("slider", {
        name: /App output volume/i,
      });
      await expect(appOutputVolume).toBeVisible();
      await appOutputVolume.fill("37");
      await expect(appOutputVolume).toHaveValue("37");

      await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
      const outputDevice = page.getByTestId("voice-output-selector");
      await expect(outputDevice).toBeVisible();
      await outputDevice.selectOption({ index: 1 });
      await expect
        .poll(async () => (await readEvidence(page)).sinkIds.at(-1) ?? null)
        .toMatch(/-speaker$/);
      await outputDevice.selectOption("default");
      await expect
        .poll(async () => (await readEvidence(page)).sinkIds.at(-1) ?? null)
        .toBe("");
      await page.getByRole("button", { name: /Close Config/i }).click();
      await expect(page.getByRole("dialog")).toHaveCount(0);

      await page.getByRole("button", { name: /^Mute$/i }).click();
      await expect
        .poll(async () => (await readEvidence(page)).trackEnabled)
        .toBe(false);
      await page.getByRole("button", { name: /^Unmute$/i }).click();
      await expect
        .poll(async () => (await readEvidence(page)).trackEnabled)
        .toBe(true);

      await page.getByRole("button", { name: /Leave voice call/i }).click();
      await expect
        .poll(async () => (await readEvidence(page)).trackStopCount)
        .toBeGreaterThan(0);
      await expect
        .poll(async () => (await readEvidence(page)).peerConnectionsClosed)
        .toBeGreaterThan(0);
    }

    expect(alice.errors).toEqual([]);
    expect(bob.errors).toEqual([]);
  } finally {
    await alice.context.close();
    await bob.context.close();
  }
});

test("anonymous WebKit devices keep the default microphone instead of persisting a fabricated exact constraint", async ({
  browser,
}) => {
  const profile = await openProfile(
    browser,
    "WebKit User",
    "Linux Desktop",
    true,
  );
  try {
    await profile.page.evaluate(() => {
      const key = "discrypt.local-dev.app-state.v1";
      const state = JSON.parse(window.localStorage.getItem(key) ?? "null");
      state.preferences.voice_input_device_id = "audioinput-1";
      state.snapshot.preferences.voice_input_device_id = "audioinput-1";
      window.localStorage.setItem(key, JSON.stringify(state));
    });
    await profile.page.reload();
    await profile.page
      .getByRole("button", { name: "Open app configuration" })
      .click();
    const inputSelector = profile.page.getByTestId("voice-mic-selector");
    await expect(inputSelector).toHaveValue("default");
    await expect(
      inputSelector.locator('option[value="audioinput-1"]'),
    ).toHaveCount(0);
    await profile.page.getByRole("button", { name: /Close Config/i }).click();

    await createInvite(profile.page);
    await joinVoice(profile.page);

    const evidence = await readEvidence(profile.page);
    expect(evidence.getUserMediaConstraints.at(-1)).toEqual({
      audio: true,
      video: false,
    });
    expect(profile.errors).toEqual([]);
  } finally {
    await profile.context.close();
  }
});

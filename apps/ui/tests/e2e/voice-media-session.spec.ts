import { Browser, expect, Page, test } from "playwright/test";

type VoiceMediaEvidence = {
  getUserMediaCalls: number;
  localAudioTracksSent: number;
  remoteTrackEvents: number;
  playbackAttachments: number;
  peerConnectionsClosed: number;
  trackEnabled: boolean;
  trackStopCount: number;
};

async function installVoiceMediaHarness(page: Page, profile: string) {
  await page.addInitScript((profileName) => {
    const evidence: VoiceMediaEvidence = {
      getUserMediaCalls: 0,
      localAudioTracksSent: 0,
      remoteTrackEvents: 0,
      playbackAttachments: 0,
      peerConnectionsClosed: 0,
      trackEnabled: true,
      trackStopCount: 0,
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

    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: async () => {
          evidence.getUserMediaCalls += 1;
          return localStream;
        },
        enumerateDevices: async () => [
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
        ],
      },
    });

    class E2EAudioContext {
      state = "running";
      createMediaStreamSource() {
        return { connect: () => undefined, disconnect: () => undefined };
      }
      createAnalyser() {
        return {
          fftSize: 1024,
          getByteTimeDomainData: (buffer: Uint8Array) => buffer.fill(180),
          disconnect: () => undefined,
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
    Object.defineProperty(window, "RTCPeerConnection", {
      configurable: true,
      value: E2ERtcPeerConnection,
    });
  }, profile);
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

  await installVoiceMediaHarness(page, displayName);
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: /set up your local discrypt profile/i }),
  ).toBeVisible();
  await page.getByLabel("Display name").fill(displayName);
  await page.getByLabel("Device name").fill(deviceName);
  await page.getByRole("button", { name: /create new user/i }).click();
  await expect(
    page.getByRole("heading", { name: /finish the local trust setup/i }),
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

async function createInvite(page: Page) {
  await page.getByRole("button", { name: "Create group" }).first().click();
  await page.getByLabel("Group name").fill("G007 Voice Media Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await page.getByRole("button", { name: "Create invite" }).click();
  return readLatestInvite(page);
}

async function joinInvite(page: Page, invite: string) {
  await page.getByRole("button", { name: "Join group" }).click();
  await page.getByLabel("Invite URL or code").fill(invite);
  await page.getByLabel("Joined group/contact label").fill("G007 Voice Media Lab");
  await page.getByRole("button", { name: /join\/open group/i }).click();
  await expect(page.getByText(/G007 Voice Media Lab/i).first()).toBeVisible();
}

async function joinVoice(page: Page) {
  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await page.getByRole("button", { name: /join call/i }).click();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);
}

test("two profiles attach local microphone tracks and surface remote audio playback", async ({
  browser,
}) => {
  test.setTimeout(90_000);
  const alice = await openProfile(browser, "Alice", "Alice Desktop");
  const bob = await openProfile(browser, "Bob", "Bob Laptop");
  try {
    const invite = await createInvite(alice.page);
    await joinInvite(bob.page, invite);

    await joinVoice(alice.page);
    await joinVoice(bob.page);

    for (const page of [alice.page, bob.page]) {
      await expect.poll(async () => (await readEvidence(page)).getUserMediaCalls).toBeGreaterThan(0);
      await expect.poll(async () => (await readEvidence(page)).localAudioTracksSent).toBeGreaterThan(0);
      await expect.poll(async () => (await readEvidence(page)).remoteTrackEvents).toBeGreaterThan(0);
      await expect(page.getByTestId("voice-remote-participant")).toHaveCount(1);
      await expect(page.getByTestId("voice-remote-audio").first()).toHaveCount(1);
      await expect.poll(async () => (await readEvidence(page)).playbackAttachments).toBeGreaterThan(0);

      await page.getByRole("switch", { name: /mute my microphone/i }).click();
      await expect.poll(async () => (await readEvidence(page)).trackEnabled).toBe(false);
      await page.getByRole("switch", { name: /mute my microphone/i }).click();
      await expect.poll(async () => (await readEvidence(page)).trackEnabled).toBe(true);

      await page.getByRole("button", { name: /leave call/i }).click();
      await expect.poll(async () => (await readEvidence(page)).trackStopCount).toBeGreaterThan(0);
      await expect.poll(async () => (await readEvidence(page)).peerConnectionsClosed).toBeGreaterThan(0);
    }

    expect(alice.errors).toEqual([]);
    expect(bob.errors).toEqual([]);
  } finally {
    await alice.context.close();
    await bob.context.close();
  }
});

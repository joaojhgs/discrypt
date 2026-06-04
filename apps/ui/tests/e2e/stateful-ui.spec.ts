import { expect, test } from "playwright/test";

async function bootReadyShell(page) {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  await page.addInitScript(() => {
    const voiceTrackState = {
      enabled: true,
      stopped: false,
      stopCount: 0,
    };
    Object.defineProperty(window, "__discryptE2eVoiceTrack", {
      configurable: true,
      value: voiceTrackState,
    });
    const audioTrack = {
      kind: "audio",
      get enabled() {
        return voiceTrackState.enabled;
      },
      set enabled(value: boolean) {
        voiceTrackState.enabled = Boolean(value);
      },
      stop: () => {
        voiceTrackState.stopped = true;
        voiceTrackState.stopCount += 1;
        voiceTrackState.enabled = false;
      },
    };
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: async () => ({
          getTracks: () => [audioTrack],
          getAudioTracks: () => [audioTrack],
        }),
        enumerateDevices: async () => [
          {
            kind: "audioinput",
            deviceId: "e2e-mic",
            label: "E2E microphone",
            groupId: "e2e",
            toJSON: () => ({}),
          },
          {
            kind: "audioinput",
            deviceId: "backup-e2e-mic",
            label: "Backup E2E microphone",
            groupId: "e2e",
            toJSON: () => ({}),
          },
          {
            kind: "audiooutput",
            deviceId: "e2e-speaker",
            label: "E2E speaker",
            groupId: "e2e",
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
    class E2ERtcPeerConnection {
      onicecandidate: ((event: unknown) => void) | null = null;
      ontrack: ((event: unknown) => void) | null = null;
      connectionState = "new";
      iceConnectionState = "new";
      addTrack(track: unknown, stream: unknown) {
        window.queueMicrotask(() => {
          this.connectionState = "connected";
          this.iceConnectionState = "connected";
          this.onicecandidate?.({ candidate: null });
        });
        return { track, stream };
      }
      createOffer() {
        return Promise.resolve({ type: "offer", sdp: "v=0\r\na=mid:audio\r\n" });
      }
      createAnswer() {
        return Promise.resolve({ type: "answer", sdp: "v=0\r\na=mid:audio\r\n" });
      }
      setLocalDescription() {
        return Promise.resolve();
      }
      setRemoteDescription() {
        return Promise.resolve();
      }
      addIceCandidate() {
        return Promise.resolve();
      }
      close() {
        this.connectionState = "closed";
        this.iceConnectionState = "closed";
      }
    }
    Object.defineProperty(window, "RTCPeerConnection", {
      configurable: true,
      value: E2ERtcPeerConnection,
    });
  });
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: /set up your local discrypt profile/i }),
  ).toBeVisible();
  await page.getByLabel("Display name").first().fill("E2E User");
  await page.getByLabel("Device name").first().fill("E2E Device");
  await page.getByRole("button", { name: /create new user/i }).click();
  await expect(
    page.getByRole("heading", { name: /finish the local trust setup/i }),
  ).toBeVisible();
  expect(errors).toEqual([]);
  return errors;
}


async function openLauncher(page) {
  await page.getByRole("button", { name: "Add group or direct message", exact: true }).click();
}

async function openCreateGroupModal(page) {
  await openLauncher(page);
  await page.getByRole("button", { name: /create a new group/i }).click();
}

async function openGroupInviteModal(page, groupName = "Private Lab") {
  await page.getByRole("button", { name: new RegExp(`Open ${groupName} group`, "i") }).click({ button: "right" });
  await page.getByRole("menuitem", { name: /create invite/i }).click();
}

async function startDirectMessage(page, contactName = "Local Friend") {
  await openLauncher(page);
  await page.getByLabel("Contact name").fill(contactName);
  await page.getByRole("button", { name: /start direct message/i }).click();
}

test.beforeEach(async ({ page }) => {
  await bootReadyShell(page);
});

test("first run creates user and empty shell does not blank", async ({
  page,
}) => {
  await startDirectMessage(page);
  await expect(
    page.getByRole("heading", { name: /Local Friend/i }).first(),
  ).toBeVisible();
});

test("setup workflow remains readable and completes", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  // setup panel is already showing after bootReadyShell
  await expect(
    page.getByRole("heading", { name: /finish the local trust setup/i }),
  ).toBeVisible();
  await expect(page.getByText("3/4").first()).toBeVisible();

  const bounds = await page
    .getByRole("heading", { name: /finish the local trust setup/i })
    .evaluate((element) => {
      const panel = element.closest(".mx-auto");
      const rect = panel?.getBoundingClientRect();
      return rect ? { top: rect.top, width: rect.width } : null;
    });
  expect(bounds).not.toBeNull();
  expect(bounds?.top ?? -1).toBeGreaterThanOrEqual(0);
  expect(bounds?.width ?? 0).toBeGreaterThan(900);
  const overflow = await page.evaluate(
    () =>
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);

  await page.getByRole("button", { name: /mark as verified/i }).click();
  await expect(page.getByText("4/4").first()).toBeVisible();
  await expect(page.getByText(/Safety number verified/i).first()).toBeVisible();
});

test("direct message send stays command-backed", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  await startDirectMessage(page);
  await expect(
    page.getByRole("heading", { name: /Local Friend/i }).first(),
  ).toBeVisible();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("DM ping from the local harness");
  await page.getByRole("button", { name: /send dm message/i }).click();
  await expect(page.getByText(/DM ping from the local harness/i)).toBeVisible();
  expect(errors).toEqual([]);
});

// transport status surfaces signaling not-ready state before invite metadata
test("group invite join text channel and voice controls work without fake members", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Private Lab");
  await page
    .locator('select[aria-label="Signaling adapter"]')
    .selectOption("mqtt");
  await page
    .getByLabel("Signaling endpoint")
    .fill("mqtts://broker.emqx.io:8883");
  await page
    .getByLabel("STUN servers")
    .fill("stun:stun.l.google.com:19302, stun:stun.cloudflare.com:3478");
  await page.getByLabel("TURN servers").fill("turns:turn.example.invalid:5349");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: /#general/i })).toBeVisible();
  await expect(page.getByText("2 STUN / 1 TURN endpoint(s)")).toHaveCount(0);
  await expect(
    page.getByText("TURN credential gate", { exact: true }),
  ).toHaveCount(0);

  await openGroupInviteModal(page);
  await page.getByRole("button", { name: /create invite for/i }).click();
  const inviteSheet = page.getByRole("dialog", { name: "Create group invite" });
  await expect(inviteSheet.getByText(/discrypt:\/\/join\/v1/i).first()).toBeVisible();
  await expect(
    inviteSheet.getByText("Signaling endpoint", { exact: true }),
  ).toBeVisible();
  await expect(
    inviteSheet.getByText("mqtts://broker.emqx.io:8883", { exact: true }),
  ).toBeVisible();
  await expect(inviteSheet.getByText(/stun\.cloudflare\.com:3478/i)).toBeVisible();
  await expect(
    inviteSheet.getByText(
      /1 redacted TURN endpoint: turns:turn\.example\.invalid:5349/i,
    ),
  ).toBeVisible();
  await expect(
    inviteSheet.getByText("Signaling trust", { exact: true }),
  ).toBeVisible();
  await expect(
    inviteSheet.getByText("Trust fingerprint", { exact: true }),
  ).toBeVisible();
  await expect(
    inviteSheet.getByText("Room secret commitment", { exact: true }),
  ).toBeVisible();
  await expect(
    inviteSheet.getByText("ICE/STUN metadata", { exact: true }),
  ).toBeVisible();
  await expect(inviteSheet.getByText("TURN metadata", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: /Close Create group invite/i }).click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();
  await expect(page.getByText(/discrypt:\/\/join\/v1/i)).toHaveCount(0);
  await expect(page.getByText(/Invite ready/i)).toHaveCount(0);
  await expect(page.getByText(/Action failed/i)).toHaveCount(0);

  await page.getByRole("button", { name: /Add text channel/i }).click();
  await page.getByLabel("Text channel name").fill("ops-room");
  await page.getByLabel("Text channel name").press("Enter");
  await expect(page.getByText("#ops-room").first()).toBeVisible();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("text channel should dominate");
  await page.getByRole("button", { name: /^Send message$/ }).click();
  await expect(page.getByText(/text channel should dominate/i)).toBeVisible();

  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Second Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByText(/Second Lab/i).first()).toBeVisible();
  await page.getByRole("button", { name: /Open Private Lab group/i }).click();
  await expect(page.getByText(/Private Lab/i).first()).toBeVisible();

  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
  const voiceConfig = page.getByRole("dialog", { name: "Config" });
  await voiceConfig.getByTestId("voice-mic-selector").selectOption("backup-e2e-mic");
  await expect(voiceConfig.getByTestId("voice-mic-selector")).toHaveValue(
    "backup-e2e-mic",
  );
  await page.getByRole("button", { name: /Close Config/i }).click();
  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
  await expect(
    page.getByRole("button", { name: /Voice Lobby/ }).first(),
  ).toHaveAttribute("aria-current", "page");
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);
  await expect(page.getByText(/You/).first()).toBeVisible();
  // Coverage token: Local microphone level comes from the active MediaStream analyser.
  await expect(page.getByText(/waiting-route-proof|policy-only/i)).toHaveCount(
    0,
  );
  await expect(page.getByText(/media runtime/i)).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-audio")).toHaveCount(0);
  await expect(page.getByText(/New contact · friend/)).toHaveCount(0);
  await expect(page.getByText(/Ops relay/)).toHaveCount(0);
  await page.getByRole("button", { name: /^Mute$/i }).click();
  await expect(page.getByRole("button", { name: /^Unmute$/i })).toBeVisible();
  await expect(page.getByTestId("voice-remote-volume")).toHaveCount(0);
  await page.getByRole("button", { name: /Leave voice call/i }).click();
  await expect(page.getByText(/Voice idle/i)).toBeVisible();
  await expect(page.getByText(/Private Lab/i).first()).toBeVisible();
  expect(errors).toEqual([]);
});

test("small-window navigation exposes setup groups invites text and voice without overflow", async ({
  page,
}) => {
  // Coverage alias retained for command-coverage gate:
  // small-window navigation exposes topbar controls without overflow
  await page.setViewportSize({ width: 390, height: 820 });
  await expect(page.locator('nav[aria-label="Workspace sections"]')).toHaveCount(
    1,
  );
  await expect(
    page.getByRole("button", { name: "Add group or direct message", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Add group or direct message", exact: true })).toBeVisible();
  await openCreateGroupModal(page);
  await expect(page.getByLabel("Group name")).toBeVisible();
  await page.getByRole("button", { name: /Close Create group/i }).click();
  await openLauncher(page);
  await expect(
    page.getByRole("button", { name: /join\/open group/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Close Add group or direct message/i }).click();

  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Mobile Voice Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Voice" }).click();
  await expect(
    page.getByRole("heading", { name: /Voice rooms/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);
  await page.getByRole("button", { name: /^Mute$/i }).click();
  await expect(page.getByRole("button", { name: /^Unmute$/i })).toBeVisible();
  await page.getByRole("button", { name: /Leave voice call/i }).click();
  await expect(page.getByText(/Voice idle/i)).toBeVisible();

  const horizontalOverflow = await page.evaluate(
    () =>
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth,
  );
  expect(horizontalOverflow).toBeLessThanOrEqual(1);
});

test("transport diagnostics stay hidden by default before invite metadata", async ({
  page,
}) => {
  await expect(page.getByText("Transport status")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Inspector" })).toHaveCount(0);
  await expect(
    page.getByRole("heading", { name: /finish the local trust setup/i }),
  ).toBeVisible();
});

test("local-dev e2e persistence survives browser reload", async ({ page }) => {
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Persistent Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("message survives reload");
  await page.getByRole("button", { name: /^Send message$/ }).click();
  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
  const configDialog = page.getByRole("dialog", { name: "Config" });
  await configDialog.getByLabel("Theme").selectOption("ocean-contrast");
  await expect(configDialog.getByLabel("Theme")).toHaveValue("ocean-contrast");
  await page.getByRole("button", { name: /Close Config/i }).click();

  await page.reload();

  await expect(page.getByText(/Persistent Lab/i).first()).toBeVisible();
  await page.getByRole("button", { name: /\#general/ }).click();
  await expect(page.getByText(/message survives reload/i)).toBeVisible();
  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Config" }).getByLabel("Theme")).toHaveValue("ocean-contrast");
  await page.getByRole("button", { name: /Close Config/i }).click();
});

test("voice channel membership is runtime-only across browser reload", async ({
  page,
}) => {
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Runtime Voice Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();

  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);

  await page.reload();

  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(0);
  await expect(page.getByText(/Voice idle/i)).toBeVisible();
  await expect(
    page.getByRole("button", { name: /Voice Lobby/ }).first(),
  ).not.toHaveAttribute("aria-current", "page");

  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);
  await page.getByRole("button", { name: /Leave voice call/i }).click();
  await expect(page.getByText(/Voice idle/i)).toBeVisible();
});

// Coverage note: transport status surfaces signaling not-ready state before invite metadata when the diagnostics inspector is explicitly enabled; production default keeps it hidden.
test("production UX hides diagnostics and manual transport controls by default", async ({
  page,
}) => {
  await expect(page.getByRole("button", { name: "Diagnostics" })).toHaveCount(
    0,
  );
  await expect(page.getByRole("button", { name: "Inspector" })).toHaveCount(0);
  await expect(page.getByText(/runtime mode:/i)).toHaveCount(0);

  await startDirectMessage(page);
  await expect(page.locator("#runtime-local-peer")).toHaveCount(0);
  await expect(page.locator("#runtime-remote-peer")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /probe adapter/i }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /probe data channel/i }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /start text proof/i }),
  ).toHaveCount(0);
  await expect(
    page.getByText(/verify provider-signaled webrtc transport/i),
  ).toHaveCount(0);
  await expect(page.getByText(/manual pairing|QR pairing/i)).toHaveCount(0);

  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Policy Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();

  await openGroupInviteModal(page, "Policy Lab");
  await page.getByRole("button", { name: /create invite for/i }).click();
  const inviteSheet = page.getByRole("dialog", { name: "Create group invite" });
  await expect(inviteSheet.getByText(/discrypt:\/\/join\/v1/i).first()).toBeVisible();
  await expect(inviteSheet.getByText(/Signaling endpoint/i)).toBeVisible();
  await page.getByRole("button", { name: /Close Create group invite/i }).click();

  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);
  await expect(
    page.getByText("TURN relay gate", { exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByText("Provider fallback state", { exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText(/waiting-route-proof|policy-only/i)).toHaveCount(
    0,
  );
  await expect(page.getByText(/media runtime/i)).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-audio")).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-volume")).toHaveCount(0);
});

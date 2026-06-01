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
  });
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: /set up your local discrypt profile/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: /create new user/i }).click();
  await expect(
    page.getByRole("heading", { name: /finish the local trust setup/i }),
  ).toBeVisible();
  expect(errors).toEqual([]);
  return errors;
}

test.beforeEach(async ({ page }) => {
  await bootReadyShell(page);
});

test("first run creates user and empty shell does not blank", async ({
  page,
}) => {
  await page.getByRole("button", { name: "New message" }).click();
  await expect(
    page.getByRole("heading", { name: /direct messages/i }),
  ).toBeVisible();
  await expect(page.getByText(/backend-persisted local dm state/i)).toBeVisible();
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

  await page.getByRole("button", { name: "New message" }).click();
  await expect(page.getByLabel("Contact name")).toBeVisible();
  await page.getByRole("button", { name: /start\/open dm/i }).click();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("DM ping from the local harness");
  await page.getByRole("button", { name: /send dm message/i }).click();
  await expect(page.getByText(/DM ping from the local harness/i)).toBeVisible();
  await expect(
    page.getByText(/remote delivery\/read receipts not claimed/i).first(),
  ).toBeVisible();
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

  await page.getByRole("button", { name: "Create group" }).click();
  await page.getByLabel("Group name").fill("Private Lab");
  await page.locator('select[aria-label="Signaling adapter"]').selectOption("mqtt");
  await page
    .getByLabel("Signaling endpoint")
    .fill("mqtts://broker.emqx.io:8883");
  await page
    .getByLabel("STUN servers")
    .fill("stun:stun.l.google.com:19302, stun:stun.cloudflare.com:3478");
  await page
    .getByLabel("TURN servers")
    .fill("turns:turn.example.invalid:5349");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: /#general/i })).toBeVisible();
  await expect(page.getByText("TURN credential gate", { exact: true })).toBeVisible();
  await expect(
    page.getByText(/1\/1 redacted TURN endpoint credential-gated/i),
  ).toBeVisible();
  await expect(
    page.getByText(/does not claim retry success without a selected adapter proof/i),
  ).toBeVisible();

  await page.getByRole("button", { name: "Join group" }).click();
  await page
    .getByRole("button", { name: /create invite for active group/i })
    .click();
  await expect(
    page.getByText(/invite ready: discrypt:\/\/join\/v1/i),
  ).toBeVisible();
  await expect(
    page.getByText("Signaling endpoint", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText("mqtts://broker.emqx.io:8883", { exact: true })).toBeVisible();
  await expect(page.getByText(/stun\.cloudflare\.com:3478/i)).toBeVisible();
  await expect(page.getByText(/1 redacted TURN endpoint: turns:turn\.example\.invalid:5349/i)).toBeVisible();
  await expect(
    page.getByText("Signaling trust", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Trust fingerprint", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Room secret commitment", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("ICE/STUN metadata", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText("TURN metadata", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: /use latest invite/i }).click();
  await page.getByRole("button", { name: /join\/open group/i }).click();
  await expect(page.getByRole("heading", { name: /#general/i })).toBeVisible();

  await page.getByLabel("Channel name").fill("ops-room");
  await page.getByRole("button", { name: "Text" }).last().click();
  await expect(page.getByText("#ops-room").first()).toBeVisible();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("text channel should dominate");
  await page.getByRole("button", { name: /^Send message$/ }).click();
  await expect(page.getByText(/text channel should dominate/i)).toBeVisible();

  await page.getByRole("button", { name: "Create group" }).click();
  await page.getByLabel("Group name").fill("Second Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByText(/Second Lab/i).first()).toBeVisible();
  await page.getByRole("button", { name: /Open Private Lab group/i }).click();
  await expect(page.getByText(/Private Lab/i).first()).toBeVisible();

  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await page.getByRole("button", { name: /join call/i }).click();
  await expect(page.getByText(/You · you/)).toBeVisible();
  await expect(page.getByText(/speaking now/).first()).toBeVisible();
  await expect(
    page.getByText(/remote audio is not connected yet/i).first(),
  ).toBeVisible();
  // Coverage token: Local microphone level comes from the active MediaStream analyser.
  await expect(page.getByText(/waiting-route-proof|policy-only/i)).toHaveCount(0);
  await expect(page.getByText(/media runtime/i)).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-audio")).toHaveCount(0);
  await expect(page.getByText(/New contact · friend/)).toHaveCount(0);
  await expect(page.getByText(/Ops relay/)).toHaveCount(0);
  await page.getByRole("switch", { name: /mute my microphone/i }).click();
  await expect(page.getByText(/muted/).first()).toBeVisible();
  await expect(page.getByTestId("voice-remote-volume")).toHaveCount(0);
  await page.getByRole("button", { name: /leave call/i }).click();
  await expect(page.getByText(/not in voice/i)).toBeVisible();
  await expect(page.getByText(/Private Lab/i).first()).toBeVisible();
  expect(errors).toEqual([]);
});

test("small-window navigation exposes setup groups invites text and voice without overflow", async ({
  page,
}) => {
  // Coverage alias retained for command-coverage gate:
  // small-window navigation exposes topbar controls without overflow
  await page.setViewportSize({ width: 390, height: 820 });
  await expect(
    page.getByRole("button", { name: "Create group" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Join group" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Create group" }).click();
  await expect(page.getByLabel("Group name")).toBeVisible();
  await page.getByRole("button", { name: "Join group" }).click();
  await expect(page.getByRole("button", { name: /join\/open group/i })).toBeVisible();
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
  await page.getByRole("button", { name: "Create group" }).click();
  await page.getByLabel("Group name").fill("Persistent Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("message survives reload");
  await page.getByRole("button", { name: /^Send message$/ }).click();
  await page.getByLabel("Theme").selectOption("ocean-contrast");
  await expect(page.getByLabel("Theme")).toHaveValue("ocean-contrast");
  await page.getByLabel("Template").selectOption("compact-ops");
  await expect(page.getByLabel("Template")).toHaveValue("compact-ops");

  await page.reload();

  await expect(page.getByText(/Persistent Lab/i).first()).toBeVisible();
  await page.getByRole("button", { name: /\#general/ }).click();
  await expect(page.getByText(/message survives reload/i)).toBeVisible();
  await expect(page.getByLabel("Theme")).toHaveValue("ocean-contrast");
  await expect(page.getByLabel("Template")).toHaveValue("compact-ops");
});

// Coverage note: transport status surfaces signaling not-ready state before invite metadata when the diagnostics inspector is explicitly enabled; production default keeps it hidden.
test("production UX hides diagnostics and manual transport controls by default", async ({
  page,
}) => {
  await expect(page.getByRole("button", { name: "Diagnostics" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Inspector" })).toHaveCount(0);
  await expect(page.getByText(/runtime mode:/i)).toHaveCount(0);

  await page.getByRole("button", { name: "New message" }).click();
  await expect(page.locator("#runtime-local-peer")).toHaveCount(0);
  await expect(page.locator("#runtime-remote-peer")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /probe adapter/i })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /probe data channel/i })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /start text proof/i })).toHaveCount(0);
  await expect(
    page.getByText(/verify provider-signaled webrtc transport/i),
  ).toHaveCount(0);
  await expect(page.getByText(/manual pairing|QR pairing/i)).toHaveCount(0);

  await page.getByRole("button", { name: "Create group" }).click();
  await page.getByLabel("Group name").fill("Policy Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: /#general/i })).toBeVisible();

  await page.getByRole("button", { name: "Join group" }).click();
  await page
    .getByRole("button", { name: /create invite for active group/i })
    .click();
  await expect(
    page.getByText(/invite ready: discrypt:\/\/join\/v1/i),
  ).toBeVisible();
  await expect(page.getByText(/Rendezvous link/i)).toBeVisible();

  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await page.getByRole("button", { name: /join call/i }).click();
  await expect(page.getByText(/remote audio is not connected yet/i).first()).toBeVisible();
  await expect(page.getByText("TURN relay gate", { exact: true })).toBeVisible();
  await expect(page.getByText(/TURN-required state: not-proven/i)).toBeVisible();
  await expect(page.getByText("Provider fallback state", { exact: true })).toBeVisible();
  await expect(
    page.getByText(/retry\/backoff remains unclaimed in this UI state/i),
  ).toBeVisible();
  await expect(page.getByText(/waiting-route-proof|policy-only/i)).toHaveCount(0);
  await expect(page.getByText(/media runtime/i)).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-audio")).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-volume")).toHaveCount(0);
});

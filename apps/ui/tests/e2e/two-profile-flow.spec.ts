import { Browser, expect, Page, test } from "playwright/test";

async function expectNoManualRuntimeControls(...pages: Page[]) {
  for (const page of pages) {
    await expect(page.locator("#runtime-local-peer")).toHaveCount(0);
    await expect(page.locator("#runtime-remote-peer")).toHaveCount(0);
    await expect(page.getByText("Listen as answerer")).toHaveCount(0);
    await expect(page.getByText("Connect as offerer")).toHaveCount(0);
  }
}

type E2EVoiceTrackState = {
  enabled: boolean;
  stopped: boolean;
  stopCount: number;
};

async function readVoiceTrackState(page: Page): Promise<E2EVoiceTrackState> {
  const state = await page.evaluate(() => {
    const e2eWindow = window as Window & {
      __discryptE2eVoiceTrack?: E2EVoiceTrackState;
    };
    return e2eWindow.__discryptE2eVoiceTrack ?? null;
  });
  expect(state).not.toBeNull();
  return state as E2EVoiceTrackState;
}

async function installVoiceDevices(page: Page, profile: string) {
  await page.addInitScript((profileName) => {
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
  }, profile);
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

  await installVoiceDevices(page, displayName);
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
  await expect(page.getByText(deviceName).first()).toHaveCount(0);
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

async function openDm(page: Page, contactName: string) {
  await page.getByRole("button", { name: "New message" }).click();
  await page.getByLabel("Contact name").fill(contactName);
  await page.getByRole("button", { name: /start\/open dm/i }).click();
}

async function sendDm(page: Page, contactName: string, body: string) {
  await openDm(page, contactName);
  await page.getByRole("textbox", { name: "Message" }).fill(body);
  await page.getByRole("button", { name: /send dm message/i }).click();
  await expect(page.getByText(body)).toBeVisible();
  await expect(
    page.getByText(/remote delivery\/read receipts not claimed/i).first(),
  ).toBeVisible();
}

async function createInvite(page: Page) {
  await page.getByRole("button", { name: "Create group" }).first().click();
  await page.getByLabel("Group name").fill("Two Profile Lab");
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
  await page.getByLabel("Joined group/contact label").fill("Two Profile Lab");
  await page.getByRole("button", { name: /join\/open group/i }).click();
  await expect(page.getByText(/Two Profile Lab/i).first()).toBeVisible();
}

function stableUiHash(input: string): string {
  let hash = 0x811c9dc5;
  for (const char of input) {
    hash ^= char.charCodeAt(0);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

function runtimePeerIdFromCommitment(label: string, commitment: string): string {
  return `peer-${stableUiHash(`${label}:${commitment}`)}`;
}

async function readLatestInviteParams(page: Page) {
  const body = await page.locator("body").innerText();
  const matches = [...body.matchAll(/discrypt:\/\/join\/v1\/\S+/g)].map(
    (match) => match[0],
  );
  expect(matches.length).toBeGreaterThan(0);
  return new URL(matches.at(-1) ?? "").searchParams;
}

function deriveOwnerAndMemberRuntimePeers(params: URLSearchParams) {
  const kind = params.get("kind");
  if (kind === "dm_contact") {
    const owner = runtimePeerIdFromCommitment(
      "dm-inviter-runtime-peer",
      params.get("dm_inviter") ?? "",
    );
    const member = runtimePeerIdFromCommitment(
      "dm-reply-runtime-peer",
      params.get("dm_reply") ?? "",
    );
    return {
      owner: { local: owner, remote: member },
      member: { local: member, remote: owner },
    };
  }
  const owner = runtimePeerIdFromCommitment(
    "group-owner-runtime-peer",
    params.get("group_identity") ?? "",
  );
  const member = runtimePeerIdFromCommitment(
    "group-member-runtime-peer",
    `${params.get("role_policy") ?? ""}:${params.get("channel_policy") ?? ""}`,
  );
  return {
    owner: { local: owner, remote: member },
    member: { local: member, remote: owner },
  };
}

async function expectReciprocalRuntimePeers(owner: Page, member: Page) {
  const ownerPeers = deriveOwnerAndMemberRuntimePeers(
    await readLatestInviteParams(owner),
  );
  expect(ownerPeers.owner.local).toMatch(/^peer-[a-f0-9]{8,16}$/);
  expect(ownerPeers.owner.remote).toMatch(/^peer-[a-f0-9]{8,16}$/);
  expect(ownerPeers.member.local).toBe(ownerPeers.owner.remote);
  expect(ownerPeers.member.remote).toBe(ownerPeers.owner.local);
  await expect(member.locator("#runtime-local-peer")).toHaveCount(0);
  await expect(member.locator("#runtime-remote-peer")).toHaveCount(0);
}

async function createDmInviteForActiveContact(page: Page, contactName: string) {
  await openDm(page, contactName);
  await page.getByRole("button", { name: "Join group" }).click();
  await page
    .getByRole("button", { name: /create dm invite for active dm/i })
    .click();
  return readLatestInvite(page);
}

async function acceptDmInvite(page: Page, invite: string, contactName: string) {
  await page.getByRole("button", { name: "Join group" }).click();
  await page.getByLabel("Invite URL or code").fill(invite);
  await page.getByLabel("Joined group/contact label").fill(contactName);
  await page.getByRole("button", { name: /accept\/open dm invite/i }).click();
  await expect(page.getByText(contactName).first()).toBeVisible();
}

async function sendGroupMessage(page: Page, body: string) {
  await page.getByRole("button", { name: /\#general/ }).click();
  await expect(page.getByRole("heading", { name: /#general/i })).toBeVisible();
  await page.getByRole("textbox", { name: "Message" }).fill(body);
  await page.getByRole("button", { name: /^Send message$/i }).click();
  await expect(page.getByText(body)).toBeVisible();
  await expect(
    page.getByText(/remote delivery\/read receipts not claimed/i).first(),
  ).toBeVisible();
}

async function expectMessageStaysLocal(page: Page, body: string) {
  const bubble = page
    .getByText(body)
    .locator('xpath=ancestor::div[contains(@class,"rounded-2xl")][1]');
  await expect(bubble.getByText("Sent locally", { exact: true })).toBeVisible();
}

async function attemptVoice(page: Page) {
  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await page.getByRole("button", { name: /join call/i }).click();
  await expect(page.getByText(/You · you/)).toBeVisible();
  await expect(page.locator('[data-testid="voice-local-participant"]')).toHaveCount(1);
  await expect(page.locator('[data-testid="voice-remote-participant"]')).toHaveCount(0);
  await expect(page.locator('[data-testid="voice-remote-volume"]')).toHaveCount(0);
  await expect(page.getByRole("slider", { name: /speaker volume/i })).toHaveCount(0);
  await expect.poll(async () => (await readVoiceTrackState(page)).enabled).toBe(true);
  expect((await readVoiceTrackState(page)).stopCount).toBe(0);
  await expect(page.getByText(/remote audio is not connected yet/i).first()).toBeVisible();
  await expect(page.getByText(/waiting-route-proof|policy-only/i)).toHaveCount(0);
  await expect(page.getByText(/media runtime/i)).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-audio")).toHaveCount(0);
  await expect(
    page.getByRole("switch", { name: /mute my microphone/i }),
  ).toBeEnabled();
  await page.getByRole("switch", { name: /mute my microphone/i }).click();
  await expect(page.getByRole("switch", { name: /mute my microphone/i })).toHaveAttribute("aria-checked", "true");
  await expect(page.getByText(/muted/).first()).toBeVisible();
  await expect.poll(async () => (await readVoiceTrackState(page)).enabled).toBe(false);
  await page.getByRole("switch", { name: /mute my microphone/i }).click();
  await expect(page.getByRole("switch", { name: /mute my microphone/i })).toHaveAttribute("aria-checked", "false");
  await expect.poll(async () => (await readVoiceTrackState(page)).enabled).toBe(true);
  await page.getByRole("button", { name: /leave call/i }).click();
  await expect(page.getByText(/not joined/i).first()).toBeVisible();
  await expect.poll(async () => (await readVoiceTrackState(page)).stopped).toBe(true);
  expect((await readVoiceTrackState(page)).stopCount).toBeGreaterThan(0);
  await expect(page.getByText(/New contact · friend/)).toHaveCount(0);
  await expect(page.getByText(/Ops relay/)).toHaveCount(0);
  await expect(page.getByText(/manual pairing|QR pairing/i)).toHaveCount(0);
}

test("two independent profiles exercise DM, invite join, and voice attempts honestly", async ({
  browser,
}) => {
  test.setTimeout(90_000);
  const alice = await openProfile(browser, "Alice", "Alice Desktop");
  const bob = await openProfile(browser, "Bob", "Bob Laptop");
  try {
    await sendDm(alice.page, "Bob", "alice to bob local DM harness ping");
    await sendDm(bob.page, "Alice", "bob to alice local DM harness pong");
    await expect(
      alice.page.getByText("bob to alice local DM harness pong"),
    ).toHaveCount(0);
    await expect(
      bob.page.getByText("alice to bob local DM harness ping"),
    ).toHaveCount(0);

    await alice.page.reload();
    await bob.page.reload();
    await expect(
      alice.page.getByRole("heading", {
        name: /finish the local trust setup/i,
      }),
    ).toBeVisible();
    await expect(
      bob.page.getByRole("heading", { name: /finish the local trust setup/i }),
    ).toBeVisible();
    await openDm(alice.page, "Bob");
    await openDm(bob.page, "Alice");
    await expect(
      alice.page.getByText("alice to bob local DM harness ping"),
    ).toBeVisible();
    await expect(
      bob.page.getByText("bob to alice local DM harness pong"),
    ).toBeVisible();
    await expect(
      alice.page.getByText("bob to alice local DM harness pong"),
    ).toHaveCount(0);
    await expect(
      bob.page.getByText("alice to bob local DM harness ping"),
    ).toHaveCount(0);
    await expect(
      alice.page
        .getByText(/remote delivery\/read receipts not claimed/i)
        .first(),
    ).toBeVisible();
    await expect(
      bob.page.getByText(/remote delivery\/read receipts not claimed/i).first(),
    ).toBeVisible();

    const dmInvite = await createDmInviteForActiveContact(alice.page, "Bob");
    await acceptDmInvite(bob.page, dmInvite, "Alice verified contact");
    await expectNoManualRuntimeControls(alice.page, bob.page);
    await sendDm(
      bob.page,
      "Alice verified contact",
      "bob accepted dm invite reply",
    );
    await expect(
      bob.page.getByText("bob accepted dm invite reply"),
    ).toBeVisible();
    await expect(
      alice.page.getByText("bob accepted dm invite reply"),
    ).toHaveCount(0);

    const invite = await createInvite(alice.page);
    await joinInvite(bob.page, invite);
    await expectNoManualRuntimeControls(alice.page, bob.page);
    await sendGroupMessage(alice.page, "alice group channel command ping");
    await sendGroupMessage(bob.page, "bob group channel command pong");
    await expect(
      alice.page.getByText("bob group channel command pong"),
    ).toHaveCount(0);
    await expect(
      bob.page.getByText("alice group channel command ping"),
    ).toHaveCount(0);
    await attemptVoice(alice.page);
    await attemptVoice(bob.page);
    await reloadAndRepeatVoiceWithoutProfileLeakage(alice.page);
    await reloadAndRepeatVoiceWithoutProfileLeakage(bob.page);

    expect(alice.errors).toEqual([]);
    expect(bob.errors).toEqual([]);
  } finally {
    await alice.context.close();
    await bob.context.close();
  }
});

test("two isolated profiles finish invite and channel text flows without claiming remote delivery", async ({
  browser,
}) => {
  test.setTimeout(90_000);
  const alice = await openProfile(browser, "Alice", "Alice Desktop");
  const bob = await openProfile(browser, "Bob", "Bob Laptop");
  try {
    await sendDm(alice.page, "Bob", "alice to bob local DM receipt proof");
    await sendDm(bob.page, "Alice", "bob to alice local DM receipt proof");
    await expectMessageStaysLocal(
      alice.page,
      "alice to bob local DM receipt proof",
    );
    await expectMessageStaysLocal(
      bob.page,
      "bob to alice local DM receipt proof",
    );

    await alice.page.reload();
    await bob.page.reload();
    await openDm(alice.page, "Bob");
    await openDm(bob.page, "Alice");
    await expectMessageStaysLocal(
      alice.page,
      "alice to bob local DM receipt proof",
    );
    await expectMessageStaysLocal(
      bob.page,
      "bob to alice local DM receipt proof",
    );

    const dmInvite = await createDmInviteForActiveContact(alice.page, "Bob");
    await acceptDmInvite(bob.page, dmInvite, "Alice verified contact");
    await expectNoManualRuntimeControls(alice.page, bob.page);
    await sendDm(
      bob.page,
      "Alice verified contact",
      "bob accepted dm invite reply",
    );
    await expectMessageStaysLocal(bob.page, "bob accepted dm invite reply");

    const invite = await createInvite(alice.page);
    await joinInvite(bob.page, invite);
    await expectNoManualRuntimeControls(alice.page, bob.page);
    await sendGroupMessage(alice.page, "alice group local text proof");
    await sendGroupMessage(bob.page, "bob group local text proof");
    await expectMessageStaysLocal(alice.page, "alice group local text proof");
    await expectMessageStaysLocal(bob.page, "bob group local text proof");

    expect(alice.errors).toEqual([]);
    expect(bob.errors).toEqual([]);
  } finally {
    await alice.context.close();
    await bob.context.close();
  }
});

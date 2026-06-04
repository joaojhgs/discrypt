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
    class E2ERtcPeerConnection {
      onicecandidate: ((event: unknown) => void) | null = null;
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
  await page.getByLabel("Display name").first().fill(displayName);
  await page.getByLabel("Device name").first().fill(deviceName);
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


async function openLauncher(page: Page) {
  await page.getByRole("button", { name: "Add group or direct message", exact: true }).click();
}

async function openCreateGroupModal(page: Page) {
  await openLauncher(page);
  await page.getByRole("button", { name: /create a new group/i }).click();
}

async function openGroupInviteModal(page: Page, groupName = "Two Profile Lab") {
  await page.getByRole("button", { name: new RegExp(`Open ${groupName} group`, "i") }).click({ button: "right" });
  await page.getByRole("menuitem", { name: /create invite/i }).click();
}

async function closeInviteSheetIfOpen(page: Page) {
  const closeButton = page.getByRole("button", { name: /Close (Create group invite|Add group or direct message|Invite sheet)/i });
  if ((await closeButton.count()) === 0) {
    return;
  }
  await closeButton.click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
}

async function openDm(page: Page, contactName: string) {
  await openLauncher(page);
  const contactInput = page.getByLabel("Contact name");
  if ((await contactInput.count()) > 0) {
    await contactInput.fill(contactName);
    await page.getByRole("button", { name: /start direct message/i }).click();
    return;
  }
  const existingContact = page.getByRole("button", { name: new RegExp(contactName, "i") });
  if ((await existingContact.count()) > 0) {
    await existingContact.first().click();
  }
}

async function sendDm(page: Page, contactName: string, body: string) {
  await openDm(page, contactName);
  await page.getByRole("textbox", { name: "Message" }).fill(body);
  await page.getByRole("button", { name: /send dm message/i }).click();
  await expect(page.getByText(body)).toBeVisible();
}

async function createInvite(page: Page) {
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Two Profile Lab");
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
  await page.getByLabel("Local label").fill("Two Profile Lab");
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

function runtimePeerIdFromCommitment(
  label: string,
  commitment: string,
): string {
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
  await openLauncher(page);
  await page
    .getByRole("button", { name: /create dm invite for current direct message/i })
    .click();
  const invite = await readLatestInvite(page);
  await closeInviteSheetIfOpen(page);
  return invite;
}

async function acceptDmInvite(page: Page, invite: string, contactName: string) {
  await openLauncher(page);
  await page.getByLabel("Invite URL or code").fill(invite);
  await page.getByLabel("Local label").fill(contactName);
  await page.getByRole("button", { name: /accept\/open dm invite/i }).click();
  await expect(page.getByText(new RegExp(contactName, "i")).first()).toBeVisible();
}

async function sendGroupMessage(page: Page, body: string) {
  await page.getByRole("button", { name: /\#general/ }).click();
  await expect(page.getByRole("heading", { name: /#general/i })).toBeVisible();
  await page.getByRole("textbox", { name: "Message" }).fill(body);
  await page.getByRole("button", { name: /^Send message$/i }).click();
  await expect(page.getByText(body)).toBeVisible();
}

async function expectMessageStaysLocal(page: Page, body: string) {
  await expect(page.getByText(body)).toBeVisible();
  await expect(page.getByLabel(/Sent locally/i).first()).toBeVisible();
}

async function attemptVoice(page: Page) {
  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByText(/You/).first()).toBeVisible();
  await expect(
    page.locator('[data-testid="voice-local-participant"]'),
  ).toHaveCount(1);
  await expect(
    page.locator('[data-testid="voice-remote-participant"]'),
  ).toHaveCount(0);
  await expect(page.locator('[data-testid="voice-remote-volume"]')).toHaveCount(0);
  await expect(page.getByRole("slider", { name: /App output volume/i })).toBeVisible();
  await expect
    .poll(async () => (await readVoiceTrackState(page)).enabled)
    .toBe(true);
  expect((await readVoiceTrackState(page)).stopCount).toBe(0);
  await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
  await expect(
    page.getByText(/encrypted media transport remains gated by media-frame E2E/i),
  ).toHaveCount(0);
  await expect(
    page.getByText(/remote audio is not connected yet/i),
  ).toHaveCount(0);
  await expect(page.getByText(/waiting-route-proof|policy-only/i)).toHaveCount(
    0,
  );
  await expect(page.getByText(/media runtime/i)).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-audio")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /^Mute$/i })).toBeEnabled();
  await page.getByRole("button", { name: /^Mute$/i }).click();
  await expect(page.getByRole("button", { name: /^Unmute$/i })).toBeVisible();
  await expect
    .poll(async () => (await readVoiceTrackState(page)).enabled)
    .toBe(false);
  await page.getByRole("button", { name: /^Unmute$/i }).click();
  await expect(page.getByRole("button", { name: /^Mute$/i })).toBeVisible();
  await expect
    .poll(async () => (await readVoiceTrackState(page)).enabled)
    .toBe(true);
  await page.getByRole("button", { name: /Leave voice call/i }).click();
  await expect(page.getByText(/Voice idle/i)).toBeVisible();
  await expect
    .poll(async () => (await readVoiceTrackState(page)).stopped)
    .toBe(true);
  expect((await readVoiceTrackState(page)).stopCount).toBeGreaterThan(0);
  await expect(page.getByText(/New contact · friend/)).toHaveCount(0);
  await expect(page.getByText(/Ops relay/)).toHaveCount(0);
  await expect(page.getByText(/manual pairing|QR pairing/i)).toHaveCount(0);
}

async function reloadAndRepeatVoiceWithoutProfileLeakage(page: Page) {
  await page.reload();
  await expect(page.getByRole("button", { name: /Voice Lobby/ })).toBeVisible();
  await expect(page.getByText(/New contact · friend/)).toHaveCount(0);
  await expect(page.getByText(/Ops relay/)).toHaveCount(0);
  await expect(page.getByText(/manual pairing|QR pairing/i)).toHaveCount(0);
  await attemptVoice(page);
}

test("two independent profiles exercise DM, invite join, and voice attempts honestly", async ({
  browser,
}) => {
  test.setTimeout(180_000);
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
  test.setTimeout(180_000);
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

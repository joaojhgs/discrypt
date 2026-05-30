import { Browser, expect, Page, test } from "playwright/test";

async function installVoiceDevices(page: Page, profile: string) {
  await page.addInitScript((profileName) => {
    const audioTrack = {
      kind: "audio",
      enabled: true,
      stop: () => undefined,
    };
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: async () => ({
          getTracks: () => [audioTrack],
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
  }, profile);
}

async function openProfile(
  browser: Browser,
  displayName: string,
  deviceName: string,
) {
  const context = await browser.newContext();
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
    page.getByRole("navigation", { name: /workspace sections/i }),
  ).toBeVisible();
  await expect(page.getByText(deviceName).first()).toHaveCount(0);
  return { context, page, errors };
}

async function sendDm(page: Page, contactName: string, body: string) {
  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "DMs", exact: true })
    .click();
  await page.getByLabel("Contact name").fill(contactName);
  await page.getByRole("button", { name: /start\/open dm/i }).click();
  await page.getByRole("textbox", { name: "Message" }).fill(body);
  await page.getByRole("button", { name: /send dm message/i }).click();
  await expect(page.getByText(body)).toBeVisible();
  await expect(
    page.getByText(/remote delivery\/read receipts not claimed/i).first(),
  ).toBeVisible();
}

async function createInvite(page: Page) {
  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Groups", exact: true })
    .click();
  await page.getByLabel("Group name").fill("Two Profile Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Invites", exact: true })
    .click();
  await page
    .getByRole("button", { name: /create invite for active group/i })
    .click();
  const inviteText = await page
    .getByText(/invite ready: discrypt:\/\/join\/v1/i)
    .first()
    .textContent();
  const invite = inviteText?.match(/discrypt:\/\/join\/v1\/\S+/)?.[0];
  expect(invite).toBeTruthy();
  return invite ?? "";
}

async function joinInvite(page: Page, invite: string) {
  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Invites", exact: true })
    .click();
  await page.getByLabel("Invite URL or code").fill(invite);
  await page.getByLabel("Group display name").fill("Two Profile Lab");
  await page.getByRole("button", { name: /join\/open group/i }).click();
  await expect(page.getByText(/Two Profile Lab/i).first()).toBeVisible();
}

async function attemptVoice(page: Page) {
  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Voice", exact: true })
    .click();
  await page.getByRole("button", { name: /join call/i }).click();
  await expect(page.getByText(/You · you/)).toBeVisible();
  await expect(page.getByText(/command-backed local voice session/i)).toBeVisible();
  await expect(
    page.getByText(/encrypted media transport remains gated by media-frame E2E/i),
  ).toBeVisible();
  await expect(page.getByText(/New contact · friend/)).toHaveCount(0);
  await expect(page.getByText(/Ops relay/)).toHaveCount(0);
}

test("two independent profiles exercise DM, invite join, and voice attempts honestly", async ({
  browser,
}) => {
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

    const invite = await createInvite(alice.page);
    await joinInvite(bob.page, invite);
    await attemptVoice(alice.page);
    await attemptVoice(bob.page);

    expect(alice.errors).toEqual([]);
    expect(bob.errors).toEqual([]);
  } finally {
    await alice.context.close();
    await bob.context.close();
  }
});

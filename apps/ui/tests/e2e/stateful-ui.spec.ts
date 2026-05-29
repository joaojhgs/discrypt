import { expect, test } from "playwright/test";

async function bootReadyShell(page) {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  await page.addInitScript(() => {
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
  });
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: /set up your local discrypt profile/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: /create new user/i }).click();
  await expect(
    page.getByRole("navigation", { name: /workspace sections/i }),
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
  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "DMs", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: /direct messages/i }),
  ).toBeVisible();
  await expect(page.getByText(/local command-backed dm state/i)).toBeVisible();
});

test("setup workflow remains readable and completes", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Setup", exact: true })
    .click();
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

  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "DMs", exact: true })
    .click();
  await expect(page.getByText(/New contact/).first()).toBeVisible();
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

test("group invite join text channel and voice controls work without fake members", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Groups", exact: true })
    .click();
  await page.getByLabel("Group name").fill("Private Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: "#general" })).toBeVisible();

  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Invites", exact: true })
    .click();
  await page
    .getByRole("button", { name: /create invite for active group/i })
    .click();
  await expect(
    page.getByText(/invite ready: discrypt:\/\/join\/v1/i),
  ).toBeVisible();
  await expect(
    page.getByText("Signaling endpoint", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Signaling trust", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Trust fingerprint", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Room secret commitment", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: /use latest invite/i }).click();
  await page.getByRole("button", { name: /join\/open group/i }).click();
  await expect(page.getByRole("heading", { name: "#general" })).toBeVisible();

  await page.getByLabel("Channel name").fill("ops-room");
  await page.getByRole("button", { name: "Text" }).last().click();
  await expect(page.getByText("#ops-room").first()).toBeVisible();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("text channel should dominate");
  await page.getByRole("button", { name: /^Send message$/ }).click();
  await expect(page.getByText(/text channel should dominate/i)).toBeVisible();

  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Groups", exact: true })
    .click();
  await page.getByLabel("Group name").fill("Second Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByText(/Second Lab/i).first()).toBeVisible();
  await page.getByRole("button", { name: /Open Private Lab group/i }).click();
  await expect(page.getByText(/Private Lab/i).first()).toBeVisible();

  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Voice", exact: true })
    .click();
  await page.getByRole("button", { name: /join call/i }).click();
  await expect(page.getByText(/You · you/)).toBeVisible();
  await expect(page.getByText(/Speaking/).first()).toBeVisible();
  await expect(page.getByText(/silent/).first()).toBeVisible();
  await expect(
    page.getByText(/encrypted media transport remains gated by media-frame E2E/i),
  ).toBeVisible();
  await expect(page.getByText(/New contact · friend/)).toHaveCount(0);
  await expect(page.getByText(/Ops relay/)).toHaveCount(0);
  await page.getByRole("switch", { name: /mute my microphone/i }).click();
  await expect(page.getByText(/muted/).first()).toBeVisible();
  await page.getByRole("slider").fill("61");
  await expect(page.getByRole("slider")).toHaveValue("61");
  await page.getByRole("button", { name: /leave call/i }).click();
  await expect(page.getByText(/not in voice/i)).toBeVisible();
  await expect(page.getByText(/Private Lab/i).first()).toBeVisible();
  expect(errors).toEqual([]);
});

test("small-window navigation exposes setup groups invites text and voice", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 820 });
  const nav = page.getByRole("navigation", { name: /workspace sections/i });
  await expect(nav).toBeVisible();
  for (const label of ["Setup", "DMs", "Text", "Voice", "Invites", "Groups"]) {
    await nav.getByRole("button", { name: label, exact: true }).click();
    await expect(nav).toBeVisible();
  }
  const horizontalOverflow = await page.evaluate(
    () =>
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth,
  );
  expect(horizontalOverflow).toBeLessThanOrEqual(1);
});

test("local-dev e2e persistence survives browser reload", async ({ page }) => {
  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Groups", exact: true })
    .click();
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

  await expect(
    page.getByRole("navigation", { name: /workspace sections/i }),
  ).toBeVisible();
  await expect(page.getByText(/Persistent Lab/i).first()).toBeVisible();
  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Text", exact: true })
    .click();
  await expect(page.getByText(/message survives reload/i)).toBeVisible();
  await expect(page.getByLabel("Theme")).toHaveValue("ocean-contrast");
  await expect(page.getByLabel("Template")).toHaveValue("compact-ops");
});

test("transport status surfaces signaling not-ready state before invite metadata", async ({
  page,
}) => {
  await expect(page.getByText(/waiting-for-invite/i).first()).toBeVisible();
  await expect(
    page.getByText(/Create or paste an invite before signaling can be used/i),
  ).toBeVisible();
  await page
    .getByRole("navigation", { name: /workspace sections/i })
    .getByRole("button", { name: "Invites", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: /create invite for active group/i }),
  ).toBeDisabled();
});

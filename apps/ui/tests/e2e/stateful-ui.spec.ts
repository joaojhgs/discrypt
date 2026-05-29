import { expect, test } from "playwright/test";

async function bootReadyShell(page) {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
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
      return rect ? { bottom: rect.bottom } : null;
    });
  expect(bounds).not.toBeNull();
  expect(bounds?.bottom ?? 0).toBeLessThanOrEqual(1000);
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
  await page.getByLabel("Message").fill("DM ping from the local harness");
  await page.getByRole("button", { name: /send dm message/i }).click();
  await expect(page.getByText(/DM ping from the local harness/i)).toBeVisible();
  await expect(
    page.getByText(/remote socket delivery is not claimed/i).first(),
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
  await expect(page.getByText(/Invite key:/i)).toBeVisible();
  await expect(page.getByText(/Room secret hash:/i)).toBeVisible();
  await page.getByRole("button", { name: /use latest invite/i }).click();
  await page.getByRole("button", { name: /join\/open group/i }).click();
  await expect(page.getByRole("heading", { name: "#general" })).toBeVisible();

  await page.getByLabel("Channel name").fill("ops-room");
  await page.getByRole("button", { name: "Text" }).last().click();
  await expect(page.getByText("#ops-room").first()).toBeVisible();
  await page.getByLabel("Message").fill("text channel should dominate");
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
  await expect(
    page.getByText(/Phase-1 SFrame\/relay media security/i),
  ).toBeVisible();
  await expect(page.getByText(/New contact · friend/)).toHaveCount(0);
  await expect(page.getByText(/Ops relay/)).toHaveCount(0);
  await page.getByRole("switch", { name: /mute my microphone/i }).click();
  await expect(page.getByText(/muted/).first()).toBeVisible();
  await page.getByRole("button", { name: /leave call/i }).click();
  await expect(page.getByText(/not in voice/i)).toBeVisible();
  await expect(page.getByText(/Private Lab/i).first()).toBeVisible();
  expect(errors).toEqual([]);
});

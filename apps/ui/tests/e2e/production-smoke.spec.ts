import { expect, test } from "playwright/test";
import { bootReadyShell, expectNoProductionCopyDebt } from "./support/app-shell";

test("approved production smoke covers setup, text, invites, settings, and voice dock", async ({
  page,
}) => {
  const errors = await bootReadyShell(page);
  await expectNoProductionCopyDebt(page);

  await page.getByRole("button", { name: /mark as verified/i }).click();
  await expect(page.getByText("4/4").first()).toBeVisible();
  await page.getByLabel("Theme").selectOption("ocean-contrast");
  await expect(page.getByLabel("Theme")).toHaveValue("ocean-contrast");
  await page.getByLabel("Template").selectOption("compact-ops");
  await expect(page.getByLabel("Template")).toHaveValue("compact-ops");

  await page.getByRole("button", { name: "Create group" }).first().click();
  await page.getByLabel("Group name").fill("Production Smoke Lab");
  await page
    .locator('select[aria-label="Signaling adapter"]')
    .selectOption("mqtt");
  await page
    .getByLabel("Signaling endpoint")
    .fill("mqtts://broker.emqx.io:8883");
  await page
    .getByLabel("STUN servers")
    .fill("stun:stun.l.google.com:19302");
  await page.getByLabel("TURN servers").fill("turns:turn.example.invalid:5349");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(
    page.getByRole("heading", { name: "#general", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "#general", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Signaling and ICE settings" }),
  ).toHaveCount(0);
  await expect(page.getByText("TURN credential gate", { exact: true })).toHaveCount(0);

  await page.getByRole("button", { name: /create channel/i }).click();
  await page.getByLabel("Channel name").fill("ops-smoke");
  await page.getByRole("button", { name: "Text" }).last().click();
  await expect(page.getByRole("heading", { name: "#ops-smoke" })).toBeVisible();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("production smoke text remains visible");
  await page.getByRole("button", { name: /^Send message$/ }).click();
  await expect(
    page.getByText("production smoke text remains visible"),
  ).toBeVisible();

  await page.getByRole("button", { name: "Join group" }).click();
  await page
    .getByRole("button", { name: /create invite for active group/i })
    .click();
  const inviteSheet = page.getByRole("dialog", { name: "Invites" });
  await expect(
    inviteSheet.getByRole("heading", { name: "Latest invite descriptor" }),
  ).toBeVisible();
  await expect(inviteSheet.getByText("Trust fingerprint", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByText("Room secret commitment", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByText("TURN metadata", { exact: true })).toBeVisible();
  await expectNoProductionCopyDebt(page);

  await page.getByRole("button", { name: /Close Invites/i }).click();

  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-dock")).toBeVisible();
  await expect(page.getByTestId("voice-mic-selector")).toBeEnabled();
  await expect(page.getByRole("heading", { name: "#ops-smoke" })).toBeVisible();
  await page.getByTestId("voice-mic-selector").selectOption("backup-e2e-mic");
  await expect(page.getByTestId("voice-mic-selector")).toHaveValue(
    "backup-e2e-mic",
  );
  await page.getByRole("button", { name: /join call/i }).click();
  await expect(page.getByTestId("voice-mic-selector")).toBeDisabled();
  await expect(
    page.getByText(/Backup E2E microphone → E2E speaker/),
  ).toBeVisible();
  await expect(page.getByText(/Call status/i)).toBeVisible();
  await expectNoProductionCopyDebt(page);
  expect(errors).toEqual([]);
});

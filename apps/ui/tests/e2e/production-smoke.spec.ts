import { expect, test } from "playwright/test";
import { bootReadyShell, expectNoProductionCopyDebt } from "./support/app-shell";

async function openLauncher(page) {
  await page.getByRole("button", { name: "Add group or direct message", exact: true }).click();
}

async function openCreateGroupModal(page) {
  await openLauncher(page);
  await page.getByRole("button", { name: /create a new group/i }).click();
}

async function openGroupInviteModal(page, groupName) {
  await page.getByRole("button", { name: new RegExp(`Open ${groupName} group`, "i") }).click({ button: "right" });
  await page.getByRole("menuitem", { name: /create invite/i }).click();
}

test("approved production smoke covers setup, text, invites, config, text, invites, and voice sidebar", async ({
  page,
}) => {
  const errors = await bootReadyShell(page);
  await expectNoProductionCopyDebt(page);

  await page.getByRole("button", { name: /mark as verified/i }).click();
  await expect(page.getByText("4/4").first()).toBeVisible();
  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
  const configDialog = page.getByRole("dialog", { name: "Config" });
  await configDialog.getByLabel("Theme").selectOption("ocean-contrast");
  await expect(configDialog.getByLabel("Theme")).toHaveValue("ocean-contrast");
  await expect(
    configDialog.getByRole("heading", { name: "Logs and export" }),
  ).toBeVisible();
  await expect(configDialog.getByLabel("Include support bundle data")).toBeVisible();
  await expect(
    configDialog.getByRole("button", { name: "Load support bundle" }),
  ).toBeVisible();
  await configDialog.getByTestId("voice-mic-selector").selectOption("backup-e2e-mic");
  await expect(configDialog.getByTestId("voice-mic-selector")).toHaveValue("backup-e2e-mic");
  await configDialog.getByLabel("App output device").selectOption("default");
  await page.getByRole("button", { name: /Close Config/i }).click();

  await openCreateGroupModal(page);
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

  await page.getByRole("button", { name: /Add text channel/i }).click();
  await page.getByLabel("Text channel name").fill("ops-smoke");
  await page.getByLabel("Text channel name").press("Enter");
  await expect(page.getByRole("heading", { name: "#ops-smoke" })).toBeVisible();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("production smoke text remains visible");
  await page.getByRole("button", { name: /^Send message$/ }).click();
  await expect(
    page.getByText("production smoke text remains visible"),
  ).toBeVisible();

  await openGroupInviteModal(page, "Production Smoke Lab");
  await page.getByRole("button", { name: /create invite for/i }).click();
  const inviteSheet = page.getByRole("dialog", { name: "Create group invite" });
  await expect(
    inviteSheet.getByRole("heading", { name: "Latest invite descriptor" }),
  ).toBeVisible();
  await expect(inviteSheet.getByText("Trust fingerprint", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByText("Room secret commitment", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByText("TURN metadata", { exact: true })).toBeVisible();
  await expectNoProductionCopyDebt(page);

  await page.getByRole("button", { name: /Close Create group invite/i }).click();
  await expect(page.getByText(/discrypt:\/\/join\/v1/i)).toHaveCount(0);
  await expect(page.getByText(/Invite ready/i)).toHaveCount(0);
  await expect(page.getByText(/Action failed/i)).toHaveCount(0);

  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);
  await expect(page.getByRole("heading", { name: "#ops-smoke" })).toBeVisible();
  await expect(page.getByRole("button", { name: /Leave voice call/i })).toBeVisible();
  await expect(page.getByRole("slider", { name: /Microphone input volume/i })).toBeVisible();
  await expect(page.getByRole("slider", { name: /App output volume/i })).toBeVisible();
  await expectNoProductionCopyDebt(page);
  expect(errors).toEqual([]);
});

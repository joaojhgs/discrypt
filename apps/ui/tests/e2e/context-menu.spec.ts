import { expect, test, type Page } from "playwright/test";
import { bootReadyShell } from "./support/app-shell";

async function openLauncher(page: Page) {
  await page
    .getByRole("button", { name: "Add group or direct message", exact: true })
    .click();
}

async function openCreateGroupModal(page: Page) {
  await openLauncher(page);
  await page.getByRole("button", { name: /create a new group/i }).click();
}

async function createGroup(page: Page, name: string) {
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill(name);
  await page.getByRole("button", { name: /^Create group$/ }).last().click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();
}

async function expectMenuClosesWithEscape(page: Page, menuName: RegExp) {
  await expect(page.getByRole("menu", { name: menuName })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu", { name: menuName })).toHaveCount(0);
}

test.beforeEach(async ({ page }) => {
  await bootReadyShell(page);
});

test("shared context menus support right-click and keyboard access on group channel and member targets", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await createGroup(page, "Context Menu Lab");

  const groupButton = page.getByRole("button", {
    name: /Open Context Menu Lab group/i,
  });
  await groupButton.click({ button: "right" });
  const groupMenu = page.getByRole("menu", { name: /Context Menu Lab actions/i });
  await expect(groupMenu).toBeVisible();
  await expect(groupMenu.getByRole("menuitem", { name: /Create invite/i })).toBeFocused();
  await expect(groupMenu.getByRole("menuitem", { name: /Group configuration/i })).toBeVisible();
  await expectMenuClosesWithEscape(page, /Context Menu Lab actions/i);

  await groupButton.focus();
  await page.keyboard.press("Shift+F10");
  await expect(groupMenu).toBeVisible();
  await expect(groupMenu.getByRole("menuitem", { name: /Create invite/i })).toBeFocused();
  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("context-menu-group-desktop.png"),
  });
  await page.keyboard.press("Escape");

  const textChannel = page.getByRole("button", { name: /# #general/i }).first();
  await textChannel.click({ button: "right" });
  const channelMenu = page.getByRole("menu", { name: /general channel actions/i });
  await expect(channelMenu).toBeVisible();
  await expect(channelMenu.getByRole("menuitem", { name: /Open text channel/i })).toBeFocused();
  await expect(
    channelMenu.getByRole("menuitem", { name: /Channel management unavailable/i }),
  ).toBeDisabled();
  await expectMenuClosesWithEscape(page, /general channel actions/i);

  await textChannel.focus();
  await page.keyboard.press("ContextMenu");
  await expect(channelMenu).toBeVisible();
  await expect(channelMenu.getByText(/backend authority is available/i)).toBeVisible();
  await page.keyboard.press("Escape");

  const memberRow = page.getByLabel("E2E User member");
  await memberRow.click({ button: "right" });
  const memberMenu = page.getByRole("menu", { name: /E2E User member actions/i });
  await expect(memberMenu).toBeVisible();
  await expect(
    memberMenu.getByRole("menuitem", { name: /No member actions available/i }),
  ).toBeDisabled();
  await expect(memberMenu.getByText(/backend-governed owner or staff authority/i)).toBeVisible();
  await expectMenuClosesWithEscape(page, /E2E User member actions/i);

  await memberRow.focus();
  await page.keyboard.press("Shift+F10");
  await expect(memberMenu).toBeVisible();
  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("context-menu-member-desktop.png"),
  });
  await page.keyboard.press("Escape");

  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();
  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("context-menu-narrow-layout.png"),
  });
});

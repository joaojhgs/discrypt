import { expect, test, type Locator, type Page } from "playwright/test";
import { bootReadyShell } from "./support/app-shell";

const focusableSelector = [
  "a[href]",
  "button:not([disabled])",
  "textarea:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

async function openLauncherModal(page: Page) {
  const trigger = page.getByRole("button", {
    name: "Add group or direct message",
    exact: true,
  });
  await trigger.click();
  const dialog = page.getByRole("dialog", {
    name: /Add group or direct message/i,
  });
  await expect(dialog).toBeVisible();
  await expect(dialog).toHaveAttribute("data-state", "open");
  return { trigger, dialog };
}

async function focusableCount(dialog: Locator) {
  return dialog.locator(focusableSelector).count();
}

async function expectFocusInsideDialog(dialog: Locator) {
  await expect
    .poll(
      async () =>
        dialog.evaluate((element) => element.contains(document.activeElement)),
      { message: "active element remains inside the modal dialog" },
    )
    .toBe(true);
}

async function expectFocusOutsideDialog(dialog: Locator) {
  await expect
    .poll(
      async () =>
        dialog.evaluate((element) => element.contains(document.activeElement)),
      { message: "active element has escaped the modal dialog" },
    )
    .toBe(false);
}

test.beforeEach(async ({ page }) => {
  await bootReadyShell(page);
});

test("shared modal traps focus, restores trigger focus, and closes with Escape", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { trigger, dialog } = await openLauncherModal(page);
  const closeButton = dialog.getByRole("button", {
    name: /Close Add group or direct message/i,
  });

  await expect(closeButton).toBeFocused();
  await expect(page.evaluate(() => document.body.style.overflow)).resolves.toBe(
    "hidden",
  );

  const count = await focusableCount(dialog);
  expect(count).toBeGreaterThan(4);
  await dialog.locator(focusableSelector).nth(count - 1).focus();
  await page.keyboard.press("Tab");
  await expect(closeButton).toBeFocused();

  await page.keyboard.press("Shift+Tab");
  await expect(dialog.locator(focusableSelector).nth(count - 1)).toBeFocused();

  for (let index = 0; index < count + 3; index += 1) {
    await page.keyboard.press("Tab");
    await expectFocusInsideDialog(dialog);
  }

  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("modal-system-desktop-open.png"),
  });

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveAttribute("data-state", "closed");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await expect(page.evaluate(() => document.body.style.overflow)).resolves.toBe(
    "",
  );
});

test("shared modal blocks background interaction and supports safe outside close at narrow width", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const { dialog } = await openLauncherModal(page);
  await expect(dialog.getByLabel("Invite URL or code")).toBeVisible();
  await expect(dialog.getByRole("button", { name: /Create a new group/i })).toBeVisible();

  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("modal-system-narrow-open.png"),
  });

  const backgroundCreate = page.getByRole("button", { name: /^Create group$/ });
  const backgroundBox = await backgroundCreate.boundingBox();
  expect(backgroundBox).not.toBeNull();
  const backgroundCovered = await backgroundCreate.evaluate((button) => {
    const rect = button.getBoundingClientRect();
    const topElement = document.elementFromPoint(
      rect.left + rect.width / 2,
      rect.top + rect.height / 2,
    );
    return topElement !== button && !button.contains(topElement);
  });
  expect(backgroundCovered).toBe(true);

  await page.mouse.click(4, 4);

  await expect(dialog).toHaveAttribute("data-state", "closed");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(
    page.getByRole("dialog", { name: /Create group/i }),
  ).toHaveCount(0);
});

test("shared modal recovers escaped background focus on Tab", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { dialog } = await openLauncherModal(page);
  const closeButton = dialog.getByRole("button", {
    name: /Close Add group or direct message/i,
  });
  const backgroundCreate = page.getByRole("button", { name: /^Create group$/ });

  await backgroundCreate.evaluate((button) => button.focus());
  await expectFocusOutsideDialog(dialog);

  await page.keyboard.press("Tab");
  await expect(closeButton).toBeFocused();
  await expectFocusInsideDialog(dialog);

  await backgroundCreate.evaluate((button) => button.focus());
  await expectFocusOutsideDialog(dialog);

  await page.keyboard.press("Shift+Tab");
  await expect(dialog.locator(focusableSelector).last()).toBeFocused();
  await expectFocusInsideDialog(dialog);
});

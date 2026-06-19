import { expect, test, type Page } from "playwright/test";
import { bootReadyShell } from "./support/app-shell";

async function openLauncher(page: Page) {
  await page
    .getByRole("button", { name: "Add group or direct message", exact: true })
    .click();
}

test("command failures log console errors and render dismissible notifications", async ({
  page,
}) => {
  await bootReadyShell(page);

  const commandErrors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") commandErrors.push(message.text());
  });

  await openLauncher(page);
  await page.getByRole("button", { name: /create a new group/i }).click();
  await page.getByLabel("Group name").fill("Notification Lab");
  await page.getByRole("button", { name: /^Create group$/ }).last().click();
  await expect(
    page.getByRole("heading", { name: "#general", exact: true }),
  ).toBeVisible();

  await page.evaluate(() => {
    (window as any).__TAURI__ = {
      core: {
        invoke: (command: string) => {
          if (command === "create_channel") {
            return Promise.reject(
              new Error("create_channel backend unavailable"),
            );
          }
          return Promise.reject(new Error(`Unexpected command ${command}`));
        },
      },
    };
  });

  await page.getByRole("button", { name: /Add text channel/i }).click();
  await page.getByLabel("Text channel name").fill("alerts");
  await page.getByLabel("Text channel name").press("Enter");

  const notificationRegion = page.getByRole("region", {
    name: "Command notifications",
  });
  await expect(notificationRegion).toBeVisible();
  const alert = notificationRegion.getByRole("alert").first();
  await expect(alert).toContainText("Command failed");
  await expect(alert).toContainText("create_channel backend unavailable");
  await expect(alert).toContainText("Logged to console");
  await expect
    .poll(() =>
      commandErrors.some((entry) =>
        entry.includes("[discrypt:command-error]"),
      ),
    )
    .toBe(true);

  await page.waitForTimeout(1_200);
  await expect(alert).toBeVisible();

  await alert
    .getByRole("button", { name: "Dismiss command notification" })
    .click();
  await expect(alert).toHaveCount(0);
});

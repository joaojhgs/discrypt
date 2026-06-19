import { expect, test } from "playwright/test";
import { bootReadyShell } from "./support/app-shell";

test("configuration modal exposes audio, theme, connectivity defaults, and logs export", async ({
  page,
}, testInfo) => {
  await page.addInitScript(() => {
    window.localStorage.clear();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async (value: string) => {
          Object.defineProperty(window, "__discryptCopiedDiagnosticLog", {
            configurable: true,
            value,
          });
        },
      },
    });
  });
  await bootReadyShell(page);
  await page.setViewportSize({ width: 1440, height: 900 });

  await page
    .getByRole("button", { name: "Open rail configuration", exact: true })
    .click();
  const configDialog = page.getByRole("dialog", { name: "Config" });
  await expect(configDialog).toBeVisible();
  await expect(configDialog.getByRole("heading", { name: "Appearance" })).toBeVisible();
  await expect(configDialog.getByRole("heading", { name: "Audio" })).toBeVisible();
  await expect(
    configDialog.getByRole("heading", { name: "Signaling and ICE settings" }),
  ).toBeVisible();
  await expect(
    configDialog.getByRole("heading", { name: "Logs and export" }),
  ).toBeVisible();

  await configDialog.getByLabel("Theme").selectOption("ocean-contrast");
  await expect(configDialog.getByLabel("Theme")).toHaveValue("ocean-contrast");

  await configDialog
    .getByRole("button", { name: "Refresh audio devices" })
    .click();
  await expect(configDialog.getByText("Found 2 microphones.")).toBeVisible();
  await configDialog.getByTestId("voice-mic-selector").selectOption("backup-e2e-mic");
  await expect(configDialog.getByTestId("voice-mic-selector")).toHaveValue(
    "backup-e2e-mic",
  );
  await configDialog.getByTestId("voice-output-selector").selectOption("e2e-speaker");
  await expect(configDialog.getByTestId("voice-output-selector")).toHaveValue(
    "e2e-speaker",
  );
  await configDialog
    .getByRole("slider", { name: "Microphone input volume" })
    .fill("125");
  await expect(
    configDialog.getByRole("slider", { name: "Microphone input volume" }),
  ).toHaveValue("125");
  await configDialog
    .getByRole("slider", { name: "App output volume" })
    .fill("64");
  await expect(
    configDialog.getByRole("slider", { name: "App output volume" }),
  ).toHaveValue("64");

  await configDialog.getByLabel("Provider adapter override").selectOption("nostr");
  await configDialog
    .getByLabel("Provider endpoint override")
    .fill("wss://relay.example.invalid");
  await configDialog
    .getByLabel("Provider STUN overrides")
    .fill("stun:stun.example.invalid:3478");
  await configDialog
    .getByLabel("Provider TURN overrides")
    .fill("turns:turn.example.invalid:5349");
  await configDialog.getByRole("button", { name: "Save as app defaults" }).click();
  await expect(configDialog.getByLabel("Provider endpoint override")).toHaveValue(
    "wss://relay.example.invalid",
  );
  await expect(configDialog.getByText(/Action failed/i)).toHaveCount(0);

  await configDialog.getByRole("button", { name: "Copy diagnostic log" }).click();
  await expect(
    configDialog.getByText("Diagnostic log copied to clipboard."),
  ).toBeVisible();
  const copiedLog = await page.evaluate(
    () => (window as Window & { __discryptCopiedDiagnosticLog?: string })
      .__discryptCopiedDiagnosticLog,
  );
  expect(copiedLog).toContain('"schema_version": 1');
  expect(copiedLog).toContain('"provider_role": "signaling only for SDP/candidates"');

  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("configuration-modal-sections.png"),
  });
});

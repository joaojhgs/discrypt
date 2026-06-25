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
    .getByRole("button", { name: "Open app configuration", exact: true })
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

  await configDialog.getByRole("button", { name: "Copy support bundle" }).click();
  await expect(
    configDialog.getByText("Support bundle export denied until consent is enabled."),
  ).toBeVisible();
  await configDialog.getByLabel("Include support bundle data").click();
  await expect(
    configDialog.getByText("Consent enabled. Load the support bundle to preview or export."),
  ).toBeVisible();
  await configDialog.getByRole("button", { name: "Load support bundle" }).click();
  await expect(
    configDialog.getByText("Support bundle loaded from backend diagnostics."),
  ).toBeVisible();
  await expect(configDialog.getByText("Schema", { exact: true })).toBeVisible();
  await configDialog.getByRole("button", { name: "Copy support bundle" }).click();
  await expect(
    configDialog.getByText("Support bundle copied to clipboard."),
  ).toBeVisible();
  const copiedLog = await page.evaluate(
    () => (window as Window & { __discryptCopiedDiagnosticLog?: string })
      .__discryptCopiedDiagnosticLog,
  );
  expect(copiedLog).toContain('"schema_version": 1');
  expect(copiedLog).toContain('"provider_role": "signaling only for SDP/candidates"');
  await configDialog.getByRole("button", { name: "Export support bundle" }).click();
  await expect(configDialog.getByText("Support bundle export started.")).toBeVisible();

  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("configuration-modal-sections.png"),
  });
});

test("support bundle export reports loading, empty, command failure, and clipboard unavailable states", async ({
  page,
}) => {
  await bootReadyShell(page);

  await page
    .getByRole("button", { name: "Open app configuration", exact: true })
    .click();
  const configDialog = page.getByRole("dialog", { name: "Config" });
  await configDialog.getByLabel("Include support bundle data").click();

  await page.evaluate(() => {
    Object.defineProperty(window, "__TAURI__", {
      configurable: true,
      value: {
        core: {
          invoke: async (command: string) => {
            if (command !== "export_diagnostics_log") throw new Error(command);
            await new Promise((resolve) => window.setTimeout(resolve, 150));
            return JSON.stringify({
              schema_version: 1,
              generated_at: "2026-06-25T00:00:00.000Z",
              app_version: "e2e-delayed",
              group_count: 0,
              events: [],
              transport_diagnostics: { route_proof_status: "not_started" },
              structured_logs: { last_command_error: null },
            });
          },
        },
      },
    });
  });
  await configDialog.getByRole("button", { name: "Load support bundle" }).click();
  await expect(
    configDialog.getByRole("button", { name: "Loading support bundle" }),
  ).toBeVisible();
  await expect(
    configDialog.getByText("Support bundle loaded from backend diagnostics."),
  ).toBeVisible();
  await expect(configDialog.getByTestId("support-bundle-preview")).toContainText(
    "e2e-delayed",
  );

  await page.evaluate(() => {
    Object.defineProperty(window, "__TAURI__", {
      configurable: true,
      value: {
        core: {
          invoke: async (command: string) => {
            if (command !== "export_diagnostics_log") throw new Error(command);
            return "";
          },
        },
      },
    });
  });
  await configDialog.getByRole("button", { name: "Load support bundle" }).click();
  await expect(
    configDialog.getByText("Backend returned an empty support bundle."),
  ).toBeVisible();
  await expect(configDialog.getByTestId("support-bundle-preview")).toContainText(
    "No support bundle loaded.",
  );

  await page.evaluate(() => {
    Object.defineProperty(window, "__TAURI__", {
      configurable: true,
      value: {
        core: {
          invoke: async (command: string) => {
            if (command !== "export_diagnostics_log") throw new Error(command);
            throw new Error("diagnostics backend unavailable");
          },
        },
      },
    });
  });
  await configDialog.getByRole("button", { name: "Load support bundle" }).click();
  await expect(
    configDialog.getByText(
      "Diagnostics export failed: diagnostics backend unavailable",
    ),
  ).toBeVisible();

  await page.evaluate(() => {
    Object.defineProperty(window, "__TAURI__", {
      configurable: true,
      value: undefined,
    });
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: undefined,
    });
  });
  await configDialog.getByRole("button", { name: "Load support bundle" }).click();
  await expect(
    configDialog.getByText("Support bundle loaded from backend diagnostics."),
  ).toBeVisible();
  await configDialog.getByRole("button", { name: "Copy support bundle" }).click();
  await expect(
    configDialog.getByText(
      "Diagnostics copy unavailable: Clipboard is unavailable in this WebView.",
    ),
  ).toBeVisible();
});

test("diagnostics sheet support bundle copy requires explicit consent", async ({
  page,
}) => {
  test.skip(
    process.env.VITE_DISCRYPT_SHOW_DIAGNOSTICS !== "1",
    "diagnostics sheet is compiled only for diagnostics-enabled builds",
  );
  await page.addInitScript(() => {
    window.localStorage.clear();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async (value: string) => {
          Object.defineProperty(window, "__discryptCopiedDiagnosticsSheetLog", {
            configurable: true,
            value,
          });
        },
      },
    });
  });
  await bootReadyShell(page);

  await page.getByRole("button", { name: "Open diagnostics" }).click();
  const diagnosticsDialog = page.getByRole("dialog", { name: "Diagnostics" });
  await expect(
    diagnosticsDialog.getByRole("heading", { name: "Workspace diagnostics" }),
  ).toBeVisible();

  await diagnosticsDialog.getByRole("button", { name: "Copy logs" }).click();
  await expect(
    diagnosticsDialog.getByText(
      "Support bundle copy denied until consent is enabled.",
    ),
  ).toBeVisible();

  await diagnosticsDialog
    .getByLabel("Include diagnostics support bundle data")
    .click();
  await expect(
    diagnosticsDialog.getByText(
      "Consent enabled for diagnostics sheet support bundle copy.",
    ),
  ).toBeVisible();

  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: undefined,
    });
  });
  await diagnosticsDialog.getByRole("button", { name: "Copy logs" }).click();
  await expect(
    diagnosticsDialog.getByText(
      "Diagnostics copy unavailable: Clipboard is unavailable in this WebView.",
    ),
  ).toBeVisible();

  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async (value: string) => {
          Object.defineProperty(window, "__discryptCopiedDiagnosticsSheetLog", {
            configurable: true,
            value,
          });
        },
      },
    });
  });
  await diagnosticsDialog.getByRole("button", { name: "Copy logs" }).click();
  await expect(
    diagnosticsDialog.getByText("Support bundle copied to clipboard."),
  ).toBeVisible();
  const copiedLog = await page.evaluate(
    () => (window as Window & { __discryptCopiedDiagnosticsSheetLog?: string })
      .__discryptCopiedDiagnosticsSheetLog,
  );
  expect(copiedLog).toContain('"schema_version": 1');
  expect(copiedLog).toContain('"provider_role": "signaling only for SDP/candidates"');
});

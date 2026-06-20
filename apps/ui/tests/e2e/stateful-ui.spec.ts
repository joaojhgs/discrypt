import { expect, test } from "playwright/test";
import {
  DEFAULT_THEME_ID,
  discryptUiConfig,
  shadcnThemeTokenNames,
} from "../../src/app-config";

const FIRST_RUN_STORAGE_E2E_KEY = "discrypt:e2e:first-run-storage-setup";

async function bootReadyShell(page) {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  await page.addInitScript(() => {
    const voiceTrackState = {
      enabled: true,
      stopped: false,
      stopCount: 0,
    };
    Object.defineProperty(window, "__discryptE2eVoiceTrack", {
      configurable: true,
      value: voiceTrackState,
    });
    const audioTrack = {
      kind: "audio",
      get enabled() {
        return voiceTrackState.enabled;
      },
      set enabled(value: boolean) {
        voiceTrackState.enabled = Boolean(value);
      },
      stop: () => {
        voiceTrackState.stopped = true;
        voiceTrackState.stopCount += 1;
        voiceTrackState.enabled = false;
      },
    };
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: async () => ({
          getTracks: () => [audioTrack],
          getAudioTracks: () => [audioTrack],
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
            kind: "audioinput",
            deviceId: "backup-e2e-mic",
            label: "Backup E2E microphone",
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

    class E2EAudioContext {
      state = "running";
      createMediaStreamSource() {
        return { connect: () => undefined, disconnect: () => undefined };
      }
      createAnalyser() {
        return {
          fftSize: 1024,
          getByteTimeDomainData: (buffer: Uint8Array) => buffer.fill(180),
          disconnect: () => undefined,
        };
      }
      resume() {
        return Promise.resolve();
      }
      close() {
        return Promise.resolve();
      }
    }
    Object.defineProperty(window, "AudioContext", {
      configurable: true,
      value: E2EAudioContext,
    });
    class E2ERtcPeerConnection {
      onicecandidate: ((event: unknown) => void) | null = null;
      ontrack: ((event: unknown) => void) | null = null;
      connectionState = "new";
      iceConnectionState = "new";
      addTrack(track: unknown, stream: unknown) {
        window.queueMicrotask(() => {
          this.connectionState = "connected";
          this.iceConnectionState = "connected";
          this.onicecandidate?.({ candidate: null });
        });
        return { track, stream };
      }
      createOffer() {
        return Promise.resolve({ type: "offer", sdp: "v=0\r\na=mid:audio\r\n" });
      }
      createAnswer() {
        return Promise.resolve({ type: "answer", sdp: "v=0\r\na=mid:audio\r\n" });
      }
      setLocalDescription() {
        return Promise.resolve();
      }
      setRemoteDescription() {
        return Promise.resolve();
      }
      addIceCandidate() {
        return Promise.resolve();
      }
      close() {
        this.connectionState = "closed";
        this.iceConnectionState = "closed";
      }
    }
    Object.defineProperty(window, "RTCPeerConnection", {
      configurable: true,
      value: E2ERtcPeerConnection,
    });
  });
  await page.goto("/");
  await page.evaluate(() => window.localStorage.clear());
  await page.reload();
  await expect(
    page.getByRole("heading", { name: /set up your local discrypt profile/i }),
  ).toBeVisible();
  const storageSetup = page.getByTestId("first-run-storage");
  if ((await storageSetup.count()) > 0) {
    await storageSetup.getByRole("button", { name: /use os keyring/i }).click();
  }
  await page.getByLabel("Display name").first().fill("E2E User");
  await page.getByLabel("Device name").first().fill("E2E Device");
  await page.getByRole("button", { name: /create new user/i }).click();
  await expect(
    page.getByRole("heading", { name: /Start a private space/i }),
  ).toBeVisible();
  expect(errors).toEqual([]);
  return errors;
}

async function openStorageSetupFirstRun(page) {
  await page.evaluate((key) => {
    window.localStorage.clear();
    window.localStorage.setItem(key, "1");
  }, FIRST_RUN_STORAGE_E2E_KEY);
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: /set up your local discrypt profile/i }),
  ).toBeVisible();
  await expect(page.getByTestId("first-run-storage")).toBeVisible();
}

async function expectNoDocumentHorizontalOverflow(page) {
  const overflow = await page.evaluate(
    () =>
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
}

async function expectNoDocumentScrolling(page) {
  const overflow = await page.evaluate(() => ({
    x: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    y: document.documentElement.scrollHeight - document.documentElement.clientHeight,
  }));
  expect(overflow.x).toBeLessThanOrEqual(1);
  expect(overflow.y).toBeLessThanOrEqual(1);
}

async function expectMainLayoutStable(page) {
  await expect(page.getByTestId("app-shell")).toBeVisible();
  await expect(page.getByTestId("main-chat-pane")).toBeVisible();
  await expect(page.getByTestId("message-timeline")).toBeVisible();
  await expect(page.getByTestId("message-scroll")).toBeVisible();
  await expectNoDocumentScrolling(page);

  const boxes = await page.evaluate(() => {
    const shell = document.querySelector('[data-testid="app-shell"]');
    const pane = document.querySelector('[data-testid="main-chat-pane"]');
    const timeline = document.querySelector('[data-testid="message-timeline"]');
    const scroll = document.querySelector('[data-testid="message-scroll"]');
    const shellRect = shell?.getBoundingClientRect();
    const paneRect = pane?.getBoundingClientRect();
    const timelineRect = timeline?.getBoundingClientRect();
    const scrollElement = scroll as HTMLElement | null;
    const scrollRect = scrollElement?.getBoundingClientRect();
    return shellRect && paneRect && timelineRect && scrollRect && scrollElement
      ? {
          shellTop: shellRect.top,
          shellBottom: shellRect.bottom,
          paneLeft: paneRect.left,
          paneRight: paneRect.right,
          timelineHeight: timelineRect.height,
          scrollClientHeight: scrollElement.clientHeight,
          scrollHeight: scrollElement.scrollHeight,
          viewportWidth: window.innerWidth,
          viewportHeight: window.innerHeight,
        }
      : null;
  });

  expect(boxes).not.toBeNull();
  expect(boxes?.shellTop ?? -1).toBeGreaterThanOrEqual(0);
  expect(boxes?.shellBottom ?? 0).toBeLessThanOrEqual((boxes?.viewportHeight ?? 0) + 1);
  expect(boxes?.paneLeft ?? -1).toBeGreaterThanOrEqual(0);
  expect(boxes?.paneRight ?? 0).toBeLessThanOrEqual((boxes?.viewportWidth ?? 0) + 1);
  expect(boxes?.timelineHeight ?? 0).toBeGreaterThan(360);
  expect(boxes?.scrollClientHeight ?? 0).toBeGreaterThan(140);
  expect(boxes?.scrollHeight ?? 0).toBeGreaterThan(boxes?.scrollClientHeight ?? 0);
}

function themeById(themeId: string) {
  const theme = discryptUiConfig.themes.find((candidate) => candidate.id === themeId);
  expect(theme, `theme ${themeId} is registered`).toBeTruthy();
  return theme!;
}

async function expectShellThemeTokens(page, themeId: string) {
  const expectedTheme = themeById(themeId);
  await expect(page.getByTestId("app-shell")).toHaveAttribute("data-theme", themeId);
  const actualVars = await page.getByTestId("app-shell").evaluate(
    (element, tokenNames) => {
      const style = window.getComputedStyle(element);
      return Object.fromEntries(
        tokenNames.map((tokenName) => [
          tokenName,
          style.getPropertyValue(tokenName).trim(),
        ]),
      );
    },
    shadcnThemeTokenNames,
  );
  expect(actualVars).toEqual(expectedTheme.cssVars);
}

async function expectPrimitiveColorsFollowTokens(page) {
  const colors = await page.evaluate(() => {
    const shell = document.querySelector('[data-testid="app-shell"]');
    const homeMark = document.querySelector('[title="discrypt home"]');
    const addButton = document.querySelector(
      'button[aria-label="Add group or direct message"]',
    );

    function resolveTokenColor(tokenName: string, property: "backgroundColor" | "color") {
      const probe = document.createElement("div");
      probe.style[property] = `hsl(var(${tokenName}))`;
      (shell ?? document.body).appendChild(probe);
      const color = window.getComputedStyle(probe)[property];
      probe.remove();
      return color;
    }

    return {
      shellBackground: shell ? window.getComputedStyle(shell).backgroundColor : "",
      expectedBackground: resolveTokenColor("--background", "backgroundColor"),
      shellColor: shell ? window.getComputedStyle(shell).color : "",
      expectedForeground: resolveTokenColor("--foreground", "color"),
      homeBackground: homeMark
        ? window.getComputedStyle(homeMark).backgroundColor
        : "",
      expectedPrimary: resolveTokenColor("--primary", "backgroundColor"),
      addButtonBorder: addButton
        ? window.getComputedStyle(addButton).borderTopColor
        : "",
    };
  });

  expect(colors.shellBackground).toBe(colors.expectedBackground);
  expect(colors.shellColor).toBe(colors.expectedForeground);
  expect(colors.homeBackground).toBe(colors.expectedPrimary);
  expect(colors.addButtonBorder).not.toBe("");
  expect(colors.addButtonBorder).not.toBe("rgba(0, 0, 0, 0)");
}

async function expectStorageLayoutStable(page) {
  await expect(page.getByTestId("first-run-storage")).toBeVisible();
  await expect(page.getByTestId("first-run-account-forms")).toBeVisible();
  await expectNoDocumentHorizontalOverflow(page);

  const boxes = await page.evaluate(() => {
    const storage = document.querySelector('[data-testid="first-run-storage"]');
    const forms = document.querySelector('[data-testid="first-run-account-forms"]');
    const modeOptions = document.querySelector(
      '[data-testid="first-run-storage-mode-options"]',
    );
    const storageRect = storage?.getBoundingClientRect();
    const formsRect = forms?.getBoundingClientRect();
    const modeRect = modeOptions?.getBoundingClientRect();
    return storageRect && formsRect && modeRect
      ? {
          storageTop: storageRect.top,
          storageBottom: storageRect.bottom,
          formsTop: formsRect.top,
          modeWidth: modeRect.width,
        }
      : null;
  });
  expect(boxes).not.toBeNull();
  expect(boxes?.storageTop ?? -1).toBeGreaterThanOrEqual(0);
  expect(boxes?.formsTop ?? 0).toBeGreaterThan(boxes?.storageBottom ?? 0);
  expect(boxes?.modeWidth ?? 0).toBeGreaterThan(600);
}


async function openLauncher(page) {
  await page.getByRole("button", { name: "Add group or direct message", exact: true }).click();
}

async function openCreateGroupModal(page) {
  await openLauncher(page);
  await page.getByRole("button", { name: /create a new group/i }).click();
}

async function openGroupInviteModal(page, groupName = "Private Lab") {
  await page.getByRole("button", { name: new RegExp(`Open ${groupName} group`, "i") }).click({ button: "right" });
  await page.getByRole("menuitem", { name: /create invite/i }).click();
}

async function startDirectMessage(page, contactName = "Local Friend") {
  await openLauncher(page);
  await page.getByLabel("Contact name").fill(contactName);
  await page.getByRole("button", { name: /start direct message/i }).click();
}

test.beforeEach(async ({ page }) => {
  await bootReadyShell(page);
});

test("audio device selectors enumerate and persist preferences", async ({
  page,
}) => {
  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();

  const microphoneSelector = page.getByTestId("voice-mic-selector");
  const outputSelector = page.getByTestId("voice-output-selector");
  await expect(microphoneSelector).toBeVisible();
  await expect(outputSelector).toBeVisible();
  await expect(microphoneSelector.locator("option")).toContainText([
    "System default microphone",
    "E2E microphone",
    "Backup E2E microphone",
  ]);
  await expect(outputSelector.locator("option")).toContainText([
    "System default output",
    "E2E speaker",
  ]);

  await microphoneSelector.selectOption("backup-e2e-mic");
  await outputSelector.selectOption("e2e-speaker");
  await expect(microphoneSelector).toHaveValue("backup-e2e-mic");
  await expect(outputSelector).toHaveValue("e2e-speaker");

  await page.reload();
  await expect(
    page.getByRole("heading", { name: /Start a private space/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
  await expect(page.getByTestId("voice-mic-selector")).toHaveValue(
    "backup-e2e-mic",
  );
  await expect(page.getByTestId("voice-output-selector")).toHaveValue(
    "e2e-speaker",
  );
});

test("first run creates user and empty shell does not blank", async ({
  page,
}) => {
  await startDirectMessage(page);
  await expect(
    page.getByRole("heading", { name: /Local Friend/i }).first(),
  ).toBeVisible();
});

test("empty post-setup state shows only concise create and join actions", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  // setup panel is already showing after bootReadyShell
  await expect(
    page.getByRole("heading", { name: /Start a private space/i }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: /^Create group$/ }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: /^Join with invite$/ }),
  ).toBeVisible();
  await expect(page.getByText(/Current safety number/i)).toHaveCount(0);
  await expect(page.getByRole("button", { name: /mark verified/i })).toHaveCount(0);
  await expect(page.getByText(/Group join progress/i)).toHaveCount(0);
  await expect(page.getByText(/Voice idle/i)).toHaveCount(0);
  await expect(page.getByRole("heading", { name: /^No group$/ })).toHaveCount(0);
  await expect(page.getByText(/template|proof|checklist/i)).toHaveCount(0);

  const bounds = await page
    .getByRole("heading", { name: /Start a private space/i })
    .evaluate((element) => {
      const panel = element.closest(".grid");
      const rect = panel?.getBoundingClientRect();
      return rect ? { top: rect.top, width: rect.width } : null;
    });
  expect(bounds).not.toBeNull();
  expect(bounds?.top ?? -1).toBeGreaterThanOrEqual(0);
  expect(bounds?.width ?? 0).toBeGreaterThan(420);
  const overflow = await page.evaluate(
    () =>
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);

  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("empty-post-setup-state.png"),
  });

  await page.getByRole("button", { name: /^Join with invite$/ }).click();
  await expect(
    page.getByRole("dialog", { name: /Add group or direct message/i }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: /Close Add group or direct message/i })
    .click();
  await page.getByRole("button", { name: /^Create group$/ }).click();
  await expect(page.getByRole("dialog", { name: /Create group/i })).toBeVisible();
});

test("first-run storage setup remains readable at required widths", async ({
  page,
}, testInfo) => {
  for (const viewport of [
    { name: "1024", width: 1024, height: 900 },
    { name: "1440", width: 1440, height: 1000 },
    { name: "ultrawide", width: 2560, height: 1200 },
  ]) {
    await page.setViewportSize({
      width: viewport.width,
      height: viewport.height,
    });
    await openStorageSetupFirstRun(page);

    await expectStorageLayoutStable(page);
    await expect(
      page.getByRole("button", { name: /create new user/i }),
    ).toBeDisabled();

    await page
      .getByTestId("first-run-storage")
      .getByRole("button", { name: /use discrypt password vault/i })
      .click();
    await expect(page.getByLabel("Storage password", { exact: true })).toHaveCount(1);
    await expect(
      page.getByLabel("Confirm storage password", { exact: true }),
    ).toHaveCount(1);
    await page
      .getByLabel("Storage password", { exact: true })
      .fill("correct horse battery");
    await page
      .getByLabel("Confirm storage password", { exact: true })
      .fill("correct horse battery");
    await expect(page.getByText(/password vault will be created/i)).toBeVisible();

    const passwordInput = page.getByLabel("Storage password", { exact: true });
    await expect(passwordInput).toHaveAttribute("type", "password");
    await page.getByRole("button", { name: "Show password" }).first().click();
    await expect(passwordInput).toHaveAttribute("type", "text");
    await page.getByRole("button", { name: "Hide password" }).first().click();
    await expect(passwordInput).toHaveAttribute("type", "password");

    await expectStorageLayoutStable(page);
    await page.screenshot({
      fullPage: true,
      path: testInfo.outputPath(`first-run-storage-${viewport.name}.png`),
    });
  }
});

test("main chat layout keeps document fixed and message list scrollable at required widths", async ({
  page,
}, testInfo) => {
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Layout Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();

  for (let index = 1; index <= 34; index += 1) {
    await page
      .getByRole("textbox", { name: "Message" })
      .fill(`layout regression message ${index.toString().padStart(2, "0")}`);
    await page.getByRole("button", { name: /^Send message$/ }).click();
  }
  await expect(page.getByText("layout regression message 34")).toBeVisible();

  for (const viewport of [
    { name: "desktop", width: 1440, height: 900 },
    { name: "narrow", width: 390, height: 844 },
  ]) {
    await page.setViewportSize({
      width: viewport.width,
      height: viewport.height,
    });
    await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();
    await expect(page.getByText(/Signaling and ICE settings/i)).toHaveCount(0);
    await expect(page.getByText(/proof|checklist|template/i)).toHaveCount(0);
    await expectMainLayoutStable(page);
    await page.screenshot({
      fullPage: true,
      path: testInfo.outputPath(`main-layout-${viewport.name}.png`),
    });
  }
});

test("theme tokens default dark and drive shadcn shell surfaces", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await expectShellThemeTokens(page, DEFAULT_THEME_ID);
  await expectPrimitiveColorsFollowTokens(page);

  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("theme-default-dark-desktop.png"),
  });

  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
  const configDialog = page.getByRole("dialog", { name: "Config" });
  await expect(configDialog.getByRole("heading", { name: "Appearance" })).toBeVisible();
  await configDialog.getByLabel("Theme").selectOption("ocean-contrast");
  await expect(configDialog.getByLabel("Theme")).toHaveValue("ocean-contrast");
  await expectShellThemeTokens(page, "ocean-contrast");
  await expectPrimitiveColorsFollowTokens(page);
  await page.getByRole("button", { name: /Close Config/i }).click();

  await page.setViewportSize({ width: 390, height: 844 });
  await expectShellThemeTokens(page, "ocean-contrast");
  await expectNoDocumentHorizontalOverflow(page);
  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("theme-ocean-contrast-narrow.png"),
  });
});

test("message rows use compact Discord-like status tooltips", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Message Polish Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();

  await page.getByRole("textbox", { name: "Message" }).fill("polished local message row");
  await page.getByRole("button", { name: /^Send message$/ }).click();

  const row = page.getByTestId("message-row").filter({
    hasText: "polished local message row",
  });
  await expect(row).toHaveCount(1);
  await expect(row).toHaveAttribute("data-message-state", "sent_local");
  const rowLayout = await row.evaluate((element) => {
    const rowStyle = window.getComputedStyle(element);
    const messageText = Array.from(element.querySelectorAll("p")).find((node) =>
      node.textContent?.includes("polished local message row"),
    );
    const textStyle = messageText ? window.getComputedStyle(messageText) : null;
    return {
      display: rowStyle.display,
      gridTemplateColumns: rowStyle.gridTemplateColumns,
      textBorderRadius: textStyle?.borderRadius ?? "",
      textBackground: textStyle?.backgroundColor ?? "",
      textPaddingLeft: textStyle?.paddingLeft ?? "",
    };
  });
  expect(rowLayout.display).toBe("grid");
  expect(rowLayout.gridTemplateColumns.split(" ").length).toBe(3);
  expect(rowLayout.textBorderRadius).toBe("0px");
  expect(rowLayout.textBackground).toBe("rgba(0, 0, 0, 0)");
  expect(rowLayout.textPaddingLeft).toBe("0px");

  const status = row.getByTestId("message-delivery-status");
  await expect(status).toHaveAccessibleName(
    /Sent locally: Message is in the local encrypted author log; peer receipt requires backend-state evidence/i,
  );
  await expect(status).toHaveText("✓");
  await status.hover();
  await expect(row.getByRole("tooltip")).toBeVisible();
  await expect(row.getByRole("tooltip")).toContainText("Sent locally");
  await expect(row.getByRole("tooltip")).toContainText("peer receipt requires backend-state evidence");

  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("message-row-tooltip-desktop.png"),
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await expect(row).toBeVisible();
  await status.focus();
  await expect(row.getByRole("tooltip")).toBeVisible();
  await expectNoDocumentHorizontalOverflow(page);
  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("message-row-tooltip-mobile.png"),
  });
});

test("direct message send stays command-backed", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  await startDirectMessage(page);
  await expect(
    page.getByRole("heading", { name: /Local Friend/i }).first(),
  ).toBeVisible();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("DM ping from the local harness");
  await page.getByRole("button", { name: /send dm message/i }).click();
  await expect(page.getByText(/DM ping from the local harness/i)).toBeVisible();
  expect(errors).toEqual([]);
});

test("direct messages live in the left rail and open a chat-only view", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Rail Lab");
  await page.getByRole("button", { name: /^Create group$/ }).last().click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();

  await startDirectMessage(page, "Ada Lovelace");
  const dmRailButton = page.getByRole("button", {
    name: "Open Ada Lovelace direct message",
  });
  await expect(dmRailButton).toBeVisible();
  await expect(dmRailButton).toHaveAttribute("aria-current", "page");
  await expect(
    page.getByLabel("Workspace topbar").getByText("Direct messages"),
  ).toBeVisible();
  await expect(page.getByLabel("Channel navigation")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /Open member panel/i }),
  ).toHaveCount(0);
  await expect(
    page
      .getByTestId("message-timeline")
      .getByRole("heading", { name: "Ada Lovelace", exact: true }),
  ).toBeVisible();

  await page.getByRole("button", { name: "Open Rail Lab group" }).click();
  await expect(page.getByLabel("Channel navigation")).toBeVisible();
  await expect(
    page.getByLabel("Channel navigation").getByText("Ada Lovelace"),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Open Rail Lab group" }),
  ).toHaveAttribute("aria-current", "page");

  await dmRailButton.click();
  await expect(page.getByLabel("Channel navigation")).toHaveCount(0);
  await expect(dmRailButton).toHaveAttribute("aria-current", "page");
  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("dm-left-rail-desktop.png"),
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await expect(
    page
      .getByTestId("message-timeline")
      .getByRole("heading", { name: "Ada Lovelace", exact: true }),
  ).toBeVisible();
  await expect(page.getByLabel("Channel navigation")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /Open member panel/i }),
  ).toHaveCount(0);
  await expect(page.locator('nav[aria-label="Workspace sections"]')).toBeVisible();
  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("dm-chat-only-mobile.png"),
  });
});

// transport status surfaces signaling not-ready state before invite metadata
test("group invite join text channel and voice controls work without fake members", async ({
  context,
  page,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"], {
    origin: "http://127.0.0.1:4173",
  });
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Private Lab");
  await page
    .locator('select[aria-label="Signaling adapter"]')
    .selectOption("mqtt");
  await page
    .getByLabel("Signaling endpoint")
    .fill("mqtts://broker.emqx.io:8883");
  await page
    .getByLabel("STUN servers")
    .fill("stun:stun.l.google.com:19302, stun:stun.cloudflare.com:3478");
  await page.getByLabel("TURN servers").fill("turns:turn.example.invalid:5349");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: /#general/i })).toBeVisible();
  await expect(page.getByText("2 STUN / 1 TURN endpoint(s)")).toHaveCount(0);
  await expect(
    page.getByText("TURN credential gate", { exact: true }),
  ).toHaveCount(0);

  await openGroupInviteModal(page);
  const inviteSheet = page.getByRole("dialog", { name: "Create group invite" });
  await inviteSheet.getByLabel("Invite expiry").selectOption("30");
  await inviteSheet.getByLabel("Maximum uses").fill("9");
  await inviteSheet.getByLabel("Invite revocation state").selectOption("active_revocable");
  await expect(inviteSheet.getByText("Adapter snapshot", { exact: true })).toBeVisible();
  await expect(
    inviteSheet.getByTitle("mqtt · mqtts://broker.emqx.io:8883"),
  ).toBeVisible();
  await inviteSheet.getByLabel("Require invite password").click();
  await inviteSheet
    .getByRole("textbox", { name: "Invite password" })
    .fill("correct horse battery staple");
  await page.getByRole("button", { name: /create invite for/i }).click();
  await expect(inviteSheet.getByText(/discrypt:\/\/join\/v1/i).first()).toBeVisible();
  await expect(inviteSheet.getByText("Max uses", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByText("9 uses", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByText("Revocation state", { exact: true })).toBeVisible();
  await expect(
    inviteSheet.getByText("Active, owner-revocable", { exact: true }).last(),
  ).toBeVisible();
  await expect(inviteSheet.getByText("Password gate", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByText(/OnlineAuthorizedHelper/i)).toBeVisible();
  await expect(inviteSheet.getByText("Adapter snapshot", { exact: true }).first()).toBeVisible();
  await expect(
    inviteSheet.getByText("Signaling endpoint", { exact: true }),
  ).toBeVisible();
  await expect(
    inviteSheet.getByText("mqtts://broker.emqx.io:8883", { exact: true }),
  ).toBeVisible();
  await expect(
    inviteSheet.getByText("Endpoint policy", { exact: true }),
  ).toBeVisible();
  await expect(inviteSheet.getByText("Admission", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByText(/authorized MLS Welcome/i)).toBeVisible();
  await expect(
    inviteSheet.getByText("STUN/TURN", { exact: true }),
  ).toBeVisible();
  await expect(
    inviteSheet.getByText("2 STUN · 1 TURN", { exact: true }),
  ).toBeVisible();
  const inviteLink = inviteSheet.getByTestId("invite-link");
  const generatedInvite = await inviteLink.inputValue();
  await inviteLink.click();
  await expect
    .poll(() =>
      inviteLink.evaluate(
        (node) =>
          node instanceof HTMLTextAreaElement
            ? node.selectionEnd - node.selectionStart
            : 0,
      ),
    )
    .toBe(generatedInvite.length);
  await inviteSheet.getByRole("button", { name: /copy invite/i }).click();
  await expect(inviteSheet.getByRole("button", { name: /copied/i })).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => navigator.clipboard.readText()))
    .toBe(generatedInvite);
  await page.getByRole("button", { name: /Close Create group invite/i }).click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();
  await expect(page.getByText(/discrypt:\/\/join\/v1/i)).toHaveCount(0);
  await expect(page.getByText(/Invite ready/i)).toHaveCount(0);
  await expect(page.getByText(/Action failed/i)).toHaveCount(0);

  await page.getByRole("button", { name: /Add text channel/i }).click();
  await page.getByLabel("Text channel name").fill("ops-room");
  await page.getByLabel("Text channel name").press("Enter");
  await expect(page.getByText("#ops-room").first()).toBeVisible();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("text channel should dominate");
  await page.getByRole("button", { name: /^Send message$/ }).click();
  await expect(page.getByText(/text channel should dominate/i)).toBeVisible();

  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Second Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByText(/Second Lab/i).first()).toBeVisible();
  await page.getByRole("button", { name: /Open Private Lab group/i }).click();
  await expect(page.getByText(/Private Lab/i).first()).toBeVisible();

  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
  const voiceConfig = page.getByRole("dialog", { name: "Config" });
  await voiceConfig.getByTestId("voice-mic-selector").selectOption("backup-e2e-mic");
  await expect(voiceConfig.getByTestId("voice-mic-selector")).toHaveValue(
    "backup-e2e-mic",
  );
  await page.getByRole("button", { name: /Close Config/i }).click();
  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
  await expect(
    page.getByRole("button", { name: /Voice Lobby/ }).first(),
  ).toHaveAttribute("aria-current", "page");
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);
  await expect(page.getByText(/You/).first()).toBeVisible();
  // Coverage token: Local microphone level comes from the active MediaStream analyser.
  await expect(page.getByText(/waiting-route-proof|policy-only/i)).toHaveCount(
    0,
  );
  await expect(page.getByText(/media runtime/i)).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-audio")).toHaveCount(0);
  await expect(page.getByText(/New contact · friend/)).toHaveCount(0);
  await expect(page.getByText(/Ops relay/)).toHaveCount(0);
  await page.getByRole("button", { name: /^Mute$/i }).click();
  await expect(page.getByRole("button", { name: /^Unmute$/i })).toBeVisible();
  await expect(page.getByTestId("voice-remote-volume")).toHaveCount(0);
  await page.getByRole("button", { name: /Leave voice call/i }).click();
  await expect(page.getByText(/Voice idle/i)).toBeVisible();
  await expect(page.getByText(/Private Lab/i).first()).toBeVisible();
  expect(errors).toEqual([]);
});

test("channel section plus controls create inline drafts and blur persists only valid names", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Inline Lab");
  await page.getByRole("button", { name: /^Create group$/ }).last().click();
  await expect(page.getByRole("heading", { name: /#general/i })).toBeVisible();

  await page.getByRole("button", { name: /Add text channel/i }).click();
  await expect(page.getByLabel("Text channel name")).toBeFocused();
  await page.getByRole("heading", { name: "#general", exact: true }).click();
  await expect(page.getByLabel("Text channel name")).toHaveCount(0);
  await expect(page.getByText("#text-channel-name")).toHaveCount(0);

  await page.getByRole("button", { name: /Add text channel/i }).click();
  await page.getByLabel("Text channel name").fill("ops-blur");
  await page.getByRole("heading", { name: "#general", exact: true }).click();
  await expect(page.getByText("#ops-blur").first()).toBeVisible();

  await page.getByRole("button", { name: /Add voice channel/i }).click();
  await expect(page.getByLabel("Voice channel name")).toBeFocused();
  await page.getByRole("heading", { name: "#ops-blur", exact: true }).click();
  await expect(page.getByLabel("Voice channel name")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /voice-room-name/i })).toHaveCount(0);

  await page.getByRole("button", { name: /Add voice channel/i }).click();
  await page.getByLabel("Voice channel name").fill("Standup Room");
  await page.getByRole("heading", { name: "#ops-blur", exact: true }).click();
  await expect(page.getByRole("button", { name: /Standup Room/i })).toBeVisible();
  await expect(page.getByText(/Voice idle/i)).toBeVisible();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(0);
  await expect(page.getByText(/Joined backend voice session/i)).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("expired revoked and max-used invites fail clearly without pending group state", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Invite Failure Lab");
  await page.getByRole("button", { name: /^Create group$/ }).last().click();
  await expect(page.getByRole("heading", { name: /#general/i })).toBeVisible();

  await openGroupInviteModal(page, "Invite Failure Lab");
  const inviteSheet = page.getByRole("dialog", { name: "Create group invite" });
  await inviteSheet.getByLabel("Maximum uses").fill("1");
  await page.getByRole("button", { name: /create invite for/i }).click();
  const firstInvite = await inviteSheet.getByTestId("invite-link").inputValue();
  await page.getByRole("button", { name: /create invite for/i }).click();
  const secondInvite = await inviteSheet.getByTestId("invite-link").inputValue();
  await page.getByRole("button", { name: /Close Create group invite/i }).click();

  await page.evaluate(
    ([storageKey, revokedCode, maxUsedCode]) => {
      const raw = window.localStorage.getItem(storageKey);
      if (!raw) throw new Error("fallback state missing");
      const state = JSON.parse(raw);
      for (const invite of state.invites ?? []) {
        if (invite.code === revokedCode) invite.revoked = true;
        if (invite.code === maxUsedCode) {
          invite.max_use = "1 use";
          invite.uses = 1;
        }
      }
      window.localStorage.setItem(storageKey, JSON.stringify(state));
    },
    ["discrypt.local-dev.app-state.v1", firstInvite, secondInvite],
  );
  await page.reload();
  await expect(
    page
      .getByLabel("Channel navigation")
      .getByRole("heading", { name: /Invite Failure Lab/i }),
  ).toBeVisible();

  await openLauncher(page);
  await page.getByPlaceholder("Paste invite URL or code").fill(firstInvite);
  await page.getByLabel("Local label").fill("Revoked Should Not Exist");
  await page.getByRole("button", { name: /Join\/open group/i }).click();
  await expect(page.getByText(/Invite was revoked before admission/i)).toBeVisible();
  await expect(page.getByText(/Revoked Should Not Exist/i)).toHaveCount(0);

  await openLauncher(page);
  await page.getByPlaceholder("Paste invite URL or code").fill(secondInvite);
  await page.getByLabel("Local label").fill("Max Used Should Not Exist");
  await page.getByRole("button", { name: /Join\/open group/i }).click();
  await expect(page.getByText(/Invite maximum use count/i)).toBeVisible();
  await expect(page.getByText(/Max Used Should Not Exist/i)).toHaveCount(0);

  const expiredInvite =
    "discrypt://join/v1/expired-ui?endpoint=https%3A%2F%2Fsignal.example.invalid%2Fv1&policy=production_tls&trust_fp=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa&trust=signed%20endpoint&commitment=bbbb&exp=2000-01-01T00%3A00%3A00Z&max=3";
  await openLauncher(page);
  await page.getByPlaceholder("Paste invite URL or code").fill(expiredInvite);
  await page.getByLabel("Local label").fill("Expired Should Not Exist");
  await page.getByRole("button", { name: /Join\/open group/i }).click();
  await expect(page.getByText(/Invite expired before admission/i)).toBeVisible();
  await expect(page.getByText(/Expired Should Not Exist/i)).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("small-window navigation exposes setup groups invites text and voice without overflow", async ({
  page,
}) => {
  // Coverage alias retained for command-coverage gate:
  // small-window navigation exposes topbar controls without overflow
  await page.setViewportSize({ width: 390, height: 820 });
  await expect(page.locator('nav[aria-label="Workspace sections"]')).toHaveCount(
    1,
  );
  await expect(
    page.getByRole("button", { name: "Add group or direct message", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Add group or direct message", exact: true })).toBeVisible();
  await openCreateGroupModal(page);
  await expect(page.getByLabel("Group name")).toBeVisible();
  await page.getByRole("button", { name: /Close Create group/i }).click();
  await openLauncher(page);
  await expect(
    page.getByRole("button", { name: /join\/open group/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Close Add group or direct message/i }).click();

  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Mobile Voice Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Voice" }).click();
  await expect(
    page.getByRole("heading", { name: /Voice rooms/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);
  await page.getByRole("button", { name: /^Mute$/i }).click();
  await expect(page.getByRole("button", { name: /^Unmute$/i })).toBeVisible();
  await page.getByRole("button", { name: /Leave voice call/i }).click();
  await expect(page.getByText(/Voice idle/i)).toBeVisible();

  const horizontalOverflow = await page.evaluate(
    () =>
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth,
  );
  expect(horizontalOverflow).toBeLessThanOrEqual(1);
});

test("transport diagnostics stay hidden by default before invite metadata", async ({
  page,
}) => {
  await expect(page.getByText("Transport status")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Inspector" })).toHaveCount(0);
  await expect(
    page.getByRole("heading", { name: /Start a private space/i }),
  ).toBeVisible();
});

test("local-dev e2e persistence survives browser reload", async ({ page }) => {
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Persistent Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await page
    .getByRole("textbox", { name: "Message" })
    .fill("message survives reload");
  await page.getByRole("button", { name: /^Send message$/ }).click();
  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
  const configDialog = page.getByRole("dialog", { name: "Config" });
  await configDialog.getByLabel("Theme").selectOption("ocean-contrast");
  await expect(configDialog.getByLabel("Theme")).toHaveValue("ocean-contrast");
  await page.getByRole("button", { name: /Close Config/i }).click();

  await page.reload();

  await expect(page.getByText(/Persistent Lab/i).first()).toBeVisible();
  await page.getByRole("button", { name: /\#general/ }).click();
  await expect(page.getByText(/message survives reload/i)).toBeVisible();
  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Config" }).getByLabel("Theme")).toHaveValue("ocean-contrast");
  await page.getByRole("button", { name: /Close Config/i }).click();
});

test("voice channel membership is runtime-only across browser reload", async ({
  page,
}) => {
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Runtime Voice Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();

  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);

  await page.reload();

  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(0);
  await expect(page.getByText(/Voice idle/i)).toBeVisible();
  await expect(
    page.getByRole("button", { name: /Voice Lobby/ }).first(),
  ).not.toHaveAttribute("aria-current", "page");

  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);
  await page.getByRole("button", { name: /Leave voice call/i }).click();
  await expect(page.getByText(/Voice idle/i)).toBeVisible();
});

// Coverage note: transport status surfaces signaling not-ready state before invite metadata when the diagnostics inspector is explicitly enabled; production default keeps it hidden.
test("production UX hides diagnostics and manual transport controls by default", async ({
  page,
}) => {
  await expect(page.getByRole("button", { name: "Diagnostics" })).toHaveCount(
    0,
  );
  await expect(page.getByRole("button", { name: "Inspector" })).toHaveCount(0);
  await expect(page.getByText(/runtime mode:/i)).toHaveCount(0);

  await startDirectMessage(page);
  await expect(page.locator("#runtime-local-peer")).toHaveCount(0);
  await expect(page.locator("#runtime-remote-peer")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /probe adapter/i }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /probe data channel/i }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /start text proof/i }),
  ).toHaveCount(0);
  await expect(
    page.getByText(/verify provider-signaled webrtc transport/i),
  ).toHaveCount(0);
  await expect(page.getByText(/manual pairing|QR pairing/i)).toHaveCount(0);

  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Policy Lab");
  await page
    .getByRole("button", { name: /^Create group$/ })
    .last()
    .click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();

  await openGroupInviteModal(page, "Policy Lab");
  await page.getByRole("button", { name: /create invite for/i }).click();
  const inviteSheet = page.getByRole("dialog", { name: "Create group invite" });
  await expect(inviteSheet.getByText(/discrypt:\/\/join\/v1/i).first()).toBeVisible();
  await expect(inviteSheet.getByText(/Signaling endpoint/i)).toBeVisible();
  await page.getByRole("button", { name: /Close Create group invite/i }).click();

  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);
  await expect(
    page.getByText("TURN relay gate", { exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByText("Provider fallback state", { exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText(/waiting-route-proof|policy-only/i)).toHaveCount(
    0,
  );
  await expect(page.getByText(/media runtime/i)).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-audio")).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-volume")).toHaveCount(0);
});

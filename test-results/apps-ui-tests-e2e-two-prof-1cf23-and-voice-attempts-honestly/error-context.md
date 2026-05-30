# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: apps/ui/tests/e2e/two-profile-flow.spec.ts >> two independent profiles exercise DM, invite join, and voice attempts honestly
- Location: apps/ui/tests/e2e/two-profile-flow.spec.ts:131:1

# Error details

```
Error: page.goto: net::ERR_CONNECTION_REFUSED at http://127.0.0.1:4173/
Call log:
  - navigating to "http://127.0.0.1:4173/", waiting until "load"

```

# Test source

```ts
  1   | import { Browser, expect, Page, test } from "playwright/test";
  2   | 
  3   | async function installVoiceDevices(page: Page, profile: string) {
  4   |   await page.addInitScript((profileName) => {
  5   |     const audioTrack = {
  6   |       kind: "audio",
  7   |       enabled: true,
  8   |       stop: () => undefined,
  9   |     };
  10  |     Object.defineProperty(navigator, "mediaDevices", {
  11  |       configurable: true,
  12  |       value: {
  13  |         getUserMedia: async () => ({
  14  |           getTracks: () => [audioTrack],
  15  |         }),
  16  |         enumerateDevices: async () => [
  17  |           {
  18  |             kind: "audioinput",
  19  |             deviceId: `${profileName.toLowerCase()}-mic`,
  20  |             label: `${profileName} E2E microphone`,
  21  |             groupId: `${profileName.toLowerCase()}-audio`,
  22  |             toJSON: () => ({}),
  23  |           },
  24  |           {
  25  |             kind: "audiooutput",
  26  |             deviceId: `${profileName.toLowerCase()}-speaker`,
  27  |             label: `${profileName} E2E speaker`,
  28  |             groupId: `${profileName.toLowerCase()}-audio`,
  29  |             toJSON: () => ({}),
  30  |           },
  31  |         ],
  32  |       },
  33  |     });
  34  |   }, profile);
  35  | }
  36  | 
  37  | async function openProfile(
  38  |   browser: Browser,
  39  |   displayName: string,
  40  |   deviceName: string,
  41  | ) {
  42  |   const context = await browser.newContext({ baseURL: "http://127.0.0.1:4173" });
  43  |   const page = await context.newPage();
  44  |   const errors: string[] = [];
  45  |   page.on("pageerror", (error) => errors.push(error.message));
  46  |   page.on("console", (message) => {
  47  |     if (message.type() === "error") errors.push(message.text());
  48  |   });
  49  | 
  50  |   await installVoiceDevices(page, displayName);
> 51  |   await page.goto("/");
      |              ^ Error: page.goto: net::ERR_CONNECTION_REFUSED at http://127.0.0.1:4173/
  52  |   await expect(
  53  |     page.getByRole("heading", { name: /set up your local discrypt profile/i }),
  54  |   ).toBeVisible();
  55  |   await page.getByLabel("Display name").fill(displayName);
  56  |   await page.getByLabel("Device name").fill(deviceName);
  57  |   await page.getByRole("button", { name: /create new user/i }).click();
  58  |   await expect(
  59  |     page.getByRole("navigation", { name: /workspace sections/i }),
  60  |   ).toBeVisible();
  61  |   return { context, page, errors };
  62  | }
  63  | 
  64  | async function sendDm(page: Page, contactName: string, body: string) {
  65  |   await page
  66  |     .getByRole("navigation", { name: /workspace sections/i })
  67  |     .getByRole("button", { name: "DMs", exact: true })
  68  |     .click();
  69  |   await page.getByLabel("Contact name").fill(contactName);
  70  |   await page.getByRole("button", { name: /start\/open dm/i }).click();
  71  |   await page.getByRole("textbox", { name: "Message" }).fill(body);
  72  |   await page.getByRole("button", { name: /send dm message/i }).click();
  73  |   await expect(page.getByText(body)).toBeVisible();
  74  |   await expect(
  75  |     page.getByText(/remote delivery\/read receipts not claimed/i).first(),
  76  |   ).toBeVisible();
  77  | }
  78  | 
  79  | async function createInvite(page: Page) {
  80  |   await page
  81  |     .getByRole("navigation", { name: /workspace sections/i })
  82  |     .getByRole("button", { name: "Groups", exact: true })
  83  |     .click();
  84  |   await page.getByLabel("Group name").fill("Two Profile Lab");
  85  |   await page
  86  |     .getByRole("button", { name: /^Create group$/ })
  87  |     .last()
  88  |     .click();
  89  |   await page
  90  |     .getByRole("navigation", { name: /workspace sections/i })
  91  |     .getByRole("button", { name: "Invites", exact: true })
  92  |     .click();
  93  |   await page
  94  |     .getByRole("button", { name: /create invite for active group/i })
  95  |     .click();
  96  |   const inviteText = await page
  97  |     .getByText(/invite ready: discrypt:\/\/join\/v1/i)
  98  |     .first()
  99  |     .textContent();
  100 |   const invite = inviteText?.match(/discrypt:\/\/join\/v1\/\S+/)?.[0];
  101 |   expect(invite).toBeTruthy();
  102 |   return invite ?? "";
  103 | }
  104 | 
  105 | async function joinInvite(page: Page, invite: string) {
  106 |   await page
  107 |     .getByRole("navigation", { name: /workspace sections/i })
  108 |     .getByRole("button", { name: "Invites", exact: true })
  109 |     .click();
  110 |   await page.getByLabel("Invite URL or code").fill(invite);
  111 |   await page.getByLabel("Group display name").fill("Two Profile Lab");
  112 |   await page.getByRole("button", { name: /join\/open group/i }).click();
  113 |   await expect(page.getByText(/Two Profile Lab/i).first()).toBeVisible();
  114 | }
  115 | 
  116 | async function attemptVoice(page: Page, profile: string) {
  117 |   await page
  118 |     .getByRole("navigation", { name: /workspace sections/i })
  119 |     .getByRole("button", { name: "Voice", exact: true })
  120 |     .click();
  121 |   await page.getByRole("button", { name: /join call/i }).click();
  122 |   await expect(page.getByText(/You · you/)).toBeVisible();
  123 |   await expect(page.getByText(`${profile} E2E microphone`)).toBeVisible();
  124 |   await expect(
  125 |     page.getByText(/encrypted media transport remains gated by media-frame E2E/i),
  126 |   ).toBeVisible();
  127 |   await expect(page.getByText(/New contact · friend/)).toHaveCount(0);
  128 |   await expect(page.getByText(/Ops relay/)).toHaveCount(0);
  129 | }
  130 | 
  131 | test("two independent profiles exercise DM, invite join, and voice attempts honestly", async ({
  132 |   browser,
  133 | }) => {
  134 |   const alice = await openProfile(browser, "Alice", "Alice Desktop");
  135 |   const bob = await openProfile(browser, "Bob", "Bob Laptop");
  136 |   try {
  137 |     await sendDm(alice.page, "Bob", "alice to bob local DM harness ping");
  138 |     await sendDm(bob.page, "Alice", "bob to alice local DM harness pong");
  139 |     await expect(
  140 |       alice.page.getByText("bob to alice local DM harness pong"),
  141 |     ).toHaveCount(0);
  142 |     await expect(
  143 |       bob.page.getByText("alice to bob local DM harness ping"),
  144 |     ).toHaveCount(0);
  145 | 
  146 |     const invite = await createInvite(alice.page);
  147 |     await joinInvite(bob.page, invite);
  148 |     await attemptVoice(alice.page, "Alice");
  149 |     await attemptVoice(bob.page, "Bob");
  150 | 
  151 |     expect(alice.errors).toEqual([]);
```
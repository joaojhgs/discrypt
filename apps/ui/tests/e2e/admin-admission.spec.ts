import { expect, test } from "playwright/test";

async function bootReadyShell(page) {
  await page.goto("/");
  await page.evaluate(() => window.localStorage.clear());
  await page.reload();
  await expect(
    page.getByRole("heading", { name: /set up your local discrypt profile/i }),
  ).toBeVisible();
  await page.getByLabel("Display name").first().fill("Governance Owner");
  await page.getByLabel("Device name").first().fill("Owner Laptop");
  await page.getByRole("button", { name: /create new user/i }).click();
  await expect(
    page.getByRole("heading", { name: /Start a private space/i }),
  ).toBeVisible();
}

async function openLauncher(page) {
  await page.getByRole("button", { name: "Add group or direct message", exact: true }).click();
}

async function openCreateGroupModal(page) {
  await openLauncher(page);
  await page.getByRole("button", { name: /create a new group/i }).click();
}

test.beforeEach(async ({ page }) => {
  await bootReadyShell(page);
});

test("group creation and configuration expose admission mode controls", async ({ page }) => {
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Governance Lab");
  await page.getByLabel("Invite admission mode").selectOption("automatic_when_authorized_online");
  await page.getByRole("button", { name: /^Create group$/ }).last().click();

  await expect(page.getByRole("heading", { name: /#general/i })).toBeVisible();
  await expect(page.getByRole("complementary", { name: "Member panel" })).toBeVisible();
  await expect(page.getByText(/Governance Owner/).first()).toBeVisible();

  await page.getByRole("button", { name: /Open Governance Lab group/i }).click({ button: "right" });
  await page.getByRole("menuitem", { name: /group configuration/i }).click();
  await expect(page.getByLabel("Group admission mode")).toHaveValue("automatic_when_authorized_online");
  await page.getByLabel("Group admission mode").selectOption("manual_approval");
  await page.getByRole("button", { name: /save group configuration/i }).click();
  await expect(page.getByRole("dialog", { name: "Group configuration" })).toHaveCount(0);

  await page.getByRole("button", { name: /Open Governance Lab group/i }).click({ button: "right" });
  await page.getByRole("menuitem", { name: /group configuration/i }).click();
  await expect(page.getByLabel("Group admission mode")).toHaveValue("manual_approval");
});

test("topbar member button toggles the role-backed member panel", async ({ page }) => {
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Members Lab");
  await page.getByRole("button", { name: /^Create group$/ }).last().click();

  await expect(page.getByRole("complementary", { name: "Member panel" })).toBeVisible();
  await page.getByRole("button", { name: "Close member panel" }).click();
  await expect(page.getByRole("complementary", { name: "Member panel" })).toHaveCount(0);
  await page.getByRole("button", { name: "Open member panel" }).click();
  await expect(page.getByRole("complementary", { name: "Member panel" })).toBeVisible();
  await expect(page.getByText(/Role and presence are read from backend governance state/i)).toBeVisible();
});

test("admission review pane is available from mobile requests navigation", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 820 });
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Review Lab");
  await page.getByLabel("Invite admission mode").selectOption("manual_approval");
  await page.getByRole("button", { name: /^Create group$/ }).last().click();

  await page.getByRole("button", { name: "Requests" }).click();
  const reviewPane = page.getByRole("region", { name: "Main chat pane" });
  await expect(reviewPane.getByRole("heading", { name: /Pending admission requests/i }).last()).toBeVisible();
  await expect(reviewPane.getByText(/No pending requests/i)).toBeVisible();
});

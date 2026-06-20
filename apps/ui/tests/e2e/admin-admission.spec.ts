import { readFileSync } from "node:fs";
import { expect, test, type Page } from "playwright/test";

const auditLogCommandSnapshot = JSON.parse(
  readFileSync(
    new URL("./support/p5-t09-audit-log-command-snapshot.json", import.meta.url),
    "utf8",
  ),
) as Array<Record<string, unknown>>;

async function bootReadyShell(page) {
  await page.goto("/");
  await page.evaluate(() => window.localStorage.clear());
  await page.goto("/");
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

async function seedGovernancePanelState(page: Page, groupName: string) {
  await page.evaluate(({ name, auditLog }) => {
    const key = "discrypt.local-dev.app-state.v1";
    const stored = window.localStorage.getItem(key);
    if (!stored) throw new Error("Missing fallback command state");
    const state = JSON.parse(stored) as {
      groups?: Array<{
        group_id: string;
        name: string;
        members?: Array<Record<string, unknown>>;
        admission_requests?: Array<Record<string, unknown>>;
        governance_log?: Array<Record<string, unknown>>;
        role_policy?: Record<string, unknown>;
      }>;
    };
    const group = state.groups?.find((item) => item.name === name);
    if (!group) throw new Error(`Missing group ${name}`);
    const now = new Date();
    const future = new Date(now.getTime() + 5 * 60 * 1000).toISOString();
    const past = new Date(now.getTime() - 5 * 60 * 1000).toISOString();
    group.role_policy = {
      ...(group.role_policy ?? {}),
      admission_mode: "manual_approval",
    };
    group.members = [
      {
        member_id: "owner-governance",
        display_name: "Olivia Owner",
        device_id: "owner-laptop",
        role: "owner",
        status: "online",
        signer_public_key_hex: "owner-key",
        joined_at: past,
        last_seen_at: now.toISOString(),
        presence_expires_at: future,
        revoked_at: null,
        revoked_by: null,
      },
      {
        member_id: "staff-governance",
        display_name: "Sam Staff",
        device_id: "staff-laptop",
        role: "staff",
        status: "online",
        signer_public_key_hex: "staff-key",
        joined_at: past,
        last_seen_at: now.toISOString(),
        presence_expires_at: future,
        revoked_at: null,
        revoked_by: null,
      },
      {
        member_id: "member-governance",
        display_name: "Mira Member",
        device_id: "member-phone",
        role: "member",
        status: "online",
        signer_public_key_hex: "member-key",
        joined_at: past,
        last_seen_at: now.toISOString(),
        presence_expires_at: future,
        revoked_at: null,
        revoked_by: null,
      },
      {
        member_id: "offline-governance",
        display_name: "Owen Offline",
        device_id: "offline-tablet",
        role: "member",
        status: "offline",
        signer_public_key_hex: "offline-key",
        joined_at: past,
        last_seen_at: past,
        presence_expires_at: past,
        revoked_at: null,
        revoked_by: null,
      },
    ];
    group.admission_requests = [
      {
        request_id: "request-pending-panel",
        group_id: group.group_id,
        invite_id: "invite-panel",
        display_name: "Priya Pending",
        device_name: "Pending Laptop",
        member_identity: "pending-governance",
        signer_public_key_hex: "pending-key",
        key_package: [1, 2, 3],
        status: "pending",
        requested_at: now.toISOString(),
        decided_by: null,
        decided_at: null,
        decision_reason: null,
        policy_epoch_at_request: 1,
        admission_mode_at_request: "manual_approval",
      },
      {
        request_id: "request-approved-panel",
        group_id: group.group_id,
        invite_id: "invite-panel",
        display_name: "Priya Pending",
        device_name: "Pending Laptop",
        member_identity: "pending-governance",
        signer_public_key_hex: "pending-key",
        key_package: [1, 2, 3],
        status: "approved",
        requested_at: past,
        decided_by: "owner-governance",
        decided_at: "2026-06-20T08:45:00.000Z",
        decision_reason: null,
        policy_epoch_at_request: 1,
        admission_mode_at_request: "manual_approval",
      },
      {
        request_id: "request-refused-panel",
        group_id: group.group_id,
        invite_id: "invite-panel",
        display_name: "Riley Request",
        device_name: "Request Phone",
        member_identity: "refused-governance",
        signer_public_key_hex: "refused-key",
        key_package: [4, 5, 6],
        status: "refused",
        requested_at: past,
        decided_by: "staff-governance",
        decided_at: "2026-06-20T08:46:00.000Z",
        decision_reason: "Owner/staff refused request",
        policy_epoch_at_request: 1,
        admission_mode_at_request: "manual_approval",
      },
    ];
    group.governance_log = auditLog.map((entry) => ({
      ...entry,
      group_id: group.group_id,
    }));
    window.localStorage.setItem(key, JSON.stringify(state));
  }, { name: groupName, auditLog: auditLogCommandSnapshot });
  await page.reload();
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

test("right member panel sections backend governance and manual admission state", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Panel Lab");
  await page.getByLabel("Invite admission mode").selectOption("manual_approval");
  await page.getByRole("button", { name: /^Create group$/ }).last().click();
  await seedGovernancePanelState(page, "Panel Lab");

  const memberPanel = page.getByRole("complementary", { name: "Member panel" });
  await expect(memberPanel).toBeVisible();
  await expect(
    page.getByLabel("Channel navigation").getByRole("button", {
      name: /Pending requests · 1/i,
    }),
  ).toBeVisible();
  await expect(memberPanel.getByLabel("Manual admission")).toContainText("1 pending");

  const ownerSection = memberPanel.getByLabel("Owner member section");
  const staffSection = memberPanel.getByLabel("Staff member section");
  const membersSection = memberPanel.getByLabel("Members member section");
  const offlineSection = memberPanel.getByLabel("Offline member section");
  await expect(ownerSection.getByText("Olivia Owner")).toBeVisible();
  await expect(staffSection.getByText("Sam Staff")).toBeVisible();
  await expect(membersSection.getByText("Mira Member")).toBeVisible();
  await expect(offlineSection.getByText("Owen Offline")).toBeVisible();
  await expect(offlineSection.getByText("member · offline")).toBeVisible();

  await page.getByLabel("Mira Member member").click({ button: "right" });
  const memberMenu = page.getByRole("menu", { name: /Mira Member member actions/i });
  await expect(memberMenu.getByRole("menuitem", { name: /Make staff/i })).toBeVisible();
  await expect(memberMenu.getByRole("menuitem", { name: /Revoke access/i })).toBeVisible();
  await page.keyboard.press("Escape");

  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("right-member-panel-governance-sections.png"),
  });
});

test("governance audit log summarizes and expands backend command history", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Audit Lab");
  await page.getByLabel("Invite admission mode").selectOption("manual_approval");
  await page.getByRole("button", { name: /^Create group$/ }).last().click();
  await seedGovernancePanelState(page, "Audit Lab");

  await page
    .getByLabel("Channel navigation")
    .getByRole("button", { name: /Audit log/i })
    .click();

  const auditPane = page.getByTestId("main-chat-content");
  await expect(
    auditPane.getByRole("heading", { name: /Governance audit log/i }),
  ).toBeVisible();
  await expect(auditPane.getByText("owner").first()).toBeVisible();
  await expect(auditPane.getByText("4 events")).toBeVisible();
  await expect(auditPane.getByText("Approved").first()).toBeVisible();
  await expect(auditPane.getByText("Refused").first()).toBeVisible();
  await expect(auditPane.getByText("Promoted").first()).toBeVisible();
  await expect(auditPane.getByText("Revoked").first()).toBeVisible();

  const approvedEntry = page
    .locator("article")
    .filter({ hasText: "Approved Priya Pending admission" });
  await expect(approvedEntry).toBeVisible();
  await expect(approvedEntry.getByText("Priya Pending (approved)")).toBeVisible();
  await expect(
    page.locator("article").filter({ hasText: "Refused Riley Request admission" }),
  ).toBeVisible();
  await expect(
    page.locator("article").filter({ hasText: "Promoted Mira Member to staff" }),
  ).toBeVisible();
  await expect(
    page.locator("article").filter({ hasText: "Revoked Owen Offline access" }),
  ).toBeVisible();

  await auditPane.getByText("Approved history").click();
  await expect(approvedEntry).toBeHidden();
  await auditPane.getByText("Approved history").click();
  await expect(approvedEntry).toBeVisible();

  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("governance-audit-log-history.png"),
  });
});

test("governance audit log shows empty and degraded mobile states without fabricating events", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 820 });
  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("Empty Audit Lab");
  await page.getByRole("button", { name: /^Create group$/ }).last().click();

  await page.getByRole("button", { name: "Audit" }).click();
  const auditPane = page.getByTestId("main-chat-content");
  await expect(
    auditPane.getByRole("heading", { name: /Governance audit log/i }),
  ).toBeVisible();
  await expect(auditPane.getByText("No audit events yet")).toBeVisible();
  await expect(
    auditPane.getByText(/backend governance commands record them/i),
  ).toBeVisible();

  await page.evaluate(() => {
    const key = "discrypt.local-dev.app-state.v1";
    const stored = window.localStorage.getItem(key);
    if (!stored) throw new Error("Missing fallback command state");
    const state = JSON.parse(stored) as {
      groups?: Array<Record<string, unknown>>;
    };
    const group = state.groups?.find((item) => item.name === "Empty Audit Lab");
    if (!group) throw new Error("Missing Empty Audit Lab");
    group.governance_log = "malformed-audit-snapshot";
    window.localStorage.setItem(key, JSON.stringify(state));
  });
  await page.reload();
  await page.getByRole("button", { name: "Audit" }).click();
  await expect(auditPane.getByText("Audit snapshot unavailable")).toBeVisible();
  await expect(auditPane.getByText(/not making audit claims/i)).toBeVisible();
});

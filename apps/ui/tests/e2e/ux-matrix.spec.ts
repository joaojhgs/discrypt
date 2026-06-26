import { expect, test, type Page } from "playwright/test";
import { bootReadyShell, expectNoProductionCopyDebt } from "./support/app-shell";

const FALLBACK_STORAGE_KEY = "discrypt.local-dev.app-state.v1";

async function openLauncher(page: Page) {
  await page
    .getByRole("button", { name: "Add group or direct message", exact: true })
    .click();
}

async function openCreateGroupModal(page: Page) {
  await openLauncher(page);
  await page.getByRole("button", { name: /create a new group/i }).click();
}

async function openGroupContextMenu(page: Page, groupName: string) {
  await page
    .getByRole("button", { name: new RegExp(`Open ${groupName} group`, "i") })
    .click({ button: "right" });
}

async function openGroupConfiguration(page: Page, groupName: string) {
  await openGroupContextMenu(page, groupName);
  await page.getByRole("menuitem", { name: /group configuration/i }).click();
}

async function openGroupInviteModal(page: Page, groupName: string) {
  await openGroupContextMenu(page, groupName);
  await page.getByRole("menuitem", { name: /create invite/i }).click();
}

async function seedManualAdmissionAndMemberEvidence(page: Page, groupName: string) {
  await page.evaluate(
    ({ storageKey, name }) => {
      const raw = window.localStorage.getItem(storageKey);
      if (!raw) throw new Error("fallback command state missing");
      const state = JSON.parse(raw);
      const group = state.groups?.find((candidate: { name?: string }) => candidate.name === name);
      if (!group) throw new Error(`missing group ${name}`);
      const localMemberId = state.profile?.user_id ?? "local-profile-pending";
      const now = new Date();
      const future = new Date(now.getTime() + 5 * 60 * 1000).toISOString();
      const past = new Date(now.getTime() - 5 * 60 * 1000).toISOString();

      group.role_policy = {
        ...(group.role_policy ?? {}),
        admission_mode: "manual_approval",
        policy_epoch: Number(group.role_policy?.policy_epoch ?? 1) + 1,
        updated_by: localMemberId,
        updated_at: now.toISOString(),
      };
      group.members = [
        {
          ...(group.members?.[0] ?? {}),
          member_id: localMemberId,
          display_name: state.profile?.display_name ?? "E2E User",
          device_id: state.profile?.device_name ?? "E2E Device",
          role: "owner",
          status: "online",
          joined_at: past,
          last_seen_at: now.toISOString(),
          presence_expires_at: future,
          route_evidence: {
            route_kind: "direct",
            evidence_source: "backend_route_graph",
          },
        },
        {
          member_id: "matrix-turn-member",
          display_name: "Tara TURN",
          device_id: "turn-laptop",
          role: "member",
          status: "online",
          signer_public_key_hex: "turn-key",
          joined_at: past,
          last_seen_at: now.toISOString(),
          presence_expires_at: future,
          revoked_at: null,
          revoked_by: null,
          route_evidence: {
            route_kind: "turn",
            evidence_source: "backend_route_graph",
          },
        },
        {
          member_id: "matrix-provider-only",
          display_name: "Noah No Proof",
          device_id: "no-proof-phone",
          role: "member",
          status: "online",
          signer_public_key_hex: "no-proof-key",
          joined_at: past,
          last_seen_at: now.toISOString(),
          presence_expires_at: future,
          revoked_at: null,
          revoked_by: null,
          route_evidence: {
            route_kind: "provider_signaling",
            evidence_source: "signaling_adapter_only",
          },
        },
        {
          member_id: "matrix-offline-member",
          display_name: "Olive Offline",
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
          request_id: "matrix-approve-request",
          group_id: group.group_id,
          invite_id: "matrix-invite",
          display_name: "Mina Matrix",
          device_name: "Mina Laptop",
          member_identity: "matrix-mina",
          signer_public_key_hex: "mina-key",
          key_package: [1, 2, 3],
          status: "pending",
          requested_at: now.toISOString(),
          decided_by: null,
          decided_at: null,
          decision_reason: null,
          policy_epoch_at_request: group.role_policy.policy_epoch,
          admission_mode_at_request: "manual_approval",
        },
        {
          request_id: "matrix-refuse-request",
          group_id: group.group_id,
          invite_id: "matrix-invite",
          display_name: "Riley Review",
          device_name: "Riley Phone",
          member_identity: "matrix-riley",
          signer_public_key_hex: "riley-key",
          key_package: [4, 5, 6],
          status: "pending",
          requested_at: now.toISOString(),
          decided_by: null,
          decided_at: null,
          decision_reason: null,
          policy_epoch_at_request: group.role_policy.policy_epoch,
          admission_mode_at_request: "manual_approval",
        },
      ];
      window.localStorage.setItem(storageKey, JSON.stringify(state));
    },
    { storageKey: FALLBACK_STORAGE_KEY, name: groupName },
  );
  await page.reload();
}

test("PER-96 Playwright UX matrix covers command-backed setup, admission, text, voice, config, and members", async ({
  context,
  page,
}, testInfo) => {
  test.setTimeout(180_000);
  await context.grantPermissions(["clipboard-read", "clipboard-write"], {
    origin: "http://127.0.0.1:4173",
  });

  const errors = await bootReadyShell(page);
  await expectNoProductionCopyDebt(page);
  await expect(page.getByTestId("app-shell")).toHaveAttribute(
    "data-theme",
    "graphite-calm",
  );

  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
  const configDialog = page.getByRole("dialog", { name: "Config" });
  await configDialog.getByLabel("Theme").selectOption("ocean-contrast");
  await configDialog.getByTestId("voice-mic-selector").selectOption("backup-e2e-mic");
  await configDialog.getByLabel("App output device").selectOption("e2e-speaker");
  await expect(configDialog.getByLabel("Theme")).toHaveValue("ocean-contrast");
  await expect(configDialog.getByTestId("voice-mic-selector")).toHaveValue("backup-e2e-mic");
  await page.getByRole("button", { name: /Close Config/i }).click();

  await openCreateGroupModal(page);
  await page.getByLabel("Group name").fill("PER-96 Matrix Lab");
  await page
    .getByLabel("Invite admission mode")
    .selectOption("automatic_when_authorized_online");
  await page.locator('select[aria-label="Signaling adapter"]').selectOption("mqtt");
  await page.getByLabel("Signaling endpoint").fill("mqtts://broker.emqx.io:8883");
  await page.getByLabel("STUN servers").fill("stun:stun.l.google.com:19302");
  await page.getByLabel("TURN servers").fill("turns:turn.example.invalid:5349");
  await page.getByRole("button", { name: /^Create group$/ }).last().click();
  await expect(page.getByRole("heading", { name: "#general", exact: true })).toBeVisible();

  await openGroupConfiguration(page, "PER-96 Matrix Lab");
  await expect(page.getByLabel("Group admission mode")).toHaveValue(
    "automatic_when_authorized_online",
  );
  await page.getByRole("button", { name: /Close Group configuration/i }).click();

  await openGroupInviteModal(page, "PER-96 Matrix Lab");
  const inviteSheet = page.getByRole("dialog", { name: "Create group invite" });
  await inviteSheet.getByLabel("Invite expiry").selectOption("30");
  await inviteSheet.getByLabel("Maximum uses").fill("5");
  await page.getByRole("button", { name: /create invite for/i }).click();
  await expect(inviteSheet.getByRole("heading", { name: "Latest invite descriptor" })).toBeVisible();
  await expect(inviteSheet.getByText("Admission", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByText(/Welcome required/i)).toBeVisible();
  await expect(inviteSheet.getByText("Password gate", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByText("Not required", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByText("TURN metadata", { exact: true })).toBeVisible();
  await expect(inviteSheet.getByTestId("invite-link")).toHaveValue(/discrypt:\/\/join\/v1/i);
  await page.getByRole("button", { name: /Close Create group invite/i }).click();

  await page.getByRole("button", { name: /Add text channel/i }).click();
  await page.getByLabel("Text channel name").fill("matrix-ops");
  await page.getByLabel("Text channel name").press("Enter");
  await expect(page.getByRole("heading", { name: "#matrix-ops" })).toBeVisible();
  await page.getByRole("textbox", { name: "Message" }).fill("PER-96 command-backed text");
  await page.getByRole("button", { name: /^Send message$/ }).click();
  const matrixMessage = page.getByTestId("message-row").filter({
    hasText: "PER-96 command-backed text",
  });
  await expect(matrixMessage).toHaveAttribute("data-message-state", "sent_local");
  await expect(matrixMessage.getByTestId("message-delivery-status")).toHaveAccessibleName(
    /peer receipt requires backend-state evidence/i,
  );

  await seedManualAdmissionAndMemberEvidence(page, "PER-96 Matrix Lab");
  const memberPanel = page.getByRole("complementary", { name: "Member panel" });
  await expect(memberPanel.getByLabel("Manual admission")).toContainText("2 pending");
  await expect(memberPanel.getByLabel("Tara TURN member")).toContainText("route: TURN");
  await expect(memberPanel.getByLabel("Noah No Proof member")).toContainText(
    "route: no route proof",
  );
  await expect(memberPanel.getByLabel("Olive Offline member")).toContainText(
    "member · offline",
  );
  await memberPanel.getByRole("button", { name: /Review requests/i }).click();
  const admissionPanel = page.getByTestId("main-chat-content");
  await expect(
    admissionPanel.getByRole("heading", { name: "Pending admission requests" }),
  ).toBeVisible();
  await expect(admissionPanel.getByText("Mina Matrix", { exact: true })).toBeVisible();
  await admissionPanel.getByRole("button", { name: "Approve" }).first().click();
  await expect(admissionPanel.getByText("Mina Matrix", { exact: true })).toBeVisible();
  await expect(admissionPanel.getByText("approved", { exact: true })).toBeVisible();
  await admissionPanel.getByRole("button", { name: "Refuse" }).first().click();
  await expect(admissionPanel.getByText("refused", { exact: true })).toBeVisible();
  await expect(memberPanel.getByLabel("Manual admission")).toContainText("0 pending");
  await expect(memberPanel.getByLabel("Mina Matrix member")).toContainText(
    "member · offline",
  );

  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-sidebar-status")).toBeVisible();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(1);
  await page.getByRole("button", { name: /^Mute$/i }).click();
  await expect(page.getByRole("button", { name: /^Unmute$/i })).toBeVisible();
  await expect(page.getByTestId("voice-remote-audio")).toHaveCount(0);
  await expect(page.getByTestId("voice-remote-volume")).toHaveCount(0);
  await page.getByRole("button", { name: /Leave voice call/i }).click();
  await expect(page.getByText(/Voice idle/i)).toBeVisible();

  await page.evaluate(() => {
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        ...navigator.mediaDevices,
        getUserMedia: async () => {
          throw new DOMException("Permission denied", "NotAllowedError");
        },
      },
    });
  });
  await page.getByRole("button", { name: /Voice Lobby/ }).click();
  await expect(page.getByTestId("voice-local-participant")).toHaveCount(0);
  await expect(page.getByText(/Microphone permission\/input device required/i)).toBeVisible();

  await expect(page.locator("#runtime-local-peer")).toHaveCount(0);
  await expect(page.locator("#runtime-remote-peer")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /probe adapter/i })).toHaveCount(0);
  await expect(page.getByText(/manual pairing|QR pairing/i)).toHaveCount(0);

  await page.reload();
  await expect(page.getByTestId("app-shell")).toHaveAttribute(
    "data-theme",
    "ocean-contrast",
  );
  await page.getByRole("button", { name: "Open app configuration", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Config" }).getByTestId("voice-mic-selector")).toHaveValue(
    "backup-e2e-mic",
  );
  await page.getByRole("button", { name: /Close Config/i }).click();

  await page.screenshot({
    fullPage: true,
    path: testInfo.outputPath("per96-ux-matrix-final.png"),
  });
  expect(errors).toEqual(["[discrypt:command-error] command_error_reported"]);
});

import { expect, test } from 'playwright/test';

test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /set up your local discrypt profile/i })).toBeVisible();
  await page.getByRole('button', { name: /create new user/i }).click();
  await expect(page.getByRole('button', { name: /create group/i }).first()).toBeVisible();
  expect(errors).toEqual([]);
});

test('first run creates user and empty shell does not blank', async ({ page }) => {
  await expect(page.getByRole('tab', { name: 'DMs' })).toBeVisible();
  await page.getByRole('tab', { name: 'DMs' }).click();
  await expect(page.getByRole('heading', { name: /direct messages/i })).toBeVisible();
  await expect(page.getByText(/no remote delivery is claimed/i).first()).toBeVisible();
});

test('setup workflow is readable and completes without rail or dock collisions', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.getByRole('tab', { name: 'Setup' }).click();

  await expect(page.getByRole('heading', { name: /finish the local trust setup/i })).toBeVisible();
  await expect(page.getByText('3/4').first()).toBeVisible();
  await expect(page.getByText(/Voice session not joined/i)).toHaveCount(0);
  await expect(page.getByRole('tab', { name: /members/i })).toHaveCount(0);

  const setupPanelBounds = await page.getByRole('heading', { name: /finish the local trust setup/i }).evaluate((element) => {
    const panel = element.closest('.mx-auto');
    const rect = panel?.getBoundingClientRect();
    return rect ? { bottom: rect.bottom } : null;
  });
  expect(setupPanelBounds).not.toBeNull();
  expect(setupPanelBounds?.bottom ?? 0).toBeLessThanOrEqual(1000);

  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow).toBeLessThanOrEqual(1);

  await page.getByRole('button', { name: /mark as verified/i }).click();
  await expect(page.getByText('4/4').first()).toBeVisible();
  await expect(page.getByText(/Safety number verified/i).first()).toBeVisible();
});


test('direct message send stays command-backed', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });

  await page.getByRole('tab', { name: 'DMs' }).click();
  await expect(page.getByText(/Bob/).first()).toBeVisible();
  await page.getByLabel('Message').fill('DM ping from the local harness');
  await page.getByRole('button', { name: /send dm message/i }).click();
  await expect(page.getByText(/DM ping from the local harness/i)).toBeVisible();
  await expect(page.getByText(/no remote delivery is claimed/i).first()).toBeVisible();
  expect(errors).toEqual([]);
});


test('group invite text and voice leave regression remain on shell', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });

  await page.getByRole('tab', { name: 'Create group' }).click();
  await page.getByRole('button', { name: /create local setup/i }).click();
  await expect(page.getByText(/private lab/i).first()).toBeVisible();

  await page.getByRole('tab', { name: 'Join' }).click();
  await page.getByRole('button', { name: /create copyable invite/i }).click();
  await expect(page.getByText(/invite ready: discrypt:\/\/join/i)).toBeVisible();

  await page.getByRole('tab', { name: 'Channels' }).click();
  await page.getByLabel('Channels').getByRole('button', { name: /create channel/i }).click();
  await page.getByRole('button', { name: /confirm local channel/i }).click();
  await expect(page.getByText('#secure-room').first()).toBeVisible();
  await page.getByRole('button', { name: /send command-backed message/i }).click();
  await expect(page.getByText(/hello from the command-backed ui/i)).toBeVisible();

  await page.getByRole('tab', { name: 'Voice' }).click();
  await page.getByLabel('Voice', { exact: true }).getByRole('button', { name: /join call/i }).click();
  await expect(page.getByText(/voice session joined/i).first()).toBeVisible();
  await page.getByLabel('Voice', { exact: true }).getByRole('button', { name: /leave call/i }).click();
  await expect(page.getByText(/voice session not joined/i).first()).toBeVisible();
  await expect(page.getByText(/private lab/i).first()).toBeVisible();
  expect(errors).toEqual([]);
});

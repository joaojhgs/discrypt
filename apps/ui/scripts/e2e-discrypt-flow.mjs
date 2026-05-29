#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from 'playwright';

const port = 1421;
const host = `http://127.0.0.1:${port}`;
const server = spawn('npm', ['run', 'dev', '--', '--host', '127.0.0.1', '--port', String(port)], {
  cwd: new URL('..', import.meta.url),
  stdio: ['ignore', 'pipe', 'pipe'],
  detached: true,
});
let logs = '';
server.stdout.on('data', (chunk) => (logs += chunk.toString()));
server.stderr.on('data', (chunk) => (logs += chunk.toString()));

async function waitForServer() {
  for (let i = 0; i < 80; i += 1) {
    try {
      const response = await fetch(host);
      if (response.ok) return;
    } catch {}
    await delay(125);
  }
  throw new Error(`Vite server did not start. Logs:\n${logs}`);
}

try {
  await waitForServer();
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  await page.goto(host);
  await page.getByRole('heading', { name: /welcome to discrypt/i }).waitFor();
  await page.getByRole('button', { name: /create user/i }).click();
  await page.getByText(/Alice's discrypt/i).waitFor();

  await page.getByRole('tab', { name: 'DMs' }).click();
  await page.getByRole('button', { name: /start dm/i }).click();
  await page.getByRole('button', { name: /^send$/i }).click();
  await page.getByText(/Hello from discrypt/i).waitFor();

  await page.getByRole('tab', { name: 'Groups' }).click();
  await page.getByRole('button', { name: /create group/i }).click();
  const inviteButton = page.getByRole('button', { name: /^create invite/i });
  await inviteButton.waitFor();
  await inviteButton.click();
  await page.locator('code').filter({ hasText: /discrypt:\/\/join/i }).first().waitFor();
  await page.getByRole('button', { name: /create text/i }).click();
  await page.getByRole('button', { name: /create voice/i }).click();

  await page.getByRole('tab', { name: 'Voice' }).click();
  await page.getByRole('button', { name: /join/i }).first().click();
  await page.getByText(/Voice joined/i).waitFor();
  await page.getByRole('switch').click();
  await page.getByRole('button', { name: /leave/i }).last().click();
  await page.getByText(/Not in voice/i).waitFor();
  await page.getByText(/Alice's discrypt/i).waitFor();

  if (errors.length > 0) throw new Error(`Browser console/page errors:\n${errors.join('\n')}`);
  await browser.close();
  console.log('Playwright E2E passed: first-run setup, DM, group/invite/channel, voice join/mute/leave without blank screen.');
} finally {
  if (server.pid) {
    try { process.kill(-server.pid, 'SIGTERM'); } catch { server.kill('SIGTERM'); }
  }
}
process.exit(0);

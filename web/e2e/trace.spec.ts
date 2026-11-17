// The trace of the server in the page: what each piece of work cost, as
// `--trace` prints it, without a restart.

import { expect, test } from '@playwright/test';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start();
  await page.goto(server.url);
  await expect(page.getByRole('button', { name: /long: touch two places/ })).toBeVisible();
});
test.afterEach(() => server?.stop());

test('the version opens the trace, which shows the work of the server', async ({ page }) => {
  await page.locator('button.version').click();

  const dialog = page.getByRole('dialog', { name: 'Trace' });
  await expect(dialog).toBeVisible();
  // The start read the series with git, and the page asked for the session.
  await expect(dialog.locator('.trace-line', { hasText: 'git ' }).first()).toBeVisible();
  await expect(
    dialog.locator('.trace-line', { hasText: 'GET /api/session' }).first(),
  ).toBeVisible();
  // The window asks every second, and its own requests make no line.
  await expect(dialog.locator('.trace-line', { hasText: '/api/trace' })).toHaveCount(0);

  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
});

test('the server says what it waits on', async ({ page }) => {
  const api = (path: string) => new URL(path, server.url).href;
  const now = (await (await page.request.get(api('/api/activity'))).json()) as {
    waiting: unknown[];
  };
  expect(now.waiting).toEqual([]);
});

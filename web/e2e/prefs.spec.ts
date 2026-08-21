// The preferences panel. It writes a file, so this suite owns its own
// server: a setting made here would otherwise reach the next suite.

import { expect, test } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeAll(async () => {
  server = await start();
});
test.afterAll(() => server?.stop());

test.beforeEach(async ({ page }) => {
  await page.goto(server.url);
});

test('ignoring whitespace drops the file that is only spacing', async ({ page }) => {
  await openChange(page, /long: touch two places/);

  // The change touches three files, and one of them only moved a line right.
  await expect(page.locator('.file-row', { hasText: 'spacing.c' })).toBeVisible();
  await expect(page.locator('.file-row', { hasText: 'long.c' })).toBeVisible();

  await page.getByRole('button', { name: '⚙' }).click();
  await page.getByRole('dialog').getByLabel('Ignore whitespace').check();
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(page.locator('.file-row', { hasText: 'spacing.c' })).toHaveCount(0);
  await expect(page.locator('.file-row', { hasText: 'long.c' })).toBeVisible();
});

test('a preference is kept for the next run', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await expect(page.getByRole('button', { name: '+26 common lines' })).toBeVisible();

  await page.getByRole('button', { name: '⚙' }).click();
  await page.getByRole('dialog').getByLabel('Context').selectOption('25');
  await page.getByRole('button', { name: 'Save' }).click();

  // Twenty-five lines of context swallow the gap between the two hunks.
  await expect(page.getByRole('button', { name: /common lines/ })).toHaveCount(0);

  await page.reload();
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await expect(page.getByRole('button', { name: /common lines/ })).toHaveCount(0);
});

test('the theme can be chosen rather than followed', async ({ page }) => {
  // The browser is light here, so a dark page has to be asked for.
  await expect(page.locator('html')).not.toHaveAttribute('data-theme');
  const light = await page
    .locator('body')
    .evaluate((node) => getComputedStyle(node).backgroundColor);

  await page.getByRole('button', { name: '⚙' }).click();
  await page.getByRole('dialog').getByLabel('Theme').selectOption('dark');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  const dark = await page
    .locator('body')
    .evaluate((node) => getComputedStyle(node).backgroundColor);
  expect(dark).not.toBe(light);

  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

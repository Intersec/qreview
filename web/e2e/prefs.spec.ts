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

test('a comment being written survives a setting change', async ({ page }) => {
  // Reading the diff again used to replace the rows under an open box, and
  // the box went with them. On a slow machine that took the box away between
  // the click that opened it and the click that saved it.
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await page.locator('td.gutter-comment').first().click();
  await page.getByRole('textbox').first().fill('Still here.');

  await page.getByRole('button', { name: '⚙' }).click();
  await page.getByRole('dialog').getByLabel('Context').selectOption('3');
  await page.getByRole('dialog').getByRole('button', { name: 'Save' }).click();

  // Three lines of context widen the gap, so this text proves the fresh diff
  // is on the screen. Only then is the box worth asking about.
  await expect(page.getByRole('button', { name: '+40 common lines' })).toBeVisible();
  await expect(page.getByRole('textbox')).toHaveCount(1);
  await expect(page.getByRole('textbox')).toHaveValue('Still here.');

  await page.getByRole('button', { name: 'Save' }).click();
  await expect(page.locator('tr.talk', { hasText: 'Still here.' })).toHaveCount(1);
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

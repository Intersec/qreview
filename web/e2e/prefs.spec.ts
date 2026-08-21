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

test('a comment can be written on a line the diff did not carry', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await page
    .getByRole('button', { name: /common lines/ })
    .first()
    .click();
  await expect(page.getByText('line 12', { exact: true })).toBeVisible();

  const row = page.locator('tr', { has: page.getByText('line 12', { exact: true }) }).first();
  await row.locator('td.gutter-comment').click();
  await page.getByRole('textbox').first().fill('Context lines take comments too.');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(page.getByText('Context lines take comments too.')).toBeVisible();
});

test('a comment box does not follow you to the next file', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await page.locator('td.gutter-comment').first().click();
  await expect(page.getByRole('textbox')).toHaveCount(1);

  await openFile(page, 'added.py');
  await expect(page.getByRole('textbox')).toHaveCount(0);
});

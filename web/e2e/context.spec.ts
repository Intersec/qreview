// The lines a diff does not carry, opened from the bar between two hunks.

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
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
});

test('the bar says how many lines it hides', async ({ page }) => {
  // The fixture touches line 3 and line 27 of a file of 30 lines, so the
  // gap between the two hunks is lines 7 to 23.
  await expect(page.getByRole('button', { name: '+17 common lines' })).toBeVisible();
  await expect(page.getByText('line 12', { exact: true })).toBeHidden();
});

test('opening the whole gap shows the lines and takes the bar away', async ({ page }) => {
  const bar = page.getByRole('button', { name: '+17 common lines' });
  await bar.click();

  await expect(page.getByText('line 12', { exact: true })).toBeVisible();
  await expect(bar).toBeHidden();
});

test('an opened line carries both line numbers', async ({ page }) => {
  await page.getByRole('button', { name: '+17 common lines' }).click();

  // Nothing was added above line 12, so the two sides agree on the number.
  const row = page.locator('tr', { hasText: /^\s*12\s*12\s*line 12$/ });
  await expect(row).toHaveCount(1);
});

test('the short step opens the end nearest the hunk', async ({ page }) => {
  await page.getByRole('button', { name: '+10' }).click();

  await expect(page.getByText('line 23', { exact: true })).toBeVisible();
  await expect(page.getByText('line 13', { exact: true })).toBeHidden();
  await expect(page.getByRole('button', { name: '+7 common lines' })).toBeVisible();
});

test('the context opens in the side by side view too', async ({ page }) => {
  await page.getByRole('button', { name: 'Side by side' }).click();
  await page.getByRole('button', { name: '+17 common lines' }).click();

  await expect(page.getByText('line 12', { exact: true })).toHaveCount(2);
});

test('a file with no gap has no bar', async ({ page }) => {
  await openFile(page, 'added.py');

  await expect(page.getByRole('button', { name: /common lines/ })).toHaveCount(0);
});

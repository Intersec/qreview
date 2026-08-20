// Reading a series, and writing on it.

import { expect, test } from '@playwright/test';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeAll(async () => {
  server = await start();
});
test.afterAll(() => server?.stop());

test.beforeEach(async ({ page }) => {
  await page.goto(server.url);
});

test('the series stops at the merge and says so', async ({ page }) => {
  await expect(page.getByRole('button', { name: /net: retry the read/ })).toBeVisible();
  await expect(page.getByText('Merge branch side into main')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Load 5 older' })).toBeVisible();
});

test('a change shows its files and the first diff', async ({ page }) => {
  await page.getByRole('button', { name: /docs: rename the document/ }).click();

  await expect(page.locator('.file-row', { hasText: 'new-name.md' })).toBeVisible();
  // The header of the diff names both sides of the rename.
  await expect(page.locator('.file-bar h2')).toContainText('docs/old-name.md →');
  await expect(page.locator('.file-bar h2')).toContainText('docs/new-name.md');
});

test('a comment on a line comes back after a reload', async ({ page }) => {
  await page.getByRole('button', { name: /net: retry the read/ }).click();
  await page.getByRole('button', { name: /src\/net\.blk/ }).click();

  await page.locator('td.gutter-comment').first().click();
  await page.getByRole('textbox').first().fill('This loop never ends.');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(page.getByText('This loop never ends.')).toBeVisible();

  await page.reload();
  await page.getByRole('button', { name: /net: retry the read/ }).click();
  await page.getByRole('button', { name: /src\/net\.blk/ }).click();
  await expect(page.getByText('This loop never ends.')).toBeVisible();
});

test('the keyboard walks the files', async ({ page }) => {
  await page.getByRole('button', { name: /long: touch two places/ }).click();
  await expect(page.locator('header h2')).toContainText('src/added.py');

  await page.keyboard.press('j');
  await expect(page.locator('header h2')).toContainText('src/long.c');

  await page.keyboard.press('k');
  await expect(page.locator('header h2')).toContainText('src/added.py');
});

test('the page reports no error to the console', async ({ page }) => {
  const complaints: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error' || message.type() === 'warning') {
      complaints.push(message.text());
    }
  });
  page.on('pageerror', (error) => complaints.push(String(error)));

  await page.goto(server.url);
  await page.getByRole('button', { name: /long: touch two places/ }).click();
  await page.getByRole('button', { name: 'Side by side' }).click();

  expect(complaints).toEqual([]);
});

// The browser history of a review: back, forward, and a reload.

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

test('the URL names the change and the file being read', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');

  expect(page.url()).toContain('change=Ilongtwohunks');
  expect(page.url()).toContain('file=src/long.c');
});

test('back and forward walk the files that were read', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await openFile(page, 'added.py');

  await page.goBack();
  await expect(page.locator('.file-bar h2')).toContainText('long.c');

  await page.goForward();
  await expect(page.locator('.file-bar h2')).toContainText('added.py');
});

test('back walks the changes that were read', async ({ page }) => {
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  await openChange(page, /docs: rename the document/);
  await expect(page.locator('.change-bar')).toContainText('docs: rename the document');

  await page.goBack();
  await expect(page.locator('.change-bar')).toContainText('net: retry the read');
  await expect(page.locator('.file-bar h2')).toContainText('net.blk');
});

test('back returns to the base a merge was read against', async ({ page }) => {
  await openChange(page, /Merge branch side into main/);
  await openFile(page, 'net.blk');

  await page.getByLabel('Read against').selectOption('parent1');
  expect(page.url()).toContain('base=parent1');
  await expect(page.locator('tr', { hasText: '<<<<<<<' })).toHaveCount(0);

  await page.goBack();
  await expect(page.locator('.patch-bar')).toContainText('the auto-merge');
  await expect(page.locator('tr', { hasText: '<<<<<<<' })).toBeVisible();
});

test('a reload comes back to the place that was read', async ({ page }) => {
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');

  await page.reload();

  await expect(page.locator('.change-bar')).toContainText('docs: rename the document');
  await expect(page.locator('.file-bar h2')).toContainText('new-name.md');
});

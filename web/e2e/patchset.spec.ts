// The two versions of a change, read against each other.

import { expect, test } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeAll(async () => {
  server = await start({ prev: true });
});
test.afterAll(() => server?.stop());

test.beforeEach(async ({ page }) => {
  await page.goto(server.url);
  await openChange(page, /net: retry the read/);
});

test('the two selectors name the versions', async ({ page }) => {
  const base = page.locator('#base-of');
  const target = page.locator('#read-set');

  await expect(base).toBeVisible();
  await expect(base).toHaveValue('parent');
  await expect(target).toHaveValue('2');
  await expect(target.locator('option')).toHaveCount(2);
});

test('reading one version against the other shows only the work', async ({ page }) => {
  await page.locator('#base-of').selectOption('ps:1');

  await expect(page.locator('.file-row')).toHaveCount(1);
  await expect(page.locator('.file-row')).toContainText('net.blk');

  await openFile(page, 'net.blk');
  // One version differs from the other by that one word, and by nothing
  // else. Against the parent it would be the whole loop.
  await expect(page.locator('td.code-cell.row-add')).toHaveCount(1);
  await expect(page.locator('td.code-cell.row-add')).toContainText('recvmsg');
});

test('a version has no selector when it is the only one', async ({ page }) => {
  await openChange(page, /docs: rename the document/);

  await expect(page.locator('#base-of')).toHaveCount(0);
});

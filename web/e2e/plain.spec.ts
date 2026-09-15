// A file too long to paint in color, opened in one click.

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
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'huge.c');
});

test('opening a gap of thousands of lines shows them', async ({ page }) => {
  // The run is far longer than the route used to accept, and it answered a
  // 400 that nothing reported. See issue 20.
  const bar = page.getByRole('button', { name: /common lines/ }).first();
  await bar.click();

  await expect(page.getByText('int a1;', { exact: true }).first()).toBeVisible();
  await expect(page.locator('.error')).toHaveCount(0);
});

test('the colors go while the file is that long, and the bar says so', async ({ page }) => {
  // The hunk is painted until the gap opens.
  await expect(page.locator('td.code-cell span[class*="tok-"]').first()).toBeVisible();
  await expect(page.locator('.file-bar')).not.toContainText('shown without color');

  await page
    .getByRole('button', { name: /common lines/ })
    .first()
    .click();
  await expect(page.getByText('int a1;', { exact: true }).first()).toBeVisible();

  // The whole file, hunks included, and the bar says why.
  await expect(page.locator('td.code-cell span[class*="tok-"]')).toHaveCount(0);
  await expect(page.locator('.file-bar')).toContainText('shown without color');
});

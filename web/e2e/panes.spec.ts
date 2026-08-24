// The bars between the panes, and the floors they stop at.

import { expect, test, type Page } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

// Own server: a test drags a bar, and the size it leaves behind is kept in
// the browser of that page alone. The comments a test writes are its own too.
let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start();
  await page.goto(server.url);
});

test.afterEach(() => server?.stop());

async function drag(page: Page, bar: string, byX: number, byY: number) {
  const box = (await page.locator(bar).boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + byX, box.y + box.height / 2 + byY, { steps: 6 });
  await page.mouse.up();
}

/// Write one remark, so the list of comments is on the screen.
async function remark(page: Page) {
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  await page.locator('td.gutter-comment').first().click();
  await page.getByRole('textbox').first().fill('One remark.');
  await page.getByRole('button', { name: 'Save' }).click();
  await page.locator('.list-row').first().waitFor();
}

test('the series pane is made wider, and the width is kept', async ({ page }) => {
  const was = (await page.locator('.side').boundingBox())!.width;

  await drag(page, '.split-vertical', 120, 0);
  const now = (await page.locator('.side').boundingBox())!.width;
  expect(now).toBeGreaterThan(was + 100);

  await page.reload();
  await expect(page.locator('.side')).toBeVisible();
  expect((await page.locator('.side').boundingBox())!.width).toBeCloseTo(now, 0);
});

test('the series pane stops while its title is still there', async ({ page }) => {
  await drag(page, '.split-vertical', -600, 0);

  const narrow = (await page.locator('.side').boundingBox())!.width;
  expect(narrow).toBeGreaterThan(60);
  await expect(page.locator('.side-head')).toBeVisible();
});

test('the list of comments is made taller, and stops with its title showing', async ({ page }) => {
  await remark(page);
  const was = (await page.locator('.comment-list').boundingBox())!.height;

  // Up is taller: the bar sits above the list.
  await drag(page, '.split-horizontal', 0, -90);
  const taller = (await page.locator('.comment-list').boundingBox())!.height;
  expect(taller).toBeGreaterThan(was + 60);

  await drag(page, '.split-horizontal', 0, 2000);
  const short = (await page.locator('.comment-list').boundingBox())!.height;
  expect(short).toBeLessThan(40);
  await expect(page.locator('.list-head')).toBeVisible();
});

test('the arrow keys move a bar too', async ({ page }) => {
  const was = (await page.locator('.side').boundingBox())!.width;

  await page.locator('.split-vertical').focus();
  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.press('Shift+ArrowRight');

  expect((await page.locator('.side').boundingBox())!.width).toBe(was + 80);
});

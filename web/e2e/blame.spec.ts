// Who wrote the code a change works on: the blame of the left side, the way
// Gerrit shows it.

import { expect, test, type Page } from '@playwright/test';
import { openChange, openFile, useSplit, useUnified } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeAll(async () => {
  server = await start();
});
test.afterAll(() => server?.stop());

test.beforeEach(async ({ page }) => {
  await page.goto(server.url);
  // net.blk, as the merge under the series left it: the merge wrote line 3,
  // and the first commit of the history wrote the rest.
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
});

/// The cell of the blame on the row of a line of the left side.
function blameOf(page: Page, line: number) {
  return page
    .locator('tr', { has: page.locator(`td.gutter[data-column="old"]:text-is("${line}")`) })
    .locator('td.blame');
}

test('b shows who wrote each line of the left side, and b hides it', async ({ page }) => {
  await useSplit(page);
  await expect(page.locator('td.blame')).toHaveCount(0);

  await page.keyboard.press('b');

  await expect(blameOf(page, 1)).toHaveText('2026-01-01 Test');
  await expect(blameOf(page, 3)).toHaveText('2026-01-01 Test');
  await expect(page.getByRole('button', { name: 'Blame' })).toHaveAttribute('aria-pressed', 'true');

  await page.keyboard.press('b');
  await expect(page.locator('td.blame')).toHaveCount(0);
});

test('a slow blame says that it loads', async ({ page }) => {
  // The whole blame never comes until the test lets it.
  const whole = /\/blame\?(?!.*part=recent)/;
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route(whole, async (route) => {
    await held;
    await route.continue();
  });

  await page.keyboard.press('b');

  await expect(blameOf(page, 3)).toHaveText('2026-01-01 Test');
  await expect(page.getByRole('button', { name: 'Loading blame…' })).toBeVisible();
  await expect(blameOf(page, 1)).toHaveText('…');

  release();
  await expect(blameOf(page, 1)).toHaveText('2026-01-01 Test');
  await expect(page.getByRole('button', { name: 'Blame', exact: true })).toBeVisible();
});

test('the unified view puts the blame on each line of the left side', async ({ page }) => {
  await useUnified(page);
  await page.getByRole('button', { name: 'Blame' }).click();

  const old = page.locator('td.gutter[data-column="old"]').filter({ hasText: /\d/ });
  await expect(page.locator('td.blame .blame-label')).toHaveCount(await old.count());
});

test('the tooltip shows the commit the way Gerrit does', async ({ page }) => {
  await page.keyboard.press('b');
  await blameOf(page, 1).hover();

  const tip = page.getByRole('tooltip');
  await expect(tip).toContainText(/Commit [0-9a-f]{40}/);
  await expect(tip).toContainText('Author: Test Author <author@example.com>');
  await expect(tip).toContainText('Date: 2026-01-01');
  await expect(tip).toContainText('base: start the tree');
  // That commit is not in the series, so a click does nothing.
  await expect(tip).not.toContainText('Click');
  await expect(blameOf(page, 1).getByRole('link')).toHaveCount(0);

  await page.locator('.file-bar h2').hover();
  await expect(tip).toHaveCount(0);
});

test('the tooltip goes with the column', async ({ page }) => {
  await page.keyboard.press('b');
  await blameOf(page, 1).hover();
  await expect(page.getByRole('tooltip')).toBeVisible();

  // The cell goes away under the pointer, so it never hears it leave.
  await page.keyboard.press('b');
  await expect(page.getByRole('tooltip')).toHaveCount(0);
  await page.locator('.file-bar h2').hover();
  await page.keyboard.press('b');
  await expect(blameOf(page, 1)).toHaveText('2026-01-01 Test');
  await expect(page.getByRole('tooltip')).toHaveCount(0);
});

test('a line written by a change of the series opens that change there', async ({ page }) => {
  await page.keyboard.press('b');
  await blameOf(page, 3).hover();
  await expect(page.getByRole('tooltip')).toContainText('Click to open this change');

  await blameOf(page, 3).getByRole('link').click();

  await expect(page.locator('.change-bar')).toContainText('Merge branch side into main');
  await expect(page.locator('.file-bar h2')).toContainText('net.blk');
  // The keyboard lands on the line the merge wrote.
  await expect(page.locator('tr.row-cursor')).toContainText('recvmsg');
});

test('a line of the series opens in a new tab too', async ({ page, context }) => {
  await page.keyboard.press('b');
  const link = blameOf(page, 3).getByRole('link');
  await expect(link).toHaveAttribute('href', /#change=.+&file=src\/net\.blk$/);
  await expect(link).not.toHaveAttribute('target');

  const opened = context.waitForEvent('page');
  await link.click({ modifiers: ['ControlOrMeta'] });
  const tab = await opened;

  await expect(tab.locator('.change-bar')).toContainText('Merge branch side into main');
  await expect(tab.locator('.file-bar h2')).toContainText('net.blk');
  // The page of the click stays where it was.
  await expect(page.locator('.change-bar')).toContainText('net: retry the read');
});

test('a line that opens nothing is quieter than a link', async ({ page }) => {
  await page.keyboard.press('b');
  await expect(blameOf(page, 3).getByRole('link')).toBeVisible();

  const ink = (line: number) =>
    blameOf(page, line)
      .locator('.blame-label')
      .evaluate((node) => getComputedStyle(node).color);

  expect(await ink(1)).not.toBe(await ink(3));
});

test('the commit message has no blame', async ({ page }) => {
  await page.keyboard.press('b');
  await openFile(page, 'Commit message');

  await expect(page.getByRole('button', { name: 'Blame' })).toHaveCount(0);
  await expect(page.locator('td.blame')).toHaveCount(0);
});

// The remarks already posted on Gerrit, read only.
//
// A server for each test: one of them fetches a version, and a fetched
// version anchors remarks the next test wants to see unplaced.

import { expect, test } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start({ gerrit: true });
  await page.goto(server.url);
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
});

test.afterEach(() => server?.stop());

test('a thread on a line is shown, read only', async ({ page }) => {
  // Two remarks on one line are a thread, in the order the server gave them.
  const thread = page.locator('tr.talk .posted-box');
  await expect(thread).toHaveCount(2);
  await expect(thread.nth(0)).toContainText('Jane Reviewer');
  await expect(thread.nth(0)).toContainText('It still never stops.');
  await expect(thread.nth(1)).toContainText('A Developer');
  await expect(thread.nth(1)).toContainText('A cap is coming');

  // qreview writes nothing to the server, so there is nothing to press.
  await expect(thread.nth(0)).toContainText('Gerrit');
  await expect(thread.nth(0).getByRole('button')).toHaveCount(0);
});

test('a remark about the whole file stands above the diff', async ({ page }) => {
  const band = page.locator('.above-diff .posted-box').filter({ hasText: 'buildbot' });

  await expect(band).toHaveCount(1);
  await expect(band).toContainText('This file has no test.');
  // It was posted about the file, not about a line, so it is not stranded.
  await expect(band).not.toHaveClass(/talk-stranded/);
});

test('a remark on a version that is not here stands at the top of its file', async ({ page }) => {
  // The line is gone, so the file it was posted on is the nearest true
  // place. It is a card like any other, not a line in a list of its own.
  const stranded = page.locator('.above-diff .posted-box.talk-stranded');

  await expect(stranded).toContainText('Where does this loop stop?');
  await expect(stranded).toContainText('Jane Reviewer');
  await expect(stranded).toContainText('no line here');
  await expect(stranded).toContainText('src/net.blk:3');
  await expect(stranded).toContainText('patch set 1');
});

test('a remark of the session is not confused with one from the server', async ({ page }) => {
  await page.locator('td.gutter-comment[data-column="new"]').first().click();
  await page.getByRole('textbox').first().fill('Mine, not theirs.');
  await page.getByRole('button', { name: 'Save' }).click();

  const mine = page.locator('.talk-box:not(.posted-box)', { hasText: 'Mine, not theirs.' });
  await expect(mine).toHaveCount(1);
  // Only the remark of this session can be edited.
  await expect(mine.getByRole('button', { name: 'Edit' })).toBeVisible();
});

// The remarks already posted on Gerrit, read only.
//
// A server for each test: one of them fetches a version, and the others
// read a clone that does not hold it.

import { expect, test } from '@playwright/test';
import { openChange, openFile, useSplit } from './act.ts';
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
  // Not the ones standing before the first line: those have no line here.
  const thread = page
    .locator('tr.talk .posted-box')
    .filter({ hasText: /never stops|cap is coming/ });
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
  const band = page.locator('table.code .posted-box').filter({ hasText: 'buildbot' });

  await expect(band).toHaveCount(1);
  await expect(band).toContainText('This file has no test.');
  // It was posted about the file, not about a line, so it is not stranded.
  await expect(band).not.toHaveClass(/talk-stranded/);
});

test('a remark of a version off the screen is not shown', async ({ page }) => {
  // Posted on patch set 1, and patch set 2 is read against its parent. The
  // remark speaks of code that is not on the screen, so Gerrit hides it here
  // too.
  await expect(page.locator('tr.talk .posted-box').first()).toBeVisible();
  await expect(page.locator('.posted-box', { hasText: 'Where does this loop stop?' })).toHaveCount(
    0,
  );
});

test('a remark of the version read against stands on its line, on the left', async ({ page }) => {
  await useSplit(page);
  // Patch set 1 is on the server only. Opening it brings it here.
  await page.locator('#read-set').selectOption('1');
  await expect(page.locator('#read-set option').nth(0)).not.toContainText('not fetched', {
    timeout: 15000,
  });
  await page.locator('#read-set').selectOption('2');
  await page.locator('#base-of').selectOption('ps:1');
  await openFile(page, 'net.blk');

  const talk = page.locator('tr.talk', { hasText: 'Where does this loop stop?' });
  await expect(talk).toHaveCount(1);
  await expect(talk.locator('td').nth(0)).toContainText('Where does this loop stop?');
  await expect(talk.locator('td').nth(1)).not.toContainText('Where does this loop stop?');
  // Line 3 is on both sides, and patch set 2 has its own thread there.
  const remark = talk.locator('.posted-box', { hasText: 'Where does this loop stop?' });
  await expect(remark).not.toHaveClass(/talk-stranded/);
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

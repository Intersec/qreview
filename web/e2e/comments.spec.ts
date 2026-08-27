// What the session holds: the counts, and the list of them.

import { expect, test, type Page } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

// A server for each test. Every test writes a comment, and a count that one
// test leaves behind is a count the next one reads.
let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start();
  await page.goto(server.url);
});

test.afterEach(() => server?.stop());

/// Write one remark on a line of the open file.
async function remark(page: Page, nth: number, body: string) {
  await page.locator('td.gutter-comment[data-column="new"]').nth(nth).click();
  const box = page.getByRole('textbox').first();
  await box.waitFor();
  await box.fill(body);
  await page.getByRole('button', { name: 'Save' }).click();
  // Markdown makes a paragraph of each line, so wait for the first one.
  await expect(page.getByText(body.split('\n')[0]).first()).toBeVisible();
}

test('the counts say what the change and the session hold', async ({ page }) => {
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');

  // Nothing yet, and nothing to copy.
  await expect(page.getByRole('button', { name: /^Copy this change/ })).toBeDisabled();
  await expect(page.locator('.comment-list')).toHaveCount(0);

  await remark(page, 0, 'One remark.');

  await expect(page.getByRole('button', { name: 'Copy this change · 1' })).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Copy the series · 1' })).toBeEnabled();
  // On the change, and on the file it sits in.
  await expect(page.locator('.change-row', { hasText: 'net: retry the read' })).toContainText(
    '1 ✎',
  );
  await expect(page.locator('.file-row', { hasText: 'net.blk' })).toContainText('1 ✎');
  await expect(page.locator('.file-row', { hasText: 'Commit message' })).not.toContainText('✎');
});

test('the list names the place and the first line of each remark', async ({ page }) => {
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  await remark(page, 0, 'The loop never ends.\nA second line nobody reads here.');

  const list = page.locator('.comment-list');
  await expect(list).toContainText('Comments · 1');
  await expect(list.locator('.list-row')).toHaveCount(1);
  await expect(list.locator('.list-place')).toHaveText('net.blk:1');
  await expect(list.locator('.list-gist')).toHaveText('The loop never ends.');
});

test('the change being read comes first, and the rest in the order of the export', async ({
  page,
}) => {
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  await remark(page, 0, 'About net.');

  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await remark(page, 0, 'About the long file.');

  // `net` is the older commit, so the export would name it first. The list
  // puts the change on the screen ahead of it.
  const groups = page.locator('.comment-list .list-change');
  await expect(groups.first()).toContainText('long: touch two places');
  await expect(groups.nth(1)).toContainText('net: retry the read');
  await expect(page.locator('.comment-list .list-row').first()).toContainText(
    'About the long file.',
  );
});

test('a row of the list opens the place it names', async ({ page }) => {
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  await remark(page, 2, 'Down here.');

  // Away from it, then back through the list.
  await openChange(page, /docs: rename the document/);
  await expect(page.locator('.file-bar h2')).toContainText('Commit message');

  await page.locator('.comment-list .list-row', { hasText: 'Down here.' }).click();

  await expect(page.locator('.file-bar h2')).toContainText('net.blk');
  await expect(page.locator('tr.row-cursor')).toHaveCount(1);
});

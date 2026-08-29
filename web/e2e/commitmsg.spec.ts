// The commit message, reviewed like a file of the change.

import { expect, test } from '@playwright/test';
import { card, openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeAll(async () => {
  server = await start({ prev: true });
});
test.afterAll(() => server?.stop());

test.beforeEach(async ({ page }) => {
  await page.goto(server.url);
});

test('the message is the first file of the change', async ({ page }) => {
  await openChange(page, /docs: rename the document/);

  const rows = page.locator('.file-row');
  await expect(rows.first()).toContainText('Commit message');
  // Opening a change lands on the message, the way Gerrit does.
  await expect(page.locator('.file-bar h2')).toContainText('Commit message');
});

test('the whole message is new against the parent', async ({ page }) => {
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'Commit message');

  const added = page.locator('td.code-cell.row-add');
  await expect(added.first()).toContainText('docs: rename the document');
  await expect(page.locator('td.code-cell.row-remove')).toHaveCount(0);
  await expect(page.locator('.file-bar')).toContainText('added');
});

test('a comment on the message comes back after a reload', async ({ page }) => {
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'Commit message');

  await page.locator('td.gutter-comment').first().click();
  await page.getByRole('textbox').first().fill('The subject says what, not why.');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(card(page, 'The subject says what, not why.')).toBeVisible();

  await page.reload();
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'Commit message');
  await expect(card(page, 'The subject says what, not why.')).toBeVisible();
});

test('a message that did not change between two versions is still readable', async ({ page }) => {
  await openChange(page, /net: retry the read/);
  await page.locator('#base-of').selectOption('ps:1');
  await openFile(page, 'Commit message');

  // The amend touched the code alone, so the message reads as plain lines.
  await expect(page.locator('td.code-cell').first()).toContainText('net: retry the read');
  await expect(page.locator('td.code-cell.row-add')).toHaveCount(0);
  await expect(page.locator('td.code-cell.row-remove')).toHaveCount(0);
});

test('a remark about the change goes on the commit message', async ({ page }) => {
  // Gerrit has no useful place for a remark about the whole change, so the
  // convention is to write it on the message. The button says so there.
  await openChange(page, /net: retry the read/);
  await openFile(page, 'Commit message');
  await page.getByRole('button', { name: 'Comment on the change' }).click();
  await page.getByRole('textbox').first().fill('This series needs a test plan.');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(card(page, 'This series needs a test plan.')).toBeVisible();

  // On any other file the same button is about that file alone.
  await openFile(page, 'net.blk');
  await expect(page.getByRole('button', { name: 'Comment on the file' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Comment on the change' })).toHaveCount(0);
});

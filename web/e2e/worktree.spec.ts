// The work that is not committed, read as a change of the series.

import { expect, test } from '@playwright/test';
import { card, openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start({ dirty: true });
  await page.goto(server.url);
});

test.afterEach(() => server?.stop());

test('it stands at the top of the series, without a sha', async ({ page }) => {
  const first = page.locator('.side li').first();

  await expect(first).toContainText('Uncommitted changes');
  await expect(first.locator('.tag-worktree')).toHaveText('not committed');
  // The synthetic sha names nothing a reader can look up, so it is not shown.
  await expect(first.locator('code')).toHaveCount(0);
});

test('it shows the tracked changes, and nothing else', async ({ page }) => {
  await openChange(page, /Uncommitted changes/);

  // A file changed and not staged, and a file staged. Both are tracked now.
  const rows = page.locator('.file-row');
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0)).toContainText('added.py');
  await expect(rows.nth(1)).toContainText('long.c');

  // No commit message: the one on the synthetic commit is a label the tool
  // wrote. And no file nobody has added to git.
  await expect(page.locator('.file-row', { hasText: 'Commit message' })).toHaveCount(0);
  await expect(page.locator('.file-row', { hasText: 'nobody-added-me' })).toHaveCount(0);
});

test('the diff is what the last commit does not carry', async ({ page }) => {
  await openChange(page, /Uncommitted changes/);
  await openFile(page, 'long.c');

  const added = page.locator('td.code-cell.row-add');
  await expect(added.filter({ hasText: 'LINE SEVEN, NOT COMMITTED' })).toHaveCount(1);

  // Line 3 belongs to the commit under this one. It is context here, never
  // a change: what is committed is not what is being reviewed.
  await expect(added.filter({ hasText: 'LINE THREE' })).toHaveCount(0);
  await expect(
    page.locator('td.code-cell.row-context').filter({ hasText: 'LINE THREE' }),
  ).toHaveCount(1);
});

test('a remark on it is written and kept like any other', async ({ page }) => {
  await openChange(page, /Uncommitted changes/);
  await openFile(page, 'long.c');

  await page.locator('td.gutter-comment[data-column="new"]').first().click();
  await page.getByRole('textbox').first().fill('Shouting is not a fix.');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(card(page, 'Shouting is not a fix.')).toBeVisible();

  await page.reload();
  await openChange(page, /Uncommitted changes/);
  await openFile(page, 'long.c');
  await expect(card(page, 'Shouting is not a fix.')).toBeVisible();
});

test('the patch set bar stays away: there is one version', async ({ page }) => {
  await openChange(page, /Uncommitted changes/);

  await expect(page.locator('.patch-bar')).toHaveCount(0);
});

test('a clean tree adds no change', async ({ page }) => {
  server.stop();
  server = await start();
  await page.goto(server.url);

  await expect(page.locator('.side li').first()).toContainText('docs: rename the document');
  await expect(page.locator('.tag-worktree')).toHaveCount(0);
});

// Two versions of a change on two bases, read one against the other.

import { expect, test } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { build, rebaseNewest } from './fixture.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeAll(async () => {
  const fixture = build();
  fixture.previous = rebaseNewest(fixture);
  server = await start({ on: fixture, prev: true });
});
test.afterAll(() => server?.stop());

test.beforeEach(async ({ page }) => {
  await page.goto(server.url);
  await openChange(page, /docs: rename the document/);
  await page.locator('#base-of').selectOption('ps:1');
});

test('the lines that the rebase brought are painted apart', async ({ page }) => {
  await openFile(page, 'new-name.md');

  const brought = page.locator('td.code-cell.row-rebase');
  await expect(brought).toHaveCount(2);
  await expect(brought.first()).toContainText('# A document');
  await expect(brought.last()).toContainText('# The document');
  // The work of the change keeps the colours of a change.
  const work = page.locator('td.code-cell.row-add:not(.row-rebase)');
  await expect(work).toHaveCount(1);
  await expect(work).toContainText('And one more line, again.');
});

test('the file list leaves out what the rebase brought', async ({ page }) => {
  const doc = page.locator('.file-row', { hasText: 'new-name.md' });
  await expect(doc.locator('.added')).toHaveText('+1');
  await expect(doc.locator('.removed')).toHaveText('−1');
  // Its only difference came with the rebase.
  await expect(page.locator('.file-row', { hasText: 'huge.c' })).toHaveCount(0);

  await expect(page.locator('.rebased')).toContainText('came with the rebase');
});

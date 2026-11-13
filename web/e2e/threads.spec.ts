// The threads of Gerrit over REST, with their ids, their replies and their
// state, from a Gerrit that is a shell script. The fake `curl` answers the
// comments of the change from the file this suite writes.

import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { GERRIT_THREADS } from './fixture.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeAll(async () => {
  server = await start({ gerrit: true });
  writeFileSync(
    join(server.fixture.bin!, 'comments.json'),
    `)]}'\n${JSON.stringify(GERRIT_THREADS)}\n`,
  );
});
test.afterAll(() => server?.stop());

test.beforeEach(async ({ page }) => {
  await page.goto(server.url);
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
});

test('a thread of Gerrit holds its replies, and says it waits', async ({ page }) => {
  const thread = page.locator('.posted-box', { hasText: 'Where does this loop stop?' });

  await expect(thread).toContainText('unresolved');
  await expect(thread.locator('.talk-reply')).toHaveText(/At the first error\./);
  // The reply stands in its thread, not in a card of its own.
  await expect(page.locator('.posted-box', { hasText: 'At the first error.' })).toHaveCount(1);
});

test('a resolved thread folds, and opens on a click', async ({ page }) => {
  const thread = page.locator('.posted-box', { hasText: 'Rename this function.' });

  await expect(thread).toHaveClass(/talk-folded/);
  await thread.getByRole('button', { name: /Jane Reviewer/ }).click();
  await expect(thread).not.toHaveClass(/talk-folded/);
});

test('a remark on the parent stands under the line it removed', async ({ page }) => {
  const removed = page.locator('tr', {
    has: page.locator('td.gutter[data-column="old"]:text-is("3")'),
  });
  const under = page.locator('tr:has(td.gutter[data-column="old"]:text-is("3")) + tr.talk');

  await expect(removed).toContainText('return recvmsg(fd);');
  await expect(under).toContainText('This return was the only exit.');
});

test('the ssh answer stands back when REST answers', async ({ page }) => {
  // The ssh answer holds other remarks on this patch set: REST replaces them.
  await expect(page.getByText('It still never stops.')).toHaveCount(0);
});

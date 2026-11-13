// The threads of Gerrit over REST, with their ids, their replies and their
// state, from a Gerrit that is a shell script. The fake `curl` answers the
// comments of the change from the file this suite writes.

import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

const comments = {
  'src/net.blk': [
    {
      id: 'ask1',
      patch_set: 2,
      line: 3,
      author: { name: 'Jane Reviewer' },
      message: 'Where does this loop stop?',
      unresolved: true,
      updated: '2026-11-01 10:00:00.000000000',
    },
    {
      id: 'ask2',
      in_reply_to: 'ask1',
      patch_set: 2,
      line: 3,
      author: { name: 'Test Author' },
      message: 'At the first error.',
      unresolved: true,
      updated: '2026-11-01 11:00:00.000000000',
    },
    {
      id: 'ok1',
      patch_set: 2,
      line: 1,
      author: { name: 'Jane Reviewer' },
      message: 'Rename this function.',
      unresolved: false,
      updated: '2026-11-01 09:00:00.000000000',
    },
    {
      id: 'old1',
      patch_set: 2,
      side: 'PARENT',
      line: 3,
      author: { name: 'Jane Reviewer' },
      message: 'This return was the only exit.',
      unresolved: true,
      updated: '2026-11-01 09:30:00.000000000',
    },
  ],
};

test.beforeAll(async () => {
  server = await start({ gerrit: true });
  writeFileSync(join(server.fixture.bin!, 'comments.json'), `)]}'\n${JSON.stringify(comments)}\n`);
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

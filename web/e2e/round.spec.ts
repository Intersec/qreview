// The second round: read, have the work corrected, read again.
//
// The remarks of the first round are still there, keyed by the Change-Id.
// What qreview owes the reader is the version they reviewed, found on its
// own, and a word on which remarks the correction has answered.

import { execFileSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import { card, openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.afterEach(() => server?.stop());

/// Write a remark on the line the gutter names.
async function remark(page: Page, line: string, body: string) {
  const row = page.locator('tr', {
    has: page.locator(`td.gutter[data-column="new"]:text-is("${line}")`),
  });
  await row.locator('td.gutter-comment[data-column="new"]').click();
  await page.getByRole('textbox').first().fill(body);
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(card(page, body)).toBeVisible();
}

test('the version that was reviewed comes back, and what is answered is said', async ({ page }) => {
  server = await start();
  await page.goto(server.url);
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');

  await remark(page, '1', 'The title says nothing.');
  await remark(page, '4', 'This line says nothing.');

  // An agent answers the second remark and leaves the first alone.
  const repo = server.fixture.repo;
  server.kill();
  writeFileSync(
    join(repo, 'docs', 'new-name.md'),
    '# A document\n\nIt has words in it.\nIt carries the answer now.\n',
  );
  execFileSync('git', ['add', '-A'], { cwd: repo });
  execFileSync('git', ['commit', '--amend', '--no-edit'], { cwd: repo });

  // Round two, on the same repository and the same store, with no --prev.
  server = await start({ on: server.fixture });
  await page.goto(server.url);
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');

  // The version reviewed last time is offered, without being named.
  await expect(page.locator('#read-set option')).toHaveCount(2);

  // The remark whose line is gone stands at the top of the file it was
  // written on, marked, rather than in a list of its own.
  const done = page.locator('.above-diff .talk-stranded');
  await expect(done).toHaveCount(1);
  await expect(done).toContainText('answered');
  await expect(done).toContainText('docs/new-name.md:4');
  await expect(done).toContainText('This line says nothing.');
  await expect(page.locator('.stranded-head')).toContainText('One remark below was answered');

  // The one still standing is on its line, where it always was.
  await expect(card(page, 'The title says nothing.')).toBeVisible();
});

test('the answered remarks are dropped in one action', async ({ page }) => {
  server = await start();
  await page.goto(server.url);
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');
  await remark(page, '4', 'This line says nothing.');

  const repo = server.fixture.repo;
  server.kill();
  writeFileSync(
    join(repo, 'docs', 'new-name.md'),
    '# A document\n\nIt has words in it.\nAnswered.\n',
  );
  execFileSync('git', ['add', '-A'], { cwd: repo });
  execFileSync('git', ['commit', '--amend', '--no-edit'], { cwd: repo });

  server = await start({ on: server.fixture });
  await page.goto(server.url);
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');

  await page.getByRole('button', { name: 'Delete every answered remark' }).click();
  await expect(page.locator('.talk-stranded')).toHaveCount(0);
});

test('an older version is read, not written on', async ({ page }) => {
  server = await start();
  await page.goto(server.url);
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');
  await remark(page, '1', 'The title says nothing.');

  const repo = server.fixture.repo;
  server.kill();
  writeFileSync(
    join(repo, 'docs', 'new-name.md'),
    '# A document\n\nIt has words in it.\nSomething else.\n',
  );
  execFileSync('git', ['add', '-A'], { cwd: repo });
  execFileSync('git', ['commit', '--amend', '--no-edit'], { cwd: repo });

  server = await start({ on: server.fixture });
  await page.goto(server.url);
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');

  // On the newest version the remark can be corrected.
  await expect(
    card(page, 'The title says nothing.').getByRole('button', { name: 'Edit' }),
  ).toBeVisible();

  // On the version before it, the remark is history.
  await page.locator('#read-set').selectOption('1');
  await expect(card(page, 'The title says nothing.')).toBeVisible();
  await expect(
    card(page, 'The title says nothing.').getByRole('button', { name: 'Edit' }),
  ).toHaveCount(0);

  // And nothing invites a new one there. A remark written on an older
  // version would be anchored on the newest, at the line numbers of this
  // one, which is a remark on a line nobody chose.
  await expect(page.locator('.file-bar')).toContainText('reading an older version');
  await expect(page.locator('td.gutter-comment')).toHaveCount(0);
  await page.keyboard.press('j');
  await page.keyboard.press('c');
  await expect(page.getByRole('textbox')).toHaveCount(0);
});

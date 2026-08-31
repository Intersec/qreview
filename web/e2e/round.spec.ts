// The second round: read, have the work corrected, read again.
//
// The remarks of the first round are still there, keyed by the Change-Id.
// What qreview owes the reader is the version they reviewed, found on its
// own, and a word on which of the remarks belong to the round that is over.

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

/// Review a change, then have the work corrected and the commit amended.
///
/// The remark is written on line 4, which the correction rewrites, so the
/// version after it has no line for it.
async function reviewThenCorrect(page: import('@playwright/test').Page, body: string) {
  server = await start();
  await page.goto(server.url);
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');
  await remark(page, '4', body);

  const repo = server.fixture.repo;
  server.kill();
  writeFileSync(
    join(repo, 'docs', 'new-name.md'),
    '# A document\n\nIt has words in it.\nCorrected.\n',
  );
  execFileSync('git', ['add', '-A'], { cwd: repo });
  execFileSync('git', ['commit', '--amend', '--no-edit'], { cwd: repo });

  server = await start({ on: server.fixture });
  await page.goto(server.url);
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');
}

test('the version that was reviewed comes back as a patch set', async ({ page }) => {
  await reviewThenCorrect(page, 'This line says nothing.');

  // The version reviewed last time is offered, without being named.
  await expect(page.locator('#read-set option')).toHaveCount(2);
  await expect(page.locator('#read-set')).toHaveValue('2');

  // Its remarks are not on the newest version: they speak of code that is
  // not on the screen. The pane still lists them, under the version they
  // were written on.
  await expect(card(page, 'This line says nothing.')).toHaveCount(0);
  await expect(page.locator('.comment-list .list-previous')).toContainText('previous');
  await expect(page.locator('.comment-list .list-row.is-previous')).toContainText(
    'This line says nothing.',
  );
});

test('a remark shows again on the version it was written on', async ({ page }) => {
  await reviewThenCorrect(page, 'This line says nothing.');
  await page.locator('#read-set').selectOption('1');

  // On its own version the line is there, so the remark is on it, and it
  // says which patch set and which sha it belongs to.
  const shown = card(page, 'This line says nothing.');
  await expect(shown).toBeVisible();
  await expect(shown).toContainText('previous');
  await expect(shown).toContainText(/patch set 1 · [0-9a-f]{8}/);
});

test('a previous remark is read, never edited and never deleted', async ({ page }) => {
  await reviewThenCorrect(page, 'This line says nothing.');
  await page.locator('#read-set').selectOption('1');

  // The round it belongs to is over. It is a record of that round, and a
  // record that can be rewritten is not one.
  const shown = card(page, 'This line says nothing.');
  await expect(shown).toBeVisible();
  await expect(shown.getByRole('button')).toHaveCount(0);
});

test('the pane counts the current remarks and lists the previous ones apart', async ({ page }) => {
  server = await start();
  await page.goto(server.url);
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');
  await remark(page, '1', 'Written in the first round.');

  const repo = server.fixture.repo;
  server.kill();
  writeFileSync(
    join(repo, 'docs', 'new-name.md'),
    '# A document\n\nIt has words in it.\nAnd one more line.\nAnd another.\n',
  );
  execFileSync('git', ['add', '-A'], { cwd: repo });
  execFileSync('git', ['commit', '--amend', '--no-edit'], { cwd: repo });

  server = await start({ on: server.fixture });
  await page.goto(server.url);
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');

  // The round before is not counted, and it is not hidden either.
  const pane = page.locator('.comment-list');
  await expect(pane.locator('.list-head')).toContainText('Comments · 0');
  // The group is headed by the version its remarks were written on.
  const head = pane.locator('.list-previous');
  await expect(head).toHaveCount(1);
  await expect(head).toContainText('previous');
  await expect(head).toContainText('docs: rename the document');
  await expect(head.locator('code')).toHaveText(/^[0-9a-f]{8}$/);
  await expect(pane.locator('.list-row.is-previous')).toContainText('Written in the first round.');

  // And a remark of this round is counted, above the ones from before.
  await remark(page, '5', 'Written in the second round.');
  await expect(pane.locator('.list-head')).toContainText('Comments · 1');
  await expect(pane.locator('.list-row:not(.is-previous)')).toHaveCount(1);
});

test('an older version is read, not written on', async ({ page }) => {
  await reviewThenCorrect(page, 'This line says nothing.');
  await page.locator('#read-set').selectOption('1');

  // Nothing invites a remark there. One written on an older version would be
  // anchored on the newest, at the line numbers of this one, which is a
  // remark on a line nobody chose.
  await expect(page.locator('.file-bar')).toContainText('reading an older version');
  await expect(page.locator('td.gutter-comment')).toHaveCount(0);
  await page.keyboard.press('j');
  await page.keyboard.press('c');
  await expect(page.getByRole('textbox')).toHaveCount(0);
});

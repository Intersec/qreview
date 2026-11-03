// What the session holds: the counts, and the list of them.

import { expect, test, type Page } from '@playwright/test';
import { card, openChange, openFile, useSplit, useUnified } from './act.ts';
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
  await page.getByRole('button', { name: 'Save', exact: true }).click();
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

/// Write a remark on lines of `long.c` the way the agent does, past the page,
/// and load the page again so it reads it.
///
/// The diff of `long.c` leaves lines 14 to 39 in a gap.
async function remarkOnLong(page: Page, startLine: number, endLine: number) {
  const api = (path: string) => new URL(path, server.url).href;
  const session = (await (await page.request.get(api('/api/session'))).json()) as {
    series: { changes: { key: string; subject: string }[] };
  };
  const long = session.series.changes.find((c) => c.subject.startsWith('long:'));
  const body = `About lines ${startLine} to ${endLine}.`;
  const answer = await page.request.post(api(`/api/changes/${long?.key}/comments`), {
    data: { scope: 'line', file: 'src/long.c', side: 'new', startLine, endLine, body },
  });
  expect(answer.ok()).toBe(true);
  await page.reload();

  return body;
}

for (const view of ['unified', 'side by side'] as const) {
  test(`a row of the list goes to a line opened by hand, ${view}`, async ({ page }) => {
    const body = await remarkOnLong(page, 20, 20);
    await openChange(page, /long: touch two places/);
    await openFile(page, 'long.c');
    await (view === 'unified' ? useUnified(page) : useSplit(page));

    // The whole gap, so line 20 lands above where the bar stood.
    await page.getByRole('button', { name: '+26 common lines' }).click();
    await expect(card(page, body)).toBeVisible();

    await page.locator('.comment-list .list-row', { hasText: body }).click();

    // The keyboard landed on the line, but the row drew no cursor, so
    // nothing scrolled to it.
    await expect(page.locator('tr.row-cursor')).toHaveCount(1);
    await expect(page.locator('tr.row-cursor')).toContainText('line 20');
  });
}

for (const view of ['unified', 'side by side'] as const) {
  test(`a row of the list opens the folded lines of its thread, ${view}`, async ({ page }) => {
    const body = await remarkOnLong(page, 20, 22);
    await openChange(page, /long: touch two places/);
    await openFile(page, 'long.c');
    await (view === 'unified' ? useUnified(page) : useSplit(page));
    await expect(page.getByRole('button', { name: '+26 common lines' })).toBeVisible();

    await page.locator('.comment-list .list-row', { hasText: body }).click();

    await expect(card(page, body)).toBeVisible();
    await expect(page.locator('tr.row-cursor')).toHaveCount(1);
    await expect(page.locator('tr.row-cursor')).toContainText('line 20');
    // From the hunk above, which is nearer, through three lines past the
    // thread. The rest of the gap stays folded.
    await expect(page.getByRole('button', { name: '+14 common lines' })).toBeVisible();
  });
}

test('a thread near the hunk below opens from that hunk', async ({ page }) => {
  const body = await remarkOnLong(page, 36, 37);
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');

  await page.locator('.comment-list .list-row', { hasText: body }).click();

  await expect(card(page, body)).toBeVisible();
  await expect(page.locator('tr.row-cursor')).toContainText('line 36');
  // Three lines above the thread, then down to the hunk.
  await expect(page.getByRole('button', { name: '+19 common lines' })).toBeVisible();
});

for (const view of ['unified', 'side by side'] as const) {
  test(`N and P walk the threads of the file, folded ones too, ${view}`, async ({ page }) => {
    const folded = await remarkOnLong(page, 20, 22);
    const shown = await remarkOnLong(page, 50, 50);
    await openChange(page, /long: touch two places/);
    await openFile(page, 'long.c');
    await (view === 'unified' ? useUnified(page) : useSplit(page));
    await expect(card(page, shown)).toBeVisible();
    await expect(card(page, folded)).toBeHidden();

    // The keyboard lands on the last line of the thread, where its card
    // stands, and the folded lines open.
    await page.keyboard.press('Shift+N');
    await expect(page.locator('tr.row-cursor')).toContainText('line 22');
    await expect(card(page, folded)).toBeVisible();

    await page.keyboard.press('Shift+N');
    await expect(page.locator('tr.row-cursor')).toContainText('LINE FIFTY');

    await page.keyboard.press('Shift+P');
    await expect(page.locator('tr.row-cursor')).toContainText('line 22');
  });
}

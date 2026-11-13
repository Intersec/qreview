// Replies to a thread of Gerrit: here only, or staged and sent in the
// thread. A server for each test, as for Send: a reply sent by one test is
// a reply the next one would read.

import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { GERRIT_THREADS } from './fixture.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start({ gerrit: true });
  writeFileSync(
    join(server.fixture.bin!, 'comments.json'),
    `)]}'\n${JSON.stringify(GERRIT_THREADS)}\n`,
  );
  await page.goto(server.url);
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
});

test.afterEach(() => server?.stop());

function sent(): string {
  try {
    return readFileSync(join(server.fixture.bin!, 'curl.log'), 'utf8');
  } catch {
    return '';
  }
}

const thread = (page: import('@playwright/test').Page) =>
  page.locator('.posted-box', { hasText: 'Where does this loop stop?' });

test('a plain reply stays here, and nothing leaves', async ({ page }) => {
  await thread(page).getByRole('button', { name: 'Reply' }).click();
  await thread(page).getByRole('textbox').fill('Is that true? Check the loop.');
  await thread(page).getByRole('button', { name: 'Save', exact: true }).click();

  await expect(thread(page).locator('.talk-reply').last()).toContainText(
    'Is that true? Check the loop.',
  );
  await expect(page.getByRole('button', { name: /^Send to Gerrit/ })).toHaveCount(0);
});

test('a reply For Gerrit goes out in the thread, and resolves it', async ({ page }) => {
  await thread(page).getByRole('button', { name: 'Reply' }).click();
  await thread(page).getByRole('textbox').fill('It now stops after three tries.');
  await thread(page).getByLabel('For Gerrit').check();
  await thread(page).getByLabel('Resolve').check();
  await thread(page).getByRole('button', { name: 'Save', exact: true }).click();

  // Resolved here before Gerrit knows: the thread folds.
  await expect(thread(page)).toHaveClass(/talk-folded/);

  await page.getByRole('button', { name: 'Send to Gerrit · 1' }).click();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('dialog')).toHaveCount(0);

  const given = sent();
  expect(given).toMatch(/revisions\/[0-9a-f]{40}\/drafts/);
  expect(given).toContain('\\"in_reply_to\\":\\"ask2\\"');
  expect(given).toContain('\\"unresolved\\":false');
  expect(given).toContain('It now stops after three tries.');
});

test('the Done box stages a reply Done, and takes it back', async ({ page }) => {
  await thread(page).getByLabel('Done').check();

  await expect(thread(page)).toHaveClass(/talk-folded/);
  await expect(page.getByRole('button', { name: 'Send to Gerrit · 1' })).toBeVisible();

  await thread(page)
    .getByRole('button', { name: /Jane Reviewer/ })
    .click();
  await thread(page).getByLabel('Done').uncheck();
  await expect(thread(page)).not.toHaveClass(/talk-done/);
  await expect(page.getByRole('button', { name: /^Send to Gerrit/ })).toHaveCount(0);
});

test('the agent answers a thread of Gerrit, and only the user sends', async ({ page }) => {
  // Any comment of the thread names it: here, the reply of the author.
  server.agent(['reply', 'gerrit:ask2', '--body', 'The loop has no bound.']);
  const staged = JSON.parse(
    server.agent(['stage', 'gerrit:ask2', '--resolve', '--body', 'Done']),
  ) as { parent: string; gerrit: { state: string; resolve: boolean } };

  expect(staged.parent).toBe('gerrit:ask1');
  expect(staged.gerrit).toMatchObject({ state: 'staged', resolve: true });
  await expect(thread(page)).toHaveClass(/talk-folded/);
  await expect(page.getByRole('button', { name: 'Send to Gerrit · 1' })).toBeVisible();
  expect(sent()).not.toContain('/drafts');
});

test('Ask agent hands the thread of Gerrit to the agent, reviewers named', async ({ page }) => {
  await thread(page).getByRole('button', { name: 'Ask agent' }).click();
  await expect(thread(page).getByText('Asked')).toBeVisible();

  const asked = JSON.parse(server.agent(['wait', '--after', '0', '--timeout', '5'])) as {
    events: { kind: string; id: string; thread: { name?: string; body: string }[] }[];
  };
  const ask = asked.events.find((e) => e.kind === 'ask');
  expect(ask?.id).toBe('gerrit:ask1');
  expect(ask?.thread.map((c) => c.body)).toEqual([
    'Where does this loop stop?',
    'At the first error.',
  ]);
  expect(ask?.thread[0]?.name).toBe('Jane Reviewer');
});

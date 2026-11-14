// Sending the remarks staged for Gerrit, as drafts, to a Gerrit that is a
// shell script. A fake `curl` first on the PATH keeps what it was given, so
// a test reads what left the tool, and how.

import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { card, openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

// A server for each test: a remark sent by one is a remark the next one
// would count.
let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start({ gerrit: true });
  await page.goto(server.url);
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
});

test.afterEach(() => server?.stop());

function logged(name: string): string {
  try {
    return readFileSync(join(server.fixture.bin!, name), 'utf8');
  } catch {
    return '';
  }
}

test('a staged remark becomes a draft on Gerrit, and the password stays off the command line', async ({
  page,
}) => {
  await page.locator('td.gutter-comment[data-column="new"]').first().click();
  await page.getByRole('textbox').first().fill('Please name this function after what it does.');
  await page.locator('form.talk-box').getByLabel('For Gerrit').check();
  await page.getByRole('button', { name: 'Save', exact: true }).click();

  await page.getByRole('button', { name: 'Send to Gerrit · 1' }).click();
  const dialog = page.getByRole('dialog', { name: 'Send this change to Gerrit' });
  await expect(dialog).toContainText('patch set 2');
  await expect(dialog).toContainText('Please name this function');
  await page.keyboard.press('Enter');

  await expect(dialog).toHaveCount(0);
  const sent = card(page, 'Please name this function');
  await expect(sent.locator('.talk-tag-gerrit')).toHaveText('draft on Gerrit');
  await expect(page.getByRole('button', { name: /^Send to Gerrit/ })).toHaveCount(0);

  const given = logged('curl.log');
  expect(given).toContain('request = "PUT"');
  expect(given).toMatch(
    /url = "https:\/\/review\.example\.com\/a\/changes\/myproject~12321\/revisions\/[0-9a-f]{40}\/drafts"/,
  );
  const basic = Buffer.from('reviewer:s3cret').toString('base64');
  expect(given).toContain(`header = "Authorization: Basic ${basic}"`);
  expect(given).toContain('Please name this function after what it does.');
  expect(logged('curl-args.log')).not.toContain('s3cret');
  expect(logged('curl-args.log')).not.toContain(basic);
});

test('nothing leaves before Enter', async ({ page }) => {
  await page.locator('td.gutter-comment[data-column="new"]').first().click();
  await page.getByRole('textbox').first().fill('Kept here for now.');
  await page.locator('form.talk-box').getByLabel('For Gerrit').check();
  await page.getByRole('button', { name: 'Save', exact: true }).click();

  await page.getByRole('button', { name: 'Send to Gerrit · 1' }).click();
  await page.keyboard.press('Escape');

  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(card(page, 'Kept here for now.').locator('.talk-tag-gerrit')).toHaveText(
    'for Gerrit',
  );
  expect(logged('curl.log')).not.toContain('request = "PUT"');
});

test('the agent stages the remark that the user asked for, and only the user sends it', async ({
  page,
}) => {
  await page.locator('td.gutter-comment[data-column="new"]').first().click();
  await page.getByRole('textbox').first().fill('this name is bad, word it for Bob');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(card(page, 'this name is bad')).toBeVisible();

  const open = JSON.parse(server.agent(['export', '--json'])) as unknown;
  const id = JSON.stringify(open).match(/"id":"([^"]+)"/)?.[1];
  expect(id).toBeTruthy();
  const reply = JSON.parse(
    server.agent(['stage', id!, '--body', 'Could this function be named after what it does?']),
  ) as { parent: string };
  expect(reply.parent).toBe(id);

  // One thread, staged, whose newest reply is the text for the author.
  const thread = card(page, 'this name is bad');
  await expect(thread.locator('.talk-tag-gerrit').first()).toHaveText('for Gerrit');
  await expect(
    thread
      .locator('.talk-reply', { hasText: 'Could this function be named' })
      .locator('.talk-tag-gerrit'),
  ).toHaveText('for Gerrit');
  await expect(page.getByRole('button', { name: 'Send to Gerrit · 1' })).toBeVisible();
  expect(logged('curl.log')).not.toContain('request = "PUT"');
});

test('a staged thread sends its newest reply, which the agent words', async ({ page }) => {
  const remark = JSON.parse(
    server.agent(['comment', 'src/net.blk:new:1', '--gerrit', '--body', 'word this for Bob']),
  ) as { id: string };
  const reply = JSON.parse(
    server.agent(['stage', remark.id, '--body', 'Could this loop stop after three tries?']),
  ) as { parent: string };
  expect(reply.parent).toBe(remark.id);

  await page.reload();
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  await page.getByRole('button', { name: 'Send to Gerrit · 1' }).click();
  const dialog = page.getByRole('dialog', { name: 'Send this change to Gerrit' });
  await expect(dialog).toContainText('Could this loop stop');
  await expect(dialog).not.toContainText('word this for Bob');
  await page.keyboard.press('Enter');
  await expect(dialog).toHaveCount(0);

  const given = logged('curl.log');
  expect(given).toContain('Could this loop stop after three tries?');
  expect(given).not.toContain('word this for Bob');
});

/// Write a remark on the first line, stage it, and send it.
async function sendOne(page: import('@playwright/test').Page, body: string) {
  await page.locator('td.gutter-comment[data-column="new"]').first().click();
  await page.getByRole('textbox').first().fill(body);
  await page.locator('form.talk-box').getByLabel('For Gerrit').check();
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await page.getByRole('button', { name: 'Send to Gerrit · 1' }).click();
  await page.keyboard.press('Enter');
  await expect(card(page, body).locator('.talk-tag-gerrit')).toHaveText('draft on Gerrit');
}

test('a draft deleted on Gerrit says so, and can be staged again', async ({ page }) => {
  await sendOne(page, 'Name it after what it does.');

  // The reader deletes the draft on Gerrit, then reads the repository again.
  writeFileSync(join(server.fixture.bin!, 'drafts.ids'), '');
  writeFileSync(join(server.fixture.bin!, 'comments.json'), `)]}'\n{}\n`);
  await page.getByRole('button', { name: 'Read the repository again' }).click();

  const remark = card(page, 'Name it after what it does.');
  await expect(remark.getByText('deleted on Gerrit')).toBeVisible();
  await remark.getByLabel('For Gerrit').check();
  await expect(remark.locator('.talk-tag-gerrit')).toHaveText('for Gerrit');
  await expect(page.getByRole('button', { name: 'Send to Gerrit · 1' })).toBeVisible();
});

test('a draft published on Gerrit shows as its thread there', async ({ page }) => {
  await sendOne(page, 'Name it after what it does.');

  // The reader publishes it: Gerrit now holds it as a comment, same id.
  const id = readFileSync(join(server.fixture.bin!, 'drafts.ids'), 'utf8').trim();
  const published = {
    'src/net.blk': [
      {
        id,
        patch_set: 2,
        line: 1,
        author: { name: 'Test Reviewer' },
        message: 'Name it after what it does.',
        unresolved: true,
        updated: '2026-11-13 10:00:00.000000000',
      },
    ],
  };
  writeFileSync(join(server.fixture.bin!, 'comments.json'), `)]}'\n${JSON.stringify(published)}\n`);
  writeFileSync(join(server.fixture.bin!, 'drafts.ids'), '');
  await page.getByRole('button', { name: 'Read the repository again' }).click();

  await expect(page.locator('.posted-box', { hasText: 'Name it after what it does.' })).toHaveCount(
    1,
  );
  await expect(
    page.locator('.talk-box:not(.posted-box)', { hasText: 'Name it after' }),
  ).toHaveCount(0);
});

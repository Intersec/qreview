// Sending the remarks staged for Gerrit, as drafts, to a Gerrit that is a
// shell script. A fake `curl` first on the PATH keeps what it was given, so
// a test reads what left the tool, and how.

import { readFileSync } from 'node:fs';
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
  expect(logged('curl.log')).not.toContain('/drafts');
});

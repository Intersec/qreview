// Is a newer qreview out? Asking is best effort, and silence is the answer
// whenever anything goes wrong.

import { expect, test } from '@playwright/test';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { start, type Running } from './server.ts';

let server: Running;

test.afterEach(() => server?.stop());

test('an empty address asks nobody, and says nothing', async ({ page }) => {
  // The default address is the home of the project. No test talks to it.
  server = await start({ config: { update: { url: '' } } });
  await page.goto(server.url);

  await expect(page.locator('.top-bar')).toBeVisible();
  await expect(page.locator('.newer')).toHaveCount(0);
  await expect(page.locator('.error')).toHaveCount(0);
});

test('an address that answers nothing is not an error', async ({ page }) => {
  // A port nothing listens on. curl fails, and the reader reads their diff.
  server = await start({
    config: { update: { url: 'http://127.0.0.1:1/latest' } },
  });
  await page.goto(server.url);

  await expect(page.locator('.file-bar')).toBeVisible();
  await expect(page.locator('.newer')).toHaveCount(0);
  await expect(page.locator('.error')).toHaveCount(0);
});

test('a newer release is named, and links to its page', async ({ page }) => {
  // curl reads a file as happily as a server, so the whole path is walked:
  // the child process, the answer, and the comparison of the two versions.
  const dir = mkdtempSync(join(tmpdir(), 'qreview-update-'));
  const answer = join(dir, 'latest.json');
  writeFileSync(
    answer,
    JSON.stringify({
      tag_name: 'v99.0.0',
      html_url: 'https://example.com/Intersec/qreview/releases/tag/v99.0.0',
    }),
  );

  server = await start({ config: { update: { url: `file://${answer}` } } });
  await page.goto(server.url);

  const chip = page.locator('.newer');
  await expect(chip).toHaveText('v99.0.0 available');
  await expect(chip).toHaveAttribute(
    'href',
    'https://example.com/Intersec/qreview/releases/tag/v99.0.0',
  );
});

test('the version that is running is not announced as newer', async ({ page }) => {
  const dir = mkdtempSync(join(tmpdir(), 'qreview-update-'));
  const answer = join(dir, 'latest.json');
  writeFileSync(answer, JSON.stringify({ tag_name: 'v0.0.1' }));

  server = await start({ config: { update: { url: `file://${answer}` } } });
  await page.goto(server.url);

  await expect(page.locator('.file-bar')).toBeVisible();
  await expect(page.locator('.newer')).toHaveCount(0);
});

test('the version carries the commit it was built from', async ({ page }) => {
  server = await start({ config: { update: { url: '' } } });
  await page.goto(server.url);

  // The chip shows the release. Hovering it says which commit under that
  // release is running, because one release names a hundred of them.
  const chip = page.locator('.top-bar .version');
  const shown = (await chip.textContent())!.trim();
  const title = (await chip.getAttribute('title'))!;

  expect(title).toMatch(/^qreview \d+\.\d+\.\d+ \([0-9a-f]{7,}\)$/);
  expect(title).toContain(shown);
});

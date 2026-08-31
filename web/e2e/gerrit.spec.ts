// Everything the Gerrit path does, against a Gerrit that is a shell script.
//
// No socket is opened. A fake `ssh` first on the PATH answers the query from
// a recorded file and serves the fetch from a bare repository next door, so
// the tool runs exactly the code it runs against a real server.

import { expect, test } from '@playwright/test';
import { openChange } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeAll(async () => {
  server = await start({ gerrit: true });
});
test.afterAll(() => server?.stop());

test.beforeEach(async ({ page }) => {
  await page.goto(server.url);
  await openChange(page, /net: retry the read/);
});

test('the change carries the number the server gave it', async ({ page }) => {
  const bar = page.locator('.patch-bar');

  await expect(bar).toContainText('Gerrit change');
  await expect(bar.getByRole('link', { name: '12321' })).toHaveAttribute(
    'href',
    'https://review.example.com/c/myproject/+/12321',
  );
  await expect(bar).toContainText('on main');
  await expect(bar).toContainText('NEW');
});

test('the server owns the numbering, and says what is not here yet', async ({ page }) => {
  const options = page.locator('#read-set option');

  await expect(options).toHaveCount(2);
  await expect(options.nth(0)).toContainText('Patch set 1');
  await expect(options.nth(0)).toContainText('not fetched');
  await expect(options.nth(1)).toContainText('Patch set 2');
  await expect(options.nth(1)).not.toContainText('not fetched');
});

test('opening a version that is on the server fetches it', async ({ page }) => {
  await page.locator('#read-set').selectOption('1');

  // The fetch runs, and the version stops saying it is elsewhere.
  await expect(page.locator('#read-set option').nth(0)).not.toContainText('not fetched', {
    timeout: 15000,
  });
});

test('the version being reviewed is the last one in the list', async ({ page }) => {
  // Patch set 1 is on the server and not here; patch set 2 is the commit
  // under review. The one being reviewed is what the list ends on, and what
  // the picker opens on.
  const options = page.locator('#read-set option');

  await expect(options.nth(1)).toContainText('Patch set 2');
  await expect(page.locator('#read-set')).toHaveValue('2');

  // Every version says when it was pushed or written, the server's included.
  await expect(options.nth(0)).toContainText(/\d{4}-\d{2}-\d{2}/);
  await expect(options.nth(1)).toContainText(/\d{4}-\d{2}-\d{2}/);
});

// Reading a series, and writing on it.

import { expect, test } from '@playwright/test';
import { openChange, openFile, useSplit } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeAll(async () => {
  server = await start();
});
test.afterAll(() => server?.stop());

test.beforeEach(async ({ page }) => {
  await page.goto(server.url);
});

test('the series stops at the merge and says so', async ({ page }) => {
  await expect(page.getByRole('button', { name: /net: retry the read/ })).toBeVisible();
  await expect(page.getByText('Merge branch side into main')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Load 5 older' })).toBeVisible();
});

test('a change shows its files and the first diff', async ({ page }) => {
  await page.getByRole('button', { name: /docs: rename the document/ }).click();

  await expect(page.locator('.file-row', { hasText: 'new-name.md' })).toBeVisible();
  // The header of the diff names both sides of the rename.
  await expect(page.locator('.file-bar h2')).toContainText('docs/old-name.md →');
  await expect(page.locator('.file-bar h2')).toContainText('docs/new-name.md');
});

test('a comment on a line comes back after a reload', async ({ page }) => {
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');

  await page.locator('td.gutter-comment').first().click();
  await page.getByRole('textbox').first().fill('This loop never ends.');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(page.getByText('This loop never ends.')).toBeVisible();

  await page.reload();
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  await expect(page.getByText('This loop never ends.')).toBeVisible();
});

test('the keyboard walks the files the way Gerrit does', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await expect(page.locator('.file-bar h2')).toContainText('src/added.py');

  await page.keyboard.press(']');
  await expect(page.locator('.file-bar h2')).toContainText('src/doc.h');

  await page.keyboard.press('[');
  await expect(page.locator('.file-bar h2')).toContainText('src/added.py');
});

test('the keyboard walks the lines and the hunks', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');

  await page.keyboard.press('j');
  await expect(page.locator('tr.row-cursor')).toHaveCount(1);
  const first = await page.locator('tr.row-cursor').textContent();

  await page.keyboard.press('j');
  expect(await page.locator('tr.row-cursor').textContent()).not.toBe(first);

  // The second hunk starts at line 40, far below where j has walked.
  await page.keyboard.press('n');
  await expect(page.locator('tr.row-cursor')).toContainText('line 40');
});

test('c writes on the line the keyboard is on', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');

  await page.keyboard.press('j');
  await page.keyboard.press('j');
  await page.keyboard.press('c');

  await expect(page.getByRole('textbox')).toHaveCount(1);
  await page.getByRole('textbox').fill('Written from the keyboard.');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Written from the keyboard.')).toBeVisible();
});

test('the question mark lists the keys', async ({ page }) => {
  await page.keyboard.press('?');

  const help = page.getByRole('dialog', { name: 'Keyboard shortcuts' });
  await expect(help).toBeVisible();
  await expect(help).toContainText('The next or the previous hunk');

  await page.keyboard.press('Escape');
  await expect(help).toBeHidden();
});

test('the page reports no error to the console', async ({ page }) => {
  const complaints: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error' || message.type() === 'warning') {
      complaints.push(message.text());
    }
  });
  page.on('pageerror', (error) => complaints.push(String(error)));

  await page.goto(server.url);
  await page.getByRole('button', { name: /long: touch two places/ }).click();
  await useSplit(page);

  expect(complaints).toEqual([]);
});

test('the bar says which file of the change is open', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await expect(page.locator('.change-bar')).toContainText('File 1 of 4');
  await expect(page.locator('.change-bar')).toContainText('long: touch two places');

  await page.getByRole('button', { name: 'Next' }).click();
  await expect(page.locator('.change-bar')).toContainText('File 2 of 4');

  await page.getByRole('button', { name: 'Next' }).click();
  await page.getByRole('button', { name: 'Next' }).click();
  await expect(page.locator('.change-bar')).toContainText('File 4 of 4');
  await expect(page.getByRole('button', { name: 'Next' })).toBeDisabled();
});

test('a comment sits under the side it was written on', async ({ page }) => {
  await page.getByRole('button', { name: /net: retry the read/ }).click();
  await useSplit(page);

  await page.locator('td.gutter-comment').nth(2).click();
  await page.getByRole('textbox').first().fill('On the new side.');
  await page.getByRole('button', { name: 'Save' }).click();

  const row = page.locator('tr.talk', { hasText: 'On the new side.' }).first();
  await expect(row.locator('td')).toHaveCount(2);
  await expect(row.locator('td').nth(1)).toContainText('On the new side.');
  await expect(row.locator('td').nth(0)).not.toContainText('On the new side.');
});

test('the sidebar hides and comes back', async ({ page }) => {
  await expect(page.locator('.side')).toBeVisible();

  await page.keyboard.press('u');
  await expect(page.locator('.side')).toBeHidden();

  await page.reload();
  await expect(page.locator('.side')).toBeHidden();

  await page.keyboard.press('u');
  await expect(page.locator('.side')).toBeVisible();
});

test('a change can be marked read, and stays read', async ({ page }) => {
  const change = page.locator('li', { hasText: 'docs: rename the document' });
  const mark = change.getByRole('button', { name: '☐' });
  await mark.click();

  await expect(change.getByRole('button', { name: '☑' })).toBeVisible();

  await page.reload();
  await expect(
    page.locator('li', { hasText: 'docs: rename the document' }).getByRole('button', { name: '☑' }),
  ).toBeVisible();
});

test('the files are grouped under the directory they live in', async ({ page }) => {
  await page.getByRole('button', { name: /long: touch two places/ }).click();

  await expect(page.locator('.dir')).toHaveText(['src/']);
  await expect(page.locator('.file-row').first()).toContainText('added.py');
});

test('a comment can be written on a line the diff did not carry', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await page
    .getByRole('button', { name: /common lines/ })
    .first()
    .click();
  // The view is a preference, so this may be unified or side by side and
  // the line may be on the page once or twice.
  await expect(page.getByText('line 20', { exact: true }).first()).toBeVisible();

  const row = page.locator('tr', { hasText: 'line 20' }).first();
  await row.locator('td.gutter-comment').first().click();
  await page.getByRole('textbox').first().fill('Context lines take comments too.');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(page.getByText('Context lines take comments too.')).toBeVisible();
});

test('a comment box does not follow you to the next file', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await page.locator('td.gutter-comment').first().click();
  await expect(page.getByRole('textbox')).toHaveCount(1);

  await openFile(page, 'added.py');
  await expect(page.getByRole('textbox')).toHaveCount(0);
});

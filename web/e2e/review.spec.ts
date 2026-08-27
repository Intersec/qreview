// Reading a series, and writing on it.

import { expect, test } from '@playwright/test';
import { card, openChange, openFile, useSplit } from './act.ts';
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
  // A change opens on its message. The files after it are the work.
  await expect(page.locator('.file-bar h2')).toContainText('Commit message');

  await openFile(page, 'new-name.md');
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

  await expect(card(page, 'This loop never ends.')).toBeVisible();

  await page.reload();
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  await expect(card(page, 'This loop never ends.')).toBeVisible();
});

test('the keyboard walks the files the way Gerrit does', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  // Every change opens on its message, so the name of the file says nothing
  // about which change is open. The count of files does, and the bar shows
  // it only once a file of this change is open.
  await expect(page.locator('.change-bar')).toContainText('File 1 of 5');
  await expect(page.locator('.file-bar h2')).toContainText('Commit message');

  await page.keyboard.press(']');
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
  await expect(card(page, 'Written from the keyboard.')).toBeVisible();
});

test('the slash key moves to the file filter', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await expect(page.locator('.change-bar')).toContainText('File 1 of 5');

  await page.keyboard.press('/');
  await expect(page.locator('.file-filter')).toBeFocused();

  // And it filters, so the key led somewhere that does something.
  await page.keyboard.type('doc');
  await expect(page.locator('.file-row')).toHaveCount(1);
  await expect(page.locator('.file-row')).toContainText('doc.h');
});

test('a comment box takes the keyboard when it opens', async ({ page }) => {
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');

  // `c` opens the box. The letter opened it, so the letter is not in it.
  await page.keyboard.press('j');
  await page.keyboard.press('c');
  await expect(page.getByRole('textbox')).toHaveValue('');
  await expect(page.getByRole('textbox')).toBeFocused();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Cancel' }).click();

  await page.locator('td.gutter-comment').first().click();

  await expect(page.getByRole('textbox')).toBeFocused();
  await page.keyboard.type('Typed without touching the box.');
  await expect(page.getByRole('textbox')).toHaveValue('Typed without touching the box.');
});

test('an unfinished remark comes back without taking the keyboard', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await page.locator('td.gutter-comment').first().click();
  await page.getByRole('textbox').fill('Left half written.');

  await openFile(page, 'added.py');
  await openFile(page, 'long.c');

  // The box is there again, and the keyboard still walks the code.
  await expect(page.getByRole('textbox')).toHaveValue('Left half written.');
  await expect(page.getByRole('textbox')).not.toBeFocused();
  await page.keyboard.press('j');
  await expect(page.locator('tr.row-cursor')).toHaveCount(1);
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
  // Five files: the message, then the four the change touches.
  await expect(page.locator('.change-bar')).toContainText('File 1 of 5');
  await expect(page.locator('.change-bar')).toContainText('long: touch two places');

  await page.getByRole('button', { name: 'Next' }).click();
  await expect(page.locator('.change-bar')).toContainText('File 2 of 5');

  await page.getByRole('button', { name: 'Next' }).click();
  await page.getByRole('button', { name: 'Next' }).click();
  await page.getByRole('button', { name: 'Next' }).click();
  await expect(page.locator('.change-bar')).toContainText('File 5 of 5');
  await expect(page.getByRole('button', { name: 'Next' })).toBeDisabled();
});

test('a comment sits under the side it was written on', async ({ page }) => {
  await page.getByRole('button', { name: /net: retry the read/ }).click();
  await useSplit(page);
  await openFile(page, 'net.blk');

  await page.locator('td.gutter-comment[data-column="new"]').nth(2).click();
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
  // The message lives under no directory, so it stands above the group.
  await expect(page.locator('.file-row').first()).toContainText('Commit message');
  await expect(page.locator('.file-row').nth(1)).toContainText('added.py');
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

  await expect(card(page, 'Context lines take comments too.')).toBeVisible();
});

test('what is typed and not saved comes back with the file', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await page.locator('td.gutter-comment').first().click();
  await page.getByRole('textbox').first().fill('Half of a remark');

  // Away, without saving, and back.
  await openFile(page, 'added.py');
  await expect(page.getByRole('textbox')).toHaveCount(0);
  await openFile(page, 'long.c');

  await expect(page.getByRole('textbox').first()).toHaveValue('Half of a remark');

  // A reload is a fresh page, and the remark is still there.
  await page.reload();
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await expect(page.getByRole('textbox').first()).toHaveValue('Half of a remark');

  // Cancel drops it, so it does not come back for ever.
  await page.getByRole('button', { name: 'Cancel' }).click();
  await openFile(page, 'added.py');
  await openFile(page, 'long.c');
  await expect(page.getByRole('textbox')).toHaveCount(0);
});

test('a comment box does not follow you to the next file', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await page.locator('td.gutter-comment').first().click();
  await expect(page.getByRole('textbox')).toHaveCount(1);

  await openFile(page, 'added.py');
  await expect(page.getByRole('textbox')).toHaveCount(0);
});

test('the tab carries the logo, and the binary serves it', async ({ page }) => {
  // The mark is one file, in `web/public`. Vite copies it into `dist` and the
  // binary embeds that, so a missing icon means a build that lost an asset.
  const icon = page.locator('link[rel="icon"]');
  await expect(icon).toHaveAttribute('href', '/logo.svg');

  const answer = await page.request.get(new URL('/logo.svg', server.url).toString());
  expect(answer.status()).toBe(200);
  expect(answer.headers()['content-type']).toContain('image/svg+xml');

  await expect(page.locator('.top-bar .logo')).toBeVisible();
});

test('a file opened while the change loads keeps the pane', async ({ page }) => {
  // The page opens on a change and on the first file of it.
  await expect(page.locator('.file-bar h2')).toContainText('Commit message');

  // From here the file list answers slowly, so a change is still loading
  // when the reader picks a file of it, off the list already on the screen.
  await page.route(/\/files/, async (route) => {
    await new Promise((wait) => setTimeout(wait, 1200));
    await route.continue();
  });

  await page
    .getByRole('button', { name: /docs: rename the document/ })
    .first()
    .click();
  await page.locator('.file-row', { hasText: 'new-name.md' }).first().click();
  await expect(page.locator('.file-bar h2')).toContainText('new-name.md');

  // And it is still there once the list lands. The first file of the change
  // used to take the choice back.
  await page.waitForTimeout(2000);
  await expect(page.locator('.file-bar h2')).toContainText('new-name.md');
  await page.unroute(/\/files/);
});

test('Ctrl+S saves the comment, the way Gerrit does', async ({ page }) => {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');

  await page.keyboard.press('j');
  await page.keyboard.press('c');
  await expect(page.getByRole('textbox')).toHaveCount(1);
  await page.getByRole('textbox').fill('Saved without the mouse.');
  await page.keyboard.press('Control+s');

  await expect(card(page, 'Saved without the mouse.')).toBeVisible();
});

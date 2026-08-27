// A comment on more than one line, and on a part of a line.

import { expect, test, type Page } from '@playwright/test';
import { card, openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

// A server for each test, not one for the suite. Every test writes a
// comment, and a range that is stored is drawn on the code, so the marks of
// one test would be counted by the next.
let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start();
  await page.goto(server.url);
  await openChange(page, /long: touch two places/);
  await expect(page.locator('.change-bar')).toContainText('File 1 of 5');
  await openFile(page, 'long.c');
});

test.afterEach(() => server?.stop());

/// Drag the mouse from one line of code to another, the way a reader does.
async function drag(page: Page, from: number, to: number) {
  const start = page.locator(`td.code-cell[data-line="${from}"]`);
  const end = page.locator(`td.code-cell[data-line="${to}"]`);
  const a = (await start.boundingBox())!;
  const b = (await end.boundingBox())!;

  await page.mouse.move(a.x + 4, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + 40, b.y + b.height / 2, { steps: 8 });
  await page.mouse.up();
}

/// The point on the screen where character `char` of `line` starts, or
/// where the text ends when `char` is its length.
async function pointAt(page: Page, line: number, char: number) {
  return page.evaluate(
    ([line, char]) => {
      const cell = document.querySelector(`td.code-cell[data-line="${line}"]`)!;
      const walker = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT);
      let count = 0;
      let node = walker.nextNode() as Text | null;
      while (node) {
        const length = node.textContent?.length ?? 0;
        if (char < count + length || (char === count + length && length > 0)) {
          const range = document.createRange();
          const inside = char < count + length;
          range.setStart(node, inside ? char - count : char - count - 1);
          range.setEnd(node, inside ? char - count + 1 : char - count);
          const box = range.getBoundingClientRect();
          return { x: inside ? box.left : box.right, y: box.top + box.height / 2 };
        }
        count += length;
        node = walker.nextNode() as Text | null;
      }
      throw new Error(`no character ${char} on line ${line}`);
    },
    [line, char] as const,
  );
}

test('a selection over several lines offers to become a comment', async ({ page }) => {
  await drag(page, 5, 8);

  const offer = page.getByRole('button', { name: /Comment on (part of )?4 lines/ });
  await expect(offer).toBeVisible();

  await offer.click();
  await page.getByRole('textbox').first().fill('These four lines say one thing.');
  await page.getByRole('button', { name: 'Save' }).click();

  // The card sits under the last line of the range.
  const talk = page.locator('tr.talk', { hasText: 'These four lines say one thing.' });
  await expect(talk).toHaveCount(1);
  await expect(page.locator('.in-range')).not.toHaveCount(0);
});

test('the range comes back after a reload, over the same lines', async ({ page }) => {
  await drag(page, 5, 7);
  await page.getByRole('button', { name: /Comment on (part of )?3 lines/ }).click();
  await page.getByRole('textbox').first().fill('Three lines.');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(card(page, 'Three lines.')).toBeVisible();

  await page.reload();
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');

  await expect(card(page, 'Three lines.')).toBeVisible();
  const marked = page.locator('td.code-cell:has(.in-range)');
  await expect(marked).toHaveCount(3);
});

test('a part of one line can carry a comment of its own', async ({ page }) => {
  await drag(page, 6, 6);

  await expect(page.getByRole('button', { name: 'Comment on this' })).toBeVisible();
  await page.getByRole('button', { name: 'Comment on this' }).click();
  await page.getByRole('textbox').first().fill('This word is wrong.');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(card(page, 'This word is wrong.')).toBeVisible();
  // The mark covers a part of the line, not the whole of it.
  const cell = page.locator('td.code-cell[data-line="6"]').first();
  await expect(cell.locator('.in-range')).toHaveCount(1);
  expect(await cell.locator('.in-range').textContent()).not.toBe(await cell.textContent());
});

test('the selection stays what the reader picked once the offer appears', async ({ page }) => {
  // From inside line 6 to the end of line 7. The rows under a selection
  // must not be repainted while it stands: the browser anchors it on text
  // nodes, and an end whose node is replaced falls back to the start of
  // the line.
  const from = await pointAt(page, 6, 5);
  const to = await pointAt(page, 7, 6);
  await page.mouse.move(from.x + 1, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x + 2, to.y, { steps: 8 });
  await page.mouse.up();

  await expect(page.getByRole('button', { name: 'Comment on part of 2 lines' })).toBeVisible();
  expect(await page.evaluate(() => window.getSelection()?.toString())).toBe('6\nline 7');
});

test('the keyboard picks a range with v, and c writes on it', async ({ page }) => {
  await page.keyboard.press('j');
  await page.keyboard.press('v');
  await page.keyboard.press('j');
  await page.keyboard.press('j');

  await expect(page.locator('td.code-cell:has(.in-range)')).toHaveCount(3);

  await page.keyboard.press('c');
  await page.getByRole('textbox').first().fill('Three lines from the keyboard.');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(card(page, 'Three lines from the keyboard.')).toBeVisible();
});

test('a click puts the keyboard on the line, and c writes there', async ({ page }) => {
  await page.locator('td.code-cell[data-line="7"]').click();
  await expect(page.locator('tr.row-cursor')).toContainText('line 7');

  await page.keyboard.press('c');
  await page.getByRole('textbox').first().fill('About line 7.');
  await page.getByRole('button', { name: 'Save' }).click();

  // The card sits under line 7, not under the line the keyboard started on.
  const talk = page.locator('tr.talk', { hasText: 'About line 7.' });
  await expect(talk).toHaveCount(1);
  const before = page.locator('td.code-cell[data-line="7"]');
  await expect(before).toBeVisible();
});

test('a click while a box is open leaves the range of that box alone', async ({ page }) => {
  await drag(page, 5, 8);
  await page.getByRole('button', { name: /Comment on (part of )?4 lines/ }).click();

  // The reader looks at another line before finishing the remark.
  await page.locator('td.code-cell[data-line="12"]').click();
  await page.getByRole('textbox').first().fill('Still about four lines.');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(card(page, 'Still about four lines.')).toBeVisible();
  await expect(page.locator('td.code-cell:has(.in-range)')).toHaveCount(4);
});

test('escape drops a range that is being picked', async ({ page }) => {
  await page.keyboard.press('j');
  await page.keyboard.press('v');
  await page.keyboard.press('j');
  await expect(page.locator('td.code-cell:has(.in-range)')).toHaveCount(2);

  await page.keyboard.press('Escape');
  await expect(page.locator('.in-range')).toHaveCount(0);
});

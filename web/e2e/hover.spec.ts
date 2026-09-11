// The word under the pointer, lit wherever else it stands in the file.

import { expect, test, type Page } from '@playwright/test';
import { openChange, openFile, useSplit } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start();
  await page.goto(server.url);
  await openChange(page, /long: touch two places/);
  await useSplit(page);
});

test.afterEach(() => server?.stop());

/// Put the pointer on a word of a line, the way a reader does.
///
/// The point is the middle of the word, measured in the page: the code is
/// drawn by the browser, so nothing here guesses where a letter landed.
async function hover(page: Page, line: number, word: string) {
  const at = await page.evaluate(
    ({ line, word }) => {
      const cell = document.querySelector(`td.code-cell[data-line="${line}"]`);
      if (!cell) {
        return null;
      }
      const walker = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT);
      while (walker.nextNode()) {
        const node = walker.currentNode;
        const start = (node.textContent ?? '').indexOf(word);
        if (start < 0) {
          continue;
        }
        const range = document.createRange();
        range.setStart(node, start);
        range.setEnd(node, start + word.length);
        const box = range.getBoundingClientRect();

        return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
      }
      return null;
    },
    { line, word },
  );

  expect(at, `${word} is on line ${line}`).not.toBeNull();
  await page.mouse.move(at!.x, at!.y);
}

test('a name lights up everywhere it stands in the file', async ({ page }) => {
  await openFile(page, 'doc.h');
  await hover(page, 1, 'pointer');

  // `pointer` is on two lines of the doc comment, and a context line is
  // drawn in both columns of the side by side view.
  await expect(page.locator('.same-word')).toHaveCount(4);
  for (const text of await page.locator('.same-word').allTextContents()) {
    expect(text).toBe('pointer');
  }
});

test('a longer name that contains it is left alone', async ({ page }) => {
  await openFile(page, 'doc.h');
  await hover(page, 1, 'pointer');

  // The line that changed declares `is_pointer`. Following a name means
  // that name, not every name that carries its letters.
  await expect(page.locator('td.code-cell[data-kind="add"] .same-word')).toHaveCount(0);
  await expect(page.locator('td.code-cell[data-kind="remove"] .same-word')).toHaveCount(0);
});

test('the light goes out when the pointer leaves the code', async ({ page }) => {
  await openFile(page, 'doc.h');
  await hover(page, 1, 'pointer');
  await expect(page.locator('.same-word')).not.toHaveCount(0);

  const bar = (await page.locator('.file-bar').boundingBox())!;
  await page.mouse.move(bar.x + bar.width / 2, bar.y + bar.height / 2);

  await expect(page.locator('.same-word')).toHaveCount(0);
});

test('a number is not a name', async ({ page }) => {
  await openFile(page, 'long.c');

  // The same line, and the same pointer: only the word under it differs.
  await hover(page, 12, 'line');
  await expect(page.locator('.same-word')).not.toHaveCount(0);

  await hover(page, 12, '12');
  await expect(page.locator('.same-word')).toHaveCount(0);
});

/// Drag a selection from the middle of one cell to the middle of another.
async function drag(page: Page, from: string, to: string) {
  const a = (await page.locator(from).first().boundingBox())!;
  const b = (await page.locator(to).first().boundingBox())!;
  await page.mouse.move(a.x + 4, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + 60, b.y + b.height / 2, { steps: 10 });
  await page.mouse.up();
}

test('a selection holds, and no word lights under it', async ({ page }) => {
  await openFile(page, 'long.c');
  await hover(page, 12, 'line');
  await expect(page.locator('.same-word')).not.toHaveCount(0);

  await drag(
    page,
    'td.code-cell[data-column="new"][data-line="5"]',
    'td.code-cell[data-column="new"][data-line="7"]',
  );

  // Lighting a word splits the line into other spans, and the browser
  // paints a selection from the spans it was made on. So the word gives
  // way: the selection reads the same after the pointer crosses it.
  await hover(page, 6, 'line');
  await expect(page.locator('.same-word')).toHaveCount(0);
  expect(await page.evaluate(() => window.getSelection()?.toString() ?? '')).toBe(
    'line 5\nline 6\nline 7',
  );
});

test('the light comes back when the selection is dropped', async ({ page }) => {
  await openFile(page, 'long.c');
  await drag(
    page,
    'td.code-cell[data-column="new"][data-line="5"]',
    'td.code-cell[data-column="new"][data-line="7"]',
  );
  await hover(page, 12, 'line');
  await expect(page.locator('.same-word')).toHaveCount(0);

  await page.locator('td.code-cell[data-line="12"]').first().click();
  await hover(page, 12, 'line');
  await expect(page.locator('.same-word')).not.toHaveCount(0);
});

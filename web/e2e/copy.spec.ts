// What lands in the clipboard when a reader selects code.
//
// The tests read the selection rather than the clipboard: the browser copies
// what is selected, and a headless clipboard is a permission dance that says
// nothing more.

import { expect, test, type Page } from '@playwright/test';
import { openChange, openFile, useSplit } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeAll(async () => {
  server = await start();
});
test.afterAll(() => server?.stop());

test.beforeEach(async ({ page }) => {
  await page.goto(server.url);
  await openChange(page, /long: touch two places/);
  await expect(page.locator('.change-bar')).toContainText('File 1 of 5');
  await openFile(page, 'long.c');
});

async function drag(page: Page, from: string, to: string) {
  const a = (await page.locator(from).first().boundingBox())!;
  const b = (await page.locator(to).first().boundingBox())!;
  await page.mouse.move(a.x + 4, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + 60, b.y + b.height / 2, { steps: 10 });
  await page.mouse.up();
}

function selected(page: Page) {
  return page.evaluate(() => window.getSelection()?.toString() ?? '');
}

test('the line numbers stay out of it', async ({ page }) => {
  await drag(page, 'td.code-cell[data-line="5"]', 'td.code-cell[data-line="7"]');

  expect(await selected(page)).toBe('line 5\nline 6\nline 7');
});

test('the bar between two hunks stays out of it', async ({ page }) => {
  await drag(page, 'td.code-cell[data-line="12"]', 'td.code-cell[data-line="41"]');

  const text = await selected(page);
  expect(text).toContain('line 13');
  expect(text).toContain('line 40');
  expect(text).not.toContain('common lines');
  expect(text).not.toContain('+10');
});

test('a line is copied once, not once per column', async ({ page }) => {
  await useSplit(page);

  for (const column of ['new', 'old']) {
    await drag(
      page,
      `td.code-cell[data-column="${column}"][data-line="5"]`,
      `td.code-cell[data-column="${column}"][data-line="7"]`,
    );

    expect(await selected(page)).toBe('line 5\nline 6\nline 7');
  }
});

test('what a selection offers does not stand where the pointer is', async ({ page }) => {
  const end = (await page.locator('td.code-cell[data-line="8"]').first().boundingBox())!;
  const x = end.x + 60;
  const y = end.y + end.height / 2;

  await drag(page, 'td.code-cell[data-line="5"]', 'td.code-cell[data-line="8"]');
  await expect(page.locator('.offer')).toBeVisible();

  // A right click on that button would open the menu of a button, which
  // carries no Copy. It has to leave the pointer alone.
  for (const below of [0, 8, 14, 20]) {
    const found = await page.evaluate(
      ([px, py]) => document.elementFromPoint(px, py)?.className ?? '',
      [x, y + below],
    );
    expect(found, `${below}px under the pointer`).not.toContain('offer');
  }
});

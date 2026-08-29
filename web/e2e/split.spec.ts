// The side-by-side view. Two columns of code, not two columns of margin.

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
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await useSplit(page);
});

test('the gutters are narrow and the code has the room', async ({ page }) => {
  const table = await page.locator('table.code').boundingBox();
  const row = page.locator('tr', { has: page.locator('td.code-cell') }).first();
  const gutters = await row.locator('td.gutter').all();
  const cells = await row.locator('td.code-cell').all();

  expect(table).not.toBeNull();
  let spent = 0;
  for (const gutter of gutters) {
    const box = await gutter.boundingBox();
    expect(box!.width).toBeLessThan(table!.width * 0.08);
    spent += box!.width;
  }
  let code = 0;
  for (const cell of cells) {
    code += (await cell.boundingBox())!.width;
  }

  expect(code).toBeGreaterThan(table!.width - spent - 4);
});

test('an added line is green and a removed line is red', async ({ page }) => {
  const added = page.locator('td.code-cell.row-add').first();
  const removed = page.locator('td.code-cell.row-remove').first();
  const context = page.locator('td.code-cell.row-context').first();

  const colour = (locator: typeof added) =>
    locator.evaluate((node) => getComputedStyle(node).backgroundColor);

  const [add, remove, plain] = await Promise.all([colour(added), colour(removed), colour(context)]);

  expect(add).not.toBe(plain);
  expect(remove).not.toBe(plain);
  expect(add).not.toBe(remove);
});

test('the two sides face each other on one row', async ({ page }) => {
  const row = page.locator('tr', { has: page.locator('td.row-remove') }).first();

  await expect(row.locator('td.code-cell')).toHaveCount(2);
  await expect(row.locator('td.row-remove')).toHaveCount(1);
  await expect(row.locator('td.row-add')).toHaveCount(1);
});

test('nothing is lost between the two views', async ({ page }) => {
  const split = await page.locator('td.code-cell.row-add').count();

  await page.getByRole('button', { name: 'Unified' }).click();
  const unified = await page.locator('td.code-cell.row-add').count();

  expect(split).toBe(unified);
});

test('a token never becomes a block of its own', async ({ page }) => {
  await openFile(page, 'doc.h');

  // `comment.block.documentation` reached the page as `comment block`, and
  // a utility class of that name set `display: block`, so `/**` sat on a
  // line of its own and the two sides drifted apart.
  const displays = await page
    .locator('td.code-cell span')
    .evaluateAll((nodes) => nodes.map((node) => getComputedStyle(node).display));

  expect(displays.length).toBeGreaterThan(0);
  expect([...new Set(displays)]).toEqual(['inline']);
});

test('a doc comment stays on the line it was written on', async ({ page }) => {
  await openFile(page, 'doc.h');

  const row = page.locator('tr', { hasText: 'Return whether the field' }).first();
  await expect(row).toContainText('/** Return whether the field is a pointer or not.');
});

test('the file bar spans the pane, whatever the code does', async ({ page }) => {
  await openFile(page, 'long.c');

  const pane = await page.locator('.diff-pane').boundingBox();
  const bar = await page.locator('.file-bar').boundingBox();

  // The pane used to scroll sideways itself, which sized the bar to its own
  // content and left a strip of colour that stopped mid-window.
  expect(bar!.width).toBeGreaterThan(pane!.width - 2);
});

test('a remark about the file stands before the first line', async ({ page }) => {
  await openFile(page, 'long.c');
  await page.getByRole('button', { name: 'Comment on the file' }).click();
  await page.getByRole('textbox').first().fill('This file does two things.');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(card(page, 'This file does two things.')).toBeVisible();

  // Inside the code, above line 1, not in a band that takes the top of the
  // screen on every file.
  const first = page.locator('table.code tr').first();
  await expect(first).toHaveClass(/talk/);
  await expect(first).toContainText('This file does two things.');
});

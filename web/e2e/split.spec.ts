// The side-by-side view. Two columns of code, not two columns of margin.

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

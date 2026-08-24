// The lines a diff does not carry, opened from the bar between two hunks.

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
});

test('the bar says how many lines it hides', async ({ page }) => {
  // The fixture touches line 3 and line 50 of a file of 60 lines, and ten
  // lines of context on each side leave 14 to 39 in the gap.
  await expect(page.getByRole('button', { name: '+26 common lines' })).toBeVisible();
  await expect(page.getByText('line 20', { exact: true })).toBeHidden();
});

test('opening the whole gap shows the lines and takes the bar away', async ({ page }) => {
  const bar = page.getByRole('button', { name: '+26 common lines' });
  await bar.click();

  await expect(page.getByText('line 20', { exact: true })).toBeVisible();
  await expect(bar).toBeHidden();
});

test('an opened line carries both line numbers', async ({ page }) => {
  await page.getByRole('button', { name: '+26 common lines' }).click();

  // Nothing was added above line 20, so the two sides agree on the number.
  const row = page.locator('tr', { hasText: /^\s*20\s*20\s*line 20$/ });
  await expect(row).toHaveCount(1);
});

test('the short step down opens the end nearest the hunk below', async ({ page }) => {
  await page.getByRole('button', { name: '+10 ↓' }).click();

  await expect(page.getByText('line 39', { exact: true })).toBeVisible();
  await expect(page.getByText('line 20', { exact: true })).toBeHidden();
  await expect(page.getByRole('button', { name: '+16 common lines' })).toBeVisible();
});

test('the short step up opens the end nearest the hunk above', async ({ page }) => {
  await page.getByRole('button', { name: '+10 ↑' }).click();

  await expect(page.getByText('line 14', { exact: true })).toBeVisible();
  await expect(page.getByText('line 39', { exact: true })).toBeHidden();
  await expect(page.getByRole('button', { name: '+16 common lines' })).toBeVisible();
});

test('the lines opened at the top land above the bar, not below it', async ({ page }) => {
  await page.getByRole('button', { name: '+10 ↑' }).click();
  await expect(page.getByText('line 14', { exact: true })).toBeVisible();

  // What the reader sees is what matters: line 14 follows the hunk it
  // continues, and the bar for what is still closed sits under it.
  const opened = await page.getByText('line 14', { exact: true }).first().boundingBox();
  const bar = await page.locator('tr.context-bar').first().boundingBox();

  expect(opened!.y).toBeLessThan(bar!.y);
});

test('the lines opened at the bottom land below the bar', async ({ page }) => {
  await page.getByRole('button', { name: '+10 ↓' }).click();
  await expect(page.getByText('line 39', { exact: true })).toBeVisible();

  const opened = await page.getByText('line 39', { exact: true }).first().boundingBox();
  const bar = await page.locator('tr.context-bar').first().boundingBox();

  expect(opened!.y).toBeGreaterThan(bar!.y);
});

test('the context opens in the side by side view too', async ({ page }) => {
  await useSplit(page);
  await page.getByRole('button', { name: '+26 common lines' }).click();

  await expect(page.getByText('line 20', { exact: true })).toHaveCount(2);
});

test('a file with no gap has no bar', async ({ page }) => {
  await openFile(page, 'added.py');

  await expect(page.getByRole('button', { name: /common lines/ })).toHaveCount(0);
});

for (const [button, line] of [
  ['+10 ↑', 'line 14'],
  ['+10 ↓', 'line 39'],
] as const) {
  test(`a line opened by ${button} takes a comment`, async ({ page }) => {
    await page.getByRole('button', { name: button }).click();
    await expect(page.getByText(line, { exact: true }).first()).toBeVisible();

    const row = page.locator('tr', { hasText: line }).first();
    await row.locator('td.gutter-comment').first().click();

    // The row drawn after the bar had no place to put the box, so the click
    // set the state and nothing appeared.
    await expect(page.getByRole('textbox')).toHaveCount(1);
    await page.getByRole('textbox').fill(`A remark on ${line}.`);
    await page.getByRole('button', { name: 'Save' }).click();
    await expect(card(page, `A remark on ${line}.`)).toBeVisible();
  });
}

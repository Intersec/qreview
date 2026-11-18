// The theme the reader chose, against the one of the system.

import { expect, test, type Browser } from '@playwright/test';
import { openChange } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeAll(async () => {
  server = await start({ config: { ui: { theme: 'light' } } });
});
test.afterAll(() => server?.stop());

/// The background of the merge bar, on a system of the given scheme.
async function mergeBar(browser: Browser, scheme: 'light' | 'dark'): Promise<string> {
  const context = await browser.newContext({ colorScheme: scheme });
  const page = await context.newPage();
  await page.goto(server.url);
  await openChange(page, /Merge branch side into main/);

  // The bar is the block that holds its button.
  const bar = page.getByRole('button', { name: 'What it brings in' }).locator('xpath=../..');
  const color = await bar.evaluate((el) => getComputedStyle(el).backgroundColor);
  await context.close();
  return color;
}

test('the merge bar follows the chosen theme, not the system', async ({ browser }) => {
  // The reader chose light, so a dark system changes nothing.
  expect(await mergeBar(browser, 'dark')).toBe(await mergeBar(browser, 'light'));
});

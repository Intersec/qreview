// The theme the reader chose, against the one of the system, and the theme
// files of the user.

import { expect, test, type Page } from '@playwright/test';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { openChange, pickTheme } from './act.ts';
import { start, type Running } from './server.ts';

const NIGHT = ':root {\n  color-scheme: dark;\n  --page: #000010;\n}\n';
const PAPER = ':root {\n  color-scheme: light;\n  --page: #fafaf0;\n}\n';
const BROKEN = ':root {\n  color-scheme: dark;\n  --page: url(x);\n}\n';

async function background(page: Page): Promise<string> {
  return page.evaluate(() => getComputedStyle(document.body).backgroundColor);
}

// The tokens of the merge bar, in each built-in palette.
for (const [theme, system, color] of [
  ['light', 'dark', 'rgb(255, 251, 235)'],
  ['dark', 'light', 'rgb(31, 22, 14)'],
] as const) {
  test.describe(`the ${theme} theme on a ${system} system`, () => {
    let server: Running;

    test.beforeAll(async () => {
      server = await start({ config: { ui: { theme } } });
    });
    test.afterAll(() => server?.stop());

    test('the merge bar follows the chosen theme, not the system', async ({ browser }) => {
      const context = await browser.newContext({ colorScheme: system });
      const page = await context.newPage();
      await page.goto(server.url);
      await openChange(page, /Merge branch side into main/);

      // The bar is the block that holds its button.
      const bar = page.getByRole('button', { name: 'What it brings in' }).locator('xpath=../..');
      await expect(bar).toHaveCSS('background-color', color);
      await context.close();
    });
  });
}

test.describe('the theme files of the user', () => {
  // Each test changes the theme of the file, so they run one after the other.
  test.describe.configure({ mode: 'serial' });

  let server: Running;

  test.beforeAll(async () => {
    server = await start({
      config: { ui: { theme: 'night' } },
      themes: { night: NIGHT, paper: PAPER, broken: BROKEN },
    });
  });
  test.afterAll(() => server?.stop());

  test('the chosen theme paints the page in its base', async ({ page }) => {
    await page.goto(server.url);

    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    expect(await background(page)).toBe('rgb(0, 0, 16)');
  });

  test('a broken file is said in the terminal and not offered', async ({ page }) => {
    expect(server.output()).toContain('qreview: theme ');
    expect(server.output()).toContain('broken.css:3: `--page` takes a hex color');

    await page.goto(server.url);
    await page.getByRole('button', { name: '⚙' }).click();
    const offered = await page
      .getByRole('dialog')
      .getByLabel('Theme')
      .locator('option')
      .evaluateAll((options) => options.map((o) => (o as HTMLOptionElement).value));

    expect(offered).toEqual(['system', 'light', 'dark', 'night', 'paper']);
  });

  test('a theme picked in the panel stays after a reload', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto(server.url);
    await pickTheme(page, 'paper');

    // A light theme on a dark system: the theme wins.
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
    expect(await background(page)).toBe('rgb(250, 250, 240)');

    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
    expect(await background(page)).toBe('rgb(250, 250, 240)');
  });

  test('following the system again gives the built-in palette', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto(server.url);
    await pickTheme(page, 'system');

    await expect(page.locator('html')).not.toHaveAttribute('data-theme');
    expect(await background(page)).toBe('rgb(255, 255, 255)');
  });

  test('an edited theme shows on a reload, with no restart', async ({ page }) => {
    await page.goto(server.url);
    await pickTheme(page, 'paper');
    await expect.poll(() => background(page)).toBe('rgb(250, 250, 240)');

    const file = join(server.fixture.config, 'qreview', 'themes', 'paper.css');
    writeFileSync(file, PAPER.replace('#fafaf0', '#f0f0ff'));
    await page.reload();

    await expect.poll(() => background(page)).toBe('rgb(240, 240, 255)');
  });
});

test.describe('a theme the repository names', () => {
  let server: Running;

  test.beforeAll(async () => {
    server = await start({
      config: { ui: { theme: 'paper' } },
      themes: { paper: PAPER },
      repoConfig: { ui: { theme: 'gone' } },
    });
  });
  test.afterAll(() => server?.stop());

  test('falls back to the theme of the user, and says so', async ({ page }) => {
    expect(server.output()).toContain('.qreview.json names `gone`');

    await page.goto(server.url);

    await expect.poll(() => background(page)).toBe('rgb(250, 250, 240)');
  });
});

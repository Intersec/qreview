// A Markdown file read rendered, beside its diff.

import { expect, test } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start({ markdown: true });
  await page.goto(server.url);
});

test.afterEach(() => server?.stop());

test('the rendered file opens beside the diff, on request', async ({ page }) => {
  let asked = 0;
  page.on('request', (request) => {
    if (request.url().includes('/rendered')) {
      asked += 1;
    }
  });
  await openChange(page, /docs: write a guide/);
  await openFile(page, 'guide.md');

  await expect(page.locator('table.code')).toBeVisible();
  expect(asked, 'nothing renders before the reader asks').toBe(0);

  await page.getByRole('button', { name: 'Show rendered' }).click();
  const pane = page.getByRole('complementary', { name: 'The file, rendered' });
  await expect(pane.locator('h1')).toHaveText('The guide');
  await expect(pane).toContainText('Raw <b>HTML</b> stays text.');
  // The diff stays where it was, and can still take a remark.
  await expect(page.locator('table.code')).toBeVisible();
  expect(asked).toBe(1);

  await page.getByRole('button', { name: 'Hide rendered' }).click();
  await expect(pane).toHaveCount(0);
});

test('the pane follows the diff as it scrolls', async ({ page }) => {
  await openChange(page, /docs: write a guide/);
  await openFile(page, 'guide.md');
  await page.getByRole('button', { name: 'Show rendered' }).click();
  const pane = page.getByRole('complementary', { name: 'The file, rendered' });
  await expect(pane.locator('h1')).toHaveText('The guide');

  // The diff goes to a section near the end, and the pane goes with it.
  const row = page.locator('td.code-cell[data-side="new"]', { hasText: '## Section 15' });
  await row.evaluate((cell) => {
    const scroller = cell.closest<HTMLElement>('.diff-pane');
    const bar = scroller?.querySelector<HTMLElement>('.file-bar');
    if (scroller && bar) {
      scroller.scrollTop += cell.getBoundingClientRect().top - bar.getBoundingClientRect().bottom;
    }
  });

  const heading = pane.locator('h2', { hasText: /^Section 15$/ });
  await expect
    .poll(async () => {
      const [box, frame] = await Promise.all([heading.boundingBox(), pane.boundingBox()]);
      return box && frame ? Math.abs(box.y - frame.y) : Infinity;
    })
    .toBeLessThan(40);
});

test('an image of the tree is read from the version', async ({ page }) => {
  await openChange(page, /docs: write a guide/);
  await openFile(page, 'guide.md');
  await page.getByRole('button', { name: 'Show rendered' }).click();

  const image = page.getByRole('img', { name: 'The flow' });
  await expect(image).toHaveAttribute('src', /\/blob\?path=docs%2Fflow\.svg/);
  await expect
    .poll(() => image.evaluate((img: HTMLImageElement) => img.naturalWidth))
    .toBeGreaterThan(0);
});

test('the pane is read only', async ({ page }) => {
  await openChange(page, /docs: write a guide/);
  await openFile(page, 'guide.md');
  await page.getByRole('button', { name: 'Show rendered' }).click();

  await page.locator('.md-doc p').first().click();
  await expect(page.getByRole('textbox')).toHaveCount(0);
});

test('a file that is not Markdown offers no rendered view', async ({ page }) => {
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');

  await expect(page.locator('table.code')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Show rendered' })).toHaveCount(0);
});

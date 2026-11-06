// A Markdown file read rendered, beside its diff.

import { expect, test } from '@playwright/test';
import { openChange, openFile, useSplit } from './act.ts';
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

test('the diff follows the pane as the reader scrolls it', async ({ page }) => {
  await openChange(page, /docs: write a guide/);
  await openFile(page, 'guide.md');
  await page.getByRole('button', { name: 'Show rendered' }).click();
  const pane = page.getByRole('complementary', { name: 'The file, rendered' });
  await expect(pane.locator('h1')).toHaveText('The guide');

  // The reader brings a section near the end to the top of the pane.
  await pane.locator('h2', { hasText: /^Section 25$/ }).evaluate((heading) => {
    const aside = heading.closest<HTMLElement>('aside');
    if (aside) {
      aside.scrollTop += heading.getBoundingClientRect().top - aside.getBoundingClientRect().top;
    }
  });

  const row = page.locator('td.code-cell[data-side="new"]', { hasText: /^## Section 25$/ });
  const bar = page.locator('.file-bar');
  await expect
    .poll(async () => {
      const [box, under] = await Promise.all([row.boundingBox(), bar.boundingBox()]);
      return box && under ? Math.abs(box.y - (under.y + under.height)) : Infinity;
    })
    .toBeLessThan(40);
  // The diff moved for the pane: the pane must not move back for the diff.
  const top = await pane.evaluate((aside) => aside.scrollTop);
  await page.waitForTimeout(300);
  expect(await pane.evaluate((aside) => aside.scrollTop)).toBe(top);
});

test('the pane follows a line inside a long code block', async ({ page }) => {
  await openChange(page, /docs: write a guide/);
  await openFile(page, 'guide.md');
  await page.getByRole('button', { name: 'Show rendered' }).click();
  const pane = page.getByRole('complementary', { name: 'The file, rendered' });
  await expect(pane.locator('h1')).toHaveText('The guide');

  const row = page.locator('td.code-cell[data-side="new"]', { hasText: 'step 20.' });
  await row.evaluate((cell) => {
    const scroller = cell.closest<HTMLElement>('.diff-pane');
    const bar = scroller?.querySelector<HTMLElement>('.file-bar');
    if (scroller && bar) {
      scroller.scrollTop += cell.getBoundingClientRect().top - bar.getBoundingClientRect().bottom;
    }
  });

  // The top of the block is far above: the line itself comes to the top.
  await expect
    .poll(() =>
      pane.evaluate((aside) => {
        const pre = [...aside.querySelectorAll('pre')].find((el) =>
          el.textContent?.includes('step 20.'),
        );
        const text = pre?.textContent ?? '';
        const at = text.indexOf('step 20.');
        const walker = document.createTreeWalker(pre ?? aside, NodeFilter.SHOW_TEXT);
        let seen = 0;
        for (let node = walker.nextNode(); node; node = walker.nextNode()) {
          const length = node.textContent?.length ?? 0;
          if (seen + length > at) {
            const range = document.createRange();
            range.setStart(node, at - seen);
            return Math.abs(range.getBoundingClientRect().top - aside.getBoundingClientRect().top);
          }
          seen += length;
        }
        return Infinity;
      }),
    )
    .toBeLessThan(40);
});

test('the pane opens on the part of the file the diff shows', async ({ page }) => {
  await openChange(page, /docs: reword a section/);
  await openFile(page, 'guide.md');
  await page.getByRole('button', { name: 'Show rendered' }).click();

  // The diff does not scroll: it is one hunk, far down the file.
  const pane = page.getByRole('complementary', { name: 'The file, rendered' });
  const changed = pane.getByText('The text of section 30, said again.');
  await expect(changed).toBeInViewport();
  await expect(pane.locator('h1')).not.toBeInViewport();
});

test('the blocks the change touches are marked in the margin', async ({ page }) => {
  await openChange(page, /docs: rework the guide/);
  await openFile(page, 'guide.md');
  await page.getByRole('button', { name: 'Show rendered' }).click();
  const pane = page.getByRole('complementary', { name: 'The file, rendered' });

  const doc = pane.locator('.md-doc');
  await expect(doc.locator('p[data-change="modify"]').first()).toContainText('A paragraph with');
  await expect(doc.locator('[data-change="add"]')).toHaveText('a second nested item');
  await expect(doc.locator('tr[data-change="modify"]')).toContainText('14');
  await expect(pane.locator('h1')).not.toHaveAttribute('data-change');
  // The bars stand in one column at the left of the page, however deep the
  // block sits: the nested item and the table row line up with the paragraph.
  const bars = pane.locator('.md-bar');
  await expect(bars).toHaveCount(await doc.locator('[data-change]').count());
  const lefts = new Set<number>();
  for (const bar of await bars.all()) {
    lefts.add(Math.round((await bar.boundingBox())?.x ?? -1));
  }
  expect(lefts.size).toBe(1);

  // The line that went away is a tick, and the tick brings the diff to it.
  const tick = pane.locator('.md-tick');
  await expect(tick).toHaveAttribute('title', '2 lines removed');
  await tick.click();
  await expect(page.locator('.row-cursor')).toContainText('Raw <b>HTML</b> stays text.');
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

test('an AsciiDoc file is rendered beside its diff', async ({ page }) => {
  await openChange(page, /docs: write a manual/);
  await openFile(page, 'manual.adoc');
  await page.getByRole('button', { name: 'Show rendered' }).click();
  const pane = page.getByRole('complementary', { name: 'The file, rendered' });

  await expect(pane.locator('h2')).toHaveText('Install');
  await expect(pane.locator('.ulist[data-lines="7-8"]')).toContainText('a nested item');
  await expect(pane.locator('pre.md-code span[class*="tok-"]').first()).toBeVisible();
  await expect(pane.getByRole('img', { name: 'The flow' })).toHaveAttribute(
    'src',
    /\/blob\?path=docs%2Fflow\.svg/,
  );
  // The raw HTML of a `++++` block never reaches the page.
  await expect(pane.locator('script')).toHaveCount(0);
  expect(await page.evaluate(() => (window as { ran?: boolean }).ran)).toBeUndefined();
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

test('the diff wraps its long lines while the pane is open', async ({ page }) => {
  await useSplit(page);
  await openChange(page, /docs: rework the guide/);
  await openFile(page, 'guide.md');
  const table = page.locator('table.code');
  await expect(table).toHaveClass(/nowrap/);

  // A table as wide as its longest line pushed the bar of a gap under the
  // pane, out of reach.
  await page.getByRole('button', { name: 'Show rendered' }).click();
  await expect(page.getByRole('button', { name: /common lines/ }).first()).toBeInViewport();
  await expect(table).not.toHaveClass(/nowrap/);

  await page.getByRole('button', { name: 'Hide rendered' }).click();
  await expect(table).toHaveClass(/nowrap/);
});

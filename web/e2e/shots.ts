// Screenshots of the interface, for a person or an agent to look at.
//
// `npm run shots` writes them under e2e/.shots. They are not a test: nothing
// passes or fails here, it only makes the interface visible.

import { chromium, type Page } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import config from '../playwright.config.ts';
import { openChange, openFile, useSplit } from './act.ts';
import { start } from './server.ts';

const OUT = join(import.meta.dirname, '.shots');

async function openLongFile(page: Page) {
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
}

const server = await start();
mkdirSync(OUT, { recursive: true });

const browser = await chromium.launch({
  executablePath: config.use?.launchOptions?.executablePath,
  args: ['--no-sandbox', '--disable-dev-shm-usage'],
});

for (const scheme of ['light', 'dark'] as const) {
  const page = await browser.newPage({
    viewport: { width: 1600, height: 1000 },
    colorScheme: scheme,
  });
  const complaints: string[] = [];
  page.on('response', (response) => {
    if (response.status() >= 400) {
      complaints.push(`${response.status()} ${response.url()}`);
    }
  });
  page.on('console', (message) => {
    if (message.type() === 'error' || message.type() === 'warning') {
      complaints.push(`${message.type()}: ${message.text()}`);
    }
  });

  await page.goto(server.url);

  // The commit message, which is the file a change opens on.
  await openChange(page, /net: retry the read/);
  await openFile(page, 'Commit message');
  await page.screenshot({ path: join(OUT, `${scheme}-message.png`) });

  await openLongFile(page);
  await page.screenshot({ path: join(OUT, `${scheme}-unified.png`) });

  // The context between the two hunks, opened.
  await page
    .getByRole('button', { name: /common lines/ })
    .first()
    .click();
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(OUT, `${scheme}-expanded.png`) });

  await useSplit(page);
  await page.screenshot({ path: join(OUT, `${scheme}-split.png`) });

  // A remark on the left side, on the line the change deletes. It sits under
  // the left column, because that is the side it speaks of.
  await page
    .locator('tr', { has: page.locator('td.gutter[data-column="old"]:text-is("3")') })
    .locator('td.gutter-comment[data-column="old"]')
    .click();
  const left = page.getByRole('textbox').first();
  await left.waitFor();
  await left.fill('This line was doing something.');
  await page.screenshot({ path: join(OUT, `${scheme}-leftside.png`) });
  await page.getByRole('button', { name: 'Cancel' }).click();

  // The panel, which is where the settings live now.
  await page.getByRole('button', { name: '⚙' }).click();
  await page.screenshot({ path: join(OUT, `${scheme}-preferences.png`) });
  await page.getByRole('button', { name: 'Cancel' }).click();

  // The whole Change-Id in the bar, and the file filter under the keyboard.
  await openChange(page, /long: touch two places/);
  await page.keyboard.press('/');
  await page.screenshot({ path: join(OUT, `${scheme}-filter.png`) });

  // A range picked with the keyboard, and the comment it offers.
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  for (const key of ['j', 'j', 'v', 'j', 'j', 'j']) {
    await page.keyboard.press(key);
  }
  await page.keyboard.press('c');
  await page.getByRole('textbox').first().fill('These four lines say one thing.');
  await page.screenshot({ path: join(OUT, `${scheme}-range.png`) });
  await page.getByRole('button', { name: 'Cancel' }).click();

  // A comment, open on its line.
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  await page.locator('td.gutter-comment[data-column="new"]').nth(2).click();
  await page.getByRole('textbox').first().fill('This loop never ends when the socket closes.');
  await page.screenshot({ path: join(OUT, `${scheme}-comment.png`) });

  // The list of what the session holds, with a remark in two changes. The
  // two themes share one server, so the second pass finds the remarks of
  // the first: the wait is on a count of rows, never on a total.
  await page.getByRole('button', { name: 'Save' }).click();
  await page.locator('.list-row').first().waitFor();

  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
  await page.locator('td.gutter-comment[data-column="new"]').nth(3).click();
  const box = page.getByRole('textbox').first();
  await box.waitFor();
  await box.fill('This line is the one to read twice.');
  await page.getByRole('button', { name: 'Save' }).click();
  await page.locator('.list-row').nth(1).waitFor();
  await page.screenshot({ path: join(OUT, `${scheme}-comment-list.png`) });

  if (complaints.length) {
    console.log(`${scheme}: the page complained\n  ${complaints.join('\n  ')}`);
  }
  await page.close();
}

await browser.close();
server.stop();
console.log(`screenshots in ${OUT}`);

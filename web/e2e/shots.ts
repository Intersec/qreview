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

  // The panel, which is where the settings live now.
  await page.getByRole('button', { name: '⚙' }).click();
  await page.screenshot({ path: join(OUT, `${scheme}-preferences.png`) });
  await page.getByRole('button', { name: 'Cancel' }).click();

  // A comment, open on its line.
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  await page.locator('td.gutter-comment').nth(2).click();
  await page.getByRole('textbox').first().fill('This loop never ends when the socket closes.');
  await page.screenshot({ path: join(OUT, `${scheme}-comment.png`) });

  if (complaints.length) {
    console.log(`${scheme}: the page complained\n  ${complaints.join('\n  ')}`);
  }
  await page.close();
}

await browser.close();
server.stop();
console.log(`screenshots in ${OUT}`);

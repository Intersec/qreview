// Screenshots of the interface, for a person or an agent to look at.
//
// `npm run shots` writes them under e2e/.shots. They are not a test: nothing
// passes or fails here, it only makes the interface visible.

import { chromium } from '@playwright/test';
import config from '../playwright.config.ts';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { start } from './server.ts';

const OUT = join(import.meta.dirname, '.shots');

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
  const errors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error' || message.type() === 'warning') {
      errors.push(`${message.type()}: ${message.text()}`);
    }
  });

  await page.goto(server.url);
  await page.getByRole('button', { name: /long: touch two places/ }).click();
  await page.getByRole('button', { name: /src\/long\.c/ }).click();
  await page.screenshot({ path: join(OUT, `${scheme}-unified.png`) });

  await page.getByRole('button', { name: /Side by side|Unified/ }).click();
  await page.screenshot({ path: join(OUT, `${scheme}-split.png`) });

  if (errors.length) {
    console.log(`${scheme}: the page complained\n  ${errors.join('\n  ')}`);
  }
  await page.close();
}

await browser.close();
server.stop();
console.log(`screenshots in ${OUT}`);

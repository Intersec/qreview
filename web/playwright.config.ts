import { existsSync } from 'node:fs';
import { defineConfig } from '@playwright/test';

/*
 * The browser is the one already on the machine. Playwright can download its
 * own, but a 150 MB download in every clone and every pipeline buys nothing
 * here: the interface has to work in the browser the reader uses.
 *
 * QREVIEW_BROWSER names one outright when the guesses below are wrong.
 */
function browser(): string | undefined {
  const guesses = [
    process.env.QREVIEW_BROWSER,
    '/usr/bin/google-chrome',
    '/usr/bin/google-chrome-stable',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/snap/bin/chromium',
  ];

  return guesses.find((path) => path && existsSync(path));
}

const found = browser();

export default defineConfig({
  testDir: './e2e',
  outputDir: './e2e/.results',
  fullyParallel: true,
  workers: 2,
  reporter: process.env.CI ? 'line' : 'list',
  timeout: 30_000,
  expect: { timeout: 5_000 },
  use: {
    // No sandbox: a container runs as root, and the review is local anyway.
    launchOptions: {
      executablePath: found,
      args: ['--no-sandbox', '--disable-dev-shm-usage'],
    },
    viewport: { width: 1600, height: 1000 },
    screenshot: 'only-on-failure',
  },
});

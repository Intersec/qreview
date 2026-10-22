// The page and a server that stops under it.

import { expect, test } from '@playwright/test';
import { start } from './server.ts';

test('a stopped server leaves a banner that says to close the tab', async ({ page }) => {
  const server = await start();
  try {
    await page.goto(server.url);
    await expect(page.getByRole('button', { name: /docs: rename the document/ })).toBeVisible();

    server.kill();

    // A tab that the test opened with a link has history, so the browser
    // refuses to close it, and the banner stays.
    await expect(page.getByRole('alert')).toContainText('The qreview server stopped');
    // Seen from another tab: the title and the icon say it too.
    await expect(page).toHaveTitle(/^⏹ qreview/);
    await expect(page.locator('link[rel="icon"]')).toHaveAttribute(
      'href',
      /^data:image\/svg\+xml,.*%23d93025/,
    );
  } finally {
    server.stop();
  }
});

test('a stopped server closes a tab that a script opened', async ({ page }) => {
  const server = await start();
  try {
    await page.goto('about:blank');
    const opened = page.waitForEvent('popup');
    await page.evaluate((url) => window.open(url), server.url);
    const review = await opened;
    await expect(review.getByRole('button', { name: /docs: rename the document/ })).toBeVisible();

    const closed = review.waitForEvent('close');
    server.kill();

    await closed;
  } finally {
    server.stop();
  }
});

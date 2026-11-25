// The modern layout: the change page of Gerrit, with the diff under its row.
//
// The layout is a preference, and every test here writes a remark or a
// preference, so each test has a server of its own.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { card, openChange } from './act.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.afterEach(() => server?.stop());

test.describe('in the modern layout', () => {
  test.beforeEach(async ({ page }) => {
    server = await start({ gerrit: true, config: { ui: { layout: 'gerrit-like' } } });
    await page.goto(server.url);
    await openChange(page, /net: retry the read/);
  });

  test('the change page holds the info, the message and the series', async ({ page }) => {
    const head = page.locator('.change-head');

    await expect(head.locator('.change-box')).toContainText('12321');
    await expect(head.locator('.change-box')).toContainText('myproject | main');
    await expect(head.locator('.commit-message')).toContainText('net: retry the read');
    // The change being read is the one the arrow marks in the chain.
    await expect(head.locator('.relation-chain li.is-picked')).toContainText('net: retry the read');
    // The series pane of the classic layout is not there.
    await expect(page.locator('.side')).toHaveCount(0);
  });

  test('no file opens by itself, and a row opens its diff under it', async ({ page }) => {
    const row = page.locator('.file-row', { hasText: 'net.blk' });
    await expect(row).toBeVisible();
    await expect(page.locator('.inline-diff')).toHaveCount(0);

    await row.click();
    await expect(row).toHaveAttribute('aria-expanded', 'true');
    await expect(page.locator('.inline-diff .file-bar h2')).toContainText('net.blk');

    // The same row closes it, and the table is whole again.
    await row.click();
    await expect(page.locator('.inline-diff')).toHaveCount(0);
  });

  test('Maximize folds the change page away, and Restore brings it back', async ({ page }) => {
    await page.locator('.file-row', { hasText: 'net.blk' }).click();
    await page.getByRole('button', { name: 'Maximize' }).click();

    await expect(page.locator('.change-page')).toHaveCount(0);
    await expect(page.locator('.diff-slot .file-bar h2')).toContainText('net.blk');

    await page.getByRole('button', { name: 'Restore' }).click();
    await expect(page.locator('.change-page')).toBeVisible();
  });

  test('a remark in the Comments tab opens its file under its row', async ({ page }) => {
    await page.getByRole('tab', { name: /^Comments/ }).click();
    await expect(page.getByText('No comments yet.')).toBeVisible();

    await page.getByRole('tab', { name: 'Files' }).click();
    const row = page.locator('.file-row', { hasText: 'net.blk' });
    await row.click();
    await page.locator('.inline-diff td.gutter-comment').first().click();
    await page.getByRole('textbox').first().fill('Down here.');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(card(page, 'Down here.')).toBeVisible();
    await row.click();
    await expect(page.locator('.inline-diff')).toHaveCount(0);

    await page.getByRole('tab', { name: /^Comments/ }).click();
    await page.locator('.comment-list .list-row', { hasText: 'Down here.' }).click();

    await expect(page.getByRole('tab', { name: 'Files' })).toHaveAttribute('aria-selected', 'true');
    await expect(
      page.locator('.inline-diff').getByRole('article').filter({ hasText: 'Down here.' }),
    ).toBeVisible();
  });
});

test('the layout is picked in the preferences, and kept', async ({ page }) => {
  server = await start();
  await page.goto(server.url);
  await expect(page.locator('.side')).toBeVisible();

  await page.getByRole('button', { name: '⚙' }).click();
  await page.getByRole('dialog').getByLabel('Layout').selectOption('gerrit-like');
  await page.getByRole('button', { name: 'Save', exact: true }).click();

  await expect(page.locator('.change-page')).toBeVisible();
  await expect(page.locator('.side')).toHaveCount(0);

  await page.reload();
  await expect(page.locator('.change-page')).toBeVisible();

  const stored = readFileSync(join(server.fixture.config, 'qreview', 'config.json'), 'utf8');
  expect(JSON.parse(stored).ui.layout).toBe('gerrit-like');
});

// The change of a colleague, section 5.9 of the design: the reader reviews
// it and never fixes it, so Fix leaves it alone and a remark is for Gerrit.

import { execFileSync } from 'node:child_process';
import { expect, test } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { build } from './fixture.ts';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeEach(async ({ page }) => {
  // The fixture commits as author@example.com: another reader makes every
  // change the change of a colleague.
  const fixture = build();
  execFileSync('git', ['config', 'user.email', 'reader@example.com'], { cwd: fixture.repo });
  server = await start({ gerrit: true, on: fixture });
  await page.goto(server.url);
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
});

test.afterEach(() => server?.stop());

test('the change of a colleague has no Fix, and a remark starts in qreview', async ({ page }) => {
  await expect(page.getByRole('button', { name: /^Fix this change/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /^Fix the series/ })).toBeVisible();

  await page.locator('td.gutter-comment[data-column="new"]').first().click();
  await expect(page.locator('form.talk-box').getByLabel('For Gerrit')).not.toBeChecked();
});

test('the export asks for help with the review, not for a fix', () => {
  server.agent(['comment', 'src/net.blk', '--body', 'Why a loop?']);

  expect(server.agent(['export'])).toContain('Help me');
  const json = JSON.parse(server.agent(['export', '--json'])) as {
    changes: { own: boolean }[];
  };
  expect(json.changes.length).toBeGreaterThan(0);
  expect(json.changes.every((change) => change.own === false)).toBe(true);
});

test('a remark of the agent on it is staged for Gerrit without the flag', async ({ page }) => {
  const remark = JSON.parse(
    server.agent(['comment', 'src/net.blk:new:1', '--body', 'Why a loop?']),
  ) as {
    gerrit?: { state: string };
  };
  expect(remark.gerrit?.state).toBe('staged');

  await page.reload();
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  await expect(page.getByRole('button', { name: 'Send to Gerrit · 1' })).toBeVisible();
});

test('without Gerrit, a remark of the agent on it stays in qreview', async () => {
  server.stop();
  const fixture = build();
  execFileSync('git', ['config', 'user.email', 'reader@example.com'], { cwd: fixture.repo });
  server = await start({ on: fixture });

  const remark = JSON.parse(
    server.agent(['comment', 'src/net.blk:new:1', '--body', 'Why a loop?']),
  ) as {
    gerrit?: unknown;
  };
  expect(remark.gerrit ?? null).toBeNull();
});

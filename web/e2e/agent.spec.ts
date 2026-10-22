// What the top bar says about the agent.

import { expect, test } from '@playwright/test';
import { start, type Running } from './server.ts';

let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start();
  await page.goto(server.url);
  await expect(page.getByRole('button', { name: /long: touch two places/ })).toBeVisible();
});
test.afterEach(() => server?.stop());

test('the top bar names the thread the agent answers, and opens it', async ({ page }) => {
  const api = (path: string) => new URL(path, server.url).href;
  const session = (await (await page.request.get(api('/api/session'))).json()) as {
    series: { changes: { key: string; subject: string }[] };
  };
  const long = session.series.changes.find((c) => c.subject.startsWith('long:'));
  expect(long).toBeDefined();
  const comments = api(`/api/changes/${long?.key}/comments`);
  const remark = (await (
    await page.request.post(comments, {
      data: {
        scope: 'line',
        file: 'src/long.c',
        side: 'new',
        startLine: 1,
        body: 'Read the loop again: it never ends.',
      },
    })
  ).json()) as { id: string };
  // Written past the page, which reads it on a load, as one it wrote.
  await page.reload();

  // A wait of the agent, so the server counts one as listening. A number
  // the server never gave answers at once.
  await page.request.get(api('/api/events?after=999999999&who=agent'));
  await expect(page.getByRole('status').filter({ hasText: 'Agent connected' })).toBeVisible();

  await page.request.post(`${comments}/${remark.id}/ask`);
  const chip = page.getByRole('status').filter({ hasText: 'Agent answering' });
  await expect(chip).toHaveText('Agent answering “Read the loop again: it never ends.”…');

  await chip.click();
  await expect(page.getByText('Read the loop again: it never ends.').last()).toBeVisible();
  await expect(page).toHaveURL(/file=src\/long\.c/);
});

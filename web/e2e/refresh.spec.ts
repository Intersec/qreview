// The refresh button: the series catches up with the repository.

import { execFileSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { start, type Running } from './server.ts';

// A repository of its own for each test: these commit into the fixture, and
// a commit one test leaves behind is a commit the next one reads.
let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start();
  await page.goto(server.url);
});
test.afterEach(() => server?.stop());

function git(args: string[]) {
  execFileSync('git', args, {
    cwd: server.fixture.repo,
    encoding: 'utf8',
    env: { ...process.env, LC_ALL: 'C' },
  });
}

function write(path: string, content: string) {
  writeFileSync(join(server.fixture.repo, path), content);
}

function refresh(page: Page) {
  return page.getByRole('button', { name: 'Read the repository again' }).click();
}

test('a commit made since the page loaded joins the series', async ({ page }) => {
  await expect(page.getByRole('button', { name: /docs: rename the document/ })).toBeVisible();

  write('src/later.c', 'int later(void)\n{\n    return 1;\n}\n');
  git(['add', '-A']);
  git(['commit', '-m', 'later: one commit more']);

  await refresh(page);

  await expect(page.getByRole('button', { name: /later: one commit more/ })).toBeVisible();
  await expect(page.getByRole('button', { name: /docs: rename the document/ })).toBeVisible();
});

test('an amend brings the new code under the same change', async ({ page }) => {
  await openChange(page, /docs: rename the document/);
  await openFile(page, 'new-name.md');
  await expect(page.locator('td.code-cell.row-add')).toContainText('And one more line.');

  write('docs/new-name.md', '# A document\n\nIt has words in it.\nAnd a better line.\n');
  git(['add', '-A']);
  git(['commit', '--amend', '--no-edit']);

  await refresh(page);

  // The reader is left on the change and the file that were on the screen.
  await expect(page.locator('.file-bar h2')).toContainText('new-name.md');
  await expect(page.locator('td.code-cell.row-add')).toContainText('And a better line.');
});

test('a commit that is gone leaves the reader on the newest change', async ({ page }) => {
  await openChange(page, /docs: rename the document/);
  await expect(page.locator('.change-bar')).toContainText('docs: rename the document');

  git(['reset', '--hard', 'HEAD~1']);

  await refresh(page);

  await expect(page.getByRole('button', { name: /docs: rename the document/ })).toHaveCount(0);
  await expect(page.locator('.change-bar')).toContainText('long: touch two places far apart');
});

// `qreview <gerrit-url>`, section 6.9 of the design: a change of Gerrit is
// fetched and reviewed on the server of the worktree, which the commands of
// the agent reach as usual.

import { execFileSync, type ChildProcess } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { openChange, openFile } from './act.ts';
import { build, type Fixture } from './fixture.ts';
import { BINARY, spawnOn } from './server.ts';

const ADDRESS = 'https://review.example.com/c/myproject/+/12321/1';

let fixture: Fixture;
let change: ChildProcess | undefined;
let env: Record<string, string | undefined>;

test.beforeEach(() => {
  fixture = build();
  // No test asks whether a newer release is out.
  mkdirSync(join(fixture.config, 'qreview'), { recursive: true });
  writeFileSync(
    join(fixture.config, 'qreview', 'config.json'),
    JSON.stringify({ update: { url: '' } }),
  );
  env = {
    XDG_STATE_HOME: fixture.state,
    XDG_CONFIG_HOME: fixture.config,
    NO_COLOR: '1',
    PATH: `${fixture.bin}:${process.env.PATH}`,
  };
});

test.afterEach(() => {
  change?.kill('SIGTERM');
  change = undefined;
  fixture?.remove();
});

function run(args: string[]): string {
  return execFileSync(BINARY, args, {
    cwd: fixture.repo,
    encoding: 'utf8',
    env: { ...process.env, ...env },
  });
}

test('an address of Gerrit opens the patch set it names, on the server of the worktree', async ({
  page,
}) => {
  const opened = await spawnOn(fixture.repo, [ADDRESS, '--no-open', '--port', '0'], env);
  change = opened.child;

  await page.goto(opened.url);
  await openChange(page, /net: retry the read/);
  await openFile(page, 'net.blk');
  // Patch set 1 lives on the server alone: it was fetched.
  await expect(page.locator('td.code-cell.row-add[data-line="3"]')).toContainText(
    'return recv(fd);',
  );

  // A second start in the worktree finds the server that runs.
  expect(run([ADDRESS, '--no-open'])).toContain('qreview already runs on this worktree');

  // The agent reaches it as usual, and the export comes from it.
  const written = JSON.parse(run(['comment', 'src/net.blk', '--body', 'Why recv?'])) as {
    id: string;
  };
  expect(written.id).toBeTruthy();
  expect(run(['export'])).toContain('Why recv?');
  const json = JSON.parse(run(['export', '--json'])) as {
    changes: { threads: { id: string }[] }[];
  };
  expect(json.changes[0]!.threads.map((t) => t.id)).toContain(written.id);
});

test('an address of a change that Gerrit does not know stops with the error', () => {
  expect(() => run(['https://review.example.com/c/myproject/+/999', '--no-open'])).toThrow(
    /knows no change 999/,
  );
});

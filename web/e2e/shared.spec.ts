// The way in for an agent in a devcontainer: a socket and a copy of the
// address in the git directory of the worktree, which the container shares
// with the machine of the server, when it shares nothing else.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { BINARY, start, type Running } from './server.ts';

let server: Running;
let elsewhere: string;

test.beforeEach(async () => {
  server = await start();
  elsewhere = mkdtempSync(join(tmpdir(), 'qreview-container-'));
});
test.afterEach(() => {
  server?.stop();
  rmSync(elsewhere, { recursive: true, force: true });
});

test('an agent with a store of its own reaches the server through the git directory', async () => {
  const dir = join(server.fixture.repo, '.git', 'qreview');
  // The server prints its address before it writes the files.
  await expect.poll(() => existsSync(join(dir, 'server.json'))).toBe(true);
  expect(statSync(dir).mode & 0o777).toBe(0o700);
  expect(statSync(join(dir, 'server.json')).mode & 0o777).toBe(0o600);
  expect(statSync(join(dir, 'agent.sock')).isSocket()).toBe(true);

  // A container: another store, and no address in it.
  const env = { ...process.env, XDG_STATE_HOME: elsewhere, XDG_CONFIG_HOME: elsewhere };
  delete env.GERRIT_HTTP_USER;
  delete env.GERRIT_HTTP_PASSWORD;
  const written = JSON.parse(
    execFileSync(BINARY, ['comment', 'src/long.c', '--body', 'From the container.'], {
      cwd: server.fixture.repo,
      encoding: 'utf8',
      env,
    }),
  ) as { id: string };

  expect(written.id).toBeTruthy();
  // The machine of the server reads it back from its own store.
  const home = {
    ...env,
    XDG_STATE_HOME: server.fixture.state,
    XDG_CONFIG_HOME: server.fixture.config,
  };
  const exported = execFileSync(BINARY, ['export'], {
    cwd: server.fixture.repo,
    encoding: 'utf8',
    env: home,
  });
  expect(exported).toContain('From the container.');
});

// The real binary, on a real repository, for the browser to talk to.

import { spawn, type ChildProcess } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { build, type Fixture } from './fixture.ts';

export interface Running {
  url: string;
  fixture: Fixture;
  stop(): void;
}

const BINARY = join(import.meta.dirname, '..', '..', 'target', 'release', 'qreview');

/// Start qreview on a fresh fixture and wait for the address it prints.
///
/// The token is read from that line rather than forced in: the test walks the
/// same path a person does.
export async function start(
  options: { prev?: boolean; gerrit?: boolean; config?: object } = {},
): Promise<Running> {
  const fixture = build();

  // No test talks to a server. qreview asks its home whether a newer
  // release is out, so every fixture says to ask nobody, and the suite that
  // is about that check names its own address.
  const config = { update: { url: '' }, ...options.config };

  mkdirSync(join(fixture.config, 'qreview'), { recursive: true });
  writeFileSync(join(fixture.config, 'qreview', 'config.json'), JSON.stringify(config, null, 2));
  const args = ['--no-open', '--port', '0'];
  if (!options.gerrit) {
    args.push('--no-gerrit');
  }
  if (options.prev && fixture.previous) {
    args.push('--prev', fixture.previous);
  }

  const child: ChildProcess = spawn(BINARY, args, {
    cwd: fixture.repo,
    env: {
      ...process.env,
      XDG_STATE_HOME: fixture.state,
      XDG_CONFIG_HOME: fixture.config,
      NO_COLOR: '1',
      // The fake ssh answers the query and serves the fetch.
      PATH: options.gerrit ? `${fixture.bin}:${process.env.PATH}` : process.env.PATH,
    },
  });

  const url = await new Promise<string>((resolve, reject) => {
    let seen = '';
    const timer = setTimeout(() => reject(new Error(`qreview said nothing:\n${seen}`)), 20000);

    child.stdout?.on('data', (chunk) => {
      seen += String(chunk);
      const found = seen.match(/http:\/\/127\.0\.0\.1:\d+\/\?t=[a-f0-9]+/);
      if (found) {
        clearTimeout(timer);
        resolve(found[0]);
      }
    });
    child.stderr?.on('data', (chunk) => {
      seen += String(chunk);
    });
    child.on('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`qreview stopped with ${code}:\n${seen}`));
    });
  });

  return {
    url,
    fixture,
    stop: () => {
      child.kill('SIGTERM');
      fixture.remove();
    },
  };
}

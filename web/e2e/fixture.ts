// A real repository for the browser to review.
//
// The same rule as the Rust fixtures: real git commands, a fixed author and
// a fixed clock, and a temporary directory that goes away with the test run.

import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

export interface Fixture {
  repo: string;
  state: string;
  remove(): void;
}

let clock = 0;

function git(repo: string, args: string[]): string {
  return execFileSync('git', args, {
    cwd: repo,
    encoding: 'utf8',
    env: { ...process.env, LC_ALL: 'C' },
  }).trim();
}

function write(repo: string, path: string, content: string) {
  const full = join(repo, path);
  mkdirSync(dirname(full), { recursive: true });
  writeFileSync(full, content);
}

function commit(repo: string, subject: string, trailer?: string) {
  clock += 1;
  const minute = String(clock % 60).padStart(2, '0');
  const date = `2026-01-01T00:${minute}:00+00:00`;
  const message = trailer ? `${subject}\n\n${trailer}\n` : subject;

  git(repo, ['add', '-A']);
  git(repo, ['commit', '--allow-empty', '--date', date, '-m', message]);
}

/// A series with the shapes a reviewer meets: a modified file, a new file, a
/// rename, two hunks far apart, and a merge under the boundary.
export function build(): Fixture {
  const base = mkdtempSync(join(tmpdir(), 'qreview-e2e-'));
  const repo = join(base, 'repo');
  const state = join(base, 'state');
  mkdirSync(repo);
  mkdirSync(state);

  git(repo, ['init', '--quiet', '--initial-branch=main', '.']);
  git(repo, ['config', 'user.name', 'Test Author']);
  git(repo, ['config', 'user.email', 'author@example.com']);
  git(repo, ['config', 'commit.gpgsign', 'false']);
  git(repo, ['config', 'core.autocrlf', 'false']);

  const long = Array.from({ length: 30 }, (_, i) => `line ${i + 1}`).join('\n') + '\n';

  write(repo, 'src/net.blk', 'int connect_once(int fd)\n{\n    return read(fd);\n}\n');
  write(repo, 'src/long.c', long);
  write(repo, 'src/spacing.c', 'int spaced(void)\n{\n    return 1;\n}\n');
  write(repo, 'docs/old-name.md', '# A document\n\nIt has words in it.\n');
  commit(repo, 'base: start the tree');

  // A branch that will be merged, so the walk meets a merge.
  git(repo, ['switch', '--quiet', '-c', 'side']);
  write(repo, 'src/net.blk', 'int connect_once(int fd)\n{\n    return recv(fd);\n}\n');
  commit(repo, 'side: read with recv');

  git(repo, ['switch', '--quiet', 'main']);
  write(repo, 'src/net.blk', 'int connect_once(int fd)\n{\n    return readv(fd);\n}\n');
  commit(repo, 'main: read with readv');

  try {
    git(repo, ['merge', '--no-commit', '--no-ff', 'side']);
  } catch {
    // The conflict is the point of the fixture.
  }
  write(repo, 'src/net.blk', 'int connect_once(int fd)\n{\n    return recvmsg(fd);\n}\n');
  commit(repo, 'Merge branch side into main');

  // The series the review opens on, above the merge.
  write(
    repo,
    'src/net.blk',
    'int connect_once(int fd)\n{\n    for (;;) {\n        recvmsg(fd);\n    }\n}\n',
  );
  commit(repo, 'net: retry the read', 'Change-Id: Iretryreadaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');

  write(
    repo,
    'src/long.c',
    long.replace('line 3\n', 'LINE THREE\n').replace('line 27\n', 'LINE TWENTY SEVEN\n'),
  );
  write(repo, 'src/added.py', 'def hello():\n    return "hi"\n');
  // The only change here is the indentation, so it is what a whitespace
  // switch has to hide.
  write(repo, 'src/spacing.c', 'int spaced(void)\n{\n        return 1;\n}\n');
  commit(
    repo,
    'long: touch two places far apart',
    'Change-Id: Ilongtwohunksaaaaaaaaaaaaaaaaaaaaaaaaaa',
  );

  git(repo, ['mv', 'docs/old-name.md', 'docs/new-name.md']);
  commit(repo, 'docs: rename the document', 'Change-Id: Irenamedocaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');

  return {
    repo,
    state,
    remove: () => rmSync(base, { recursive: true, force: true }),
  };
}

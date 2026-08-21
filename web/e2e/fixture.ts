// A real repository for the browser to review.
//
// The same rule as the Rust fixtures: real git commands, a fixed author and
// a fixed clock, and a temporary directory that goes away with the test run.

import { execFileSync } from 'node:child_process';
import { chmodSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

export interface Fixture {
  repo: string;
  state: string;
  /// Where the preferences are written. Never the ones of the person
  /// running the tests.
  config: string;
  /// An older version of the last change, for `--prev`.
  previous?: string;
  /// A directory holding a fake `ssh`, for the Gerrit tests.
  bin?: string;
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
/// A Gerrit that is a shell script.
///
/// git talks to a server over ssh, and so does the Gerrit query. One fake
/// `ssh` first on the PATH answers both: the query from a recorded file, and
/// the fetch by running git-upload-pack against a bare repository next door.
/// Nothing in the tool changes, and no socket is opened.
function fakeGerrit(base: string, repo: string): { bin: string; served: string } {
  const bare = join(base, 'origin.git');
  const server = join(base, 'server');
  const bin = join(base, 'bin');
  mkdirSync(bin);
  mkdirSync(server);

  execFileSync('git', ['init', '--quiet', '--bare', bare]);

  // A version of the change that lives on the server and nowhere else, so a
  // test can watch it being fetched.
  git(server, ['init', '--quiet', '--initial-branch=main', '.']);
  git(server, ['config', 'user.name', 'Test Author']);
  git(server, ['config', 'user.email', 'author@example.com']);
  write(server, 'src/net.blk', 'int connect_once(int fd)\n{\n    return recv(fd);\n}\n');
  git(server, ['add', '-A']);
  git(server, [
    'commit',
    '--date',
    '2026-01-01T00:00:00+00:00',
    '-m',
    'net: retry the read\n\nChange-Id: Iretryreadaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n',
  ]);
  const served = git(server, ['rev-parse', 'HEAD']);
  git(server, ['push', '--quiet', bare, 'HEAD:refs/changes/21/12321/1']);

  const local = git(repo, ['rev-parse', 'HEAD~2']);
  const answer = {
    project: 'myproject',
    branch: 'main',
    id: 'Iretryreadaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    number: 12321,
    subject: 'net: retry the read',
    url: 'https://review.example.com/c/myproject/+/12321',
    status: 'NEW',
    patchSets: [
      { number: 1, revision: served, ref: 'refs/changes/21/12321/1', createdOn: 1750000000 },
      { number: 2, revision: local, ref: 'refs/changes/21/12321/2', createdOn: 1750001000 },
    ],
  };
  writeFileSync(
    join(bin, 'query.json'),
    `${JSON.stringify(answer)}\n{"type":"stats","rowCount":1}\n`,
  );

  const ssh = join(bin, 'ssh');
  writeFileSync(
    ssh,
    [
      '#!/bin/sh',
      '# A Gerrit that is a shell script. See e2e/fixture.ts.',
      'case "$*" in',
      `  *"gerrit query"*) cat ${JSON.stringify(join(bin, 'query.json'))} ;;`,
      `  *git-upload-pack*) exec git-upload-pack ${JSON.stringify(bare)} ;;`,
      '  *) echo "fake ssh: $*" >&2; exit 1 ;;',
      'esac',
      '',
    ].join('\n'),
  );
  chmodSync(ssh, 0o755);

  git(repo, ['remote', 'add', 'origin', 'ssh://review.example.com:29418/myproject']);

  return { bin, served };
}

export function build(): Fixture {
  const base = mkdtempSync(join(tmpdir(), 'qreview-e2e-'));
  const repo = join(base, 'repo');
  const state = join(base, 'state');
  const config = join(base, 'config');
  mkdirSync(repo);
  mkdirSync(state);
  mkdirSync(config);

  git(repo, ['init', '--quiet', '--initial-branch=main', '.']);
  git(repo, ['config', 'user.name', 'Test Author']);
  git(repo, ['config', 'user.email', 'author@example.com']);
  git(repo, ['config', 'commit.gpgsign', 'false']);
  git(repo, ['config', 'core.autocrlf', 'false']);

  // Long enough that ten lines of context around the two changes still
  // leave a gap between the hunks for the context bar to open.
  const long = Array.from({ length: 60 }, (_, i) => `line ${i + 1}`).join('\n') + '\n';

  write(repo, 'src/net.blk', 'int connect_once(int fd)\n{\n    return read(fd);\n}\n');
  write(repo, 'src/long.c', long);
  write(repo, 'src/spacing.c', 'int spaced(void)\n{\n    return 1;\n}\n');
  // A doc comment, because its scope is `comment.block.documentation` and
  // that once reached the page as a class that broke the line in two.
  write(
    repo,
    'src/doc.h',
    '/** Return whether the field is a pointer or not.\n *\n * \\param[in] fdesc the field description\n * \\return true when it is a pointer\n */\nint is_pointer(int fdesc);\n',
  );
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
    'int connect_once(int fd)\n{\n    for (;;) {\n        recv(fd);\n    }\n}\n',
  );
  commit(repo, 'net: retry the read', 'Change-Id: Iretryreadaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');

  // A second version of that change, so the patch set selectors have two
  // things to choose between. The first version stays reachable by its hash.
  const previous = git(repo, ['rev-parse', 'HEAD']);
  write(
    repo,
    'src/net.blk',
    'int connect_once(int fd)\n{\n    for (;;) {\n        recvmsg(fd);\n    }\n}\n',
  );
  git(repo, ['add', '-A']);
  git(repo, ['commit', '--amend', '--no-edit']);

  write(
    repo,
    'src/long.c',
    long.replace('line 3\n', 'LINE THREE\n').replace('line 50\n', 'LINE FIFTY\n'),
  );
  write(repo, 'src/added.py', 'def hello():\n    return "hi"\n');
  // The only change here is the indentation, so it is what a whitespace
  // switch has to hide.
  write(repo, 'src/spacing.c', 'int spaced(void)\n{\n        return 1;\n}\n');
  write(
    repo,
    'src/doc.h',
    '/** Return whether the field is a pointer or not.\n *\n * \\param[in] fdesc the field description\n * \\return true when it is a pointer\n */\nbool is_pointer(int fdesc);\n',
  );
  commit(
    repo,
    'long: touch two places far apart',
    'Change-Id: Ilongtwohunksaaaaaaaaaaaaaaaaaaaaaaaaaa',
  );

  git(repo, ['mv', 'docs/old-name.md', 'docs/new-name.md']);
  commit(repo, 'docs: rename the document', 'Change-Id: Irenamedocaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');

  const gerrit = fakeGerrit(base, repo);

  return {
    repo,
    state,
    config,
    previous,
    bin: gerrit.bin,
    remove: () => rmSync(base, { recursive: true, force: true }),
  };
}

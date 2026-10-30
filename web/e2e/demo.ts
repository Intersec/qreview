// A recording of a review, for the README.
//
// `make demo` builds a small repository, runs the real binary on it, and
// drives a tour of it in a browser. The script plays the agent too, with the
// commands that a real agent runs. It writes `web/e2e/.demo/demo.webm`, and
// the Makefile turns that into the animated WebP that the README shows.
// Nothing passes or fails here: it is a camera, not a test.
//
// The repository is built here, not taken from the person who records. The
// agent fixes code, so the script must know that code, and the fix rewrites
// commits that must belong to nobody.

import { chromium, request } from '@playwright/test';
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import config from '../playwright.config.ts';
import { BINARY, spawnOn } from './server.ts';

const OUT = join(import.meta.dirname, '.demo');
const SIZE = { width: 1280, height: 800 };

// The repository: a small server of blobs, and a series that puts a cache
// in front of its disk.

const ALICE = { name: 'Alice Martin', email: 'alice@example.com' };
const BRUNO = { name: 'Bruno Costa', email: 'bruno@example.com' };

const CARGO = `[package]
name = "blobd"
version = "0.3.0"
edition = "2021"
`;

const LIB = `pub mod server;
pub mod store;
`;

const STORE = `use std::io;
use std::path::PathBuf;

/// The blobs, one file each under a root directory.
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Store { root }
    }

    pub fn read(&self, key: &str) -> io::Result<Vec<u8>> {
        std::fs::read(self.root.join(key))
    }
}
`;

const SERVER = `use std::io;

use crate::store::Store;

pub struct Server {
    store: Store,
}

impl Server {
    pub fn new(store: Store) -> Self {
        Server { store }
    }

    /// The body of \`GET /<key>\`.
    pub fn get(&mut self, key: &str) -> io::Result<Vec<u8>> {
        self.store.read(key)
    }
}
`;

const README = `# blobd

A small server for blobs kept on the disk.
`;

const CACHE = `use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// The entries used last, up to a fixed count.
pub struct Cache<K, V> {
    capacity: usize,
    map: HashMap<K, V>,
    /// The keys, the one used last first.
    order: VecDeque<K>,
}

impl<K: Hash + Eq + Clone, V> Cache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Cache {
            capacity,
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            self.touch(key);
        }
        self.map.get(key)
    }

    pub fn put(&mut self, key: K, value: V) {
        if !self.map.contains_key(&key) && self.map.len() == self.capacity {
            let oldest = self.order.pop_back().expect("a full cache has an oldest key");
            self.map.remove(&oldest);
        }
        self.touch(&key);
        self.map.insert(key, value);
    }

    /// Move the key to the front of the order.
    fn touch(&mut self, key: &K) {
        self.order.retain(|k| k != key);
        self.order.push_front(key.clone());
    }
}
`;

const COUNTED = `use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// The entries used last, up to a fixed count.
pub struct Cache<K, V> {
    capacity: usize,
    map: HashMap<K, V>,
    /// The keys, the one used last first.
    order: VecDeque<K>,
    stats: Stats,
}

/// How often a lookup found its key.
#[derive(Default)]
pub struct Stats {
    pub hits: u64,
    pub misses: u64,
}

impl Stats {
    /// The share of the lookups that found their key, from 0 to 1.
    pub fn hit_rate(&self) -> f64 {
        self.hits as f64 / (self.hits + self.misses) as f64
    }
}

impl<K: Hash + Eq + Clone, V> Cache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Cache {
            capacity,
            map: HashMap::new(),
            order: VecDeque::new(),
            stats: Stats::default(),
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            self.stats.hits += 1;
            self.touch(key);
        } else {
            self.stats.misses += 1;
        }
        self.map.get(key)
    }

    pub fn put(&mut self, key: K, value: V) {
        if !self.map.contains_key(&key) && self.map.len() == self.capacity {
            let oldest = self.order.pop_back().expect("a full cache has an oldest key");
            self.map.remove(&oldest);
        }
        self.touch(&key);
        self.map.insert(key, value);
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    /// Move the key to the front of the order.
    fn touch(&mut self, key: &K) {
        self.order.retain(|k| k != key);
        self.order.push_front(key.clone());
    }
}
`;

const CACHED_SERVER = `use std::io;
use std::time::{Duration, Instant};

use crate::cache::Cache;
use crate::store::Store;

/// How long a blob read from the disk stays good.
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct Server {
    store: Store,
    cache: Cache<String, (Instant, Vec<u8>)>,
}

impl Server {
    pub fn new(store: Store) -> Self {
        Server {
            store,
            cache: Cache::new(1024),
        }
    }

    /// The body of \`GET /<key>\`.
    pub fn get(&mut self, key: &str) -> io::Result<Vec<u8>> {
        if let Some((read, body)) = self.cache.get(&key.to_owned()) {
            if read.elapsed() < TIMEOUT {
                return Ok(body.clone());
            }
        }
        let body = self.store.read(key)?;
        self.cache.put(key.to_owned(), (Instant::now(), body.clone()));
        Ok(body)
    }
}
`;

const CACHED_README = `# blobd

A small server for blobs kept on the disk.

## Cache

The server keeps the 1024 blobs read last in memory, for 30 seconds each.
\`Cache::stats\` says how often a lookup found its key.
`;

// The lines the remarks are about, and what the agent makes of them.
const NEW = '    pub fn new(capacity: usize) -> Self {\n';
const GET = '    pub fn get(&mut self, key: &K) -> Option<&V> {\n';
const RATE = '        self.hits as f64 / (self.hits + self.misses) as f64\n';
const LIMIT = 'const TIMEOUT: Duration = Duration::from_secs(30);\n';

const CHECKED = `${NEW}        assert!(capacity > 0, "a cache holds at least one entry");\n`;
const GUARDED = `        let lookups = self.hits + self.misses;
        if lookups == 0 {
            return 0.0;
        }
        self.hits as f64 / lookups as f64
`;

const QUESTION = 'Why does get() take &mut self?';
const ANSWER =
  'A hit moves the key to the front of `order`, so even a read changes the cache. ' +
  'A `peek()` that leaves the order alone could take `&self`.';
const REQUEST = 'With a capacity of 0, put() panics on the first insert. Refuse 0 here.';
const EARLIER = [
  {
    subject: 'cache: count the hits',
    file: 'src/cache.rs',
    line: lineOf(COUNTED, RATE),
    body: 'Before the first lookup this is 0 / 0, and the dashboard shows NaN. Return 0.0 then.',
  },
  {
    subject: 'server: answer from the cache',
    file: 'src/server.rs',
    line: lineOf(CACHED_SERVER, LIMIT),
    body: 'Make this timeout configurable.',
  },
];
const ASKED_BACK = 'From a config file, or from a flag? The server reads neither yet.';

interface Commit {
  subject: string;
  body: string;
  author: { name: string; email: string };
  date: string;
  files: Record<string, string>;
}

/// The series, as the author wrote it or as the agent fixed it.
function series(fixed: boolean): Commit[] {
  const fix = (text: string, line: string, by: string) => (fixed ? swap(text, line, by) : text);
  const cache = fix(CACHE, NEW, CHECKED);
  const counted = fix(fix(COUNTED, NEW, CHECKED), RATE, GUARDED);

  return [
    {
      subject: 'cache: add an LRU cache with a fixed capacity',
      body: 'Each read goes to the disk. A cache keeps the entries read last.',
      author: ALICE,
      date: '2026-10-27T09:12:00+01:00',
      files: { 'src/cache.rs': cache, 'src/lib.rs': `pub mod cache;\n${LIB}` },
    },
    {
      subject: 'cache: count the hits and the misses',
      body: 'A cache too small for the load misses most of the time, and nobody knows.',
      author: ALICE,
      date: '2026-10-27T10:40:00+01:00',
      files: { 'src/cache.rs': counted },
    },
    {
      subject: 'server: answer from the cache before the disk',
      body: 'A blob read in the last 30 seconds comes from memory.',
      author: BRUNO,
      date: '2026-10-28T14:05:00+01:00',
      files: { 'src/server.rs': CACHED_SERVER },
    },
    {
      subject: 'docs: describe the cache',
      body: 'Say what the server keeps in memory, and for how long.',
      author: BRUNO,
      date: '2026-10-28T16:30:00+01:00',
      files: { 'README.md': CACHED_README },
    },
  ];
}

// The state and the settings of the run are thrown away with the recording.
// A demo must not write in the review store of the person recording it.
const spare = mkdtempSync(join(tmpdir(), 'qreview-demo-'));
const repo = join(spare, 'repo');
const env = {
  XDG_STATE_HOME: join(spare, 'state'),
  XDG_CONFIG_HOME: join(spare, 'config'),
  NO_COLOR: '1',
};

build();
// Nothing asks the network whether a newer release is out: the corner of
// the top bar stays the same from one recording to the next.
mkdirSync(join(env.XDG_CONFIG_HOME, 'qreview'), { recursive: true });
writeFileSync(join(env.XDG_CONFIG_HOME, 'qreview', 'config.json'), '{"update":{"url":""}}\n');

const { child, url } = await spawnOn(repo, ['--no-open', '--port', '0', '--no-gerrit'], env);

// An earlier pass of the reader left two remarks.
const api = await request.newContext({ baseURL: url });
await api.get(url);
const session = (await (await api.get('/api/session')).json()) as {
  series: { changes: { key: string; subject: string }[] };
};
for (const remark of EARLIER) {
  const change = session.series.changes.find((c) => c.subject.startsWith(remark.subject));
  await api.post(`/api/changes/${change?.key}/comments`, {
    data: {
      scope: 'line',
      file: remark.file,
      side: 'new',
      startLine: remark.line,
      body: remark.body,
    },
  });
}
await api.dispose();

// The agent listens before the reader comes.
let waiting = agentWait();

rmSync(OUT, { recursive: true, force: true });
mkdirSync(OUT, { recursive: true });

const browser = await chromium.launch({
  executablePath: config.use?.launchOptions?.executablePath,
  args: ['--no-sandbox', '--disable-dev-shm-usage'],
});
const context = await browser.newContext({
  viewport: SIZE,
  colorScheme: 'dark',
  recordVideo: { dir: OUT, size: SIZE },
});
// Copy the series writes to the clipboard, and a refusal would put an error
// in the picture.
await context.grantPermissions(['clipboard-read', 'clipboard-write'], {
  origin: new URL(url).origin,
});
const page = await context.newPage();
// The video starts with the page, and the page starts empty.
const filmed = Date.now();

/// Let the picture rest, so an eye can follow what just happened.
const beat = (ms = 900) => page.waitForTimeout(ms);

async function openChange(subject: string) {
  await page
    .getByRole('button', { name: new RegExp(subject) })
    .first()
    .click();
  await beat();
}

async function openFile(name: string) {
  const row = page.locator('.file-row', { hasText: name });
  await row.first().waitFor();
  await row.first().click();
  await beat();
}

/// Open a comment box on a line of the new version, and type in it.
async function remark(line: number, body: string, save: 'Save' | 'Save and ask') {
  const row = page.locator('tr', {
    has: page.locator(`td.gutter[data-column="new"]:text-is("${line}")`),
  });
  await row.locator('td.gutter-comment[data-column="new"]').first().click();

  const box = page.getByRole('textbox').first();
  await box.waitFor();
  await box.pressSequentially(body, { delay: 35 });
  await beat(500);
  await page.getByRole('button', { name: save, exact: true }).click();
}

await page.goto(url);
await page.locator('.file-row').first().waitFor();
await page.getByText(EARLIER[1].body).first().waitFor();
// The empty page and the first reads are cut: the README plays the
// recording in a loop, and the loop starts here.
writeFileSync(join(OUT, 'start'), `${(Date.now() - filmed) / 1000}\n`);
// The series, with the author and the size of each change, and the remarks
// of the earlier pass at the foot of the pane.
await beat(1800);

await openChange('cache: add an LRU cache');
await openFile('cache.rs');
await beat(600);

// A question, handed to the agent as it is saved. The top bar says what the
// agent answers, and the reply lands in the thread.
await remark(lineOf(CACHE, GET), QUESTION, 'Save and ask');
const asked = await waiting;
await beat(1800);
agent(['reply', eventOf(asked, 'ask').id, '--body', ANSWER]);
waiting = agentWait(asked.next);
await page.getByText('so even a read changes the cache').first().waitFor();
await beat(2200);

// A request, kept for the fix.
await remark(lineOf(CACHE, NEW), REQUEST, 'Save');
await beat(1400);

// One fix for every change: the dialog lists what the agent will deal with.
await page.getByRole('button', { name: /Fix the series/ }).click();
await beat(2800);
await page.keyboard.press('Enter');
const fix = await waiting;
await beat(2400);
fixTheSeries();
// The next wait of the agent ends the fix, and the page shows it at once.
waiting = agentWait(fix.next);
await page.getByText('a cache holds at least one entry').first().waitFor();
await beat(2600);

// The one request the agent did not understand waits for the reader.
await openChange('server: answer from the cache');
await openFile('server.rs');
await page.getByText(ASKED_BACK).first().waitFor();
await beat(2600);

// The review, in the clipboard, for a session with an agent.
await page.getByRole('button', { name: /Copy the series/ }).click();
await beat(1600);

await context.close();
await browser.close();
waiting.stop();
child.kill('SIGTERM');
rmSync(spare, { recursive: true, force: true });

// Playwright names a video after the page. One recording, one name.
const [made] = readdirSync(OUT).filter((name) => name.endsWith('.webm'));
renameSync(join(OUT, made), join(OUT, 'demo.webm'));
console.log(`the recording is in ${join(OUT, 'demo.webm')}`);

/// The base of the series on `main`, and the series on `cache`, which
/// tracks `main` so that qreview finds where the series starts.
function build() {
  mkdirSync(repo, { recursive: true });
  git(['init', '--quiet', '--initial-branch=main', '.']);
  git(['config', 'commit.gpgsign', 'false']);
  git(['config', 'core.autocrlf', 'false']);
  // The top bar names the remote, and the directory of the run otherwise.
  git(['remote', 'add', 'origin', 'https://git.example.com/blobd.git']);

  commit(
    {
      subject: 'store: read the blobs from the disk',
      body: 'The first version: no cache, one file per blob.',
      author: BRUNO,
      date: '2026-10-20T11:00:00+01:00',
      files: {
        'Cargo.toml': CARGO,
        'README.md': README,
        'src/lib.rs': LIB,
        'src/store.rs': STORE,
        'src/server.rs': SERVER,
      },
    },
    '2026-10-20T11:00:00+01:00',
  );
  git(['switch', '--quiet', '-c', 'cache', '--track', 'main']);
  for (const each of series(false)) {
    commit(each, each.date);
  }
}

/// What the agent does on a fix of the series: fix the two clear requests,
/// ask about the third, and declare each new commit.
///
/// A real agent makes a fixup commit per commit and runs one rebase. The
/// script knows the result, so it writes the fixed series at once.
function fixTheSeries() {
  const threads = (
    JSON.parse(agent(['export', '--json'])) as {
      changes: { threads: { id: string; comments: { body: string }[] }[] }[];
    }
  ).changes.flatMap((change) => change.threads);
  const idOf = (body: string) => {
    const found = threads.find((t) => t.comments[0]?.body === body);
    if (!found) {
      throw new Error(`no open thread says ${body}`);
    }
    return found.id;
  };

  const before = git(['rev-list', '--reverse', 'main..cache']).split('\n');
  git(['reset', '--quiet', '--hard', 'main']);
  for (const each of series(true)) {
    commit(each, '2026-10-30T11:20:00+01:00');
  }
  const after = git(['rev-list', '--reverse', 'main..cache']).split('\n');

  const settled = [
    ['--done', idOf(REQUEST)],
    ['--done', idOf(EARLIER[0].body)],
    ['--blocked', `${idOf(EARLIER[1].body)}=${ASKED_BACK}`],
    [],
  ];
  before.forEach((from, i) => {
    agent(['migrate', '--from', from, '--to', after[i], ...settled[i]]);
  });
}

function commit(each: Commit, committed: string) {
  for (const [path, text] of Object.entries(each.files)) {
    const full = join(repo, path);
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(full, text);
  }
  const id = createHash('sha1').update(each.subject).digest('hex');
  git(['add', '-A']);
  git(['commit', '--quiet', '-m', `${each.subject}\n\n${each.body}\n\nChange-Id: I${id}\n`], {
    GIT_AUTHOR_NAME: each.author.name,
    GIT_AUTHOR_EMAIL: each.author.email,
    GIT_AUTHOR_DATE: each.date,
    GIT_COMMITTER_NAME: each.author.name,
    GIT_COMMITTER_EMAIL: each.author.email,
    GIT_COMMITTER_DATE: committed,
  });
}

function git(args: string[], extra: Record<string, string> = {}): string {
  return execFileSync('git', args, {
    cwd: repo,
    encoding: 'utf8',
    env: { ...process.env, LC_ALL: 'C', ...extra },
  }).trim();
}

/// A command of the agent, run the way the agent runs it.
function agent(args: string[]): string {
  return execFileSync(BINARY, args, {
    cwd: repo,
    encoding: 'utf8',
    env: { ...process.env, ...env },
  });
}

interface Waited {
  events: { kind: string; id: string }[];
  next: number;
}

/// `qreview wait`, in the background, as the agent holds it. The wait
/// outlives the server it waits on, so the end of the run stops it.
function agentWait(after?: number): Promise<Waited> & { stop(): void } {
  const args = [
    'wait',
    '--timeout',
    '300',
    ...(after === undefined ? [] : ['--after', `${after}`]),
  ];
  const waiter = spawn(BINARY, args, { cwd: repo, env: { ...process.env, ...env } });
  let out = '';
  waiter.stdout.on('data', (chunk) => (out += String(chunk)));
  const done = new Promise<Waited>((resolve, reject) => {
    waiter.on('exit', (code) =>
      code === 0 ? resolve(JSON.parse(out) as Waited) : reject(new Error(`wait: ${code} ${out}`)),
    );
  });
  const stop = () => {
    // Stopped on purpose: its end is no failure.
    done.catch(() => undefined);
    waiter.kill('SIGTERM');
  };
  return Object.assign(done, { stop });
}

function eventOf(waited: Waited, kind: string) {
  const found = waited.events.find((e) => e.kind === kind);
  if (!found) {
    throw new Error(`the agent waited for ${kind}, and got ${JSON.stringify(waited)}`);
  }
  return found;
}

/// The number of the line that `line` is, in `text`.
function lineOf(text: string, line: string): number {
  const at = text.indexOf(line);
  if (at < 0) {
    throw new Error(`no line ${line.trim()}`);
  }
  return text.slice(0, at).split('\n').length;
}

/// `text` with `line` replaced, where the agent fixes it.
function swap(text: string, line: string, by: string): string {
  if (!text.includes(line)) {
    throw new Error(`no line ${line.trim()}`);
  }
  return text.replace(line, by);
}

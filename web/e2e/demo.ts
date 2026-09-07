// A recording of a review, for the README.
//
// `make demo REPO=<path>` runs the real binary on a real repository and
// drives a tour of it in a browser. It writes `web/e2e/.demo/demo.webm`, and
// the Makefile turns that into the GIF the README shows. Nothing passes or
// fails here: it is a camera, not a test.
//
// The repository is yours to choose, and what the tour shows depends on it.
// A branch of five or six commits with real diffs makes the best picture.

import { chromium, type Page } from '@playwright/test';
import { mkdirSync, renameSync, rmSync, mkdtempSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import config from '../playwright.config.ts';
import { spawnOn } from './server.ts';

const OUT = join(import.meta.dirname, '.demo');
const SIZE = { width: 1280, height: 800 };

const repo = process.argv[2];
if (!repo) {
  throw new Error('usage: demo.ts <repository> [--base <rev>]');
}

// The state and the settings of the run are thrown away with the recording.
// A demo must not write in the review store of the person recording it.
const spare = mkdtempSync(join(tmpdir(), 'qreview-demo-'));

const { child, url } = await spawnOn(repo, ['--no-open', '--port', '0', ...process.argv.slice(3)], {
  XDG_STATE_HOME: join(spare, 'state'),
  XDG_CONFIG_HOME: join(spare, 'config'),
  NO_COLOR: '1',
});

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
const page = await context.newPage();

/// Let the picture rest, so an eye can follow what just happened.
const beat = (ms = 900) => page.waitForTimeout(ms);

async function change(nth: number) {
  await page.locator('.change-row').nth(nth).click();
  await beat();
}

async function file(name: string) {
  const row = page.locator('.file-row', { hasText: name });
  await row.first().waitFor();
  await row.first().click();
  await beat();
}

await page.goto(url);
await page.locator('.change-row').first().waitFor();
await beat(1400);

// The series, and the first change of it.
await change(0);
await file('qvector.rs');
await beat(1200);

// A remark on a line, written the way a reader writes it. It lands in the
// list of the session, bottom left.
await write(page, 'Is the capacity kept when the vector is cleared?');
await beat(1400);

// The lines between two hunks, opened. The second bar and not the first:
// the first one sits at the top of the file, on the licence header.
await page
  .getByRole('button', { name: /common lines/ })
  .nth(1)
  .click();
await beat(1400);

// Another change of the series, with its own files.
await change(2);
await beat(1000);
await file('qhashset.rs');
await beat(1400);

// The review, in the clipboard, for a session with an agent.
await page.getByRole('button', { name: /Copy the series/ }).click();
await beat(1600);

await context.close();
await browser.close();
child.kill('SIGTERM');
rmSync(spare, { recursive: true, force: true });

// Playwright names a video after the page. One recording, one name.
const [made] = readdirSync(OUT).filter((name) => name.endsWith('.webm'));
renameSync(join(OUT, made), join(OUT, 'demo.webm'));
console.log(`the recording is in ${join(OUT, 'demo.webm')}`);

/// Open a comment box on a line of the diff and type in it.
async function write(page: Page, body: string) {
  const gutter = page.locator('td.gutter-comment').nth(6);
  await gutter.click();

  const box = page.getByRole('textbox').first();
  await box.waitFor();
  await box.pressSequentially(body, { delay: 35 });
  await beat();
  await page.getByRole('button', { name: 'Save' }).click();
}

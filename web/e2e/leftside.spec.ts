// A comment on the left side: the version of the file before the change.

import { expect, test, type Page } from '@playwright/test';
import { card, openChange, openFile, useSplit, useUnified } from './act.ts';
import { start, type Running } from './server.ts';

// A server for each test. Every test writes a comment, and a comment that
// one test leaves behind is a card the next one reads.
let server: Running;

test.beforeEach(async ({ page }) => {
  server = await start();
  await page.goto(server.url);
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');
});

test.afterEach(() => server?.stop());

/// The numbers of the old column that offer to take a comment.
function left(page: Page) {
  return page.locator('td.gutter-comment[data-column="old"]');
}

/// The row a number of the old column stands on.
function rowOf(page: Page, line: string) {
  return page.locator('tr', {
    has: page.locator(`td.gutter[data-column="old"]:text-is("${line}")`),
  });
}

test('every line of the left side offers its number', async ({ page }) => {
  await useSplit(page);

  // A deleted line, and a line the change did not touch. Both stand in the
  // version before the change, so both take a remark.
  await expect(left(page)).toHaveCount(await page.locator('td.gutter[data-column="old"]').count());
  await expect(left(page).first()).toHaveText('1');
});

test('a deleted line takes a comment, under the left column', async ({ page }) => {
  await useSplit(page);

  // long.c replaces `line 3`, so line 3 of the old side is deleted.
  await rowOf(page, '3').locator('td.gutter-comment[data-column="old"]').click();
  await page.getByRole('textbox').first().fill('This line was doing something.');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(card(page, 'This line was doing something.')).toBeVisible();

  // A comment sits under the side it was written on, so the left cell of the
  // row carries it and the right one stays empty.
  const talk = page.locator('tr.talk', { hasText: 'This line was doing something.' }).first();
  await expect(talk.locator('td')).toHaveCount(2);
  await expect(talk.locator('td').nth(0)).toContainText('This line was doing something.');
  await expect(talk.locator('td').nth(1)).not.toContainText('This line was doing something.');
});

test('a line the change did not touch takes one on the left too', async ({ page }) => {
  await useSplit(page);

  await rowOf(page, '5').locator('td.gutter-comment[data-column="old"]').click();
  await expect(page.getByRole('textbox').first()).toBeVisible();
  await page.getByRole('textbox').first().fill('It read better before.');
  await page.getByRole('button', { name: 'Save' }).click();

  const talk = page.locator('tr.talk', { hasText: 'It read better before.' }).first();
  await expect(talk.locator('td').nth(0)).toContainText('It read better before.');
  await expect(talk.locator('td').nth(1)).not.toContainText('It read better before.');
});

test('the two sides of one line hold two remarks', async ({ page }) => {
  await useSplit(page);

  const row = rowOf(page, '5');
  await row.locator('td.gutter-comment[data-column="old"]').click();
  await page.getByRole('textbox').first().fill('On the left.');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(card(page, 'On the left.')).toBeVisible();

  await row.locator('td.gutter-comment[data-column="new"]').click();
  await page.getByRole('textbox').first().fill('On the right.');
  await page.getByRole('button', { name: 'Save' }).click();

  const talk = page.locator('tr.talk', { hasText: 'On the left.' }).first();
  await expect(talk.locator('td').nth(0)).toContainText('On the left.');
  await expect(talk.locator('td').nth(1)).toContainText('On the right.');
});

test('the unified view offers the old number too', async ({ page }) => {
  await useUnified(page);

  // The new side of a deleted line has no number, so the old one is the only
  // place to click.
  await rowOf(page, '3').locator('td.gutter-comment[data-column="old"]').click();
  await page.getByRole('textbox').first().fill('Gone, and it should not be.');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(card(page, 'Gone, and it should not be.')).toBeVisible();
});

test('a comment on the left comes back after a reload', async ({ page }) => {
  await useSplit(page);

  await rowOf(page, '3').locator('td.gutter-comment[data-column="old"]').click();
  await page.getByRole('textbox').first().fill('Written on the left.');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(card(page, 'Written on the left.')).toBeVisible();

  await page.reload();
  await openChange(page, /long: touch two places/);
  await openFile(page, 'long.c');

  // It is anchored on the base, and stands on its line rather than at the
  // top of the file, which is where a remark with no line goes.
  await expect(card(page, 'Written on the left.')).toBeVisible();
  await expect(page.locator('.talk-stranded')).toHaveCount(0);
});

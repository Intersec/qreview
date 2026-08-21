// The few moves every suite makes, in one place.
//
// The interface says a file by its name, under the directory it lives in, so
// a test asks for it the way a reader sees it.

import { expect, type Page } from '@playwright/test';

export async function openChange(page: Page, subject: RegExp | string) {
  const change = page.getByRole('button', { name: subject });
  await change.first().waitFor();
  await change.first().click();
}

/// Put the diff in the side by side view, whatever it was in.
///
/// The view is a preference now, so it survives from one test to the next.
export async function useSplit(page: Page) {
  const toggle = page.getByRole('button', { name: /^(Unified|Side by side)$/ });
  await toggle.waitFor();
  if ((await toggle.textContent())?.trim() === 'Side by side') {
    await toggle.click();
  }
  await expect(page.getByRole('button', { name: 'Unified' })).toBeVisible();
}

export async function openFile(page: Page, name: string) {
  const row = page.locator('.file-row', { hasText: name });
  await row.first().waitFor();
  await row.first().click();
  await expect(page.locator('.file-bar h2')).toContainText(name);
}

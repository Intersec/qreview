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

export async function openFile(page: Page, name: string) {
  const row = page.locator('.file-row', { hasText: name });
  await row.first().waitFor();
  await row.first().click();
  await expect(page.locator('.file-bar h2')).toContainText(name);
}

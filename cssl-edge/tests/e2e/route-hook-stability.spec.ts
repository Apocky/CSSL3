import { expect, test } from '@playwright/test';

test('@mobile Clearing privacy client transition preserves hook order', async ({ page }) => {
  const hookErrors: string[] = [];
  const record = (message: string): void => {
    if (/Rendered (?:more|fewer) hooks|Minified React error #(300|310)/i.test(message)) {
      hookErrors.push(message);
    }
  };

  page.on('pageerror', (error) => record(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') record(message.text());
  });

  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: null }),
  }));

  await page.goto('/clearing', { waitUntil: 'domcontentloaded' });
  await expect(page.getByRole('main', { name: 'The Clearing public room' })).toBeVisible();

  await page.getByRole('button', { name: 'Privacy', exact: true }).click();

  await expect(page).toHaveURL(/\/legal\/privacy$/);
  await expect(page.getByRole('heading', { level: 1, name: 'Privacy policy', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'This page hit an error', exact: true })).toHaveCount(0);

  await page.goBack();
  await expect(page).toHaveURL(/\/clearing(?:\?.*)?$/);
  await expect(page.getByRole('main', { name: 'The Clearing public room' })).toBeVisible();

  expect(hookErrors).toEqual([]);
});

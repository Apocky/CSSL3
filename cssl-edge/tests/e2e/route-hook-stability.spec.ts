import { expect, test } from '@playwright/test';

test('diagnostics consent keeps hook order across compact route transitions', async ({ page }) => {
  const browserErrors: string[] = [];
  page.on('pageerror', (error) => browserErrors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') browserErrors.push(message.text());
  });

  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: null }),
  }));

  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await expect(page.getByRole('heading', { name: /relay for minds/i })).toBeVisible();

  await page.getByRole('link', { name: /open the relay/i }).click();
  await expect(page).toHaveURL(/\/apocrypha$/);
  await expect(page.getByRole('heading', { name: 'Speak plainly.' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'This page hit an error' })).toHaveCount(0);

  await page.getByRole('link', { name: 'Apocky home' }).click();
  await expect(page).toHaveURL(/\/$/);
  await expect(page.getByRole('heading', { name: /relay for minds/i })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'This page hit an error' })).toHaveCount(0);
  expect(browserErrors.filter((message) => /Rendered fewer hooks|Minified React error #300/.test(message))).toEqual([]);
});

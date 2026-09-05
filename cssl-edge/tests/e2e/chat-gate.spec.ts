import { expect, test } from '@playwright/test';

test('signed-out private page provides no access funnel or simulated persona', async ({ page }) => {
  await page.goto('/chat');

  await expect(page.getByRole('heading', { level: 1 })).toContainText(/this page is private/i);
  await expect(page.getByText(/not a public conversation service, access request, registration path/i)).toBeVisible();
  await expect(page.getByRole('link', { name: 'Return to apocky.com' })).toHaveAttribute('href', '/');
  await expect(page.getByRole('link', { name: 'Sign in' })).toHaveCount(0);
  await expect(page.locator('video, canvas, [data-avatar-visible="true"]')).toHaveCount(0);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
});

test('chat gate disables ambient motion when reduced motion is requested', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/chat');
  const animation = await page.locator('.chat-atmosphere').evaluate((element) => getComputedStyle(element).animationName);
  expect(animation).toBe('none');
});

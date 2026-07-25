import { expect, test } from '@playwright/test';

test('signed-out chat doorway explains access without displaying a persona', async ({ page }) => {
  await page.goto('/chat');

  await expect(page.getByRole('heading', { level: 1 })).toContainText(/private digital presence/i);
  await expect(page.getByRole('link', { name: 'Sign in' })).toHaveAttribute('href', '/login?next=%2Fchat');
  await expect(page.getByText(/no avatar is (authorized|shown)|display authority/i)).toBeVisible();
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

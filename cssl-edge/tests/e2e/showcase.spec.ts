import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test('showcase is accessible, responsive, and uses an iPhone-safe portrait player', async ({ page }, testInfo) => {
  await page.goto('/showcase', { waitUntil: 'domcontentloaded' });

  await expect(page.getByRole('heading', { level: 1, name: /Two doors/ })).toBeVisible();
  await expect(page.getByText('Illustrative concept art, not product photography.')).toBeVisible();

  const player = page.locator('video');
  await expect(player).not.toHaveAttribute('controls', /.*/);
  await expect(player).toHaveAttribute('playsinline', '');
  await expect(player).toHaveAttribute('preload', 'metadata');
  await expect(player).not.toHaveAttribute('autoplay', /.*/);
  await expect(page.locator('track[kind="captions"]')).toHaveAttribute('src', /\.vtt$/);
  await expect(page.getByRole('group', { name: 'Video controls' })).toBeVisible();

  const startsPortrait = testInfo.project.name !== 'desktop-chrome';
  await expect(player).toHaveAttribute('src', startsPortrait ? /vertical-23s-v1\.mp4$/ : /landscape-23s-v1\.mp4$/);

  await page.getByRole('button', { name: 'Landscape 16:9' }).click();
  await expect(page.locator('video')).toHaveAttribute('src', /landscape-23s-v1\.mp4$/);
  await page.getByRole('button', { name: 'Portrait 9:16' }).click();
  await expect(page.locator('video')).toHaveAttribute('src', /vertical-23s-v1\.mp4$/);

  await page.getByRole('button', { name: 'Play video' }).click();
  await expect(page.getByRole('button', { name: 'Pause video' })).toBeVisible();
  await page.getByRole('button', { name: 'Pause video' }).click();
  await expect(page.getByRole('button', { name: 'Play video' })).toBeVisible();

  await expect(page.getByRole('link', { name: /Explore the Atlas/ }).first()).toHaveAttribute('href', '/atlas');
  await expect(page.getByRole('link', { name: /Begin a free reading/ }).first()).toHaveAttribute('href', /chaos-tarot\.com\/free-reading/);
  await expect(page.getByRole('link', { name: /See membership and support/ })).toHaveAttribute('href', '/membership');

  const overflow = await page.evaluate(() => Math.max(0, document.documentElement.scrollWidth - window.innerWidth));
  expect(overflow).toBeLessThanOrEqual(1);

  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations.filter((item) => ['serious', 'critical'].includes(item.impact ?? ''))).toEqual([]);
});

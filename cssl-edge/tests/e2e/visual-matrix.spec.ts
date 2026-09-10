import fs from 'node:fs';
import path from 'node:path';

import { expect, test } from '@playwright/test';

const VIEWPORTS = [
  { width: 2560, height: 1080 },
  { width: 1440, height: 900 },
  { width: 834, height: 1112 },
  { width: 390, height: 844 },
  { width: 320, height: 568 },
] as const;

const ROUTES = [
  { slug: 'home', href: '/' },
  { slug: 'login', href: '/login?next=%2Fchat' },
  { slug: 'register', href: '/register?next=%2Fchat' },
  { slug: 'callback-error', href: '/auth/callback?next=%2Fchat' },
  { slug: 'account', href: '/account' },
] as const;

test('@visual critical routes remain coherent across the release viewport matrix', async ({ page }) => {
  test.setTimeout(120_000);
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

  const artifactRoot = path.join(process.cwd(), 'test-results', 'visual-matrix');
  fs.mkdirSync(artifactRoot, { recursive: true });

  await page.setViewportSize(VIEWPORTS[1]);
  await page.goto('/');
  await page.screenshot({ path: path.join(artifactRoot, '1440x900-consent.png'), fullPage: false });
  const off = page.getByRole('radio', { name: /^Off\b/i });
  if (await off.isVisible()) {
    await off.click();
    await page.getByRole('button', { name: 'Save choice: Off' }).click();
  }

  for (const viewport of VIEWPORTS) {
    await page.setViewportSize(viewport);
    for (const route of ROUTES) {
      await page.goto(route.href, { waitUntil: 'domcontentloaded' });
      await page.locator('main').first().waitFor({ state: 'visible' });
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect(overflow, `${route.href} overflowed at ${viewport.width}x${viewport.height}`).toBeLessThanOrEqual(1);
      await page.screenshot({
        path: path.join(artifactRoot, `${viewport.width}x${viewport.height}-${route.slug}.png`),
        fullPage: true,
      });
    }
  }

  expect(browserErrors, browserErrors.join('\n')).toEqual([]);
});

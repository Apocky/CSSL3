import fs from 'node:fs';
import path from 'node:path';

import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

const VIEWPORTS = [
  { width: 320, height: 568 },
  { width: 390, height: 844 },
  { width: 568, height: 320 },
  { width: 768, height: 1024 },
  { width: 960, height: 720 },
  { width: 1280, height: 720 },
  { width: 1440, height: 900 },
] as const;

function collectBrowserErrors(page: import('@playwright/test').Page) {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  return errors;
}

async function expectNoSeriousAccessibilityFindings(page: import('@playwright/test').Page) {
  const result = await new AxeBuilder({ page }).analyze();
  const findings = result.violations.filter((entry) => entry.impact === 'serious' || entry.impact === 'critical');
  expect(findings, JSON.stringify(findings, null, 2)).toEqual([]);
}

test('@visual native hub stays horizontally bounded and keeps primary routes reachable', async ({ page }) => {
  test.setTimeout(120_000);
  const errors = collectBrowserErrors(page);
  const artifactRoot = path.join(process.cwd(), 'test-results', 'public-route-matrix');
  fs.mkdirSync(artifactRoot, { recursive: true });

  for (const viewport of VIEWPORTS) {
    await page.setViewportSize(viewport);
    await page.goto('/', { waitUntil: 'domcontentloaded' });
    await expect(page).toHaveTitle(/digital commons/i);
    await expect(page.getByRole('heading', { level: 1, name: /A place for minds, systems, and the worlds between them/i })).toBeVisible();
    await expect(page.getByRole('link', { name: 'Meet Apocrypha', exact: true }).first()).toBeVisible();
    await expect(page.locator('main').getByRole('link', { name: /CSSL.*Visit CSSL/i })).toBeVisible();

    if (viewport.width <= 660) {
      await expect(page.locator('summary', { hasText: 'Explore' })).toBeVisible();
    } else {
      await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toBeVisible();
    }

    const horizontalOverflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(horizontalOverflow, `horizontal overflow at ${viewport.width}x${viewport.height}`).toBeLessThanOrEqual(1);

    await page.screenshot({
      path: path.join(artifactRoot, `${viewport.width}x${viewport.height}-hub.png`),
      fullPage: true,
    });
  }

  await expectNoSeriousAccessibilityFindings(page);
  expect(errors, errors.join('\n')).toEqual([]);
});

test('Atlas explains every context indicator in human language', async ({ page }) => {
  const errors = collectBrowserErrors(page);
  await page.goto('/atlas');

  await expect(page.getByRole('heading', { level: 2, name: /Six doors/i })).toBeVisible();
  const firstContext = page.locator('.coordinate-strip').first();
  for (const label of ['People', 'Meaning', 'Visibility', 'Time']) {
    await expect(firstContext.getByText(label, { exact: false })).toBeVisible();
  }
  await expect(page.getByText(/X ·|Y ·|Z ·|T ·/)).toHaveCount(0);
  const artifactRoot = path.join(process.cwd(), 'test-results', 'public-route-matrix');
  fs.mkdirSync(artifactRoot, { recursive: true });
  await page.screenshot({ path: path.join(artifactRoot, '1440x900-atlas.png'), fullPage: true });
  await expectNoSeriousAccessibilityFindings(page);
  expect(errors, errors.join('\n')).toEqual([]);
});

test('Clearing resolves as the live React social room without route aliasing', async ({ page }) => {
  const errors = collectBrowserErrors(page);
  await page.route('**/fake-supabase/**', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: '[]',
  }));
  await page.goto('/clearing?room=north-clearing');
  await expect.poll(() => new URL(page.url()).pathname).toBe('/clearing');
  await expect(page.locator('main[aria-label="The Clearing public room"]')).toBeVisible();
  await expect(page.getByRole('heading', { level: 1, name: /North Clearing|The Clearing/ })).toBeVisible();
  await expect(page.getByRole('button', { name: /Sign in to join the room|Checking your account/ })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Room actions' })).toBeVisible();
  await expect(page.locator('link[rel="canonical"]')).toHaveAttribute('href', 'https://www.apocky.com/clearing');
  await expect(page.getByText('SAMPLE', { exact: true })).toHaveCount(0);
  await expectNoSeriousAccessibilityFindings(page);
  expect(errors, errors.join('\n')).toEqual([]);
});

test('new public routes and retained application routes resolve together', async ({ request }) => {
  for (const route of [
    '/',
    '/atlas',
    '/clearing',
    '/membership',
    '/principles',
    '/chat',
    '/account',
    '/auth/callback',
    '/download',
  ]) {
    const response = await request.get(route);
    expect(response.status(), route).toBeLessThan(400);
  }
});

test('legacy Commons hub forwards to the native React homepage', async ({ page }) => {
  await page.goto('/commons');
  await expect.poll(() => new URL(page.url()).pathname).toBe('/');
  await expect(page.getByRole('heading', { level: 1, name: /A place for minds, systems, and the worlds between them/i })).toBeVisible();
});

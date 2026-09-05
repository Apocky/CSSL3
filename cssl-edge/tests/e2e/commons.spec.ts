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

test('@visual Spatial Commons stays viewport-bound and keeps primary controls reachable', async ({ page }) => {
  test.setTimeout(120_000);
  const errors = collectBrowserErrors(page);
  const artifactRoot = path.join(process.cwd(), 'test-results', 'commons-matrix');
  fs.mkdirSync(artifactRoot, { recursive: true });

  for (const viewport of VIEWPORTS) {
    await page.setViewportSize(viewport);
    await page.goto('/', { waitUntil: 'domcontentloaded' });
    await expect(page.getByRole('heading', { level: 1, name: 'North Clearing' })).toBeVisible();
    await expect(page.getByLabel('Write a local prototype message')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Open the complete path index' })).toBeVisible();

    if (viewport.width <= 900) {
      await expect(page.getByRole('button', { name: /Chat/i }).first()).toBeVisible();
      await expect(page.getByRole('button', { name: /Context/i }).first()).toBeVisible();
      await expect(page.getByRole('button', { name: /Rooms/i }).first()).toBeVisible();
    } else {
      await expect(page.getByRole('toolbar', { name: 'Context views' })).toBeVisible();
    }

    const overflow = await page.evaluate(() => ({
      horizontal: document.documentElement.scrollWidth - window.innerWidth,
      vertical: document.documentElement.scrollHeight - window.innerHeight,
    }));
    expect(overflow.horizontal, `horizontal overflow at ${viewport.width}x${viewport.height}`).toBeLessThanOrEqual(1);
    expect(overflow.vertical, `document scroll at ${viewport.width}x${viewport.height}`).toBeLessThanOrEqual(1);

    await page.screenshot({
      path: path.join(artifactRoot, `${viewport.width}x${viewport.height}-commons.png`),
      fullPage: false,
    });
  }

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
  const artifactRoot = path.join(process.cwd(), 'test-results', 'commons-matrix');
  fs.mkdirSync(artifactRoot, { recursive: true });
  await page.screenshot({ path: path.join(artifactRoot, '1440x900-atlas.png'), fullPage: true });
  await expectNoSeriousAccessibilityFindings(page);
  expect(errors, errors.join('\n')).toEqual([]);
});

test('Clearing Context branches, route origin, and local-only composer behave coherently', async ({ page }) => {
  const errors = collectBrowserErrors(page);
  const effectRequests: string[] = [];
  page.on('request', (request) => {
    if (!['GET', 'HEAD', 'OPTIONS'].includes(request.method())) effectRequests.push(`${request.method()} ${request.url()}`);
  });

  await page.goto('/clearing');
  await page.locator('[data-context-open]:visible').first().click();
  await expect(page.locator('[data-context-sheet]')).toHaveAttribute('aria-hidden', 'false');

  const routes = {
    People: /\/membership\?from=clearing&origin=river-interval&axis=people$/,
    Meaning: /\/atlas\?from=clearing&origin=river-interval&axis=meaning$/,
    Visibility: /\/principles\?from=clearing&origin=river-interval&axis=visibility$/,
    Time: /#conversation$/,
  } as const;

  for (const [label, route] of Object.entries(routes)) {
    await page.getByRole('button', { name: new RegExp(`^${label}\\b`, 'i') }).click();
    await expect(page.locator('[data-context-label]')).toHaveText(label.toUpperCase());
    await expect(page.locator('[data-context-route]')).toHaveAttribute('href', route);
  }

  await page.locator('[data-context-return]').click();
  const input = page.getByLabel('Write a local sample message');
  await input.fill('A local QA note.');
  await page.getByRole('button', { name: /Send/i }).click();
  await expect(page.getByText('A local QA note.')).toBeVisible();
  expect(effectRequests, effectRequests.join('\n')).toEqual([]);
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

test('legacy root sign-in callbacks forward to the dedicated auth route', async ({ page }) => {
  await page.route('**/auth/callback**', (route) => route.fulfill({
    status: 200,
    contentType: 'text/html',
    body: '<!doctype html><title>Callback handoff</title><p>Callback route reached.</p>',
  }));
  await page.goto('/?code=sentinel-code&next=%2Faccount');
  await expect.poll(() => new URL(page.url()).pathname).toBe('/auth/callback');
  expect(new URL(page.url()).search).toBe('?code=sentinel-code&next=%2Faccount');
});

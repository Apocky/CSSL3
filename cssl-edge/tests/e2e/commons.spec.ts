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
  const unexpectedSupportRequests: string[] = [];
  page.on('request', (request) => {
    const hostname = new URL(request.url()).hostname;
    if (hostname === 'ko-fi.com' || hostname.endsWith('.ko-fi.com') || hostname === 'patreon.com' || hostname.endsWith('.patreon.com')) {
      unexpectedSupportRequests.push(request.url());
    }
  });
  const artifactRoot = path.join(process.cwd(), 'test-results', 'public-route-matrix');
  fs.mkdirSync(artifactRoot, { recursive: true });

  for (const viewport of VIEWPORTS) {
    await page.setViewportSize(viewport);
    await page.goto('/', { waitUntil: 'domcontentloaded' });
    await expect(page).toHaveTitle(/creative works and projects/i);
    await expect(page.getByRole('heading', { level: 1, name: /Worlds, languages, symbols, and living systems/i })).toBeVisible();
    await expect(page.getByRole('link', { name: /Meet Apocrypha/i }).first()).toBeVisible();
    await expect(page.locator('main').getByRole('link', { name: /CSSL.*Visit CSSL/i })).toBeVisible();
    const supportSection = page.locator('main').getByRole('region', { name: 'Help sustain the work.' });
    await expect(supportSection).toBeVisible();
    await expect(supportSection).toContainText(/never required.*does not buy control/i);
    const koFi = supportSection.getByRole('link', { name: /Support on Ko-fi/i });
    const patreon = supportSection.getByRole('link', { name: /Support on Patreon/i });
    await expect(koFi).toHaveAttribute('href', 'https://ko-fi.com/oneinfinity');
    await expect(patreon).toHaveAttribute('href', 'https://www.patreon.com/0ne1nfinity');
    for (const supportLink of [koFi, patreon]) {
      await expect(supportLink).toBeVisible();
      await expect(supportLink).toHaveAttribute('target', '_blank');
      await expect(supportLink).toHaveAttribute('rel', 'noopener noreferrer');
      const hitArea = await supportLink.boundingBox();
      expect(hitArea?.width ?? 0).toBeGreaterThanOrEqual(44);
      expect(hitArea?.height ?? 0).toBeGreaterThanOrEqual(44);
    }

    if (viewport.width <= 1080) {
      const mobileMenu = page.locator('details.apx-mobile-menu');
      await expect(mobileMenu.locator('summary', { hasText: 'Explore' })).toBeVisible();
      await mobileMenu.locator('summary').click();
      await expect(mobileMenu.getByRole('link', { name: 'Support the work' })).toBeVisible();
      await mobileMenu.locator('summary').click();
    } else {
      await expect(page.getByRole('navigation', { name: 'Primary navigation' }).getByRole('link', { name: 'Support the work' })).toBeVisible();
    }

    const horizontalOverflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(horizontalOverflow, `horizontal overflow at ${viewport.width}x${viewport.height}`).toBeLessThanOrEqual(1);

    await page.screenshot({
      path: path.join(artifactRoot, `${viewport.width}x${viewport.height}-hub.png`),
      fullPage: true,
    });
  }

  const supportSection = page.locator('main').getByRole('region', { name: 'Help sustain the work.' });
  const supportDetails = supportSection.getByRole('link', { name: /Read how support works/i });
  const koFi = supportSection.getByRole('link', { name: /Support on Ko-fi/i });
  const patreon = supportSection.getByRole('link', { name: /Support on Patreon/i });
  await supportDetails.focus();
  await expect(supportDetails).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(koFi).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(patreon).toBeFocused();

  await expectNoSeriousAccessibilityFindings(page);
  expect(errors, errors.join('\n')).toEqual([]);
  expect(unexpectedSupportRequests, 'support providers must not load before a visitor intentionally follows a link').toEqual([]);
});

test('Atlas explains every context indicator in human language', async ({ page }) => {
  const errors = collectBrowserErrors(page);
  await page.goto('/atlas');

  await expect(page.getByRole('heading', { level: 2, name: /Seven doors/i })).toBeVisible();
  const akashicDoor = page.locator('a.atlas-entry[href="/akashic-records"]');
  await expect(akashicDoor).toBeVisible();
  await expect(akashicDoor.getByRole('heading', { name: 'Akashic Records' })).toBeVisible();
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
    '/akashic-records',
    '/atlas',
    '/clearing',
    '/membership',
    '/principles',
    '/chat',
    '/account',
    '/auth/callback',
    '/download',
    '/buy',
  ]) {
    const response = await request.get(route);
    expect(response.status(), route).toBeLessThan(400);
  }
});

test('legacy Commons hub forwards to the native React homepage', async ({ page }) => {
  await page.goto('/commons');
  await expect.poll(() => new URL(page.url()).pathname).toBe('/');
  await expect(page.getByRole('heading', { level: 1, name: /Worlds, languages, symbols, and living systems/i })).toBeVisible();
});

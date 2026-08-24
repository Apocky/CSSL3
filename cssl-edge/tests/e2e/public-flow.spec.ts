import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

async function expectNoHorizontalOverflow(page: import('@playwright/test').Page) {
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
}

async function expectNoSeriousAccessibilityFindings(page: import('@playwright/test').Page) {
  const result = await new AxeBuilder({ page }).analyze();
  const findings = result.violations.filter((entry) => entry.impact === 'serious' || entry.impact === 'critical');
  expect(findings, JSON.stringify(findings, null, 2)).toEqual([]);
}

test('home remains a useful creative-work entry point', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveTitle(/creative works and projects/i);
  await expect(page.getByRole('heading', { level: 1, name: /Worlds, languages, symbols, and living systems/i })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Explore the work', exact: true })).toHaveAttribute('href', '#projects');
  await expect(page.getByRole('link', { name: /Enter the Clearing/ }).first()).toHaveAttribute('href', '/clearing');
  await expect(page.locator('body')).not.toContainText(/apocrypha/i);
  await expect(page.locator('a[href^="/apoc"], a[href="/chat"], a[href="/apx"]')).toHaveCount(0);
  await expect(page.locator('main')).toHaveCount(1);
  await expect(page.locator('h1')).toHaveCount(1);
  await expectNoHorizontalOverflow(page);
  await expectNoSeriousAccessibilityFindings(page);
});

test('ordinary applications remain reachable while retired web paths are neutral 404s', async ({ request }) => {
  for (const route of ['/clearing', '/account', '/atlas']) {
    const response = await request.get(route);
    expect(response.status(), route).toBeLessThan(400);
  }

  for (const route of [
    '/apoc',
    '/apocrypha',
    '/apx',
    '/chat',
    '/apocrypha-manifest.json',
    '/api/apocrypha/presence',
    '/api/admin/apocv4/health',
    '/api/cron/apocrypha-sms',
  ]) {
    const response = await request.get(route, { maxRedirects: 0 });
    expect(response.status(), route).toBe(404);
    expect(await response.text(), route).toBe('');
    expect(response.headers()['location'], route).toBeUndefined();
  }
});

test('@mobile mobile navigation remains discoverable and unclipped', async ({ page }) => {
  await page.goto('/');
  const explore = page.locator('summary', { hasText: 'Explore' });
  await expect(explore).toBeVisible();
  await explore.click();
  const menu = page.getByRole('group', { name: 'Explore Apocky on mobile' });
  await expect(menu.getByRole('link', { name: 'The Clearing' })).toBeVisible();
  await expect(menu).not.toContainText(/apocrypha/i);
  await expectNoHorizontalOverflow(page);
});

test('machine-readable discovery surfaces omit retired service routes', async ({ request }) => {
  const [llms, manifest, schema, robots, sitemap, retiredManifest] = await Promise.all([
    request.get('/llms.txt'),
    request.get('/.well-known/apocky.json'),
    request.get('/schemas/site-manifest.v1.json'),
    request.get('/robots.txt'),
    request.get('/sitemap.xml'),
    request.get('/apocrypha-manifest.json'),
  ]);

  for (const response of [llms, manifest, schema, robots, sitemap]) expect(response.ok()).toBeTruthy();
  expect(retiredManifest.status()).toBe(404);
  const manifestBody = await manifest.json();
  expect(manifestBody.declared_release_state).toBe('public');
  expect(JSON.stringify(manifestBody)).not.toMatch(/apocrypha|\/chat/i);
  expect(JSON.stringify(manifestBody.entry_points)).toContain('/clearing');
  expect(await llms.text()).not.toMatch(/apocrypha/i);
  expect(await llms.text()).toContain('https://www.apocky.com/clearing');
  expect(await robots.text()).toContain('Disallow: /admin/');
  const sitemapBody = await sitemap.text();
  expect(sitemapBody).toContain('https://www.apocky.com/atlas');
  expect(sitemapBody).toContain('https://www.apocky.com/clearing');
  expect(sitemapBody).not.toMatch(/apocrypha|\/admin|\/content/i);
});

test('legacy Commons entry redirects to the native hub', async ({ page }) => {
  await page.goto('/commons');
  await expect.poll(() => new URL(page.url()).pathname).toBe('/');
  await expect(page.getByRole('heading', { level: 1, name: /Worlds, languages, symbols, and living systems/i })).toBeVisible();
});

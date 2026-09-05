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

test('home provides a truthful, conversation-first entry flow', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveTitle(/Spatial Commons/);
  await expect(page.getByRole('heading', { level: 1, name: 'North Clearing' })).toBeVisible();
  await expect(page.getByText('SAMPLE', { exact: true })).toBeVisible();
  await expect(page.getByRole('status', { name: /Authored interaction sample/i })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open live chat' })).toHaveAttribute('href', '/chat');
  await expect(page.getByRole('link', { name: 'Open your account' })).toHaveAttribute('href', '/account');
  await expect(page.getByRole('link', { name: /Open room/i })).toHaveAttribute('href', '/clearing#north');
  await expect(page.locator('main')).toHaveCount(1);
  await expect(page.locator('h1')).toHaveCount(1);
  await expectNoHorizontalOverflow(page);
  await expectNoSeriousAccessibilityFindings(page);
});

test('live application and account routes stay directly reachable', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('link', { name: 'Open live chat' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open your account' })).toBeVisible();
});

test('@mobile mobile navigation remains discoverable and unclipped', async ({ page }) => {
  await page.goto('/');
  const context = page.getByRole('button', { name: /Context/i }).first();
  await expect(context).toBeVisible();
  await context.click();
  await expect(page.getByRole('toolbar', { name: 'Context views' })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Quick destinations' }).getByRole('link', { name: 'Atlas' })).toBeVisible();
  await expectNoHorizontalOverflow(page);
  const verticalOverflow = await page.evaluate(() => document.documentElement.scrollHeight - window.innerHeight);
  expect(verticalOverflow).toBeLessThanOrEqual(1);
});

test('machine-readable discovery surfaces agree', async ({ request }) => {
  const [llms, manifest, schema, robots, sitemap] = await Promise.all([
    request.get('/llms.txt'),
    request.get('/.well-known/apocky.json'),
    request.get('/schemas/site-manifest.v1.json'),
    request.get('/robots.txt'),
    request.get('/sitemap.xml'),
  ]);

  for (const response of [llms, manifest, schema, robots, sitemap]) expect(response.ok()).toBeTruthy();
  const manifestBody = await manifest.json();
  expect(manifestBody.declared_release_state).toBe('public');
  expect(JSON.stringify(manifestBody)).not.toContain('/chat');
  expect(await llms.text()).toMatch(/does not\s+define or classify them/i);
  expect(await llms.text()).toContain('https://www.apocky.com/clearing');
  expect(await robots.text()).toContain('Disallow: /admin/');
  const sitemapBody = await sitemap.text();
  expect(sitemapBody).toContain('https://www.apocky.com/atlas');
  expect(sitemapBody).toContain('https://www.apocky.com/clearing');
  expect(sitemapBody).not.toContain('/admin');
  expect(sitemapBody).not.toContain('/content');
});

test('unavailable community demonstrations are absent rather than populated with examples', async ({ request }) => {
  for (const route of ['/content', '/gear-share', '/run-share-feed']) {
    const response = await request.get(route);
    expect(response.status(), route).toBe(404);
  }
});

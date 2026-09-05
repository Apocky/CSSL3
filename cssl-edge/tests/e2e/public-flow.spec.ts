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
  await expect(page).toHaveTitle(/digital commons/i);
  await expect(page.getByRole('heading', { level: 1, name: /A place for minds, systems, and the worlds between them/i })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Meet Apocrypha', exact: true }).first()).toHaveAttribute('href', '/apocrypha');
  await expect(page.getByRole('link', { name: 'Choose another door' })).toHaveAttribute('href', '#doorways');
  await expect(page.getByRole('link', { name: /Enter the Clearing/ }).first()).toHaveAttribute('href', '/clearing');
  await expect(page.getByText('SAMPLE', { exact: true })).toHaveCount(0);
  await expect(page.locator('main')).toHaveCount(1);
  await expect(page.locator('h1')).toHaveCount(1);
  await expectNoHorizontalOverflow(page);
  await expectNoSeriousAccessibilityFindings(page);
});

test('native applications and account routes stay directly reachable', async ({ request }) => {
  for (const route of ['/apocrypha', '/clearing', '/account']) {
    const response = await request.get(route);
    expect(response.status(), route).toBeLessThan(400);
  }
});

test('@mobile mobile navigation remains discoverable and unclipped', async ({ page }) => {
  await page.goto('/');
  const explore = page.locator('summary', { hasText: 'Explore' });
  await expect(explore).toBeVisible();
  await explore.click();
  await expect(page.getByRole('group', { name: 'Explore Apocky on mobile' }).getByRole('link', { name: 'The Clearing' })).toBeVisible();
  await expectNoHorizontalOverflow(page);
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
  expect(JSON.stringify(manifestBody.entry_points)).toContain('/clearing');
  expect(await llms.text()).toMatch(/does not\s+define or classify them/i);
  expect(await llms.text()).toContain('https://www.apocky.com/clearing');
  expect(await robots.text()).toContain('Disallow: /admin/');
  const sitemapBody = await sitemap.text();
  expect(sitemapBody).toContain('https://www.apocky.com/atlas');
  expect(sitemapBody).toContain('https://www.apocky.com/clearing');
  expect(sitemapBody).not.toContain('/admin');
  expect(sitemapBody).not.toContain('/content');
});

test('legacy Commons entry redirects to the native hub', async ({ page }) => {
  await page.goto('/commons');
  await expect.poll(() => new URL(page.url()).pathname).toBe('/');
  await expect(page.getByRole('heading', { level: 1, name: /A place for minds, systems, and the worlds between them/i })).toBeVisible();
});

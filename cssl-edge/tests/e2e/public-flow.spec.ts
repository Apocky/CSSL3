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

test('home provides a truthful, semantic entry flow', async ({ page }) => {
  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: null }),
  }));

  await page.goto('/');
  await expect(page).toHaveTitle(/Apocky/);
  await expect(page.getByRole('heading', { level: 1, name: /many kinds of mind/i })).toBeVisible();
  await expect(page.getByRole('link', { name: /sign in to check access/i })).toHaveAttribute('href', '/chat');
  await expect(page.getByText(/private beta/i).first()).toBeVisible();
  await expect(page.locator('main')).toHaveCount(1);
  await expect(page.locator('h1')).toHaveCount(1);
  await expectNoHorizontalOverflow(page);
  await expectNoSeriousAccessibilityFindings(page);
});

test('owner view changes access language without changing the doorway', async ({ page }) => {
  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: { id: 'owner-fixture' } }),
  }));
  await page.route('**/api/admin/check', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ authorized: true }),
  }));

  await page.goto('/');
  await expect(page.getByRole('link', { name: /continue your conversation/i })).toHaveAttribute('href', '/chat');
  await expect(page.getByRole('link', { name: /your account/i }).first()).toHaveAttribute('href', '/account');
});

test('@mobile mobile navigation remains discoverable and unclipped', async ({ page }) => {
  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: null }),
  }));

  await page.goto('/');
  const menu = page.getByText('Explore', { exact: true });
  await expect(menu).toBeVisible();
  await menu.click();
  await expect(page.getByLabel('Explore Apocky on mobile').getByRole('link', { name: 'Trust' })).toBeVisible();
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
  expect((await manifest.json()).declared_release_state).toBe('private_beta');
  expect(await llms.text()).toMatch(/account alone\s+does not grant conversation/i);
  expect(await robots.text()).toContain('Disallow: /admin/');
  expect(await sitemap.text()).not.toContain('/admin');
});

import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

const routes = ['/', '/download/apocrypha', '/atlas', '/spellcraft', '/account'];
for (const path of routes) {
  test(`compact public shell ${path}`, async ({ page }, info) => {
    test.setTimeout(120000);
    await page.route('**/api/auth/me', route => route.fulfill({ json: { user: null, reason: 'No active account session was found.' } }));
    await page.route('**/api/admin/check', route => route.fulfill({ status: 403, json: { authorized: false } }));
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    const response = await page.goto(path);
    expect(response?.status()).toBe(200);
    await expect(page.getByRole('heading', { level: 1 }).first()).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
    if (path === '/download/apocrypha') {
      await expect(page.getByRole('link', { name: 'Download Android preview', exact: false })).toHaveAttribute('href', /^\/downloads\/Apocrypha-Android-.+\.apk$/);
      await expect(page.getByRole('heading', { name: 'iPhone', exact: true })).toBeVisible();
    }
    await page.screenshot({ path: info.outputPath('compact-page.png'), fullPage: false });
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations.filter(item => ['serious', 'critical'].includes(item.impact ?? ''))).toEqual([]);
    if (info.project.name.startsWith('mobile')) {
      await page.setViewportSize({ width: 320, height: 568 });
      expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
      await page.screenshot({ path: info.outputPath('compact-page-320.png'), fullPage: false });
    }
    expect(errors).toEqual([]);
  });
}

test('@mobile Explore menu closes on Escape, outside press, and route change', async ({ page }) => {
  await page.route('**/api/auth/me', route => route.fulfill({ json: { user: null } }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 403, json: { authorized: false } }));
  await page.goto('/');
  const menu = page.locator('.apx-mobile-menu');
  const opener = menu.locator('summary');
  await opener.click(); await expect(menu).toHaveAttribute('open', '');
  await page.keyboard.press('Escape'); await expect(menu).not.toHaveAttribute('open', '');
  await expect(opener).toBeFocused();
  await opener.click(); await expect(menu).toHaveAttribute('open', '');
  await page.locator('.apx-footer-copy').click(); await expect(menu).not.toHaveAttribute('open', '');
  await opener.click(); await expect(menu).toHaveAttribute('open', '');
  await menu.getByRole('link', { name: 'Atlas', exact: true }).click();
  await expect(page).toHaveURL(/\/atlas$/); await expect(menu).not.toHaveAttribute('open', '');
});

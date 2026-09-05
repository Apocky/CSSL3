import { expect, test, type Page } from '@playwright/test';

// Compact visual-system contract (2026-09). Guards the shared shell + page families against
// regressions in heading scale, hit areas, overflow, and the data-sharing control placement.
const ROUTES = ['/', '/account', '/login', '/membership', '/status', '/quests', '/atlas', '/download/apocrypha', '/docs', '/legal/privacy', '/spellcraft', '/akashic-records'];
const VIEWPORTS = [
  { width: 1440, height: 900 },
  { width: 390, height: 844 },
  { width: 320, height: 568 },
] as const;
const MAX_H1_PX = 72;
const SHELLED = (path: string): boolean => !['/login', '/register', '/apocrypha', '/brain'].includes(path);

async function anonymous(page: Page): Promise<void> {
  await page.route('**/api/auth/me', (route) => route.fulfill({ json: { user: null } }));
  await page.route('**/api/admin/check', (route) => route.fulfill({ status: 403, json: { authorized: false } }));
}

test('compact system holds across routes and viewports', async ({ page }) => {
  test.setTimeout(240_000);
  await anonymous(page);
  for (const viewport of VIEWPORTS) {
    await page.setViewportSize(viewport);
    for (const path of ROUTES) {
      const response = await page.goto(path, { waitUntil: 'load' });
      expect(response?.status(), `${path} status`).toBe(200);
      await page.locator('h1').first().waitFor({ state: 'visible' });
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect(overflow, `${path} @${viewport.width} horizontal overflow`).toBeLessThanOrEqual(1);
      const h1 = await page.evaluate(() => Math.max(...[...document.querySelectorAll('h1')].map((el) => parseFloat(getComputedStyle(el).fontSize))));
      expect(h1, `${path} @${viewport.width} h1 font-size`).toBeLessThanOrEqual(MAX_H1_PX);
      if (SHELLED(path)) {
        const small = await page.evaluate(() => [...document.querySelectorAll('.apx-nav a, .apx-nav button, .apx-nav summary, .apx-footer a, .apx-footer button')]
          .map((el) => ({ label: (el.textContent ?? '').trim().slice(0, 30), height: Math.round(el.getBoundingClientRect().height) }))
          .filter((item) => item.height > 0 && item.height < 40));
        expect(small, `${path} @${viewport.width} shell targets under 40px`).toEqual([]);
        const pillVisible = await page.locator('.apx-diagnostics-opener').isVisible().catch(() => false);
        expect(pillVisible, `${path} @${viewport.width} fixed data-sharing pill must not float over shelled pages`).toBe(false);
      }
    }
  }
});

test('@mobile footer data-sharing control opens the consent dialog and returns focus', async ({ page }) => {
  await anonymous(page);
  await page.setViewportSize({ width: 320, height: 568 });
  await page.goto('/membership', { waitUntil: 'load' });
  const control = page.locator('.apx-footer-consent');
  await control.scrollIntoViewIfNeeded();
  const box = await control.boundingBox();
  expect(box?.height ?? 0).toBeGreaterThanOrEqual(44);
  await control.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog', { name: 'Optional site data' });
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(control).toBeFocused();
});

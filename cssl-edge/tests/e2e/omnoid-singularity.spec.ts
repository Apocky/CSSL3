import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

async function expectNoSeriousAccessibilityFindings(page: Page): Promise<void> {
  const result = await new AxeBuilder({ page }).analyze();
  const findings = result.violations.filter(
    (entry) => entry.impact === 'serious' || entry.impact === 'critical',
  );
  expect(findings, JSON.stringify(findings, null, 2)).toEqual([]);
}

async function expectNoHorizontalOverflow(page: Page): Promise<void> {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
}

test('Omnoid Singularity is source-layered, accessible, and responsive', async ({ page }) => {
  await page.goto('/omnoid-singularity');

  await expect(page).toHaveTitle(/Apocky’s Omnoid Singularity/);
  await expect(page.getByRole('heading', { level: 1, name: /Apocky’s Omnoid Singularity/ })).toBeVisible();
  await expect(page.getByText('Authored cosmology', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('Collaborative notation', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('Established mathematics', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('Open hypothesis', { exact: true }).first()).toBeVisible();
  await expect(page.getByRole('img', { name: /Macro and micro views/ })).toBeVisible();
  await expect(page.getByRole('img', { name: /Twelve-point Lotus/ })).toBeVisible();
  await expect(page.getByRole('img', { name: /Three independent oppositions/ })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Scrollable CSLv3 encoding' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Download the CSLv3 encoding' })).toHaveAttribute('href', '/omnoid-singularity.csl');

  const reducedMotion = await page.evaluate(() => matchMedia('(prefers-reduced-motion: reduce)').matches);
  expect(reducedMotion).toBe(true);
  await expectNoHorizontalOverflow(page);
  await expectNoSeriousAccessibilityFindings(page);
});

test('skip link reaches the shared main-content target', async ({ page }) => {
  await page.goto('/omnoid-singularity');
  const skip = page.getByRole('link', { name: 'Skip to main content' });
  for (let tabCount = 0; tabCount < 5; tabCount += 1) {
    await page.keyboard.press('Tab');
    if (await skip.evaluate((element) => element === document.activeElement)) break;
  }
  await expect(skip).toBeFocused();
  await skip.press('Enter');
  await expect(page.locator('#main-content')).toBeFocused();
});

import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test.describe('symbolic studio public workbench', () => {
  test('Oracle answers an ordinary question and blocks high-stakes authority', async ({ page }) => {
    const externalRequests: string[] = [];
    page.on('request', (request) => {
      if (!request.url().startsWith('http://127.0.0.1') && !request.url().startsWith('http://localhost')) externalRequests.push(request.url());
    });
    await page.goto('/oracle');
    const question = page.getByLabel('Ask one question that can be answered yes or no');
    await question.fill('Should I make the smallest reversible move today?');
    await page.getByRole('button', { name: 'Reveal yes / no' }).click();
    await expect(page.getByLabel('Oracle result')).toContainText(/yes|no/i);
    await expect(page.getByRole('definition').first()).not.toBeEmpty();
    await expect(page.getByLabel('Oracle result')).toContainText('apocky-oracle/1.0.0');
    await expect(page.getByRole('link', { name: 'Ask Chaos Tarot' })).toHaveAttribute('href', 'https://chaos-tarot.com/free-reading?source=apocky-oracle-result');

    await question.fill('Should I change my medication dose?');
    await page.getByRole('button', { name: 'Reveal yes / no' }).click();
    await expect(page.getByText('This oracle does not answer medical, legal, financial, safety, self-harm, surveillance, coercion, or directed-harm decisions.', { exact: false })).toBeVisible();
    expect(externalRequests).toEqual([]);
  });

  test('Spellcraft exposes valid and quarantined compiler states', async ({ page }) => {
    await page.goto('/spellcraft');
    await page.getByRole('button', { name: 'Compile and interpret' }).click();
    await expect(page.getByText('Plain-language reflection')).toBeVisible();
    await expect(page.getByText('executable: no')).toBeVisible();

    await page.getByLabel('Compose a symbolic intention').fill('xqzzy');
    await page.getByRole('button', { name: 'Compile and interpret' }).click();
    await expect(page.getByText('SYMBOLIC_UNKNOWN_QUARANTINED')).toBeVisible();
  });

  test('Sigil variants are visible, deterministic artifacts', async ({ page }) => {
    await page.goto('/sigils');
    await page.getByRole('button', { name: 'Validate and render' }).click();
    const image = page.getByRole('img', { name: /Generated geometric sigil/ });
    await expect(image).toBeVisible();
    const first = await image.getAttribute('src');
    await page.getByLabel('Visible variant').fill('7');
    await expect(image).not.toHaveAttribute('src', first ?? '');
  });

  test('Explicit save reaches the local Spellbook and can be deleted', async ({ page }) => {
    await page.goto('/spellcraft');
    await page.getByRole('button', { name: 'Compile and interpret' }).click();
    await page.getByRole('button', { name: 'Save explicitly' }).click();
    await expect(page.getByRole('button', { name: 'Saved to this browser' })).toBeVisible();
    await page.goto('/spellbook');
    await expect(page.getByText('1 working')).toBeVisible();
    await page.getByRole('button', { name: 'Delete', exact: true }).click();
    await expect(page.getByText('0 workings')).toBeVisible();
  });

  test('Atlas matrix is URL-backed and includes the new public routes', async ({ page }) => {
    await page.goto('/atlas?view=matrix');
    await expect(page.getByRole('button', { name: 'Matrix' })).toHaveAttribute('aria-pressed', 'true');
    await expect(page.getByRole('table', { name: 'Public destinations by kind and access state' })).toContainText('Spellcraft');
  });

  test('New public surfaces stay accessible and viewport-bound', async ({ page }) => {
    for (const route of ['/oracle', '/spellcraft', '/sigils', '/spellbook', '/atlas?view=matrix']) {
      await page.goto(route);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect(overflow, `${route} horizontal overflow`).toBeLessThanOrEqual(1);
      const accessibility = await new AxeBuilder({ page }).include('main').analyze();
      const severe = accessibility.violations.filter((violation) => ['serious', 'critical'].includes(violation.impact ?? ''));
      expect(severe, `${route} serious or critical accessibility findings`).toEqual([]);
    }

    await page.goto('/oracle');
    for (const button of await page.locator('[aria-label="Example questions"] button').all()) {
      expect((await button.boundingBox())?.height ?? 0).toBeGreaterThanOrEqual(44);
    }
    await page.goto('/spellcraft');
    for (const button of await page.locator('[aria-label="Example spells"] button, [aria-label="Vocabulary category"] button').all()) {
      expect((await button.boundingBox())?.height ?? 0).toBeGreaterThanOrEqual(44);
    }
  });
});

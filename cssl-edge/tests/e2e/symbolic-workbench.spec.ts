import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { serializeSpellbook } from '../../lib/spellcraft';

test.describe('symbolic studio public workbench', () => {
  test('Legacy Oracle route permanently hands off to Chaos Tarot', async ({ request }) => {
    const response = await request.get('/oracle', { maxRedirects: 0 });
    expect(response.status()).toBe(308);
    expect(response.headers()['location']).toBe('https://chaos-tarot.com/yes-no?source=apocky-oracle');
  });

  test('Spellcraft offers a useful reflection before optional technical detail', async ({ page }) => {
    await page.goto('/spellcraft');
    await page.getByRole('button', { name: 'Create my spell' }).click();
    const result = page.getByRole('region', { name: 'Your spell' });
    await expect(result).toBeFocused();
    await expect(result.getByRole('heading', { name: 'Clarity', exact: true })).toBeVisible();
    await expect(result.getByText('What is one thing you could understand more clearly today?')).toBeVisible();
    await expect(result.getByText('Program hash', { exact: true })).not.toBeVisible();
    await result.getByText('How it works', { exact: true }).click();
    await expect(result.getByText(/executable: no/)).toBeVisible();

    await page.getByText('Edit symbolic words', { exact: true }).click();
    await page.getByLabel('Symbolic words', { exact: true }).fill('xqzzy');
    await page.getByRole('button', { name: 'Create my spell' }).click();
    await expect(result.getByRole('heading', { name: 'Let’s adjust those words.' })).toBeVisible();
    await expect(result.getByRole('button', { name: 'Save to spellbook' })).toHaveCount(0);
    await result.getByText('Technical details', { exact: true }).click();
    await expect(result.getByText('SYMBOLIC_UNKNOWN_QUARANTINED')).toBeVisible();
    await result.getByRole('button', { name: 'Start with Clarity' }).click();
    await page.getByRole('button', { name: 'Create my spell' }).click();
    await expect(result.getByRole('heading', { name: 'Clarity', exact: true })).toBeVisible();
  });

  test('Every named starting point produces its own reflection', async ({ page }) => {
    await page.goto('/spellcraft');
    for (const name of ['Clarity', 'Boundaries', 'Growth', 'Balance', 'Release', 'Renewal']) {
      await page.getByRole('button', { name: new RegExp(`^${name}`) }).click();
      await page.getByRole('button', { name: 'Create my spell' }).click();
      await expect(page.getByRole('region', { name: 'Your spell' }).getByRole('heading', { name, exact: true })).toBeVisible();
    }
    await page.getByText('Choose your own combination', { exact: true }).click();
    await page.getByRole('combobox', { name: 'I want to', exact: true }).selectOption('protect');
    await page.getByRole('combobox', { name: 'Focus on', exact: true }).selectOption('alg');
    await page.getByRole('button', { name: 'Use this combination' }).click();
    await page.getByRole('button', { name: 'Create my spell' }).click();
    await expect(page.getByRole('region', { name: 'Your spell' }).getByRole('heading', { name: 'Your chosen intention' })).toBeVisible();
  });

  test('A spell can become a downloadable sigil without losing its words', async ({ page }) => {
    await page.goto('/spellcraft');
    await page.getByRole('button', { name: /^Boundaries/ }).click();
    await page.getByRole('button', { name: 'Create my spell' }).click();
    await page.getByRole('button', { name: 'Make a sigil', exact: true }).click();
    await expect(page.getByRole('img', { name: 'A geometric sigil made from your reflection' })).toBeVisible();
    const downloadPromise = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Download image' }).click();
    expect((await downloadPromise).suggestedFilename()).toMatch(/^apocky-sigil-[a-f0-9]{12}\.svg$/);
    await page.getByRole('region', { name: 'Your spell' }).getByText('How it works', { exact: true }).click();
    await expect(page.getByRole('region', { name: 'Your spell' }).getByText('nau zur', { exact: true })).toBeVisible();
  });

  test('Sigil variations change the image and can be downloaded', async ({ page }) => {
    await page.goto('/sigils');
    await page.getByRole('button', { name: 'Make my sigil' }).click();
    const result = page.getByRole('region', { name: 'Your sigil' });
    await expect(result).toBeFocused();
    const image = result.getByRole('img', { name: /Generated geometric sigil/ });
    await expect(image).toBeVisible();
    const first = await image.getAttribute('src');
    await page.getByLabel('Try another shape').fill('7');
    await expect(image).not.toHaveAttribute('src', first ?? '');
    await expect(page.getByText('Variation 8', { exact: true })).toBeVisible();
    await page.getByLabel('Try another shape').fill('0');
    await expect(image).toHaveAttribute('src', first ?? '');
    const downloadPromise = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Download image' }).click();
    expect((await downloadPromise).suggestedFilename()).toMatch(/\.svg$/);
    await page.getByRole('button', { name: 'Save to spellbook' }).click();
    await expect(page.getByRole('status').filter({ hasText: 'The words behind your sigil' })).toBeVisible();
    await page.goto('/spellbook');
    await expect(page.getByRole('heading', { name: '1 saved spell', exact: true })).toBeVisible();
  });

  test('Only an explicit save reaches the local spellbook and can be deleted', async ({ page }) => {
    await page.goto('/spellcraft');
    await page.getByRole('button', { name: 'Create my spell' }).click();
    expect(await page.evaluate(() => window.localStorage.getItem('apocky.symbolic-spellbook.v1'))).toBeNull();
    await page.getByRole('button', { name: 'Save to spellbook' }).click();
    await expect(page.getByText('Saved in this browser.', { exact: true })).toBeVisible();
    await page.goto('/spellbook');
    await expect(page.getByRole('heading', { name: '1 saved spell', exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Delete', exact: true }).click();
    await expect(page.getByRole('heading', { name: '0 saved spells', exact: true })).toBeVisible();
  });

  test('An import blocked by browser storage preserves the collection and reports failure', async ({ page }) => {
    await page.goto('/spellcraft');
    await page.getByRole('button', { name: 'Create my spell' }).click();
    await page.getByRole('button', { name: 'Save to spellbook' }).click();
    await page.goto('/spellbook');
    await expect(page.getByRole('heading', { name: '1 saved spell', exact: true })).toBeVisible();
    await page.evaluate(() => { Storage.prototype.setItem = () => { throw new DOMException('Storage blocked', 'QuotaExceededError'); }; });
    await page.getByLabel('Import verified Spellbook JSON').setInputFiles({ name: 'empty-spellbook.json', mimeType: 'application/json', buffer: Buffer.from(serializeSpellbook([])) });
    await expect(page.getByRole('status').filter({ hasText: 'could not be written' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '1 saved spell', exact: true })).toBeVisible();
    await expect(page.getByText('0 saved spells were imported into this browser.')).toHaveCount(0);
  });

  test('Unavailable storage is explained without pretending the collection is empty', async ({ page }) => {
    await page.addInitScript(() => { Object.defineProperty(window, 'localStorage', { configurable: true, get() { throw new DOMException('Storage blocked', 'SecurityError'); } }); });
    await page.goto('/spellbook');
    await expect(page.getByRole('heading', { name: 'We couldn’t open the saved collection.' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '0 saved spells' })).toHaveCount(0);
    await expect(page.getByRole('link', { name: 'Create a spell', exact: false }).first()).toBeVisible();
  });

  test('Atlas starts with a searchable directory and preserves optional comparison URLs', async ({ page }) => {
    await page.goto('/atlas');
    await expect(page.getByRole('button', { name: 'Directory', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await page.getByLabel('What are you looking for?', { exact: true }).fill('Codex Apockalypsis');
    await expect(page.locator('main a[href="/codex-apockalypsis"]')).toBeVisible();
    await page.goto('/atlas?view=matrix');
    await expect(page.getByRole('button', { name: 'Compare', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await expect(page.getByRole('table', { name: 'Public destinations by kind and access state' })).toContainText('Spellcraft');
  });

  test('Words leads with everyday meanings and searches the full technical collection', async ({ page }) => {
    await page.goto('/words');
    await expect(page.locator('#intention')).toBeVisible();
    await expect(page.locator('#api')).not.toBeVisible();
    await page.getByRole('button', { name: 'Metaphor', exact: true }).click();
    await expect(page.locator('#metaphor')).toContainText('I am carrying a mountain');
    await expect(page.locator('#metaphor a')).toHaveAttribute('href', '/codex-apockalypsis');
    await page.getByLabel('What does it mean?', { exact: true }).fill('API');
    await expect(page.locator('#api')).toBeVisible();
    await page.getByLabel('What does it mean?', { exact: true }).fill('zzzznothing');
    await expect(page.getByRole('heading', { name: 'No matching words yet.' })).toBeVisible();
    await page.getByRole('button', { name: 'See all definitions' }).click();
    await expect(page.locator('#intention')).toBeVisible();
  });

  test('Public tools stay accessible and put primary controls within a phone screen', async ({ page }) => {
    for (const route of ['/spellcraft', '/sigils', '/spellbook', '/atlas', '/atlas?view=matrix', '/words']) {
      await page.goto(route);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect(overflow, `${route} horizontal overflow`).toBeLessThanOrEqual(1);
      const accessibility = await new AxeBuilder({ page }).include('main').analyze();
      const severe = accessibility.violations.filter(violation => ['serious', 'critical'].includes(violation.impact ?? ''));
      expect(severe, `${route} serious or critical accessibility findings`).toEqual([]);
    }
    await page.setViewportSize({ width: 390, height: 844 });
    for (const [route, name] of [['/spellcraft', 'Create my spell'], ['/sigils', 'Make my sigil']] as const) {
      await page.goto(route);
      const button = page.getByRole('button', { name });
      const bounds = await button.boundingBox();
      expect(bounds, `${route} primary control`).not.toBeNull();
      expect((bounds?.y ?? 844) + (bounds?.height ?? 0), `${route} control fits first viewport`).toBeLessThanOrEqual(844);
      expect(bounds?.height ?? 0).toBeGreaterThanOrEqual(44);
    }
  });

  test('Copying a definition sends the selected meaning to the clipboard and confirms it', async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async (text: string) => { (window as Window & { copiedDefinition?: string }).copiedDefinition = text; } } });
    });
    await page.goto('/words?q=metaphor');
    await page.locator('#metaphor').getByRole('button', { name: 'Copy definition' }).click();
    await expect(page.getByText('Definition copied.', { exact: true })).toBeVisible();
    expect(await page.evaluate(() => (window as Window & { copiedDefinition?: string }).copiedDefinition)).toBe('Metaphor: Describing one thing through another to reveal a quality or feeling they share.');
  });
});

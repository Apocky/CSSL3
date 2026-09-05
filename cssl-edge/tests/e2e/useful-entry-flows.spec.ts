import { expect, test } from '@playwright/test';

test('Sign-in and registration place the real form first on narrow screens', async ({ page }) => {
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: '{"user":null}' }));
  for (const viewport of [{ width: 390, height: 844 }, { width: 320, height: 740 }]) {
    await page.setViewportSize(viewport);
    for (const route of ['/login?next=%2Fclearing', '/register?next=%2Fclearing']) {
      await page.goto(route);
      const email = page.getByLabel('Email address', { exact: true });
      await expect(email).toBeVisible();
      const bounds = await email.boundingBox();
      expect((bounds?.y ?? viewport.height) + (bounds?.height ?? 0)).toBeLessThanOrEqual(viewport.height);
      const submit = page.getByRole('button', { name: /Send (sign-in|verification) email/ });
      const submitBounds = await submit.boundingBox();
      expect((submitBounds?.y ?? viewport.height) + (submitBounds?.height ?? 0)).toBeLessThanOrEqual(viewport.height);
      await expect(page.getByRole('link', { name: '← Home', exact: true })).toHaveAttribute('href', '/');
      const ordered = await page.evaluate(() => {
        const form = document.querySelector('.apx-auth-workspace');
        const story = document.querySelector('.apx-auth-story');
        return Boolean(form && story && (form.compareDocumentPosition(story) & Node.DOCUMENT_POSITION_FOLLOWING));
      });
      expect(ordered).toBe(true);
      expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(1);
    }
  }
});

test('The Clearing keeps a route home and explains unavailable messages on mobile', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: '{"user":null}' }));
  await page.route('**/rest/v1/clearing_room*', route => route.fulfill({ status: 503, contentType: 'application/json', body: '{"message":"Room unavailable during this test"}' }));
  await page.goto('/clearing');
  await expect(page.getByRole('link', { name: 'Return to Apocky home' })).toHaveAttribute('href', '/');
  const navigation = page.getByRole('navigation', { name: 'Explore Apocky' });
  await expect(navigation.getByRole('link', { name: 'Tools', exact: true })).toHaveAttribute('href', '/tools');
  await expect(navigation.getByRole('link', { name: 'Words', exact: true })).toHaveAttribute('href', '/words');
  await expect(navigation.getByRole('link', { name: 'Thoughts', exact: true })).toHaveAttribute('href', '/conversations');
  await expect(navigation.getByRole('link', { name: 'Codex', exact: true })).toHaveAttribute('href', '/codex-apockalypsis');
  await expect(page.getByRole('status').filter({ hasText: 'The room could not be loaded.' })).toBeVisible();
  await expect(page.getByText('No messages yet.', { exact: true })).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(1);
});

test('Useful activities preserve existing progress IDs across a reload', async ({ page }) => {
  await page.goto('/quests');
  const cards = page.locator('main article');
  await expect(cards).toHaveCount(11);
  const spell = cards.filter({ has: page.getByRole('heading', { name: 'Make a spell of your own', exact: true }) });
  await expect(spell.getByRole('link', { name: 'Create a spell' })).toHaveAttribute('href', '/spellcraft');
  await spell.getByRole('button', { name: 'Mark complete', exact: true }).click();
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('apocky.public-quests.v1') ?? '[]') as string[])).toContain('compose-working');
  await page.reload();
  await expect(page.getByRole('heading', { name: '1 of 11 activities complete', exact: true })).toBeVisible();
  await expect(spell.getByRole('button', { name: 'Mark incomplete', exact: true })).toHaveAttribute('aria-pressed', 'true');
  const oracle = cards.filter({ has: page.getByRole('heading', { name: 'Ask a question, notice your reaction' }) });
  await expect(oracle).toContainText('Sign in on Chaos Tarot');
  await expect(oracle.getByRole('link')).toHaveAttribute('target', '_blank');
  await page.getByRole('button', { name: 'Reset progress', exact: true }).click();
  await expect(page.getByRole('heading', { name: '0 of 11 activities complete', exact: true })).toBeVisible();
});

import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

async function expectNoSeriousAccessibilityFindings(
  page: import('@playwright/test').Page,
): Promise<void> {
  const result = await new AxeBuilder({ page }).analyze();
  const findings = result.violations.filter(
    (entry) => entry.impact === 'serious' || entry.impact === 'critical',
  );
  expect(findings, JSON.stringify(findings, null, 2)).toEqual([]);
}

async function expectNoHorizontalOverflow(
  page: import('@playwright/test').Page,
): Promise<void> {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
}

test('signed-out Apocrypha is truthful, accessible, and does not expose a composer', async ({ page }) => {
  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: null }),
  }));

  await page.goto('/apocrypha');

  await expect(page).toHaveTitle(/Speak with Apocrypha/);
  await expect(page.getByRole('heading', { level: 1, name: 'Speak plainly.' })).toBeVisible();
  await expect(page.getByText('Sign in to begin a restricted member turn.')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Sign in', exact: true }).last()).toHaveAttribute(
    'href',
    '/login?next=%2Fapocrypha',
  );
  await expect(page.getByLabel('Message Apocrypha')).toHaveCount(0);
  await expect(page.getByText(/Training consent/)).toBeVisible();
  await expect(page.getByText('Off', { exact: true })).toBeVisible();
  await expectNoHorizontalOverflow(page);
  await expectNoSeriousAccessibilityFindings(page);
});

test('verified member can complete one exact committed browser turn', async ({ page }) => {
  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: { id: 'browser-member' } }),
  }));
  await page.route('**/api/admin/check', (route) => route.fulfill({
    status: 403,
    contentType: 'application/json',
    body: JSON.stringify({ authorized: false }),
  }));
  await page.route('**/api/apocrypha/chat', async (route) => {
    const request = route.request();
    const body = request.postDataJSON() as {
      text: string;
      conversation_id: string;
      request_id: string;
    };
    expect(body.text).toBe('A browser verification turn.');
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        text: 'A committed native response.',
        conversation_id: body.conversation_id,
        request_id: body.request_id,
        transition_id: 'transition-browser-verification',
        state_root: 'state-browser-verification',
        expression_mode: 'bootstrap_shallow',
        external_inference: false,
        effect_authority: 'deny_all_O10_membrane',
        outcome: 'committed',
        memory_scope: 'ephemeral',
        conversation_history: 'not_retained_by_public_interface',
        training_consent: false,
        duplicate_commit_protection: 'active',
      }),
    });
  });

  await page.goto('/apocrypha');
  const composer = page.getByLabel('Message Apocrypha');
  await expect(composer).toBeVisible();
  await composer.fill('A browser verification turn.');
  await page.getByRole('button', { name: /Send/ }).click();

  await expect(page.getByText('A browser verification turn.')).toBeVisible();
  await expect(page.getByText('A committed native response.')).toBeVisible();
  const receipt = page.locator('details').filter({ hasText: 'Committed turn receipt' });
  await receipt.getByText('Committed turn receipt').click();
  await expect(receipt.getByText('bootstrap_shallow')).toBeVisible();
  await expectNoHorizontalOverflow(page);
  await expectNoSeriousAccessibilityFindings(page);
});

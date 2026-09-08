import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

async function assertNoOverflow(page: import('@playwright/test').Page): Promise<void> {
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow).toBeLessThanOrEqual(1);
}

test('memory layers stay understandable and locked without a verified session', async ({ page }) => {
  await page.route('**/api/auth/me', (route) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: null }) }));
  await page.goto('/memory-tools');

  await expect(page.getByRole('heading', { level: 1, name: /Find it\. Remember it\./ })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Breadcrumb' })).toContainText('Home');
  await expect(page.getByText('PUBLIC MEMORY', { exact: true })).toBeVisible();
  await expect(page.getByText('DEVICE-LOCAL MEMORY', { exact: true })).toBeVisible();
  await expect(page.getByText('SIGNED-IN PRIVATE MNEME', { exact: true })).toBeVisible();
  await expect(page.getByText(/existing selected public conversations are expanding/i)).toBeVisible();
  await expect(page.getByText('LOCKED · NO PRIVATE DATA LOADED')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Recall', exact: true })).toHaveCount(0);

  const help = page.getByRole('button', { name: 'Why memory has three layers' });
  await help.focus();
  await expect(page.getByRole('tooltip')).toBeVisible();
  await help.press('Escape');
  await expect(page.getByRole('tooltip')).toBeHidden();

  const firstTask = page.locator('a').filter({ hasText: 'Explore public ideas and conversations' });
  const box = await firstTask.boundingBox();
  expect(box?.height ?? 0).toBeGreaterThanOrEqual(44);
  await assertNoOverflow(page);
  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(accessibility.violations.filter((item) => ['serious', 'critical'].includes(item.impact ?? ''))).toEqual([]);
});

test('verified profile exposes real member operations through the me route', async ({ page }) => {
  await page.route('**/api/auth/me', (route) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'member-browser-test' } }) }));
  await page.route('**/api/admin/check', (route) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: false }) }));
  await page.route('**/api/mneme/me/health', (route) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ profile_ready: true, storage_ready: true, semantic_ready: true }) }));
  await page.route('**/api/mneme/me/list?limit=50', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      memories: [{
        id: '11111111-1111-4111-8111-111111111111',
        profile_id: 'opaque-server-profile',
        type: 'fact',
        csl: 'private.memory.rhythm ⊗ utf8.74657374',
        paraphrase: 'I work best after a quiet morning.',
        topic_key: 'private.memory.rhythm',
        search_queries: [],
        source_msg_ids: [],
        superseded_by: null,
        created_at: '2026-09-03T12:00:00.000Z',
      }],
    }),
  }));

  let rememberBody: Record<string, unknown> | null = null;
  await page.route('**/api/mneme/me/remember', async (route) => {
    rememberBody = route.request().postDataJSON() as Record<string, unknown>;
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) });
  });
  await page.route('**/api/mneme/me/recall', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ result_nl: 'You chose quiet mornings.', confidence: 0.82, citations: ['11111111-1111-4111-8111-111111111111'] }),
  }));

  await page.goto('/memory-tools#private-mneme');
  await expect(page.getByText('SIGNED IN · USER-BOUND PROFILE')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Export my data' })).toBeVisible();

  await page.getByLabel('What are you trying to remember?').fill('What rhythm did I choose?');
  await page.getByRole('button', { name: 'Recall', exact: true }).click();
  await expect(page.getByText('You chose quiet mornings.')).toBeVisible();

  await page.getByLabel('Short label').fill('Creative rhythm');
  await page.getByLabel('Memory in your words').fill('Quiet mornings help me focus.');
  await page.getByRole('button', { name: 'Remember this' }).click();
  await expect.poll(() => rememberBody).not.toBeNull();
  const submitted = rememberBody as Record<string, unknown> | null;
  expect(submitted).not.toHaveProperty('profile_id');
  expect(String(submitted?.['csl'])).toMatch(/^private\.memory\.creative-rhythm ⊗ utf8\.[a-f0-9]+$/);

  const memory = page.getByText('I work best after a quiet morning.');
  await memory.click();
  await expect(page.getByRole('button', { name: 'Correct', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Delete', exact: true })).toBeVisible();
  await assertNoOverflow(page);
});

test('Atlas offers task-first paths and keyboard help', async ({ page }) => {
  await page.goto('/atlas');
  await expect(page.getByRole('heading', { name: 'Take a useful path first.' })).toBeVisible();
  await expect(page.getByRole('link', { name: /Read human–AI conversations/ })).toHaveAttribute('href', '/conversations');
  await expect(page.getByRole('link', { name: /See public, local, and private layers/ })).toHaveAttribute('href', '/memory-tools');
  const help = page.getByRole('button', { name: 'How the Atlas works' });
  await help.focus();
  await expect(page.getByRole('tooltip')).toBeVisible();
  await help.press('Escape');
  await expect(page.getByRole('tooltip')).toBeHidden();
  await assertNoOverflow(page);
});

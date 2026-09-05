import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const subject = '22222222-2222-4222-8222-222222222222';
const conversation = '33333333-3333-4333-8333-333333333333';
const stamp = '2026-09-04T00:00:00.000Z';
async function observeAuthChannels(page: Page) {
  await page.addInitScript(() => {
    const surface = window as typeof window & { __operatorAuthChannels?: string[] };
    surface.__operatorAuthChannels = [];
    const Original = window.BroadcastChannel;
    window.BroadcastChannel = class extends Original {
      constructor(name: string) { super(name); if (name.startsWith('sb-')) surface.__operatorAuthChannels!.push(name); }
    };
  });
}
async function authEvent(page: Page, event: 'SIGNED_OUT' | 'SIGNED_IN') {
  await expect.poll(() => page.evaluate(() => (window as typeof window & { __operatorAuthChannels?: string[] }).__operatorAuthChannels?.length ?? 0)).toBeGreaterThan(0);
  await page.evaluate(kind => {
    const channels = (window as typeof window & { __operatorAuthChannels: string[] }).__operatorAuthChannels;
    for (const name of new Set(channels)) { const channel = new BroadcastChannel(name); channel.postMessage({ event: kind, session: null }); channel.close(); }
  }, event);
}

test('operator view denies anonymous and member sessions before showing private controls', async ({ page }) => {
  let inspections = 0;
  await page.route('**/api/admin/check', route => route.fulfill({ status: 401, json: { authorized: false, reason: 'A verified operator session is required.' } }));
  await page.route('**/api/admin/apocrypha/inspect', route => { inspections += 1; return route.fulfill({ status: 403, json: { code: 'OPERATOR_REQUIRED' } }); });
  await page.goto('/admin/apocrypha');
  await expect(page.getByText('NOT AUTHORIZED', { exact: false })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Load accounts', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Run task', exact: true })).toHaveCount(0);
  expect(inspections).toBe(0);
});

test('operator selects one account, retains uncertain operation ID and clears private state on sign-out', async ({ page }, info) => {
  await observeAuthChannels(page);
  let authorized = true; const inspections: Record<string, unknown>[] = []; const controls: Record<string, unknown>[] = [];
  await page.route('**/api/admin/check', route => route.fulfill({ json: { authorized, email: authorized ? 'operator@example.test' : 'member@example.test' } }));
  await page.route('**/api/admin/apocrypha/inspect', async route => {
    const body = route.request().postDataJSON() as Record<string, unknown>; inspections.push(body);
    const result = body.action === 'users' ? { users: [{ id: subject, email: 'selected@example.test', created_at: stamp, last_sign_in_at: null }], page: 1, page_size: 50, has_more: false }
      : body.action === 'sessions' ? { sessions: [{ session_id: conversation, title: 'Selected fixture conversation', message_count: 1 }] }
      : { session: { session_id: conversation, title: 'Selected fixture conversation', messages: [{ role: 'assistant', content: 'PRIVATE_SELECTED_TEST_CONTENT' }], events_truncated: false } };
    await route.fulfill({ json: { schema_version: 'apocky.operator-inspection.v1', observed_at: stamp, action: body.action, purpose: body.purpose, result } });
  });
  await page.route('**/api/brain/control', async route => {
    const body = route.request().postDataJSON() as Record<string, unknown>; controls.push(body);
    if (body.action === 'status') await route.fulfill({ json: { schema_version: 'apocv4.remote-code.capabilities.v1', enabled: true } });
    else if (body.action === 'run') await route.fulfill({ status: 503, json: { code: 'CONTROL_RESULT_UNAVAILABLE' } });
    else await route.fulfill({ json: { schema_version: 'apocv4.remote-code.operation.v1', operation_id: body.operation_id, state: 'INDETERMINATE' } });
  });
  await page.goto('/admin/apocrypha');
  await page.getByRole('button', { name: 'Load accounts', exact: true }).click();
  await page.getByRole('button', { name: 'selected@example.test', exact: true }).click();
  await page.getByRole('button', { name: 'Selected fixture conversation · 1 messages', exact: true }).click();
  await expect(page.getByText('PRIVATE_SELECTED_TEST_CONTENT', { exact: true })).toBeVisible();
  expect(inspections).toEqual([{ action: 'users', purpose: 'debugging', page: 1 }, { action: 'sessions', purpose: 'debugging', subject }, { action: 'session', purpose: 'debugging', subject, session_id: conversation }]);
  await page.getByRole('button', { name: 'Check desktop capabilities', exact: true }).click();
  await page.getByRole('textbox', { name: 'Task', exact: true }).fill('Update a synthetic test fixture.');
  await page.getByRole('textbox', { name: 'Files the task may change', exact: true }).fill('tests/fixture.txt');
  await page.getByRole('button', { name: 'Run task', exact: true }).click();
  await expect(page.getByRole('alert').filter({ hasText: 'CONTROL_RESULT_UNAVAILABLE' })).toBeVisible();
  const operationId = await page.getByRole('textbox', { name: 'Operation ID', exact: true }).inputValue();
  expect(operationId).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  await expect(page.getByRole('button', { name: 'Run task', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'New task', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: 'Read result', exact: true }).click();
  expect(controls.filter(item => item.action === 'run')).toHaveLength(1);
  expect(controls.at(-1)).toEqual({ action: 'read', operation_id: operationId });
  await expect(page.getByRole('textbox', { name: 'Operation ID', exact: true })).toHaveValue(operationId);
  expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(accessibility.violations.filter(item => ['serious', 'critical'].includes(item.impact ?? ''))).toEqual([]);
  await page.screenshot({ path: info.outputPath('operator-selected-account.png'), fullPage: true });
  await authEvent(page, 'SIGNED_OUT');
  await expect(page.getByText('PRIVATE_SELECTED_TEST_CONTENT', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Load accounts', exact: true })).toHaveCount(0);
  authorized = false;
  await authEvent(page, 'SIGNED_IN');
  await expect(page.getByText('NOT AUTHORIZED', { exact: false })).toBeVisible();
  await expect(page.getByText('PRIVATE_SELECTED_TEST_CONTENT', { exact: true })).toHaveCount(0);
});

import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const stamp = '2026-09-04T12:00:00Z';
function observeScriptFailures(page: Page): string[] {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('requestfailed', request => { if (request.resourceType() === 'script') errors.push(`${request.url()}: ${request.failure()?.errorText}`); });
  return errors;
}
async function identity(page: Page, id: string | null): Promise<void> {
  await page.route('**/api/auth/me', route => route.fulfill({ json: { user: id ? { id, email: `${id}@example.test` } : null } }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 403, json: { authorized: false } }));
}
async function accessible(page: Page): Promise<void> {
  const result = await new AxeBuilder({ page }).analyze();
  const serious = result.violations.filter(item => item.impact === 'critical' || item.impact === 'serious');
  expect(serious, JSON.stringify(serious, null, 2)).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
}

test('public browser entry and native downloads are usable at phone, tablet and desktop widths', async ({ page }, info) => {
  const scriptErrors = observeScriptFailures(page);
  await identity(page, null);
  for (const width of [360, 390, 768, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/apocrypha');
    await expect(page.getByRole('link', { name: 'Sign in to chat', exact: true })).toHaveAttribute('href', '/login?next=%2Fapocrypha');
    await expect(page.getByRole('link', { name: 'Create an account', exact: true })).toHaveAttribute('href', '/register?next=%2Fapocrypha');
    await expect(page.getByRole('link', { name: 'Private Brain', exact: true })).toHaveCount(0);
    await accessible(page);
    await page.screenshot({ path: info.outputPath(`apocrypha-welcome-${width}.png`), fullPage: true });
    await page.getByRole('link', { name: 'Get the app', exact: true }).click();
    await expect(page).toHaveURL(/\/download\/apocrypha$/);
    await expect(page.getByRole('heading', { name: 'Android', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'iPhone', exact: true })).toBeVisible();
    await expect(page.getByText('Sign in with your Apocky account.')).toBeVisible();
    const details = page.locator('details').filter({ has: page.getByText('Preview testing details', { exact: true }) });
    await expect(details).not.toHaveAttribute('open', '');
    await page.getByText('Preview testing details', { exact: true }).click();
    await expect(page.getByText('Account sign-in and chat', { exact: true })).toBeVisible();
    await page.getByText('Preview testing details', { exact: true }).click();
    await accessible(page);
    await page.locator('h1').click();
    await page.evaluate(() => scrollTo(0, 0));
    await page.screenshot({ path: info.outputPath(`apocrypha-download-${width}.png`), fullPage: true });
  }
  expect(scriptErrors, 'production script nonce and hydration must remain valid').toEqual([]);
});

test('account chat sends, restores history, preserves retry identity and isolates another account', async ({ page }, info) => {
  const scriptErrors = observeScriptFailures(page);
  await identity(page, 'member-a');
  const turns: Record<string, string>[] = [];
  let completed = false;
  let failNext = true;
  await page.route('**/api/mobile/sessions**', route => {
    const sessionId = new URL(route.request().url()).searchParams.get('session_id');
    const turn = turns.at(-1);
    if (sessionId && turn && completed) return route.fulfill({ json: { schema_version: 'apocky.mobile.session.v1', status: 'live', session: { schema_version: 'apocky.mobile.history-session.v1', session_id: sessionId, title: 'A private test conversation', events_truncated: false, messages: [
      { role: 'user', content: turn.text, request_id: turn.request_id, recorded_at: stamp },
      { role: 'assistant', content: 'A confirmed fixture reply for member A.', request_id: turn.request_id, recorded_at: stamp },
    ] } } });
    return route.fulfill({ json: { schema_version: 'apocky.mobile.sessions.v1', status: 'live', discovery_scope: 'account_conversations', count: completed ? 1 : 0, sessions: completed && turn ? [{ session_id: turn.session_id, title: 'A private test conversation', message_count: 2 }] : [] } });
  });
  await page.route('**/api/mobile/turn', route => {
    const body = route.request().postDataJSON() as Record<string, string>; turns.push(body);
    expect(Object.keys(body).sort()).toEqual(['request_id', 'session_id', 'text']);
    if (failNext) { failNext = false; return route.fulfill({ status: 503, json: { error: 'temporarily_unavailable' } }); }
    completed = true;
    return route.fulfill({ json: { schema_version: 'apocky.mobile.turn.v1', status: 'completed', text: 'A confirmed fixture reply for member A.', session_id: body.session_id, request_id: body.request_id, model_id: 'test-fixture', response_digest: 'a'.repeat(64) } });
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/apocrypha');
  const input = page.getByRole('textbox', { name: 'Message Apocrypha', exact: true });
  await expect(input).toBeEnabled();
  await input.fill('Keep my conversation private.');
  await page.getByRole('button', { name: 'Send', exact: false }).click();
  await expect(page.getByText('Apocrypha could not be reached. Please try again shortly.')).toBeVisible();
  await expect(page.getByText('A confirmed fixture reply for member A.')).toHaveCount(0);
  await page.getByRole('button', { name: 'Retry same message' }).click();
  await expect(page.getByText('A confirmed fixture reply for member A.')).toBeVisible();
  expect(turns).toHaveLength(2); expect(turns[0]).toEqual(turns[1]);
  await accessible(page);
  await page.screenshot({ path: info.outputPath('apocrypha-account-chat-390.png'), fullPage: true });
  await page.reload();
  await expect(page.getByText('A confirmed fixture reply for member A.')).toBeVisible();
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await expect(page.getByRole('complementary', { name: 'Conversation history' })).toBeVisible();
  await accessible(page);
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.screenshot({ path: info.outputPath('apocrypha-account-chat-1440.png'), fullPage: true });
  await page.unroute('**/api/auth/me'); await identity(page, 'member-b');
  await page.unroute('**/api/mobile/sessions**');
  await page.route('**/api/mobile/sessions**', route => route.fulfill({ json: { schema_version: 'apocky.mobile.sessions.v1', status: 'live', discovery_scope: 'account_conversations', sessions: [], count: 0 } }));
  await page.reload();
  await expect(input).toBeEnabled();
  await expect(page.getByText('A confirmed fixture reply for member A.')).toHaveCount(0);
  await expect(page.getByText('Keep my conversation private.', { exact: true })).toHaveCount(0);
  expect(scriptErrors, 'account chat must not render through a failed script or hydration path').toEqual([]);
});

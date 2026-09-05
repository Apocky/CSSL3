import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const TEST_EMAIL = 'person@example.test';
const AUTH_TICKET = 'fixture-auth-attempt-'.repeat(8);

async function stubSignedOutShell(page: Page): Promise<void> {
  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: null }),
  }));
}

async function stubEmailDelivery(page: Page): Promise<void> {
  await page.route('**/auth/v1/otp**', async (route) => {
    expect(route.request().method()).toBe('POST');
    const body = route.request().postDataJSON() as { email?: unknown };
    expect(body.email).toBe(TEST_EMAIL);
    await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
  });
}

async function stubFreshAuthAttempt(page: Page): Promise<void> {
  await page.route('**/api/auth/attempt', async (route) => {
    expect(route.request().method()).toBe('POST');
    expect(route.request().postDataJSON()).toEqual({ mode: 'fresh' });
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.auth-fence.v1',
        status: 'ready',
        mode: 'fresh',
        ticket: AUTH_TICKET,
        expires_at: new Date(Date.now() + 15 * 60_000).toISOString(),
        provider_start_delay_ms: 0,
      }),
    });
  });
}

async function expectNoSeriousAccessibilityFindings(page: Page): Promise<void> {
  const result = await new AxeBuilder({ page }).analyze();
  const findings = result.violations.filter((entry) => entry.impact === 'serious' || entry.impact === 'critical');
  expect(findings, JSON.stringify(findings, null, 2)).toEqual([]);
}

async function chooseNoDiagnostics(page: Page): Promise<void> {
  const off = page.getByRole('radio', { name: /^Off\b/i });
  if (await off.isVisible()) {
    await off.click();
    await page.getByRole('button', { name: 'Save choice: Off' }).click();
  }
}

test('email code sign-in establishes the server mirror without trapping a valid member at owner Brain rebind', async ({ page }) => {
  const priorLockGeneration = '88888888-8888-4888-8888-888888888888';
  await stubSignedOutShell(page);
  await stubEmailDelivery(page);
  await stubFreshAuthAttempt(page);

  let verifyBody: Record<string, unknown> | null = null;
  await page.route('**/auth/v1/verify**', async (route) => {
    verifyBody = route.request().postDataJSON() as Record<string, unknown>;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        access_token: 'browser-test-access',
        token_type: 'bearer',
        expires_in: 3600,
        refresh_token: 'browser-test-refresh',
        user: {
          id: '00000000-0000-4000-8000-000000000001',
          aud: 'authenticated',
          role: 'authenticated',
          email: TEST_EMAIL,
          app_metadata: { provider: 'email', providers: ['email'] },
          user_metadata: {},
          identities: [],
          created_at: '2026-07-20T00:00:00.000Z',
        },
      }),
    });
  });
  let mirrored = false;
  await page.route('**/api/auth/session', async (route) => {
    expect(route.request().headers()['x-apocky-auth-protocol']).toBe('apocky.auth-fence.v1');
    expect(route.request().headers()['x-apocky-auth-attempt']).toBe(AUTH_TICKET);
    expect(route.request().postDataJSON()).toEqual({ mode: 'fresh' });
    mirrored = true;
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) });
  });
  let memberRebindDenied = false;
  await page.route('**/api/brain/mobile/unlock', async (route) => {
    const body = route.request().postDataJSON() as { lock_generation?: unknown; auth_attempt?: unknown };
    expect(body.lock_generation).toEqual(expect.stringMatching(/^[0-9a-f-]{36}$/i));
    expect(body.auth_attempt).toBe(AUTH_TICKET);
    memberRebindDenied = true;
    await route.fulfill({
      status: 403,
      contentType: 'application/json',
      body: JSON.stringify({ code: 'BRAIN_OWNER_REQUIRED' }),
    });
  });
  await page.route('**/auth/v1/user**', (route) => route.fulfill({ status: 401, body: '{}' }));

  await page.goto('/login?next=%2Faccount');
  await expect(page.getByText(/continue to your account/i)).toBeVisible();
  await page.evaluate((generation) => {
    localStorage.setItem('apocky-mini-brain-session-lock-v1', generation);
    sessionStorage.setItem('apocky-mini-brain-rebind-candidate-v1', JSON.stringify({
      schema_version: 'apocky.mini-brain.rebind-candidate.v1',
      owner_ref: 'a'.repeat(64),
      lock_generation: generation,
      expires_at_ms: Date.now() + 60_000,
    }));
  }, priorLockGeneration);
  await page.getByLabel('Email address').fill(TEST_EMAIL);
  await page.getByRole('button', { name: 'Send sign-in email' }).click();
  await expect(page.getByLabel(/one-time.*code/i)).toBeVisible();
  await expect(page.getByRole('button', { name: /resend email in 30s/i })).toBeDisabled();
  await page.getByLabel(/one-time.*code/i).fill('123456');
  await page.getByRole('button', { name: 'Verify and continue' }).click();
  await expect(page).toHaveURL(/\/account$/);
  expect(mirrored).toBe(true);
  expect(memberRebindDenied).toBe(true);
  expect(verifyBody).toMatchObject({ email: TEST_EMAIL, token: '123456', type: 'email' });
  const currentLockGeneration = await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'));
  expect(currentLockGeneration).toEqual(expect.stringMatching(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i));
  expect(currentLockGeneration).not.toBe(priorLockGeneration);
  expect(await page.evaluate(() => sessionStorage.getItem('apocky-mini-brain-rebind-candidate-v1'))).toBeNull();
});

test('registration requires explicit terms and then exposes code verification', async ({ page }) => {
  await stubSignedOutShell(page);
  await stubEmailDelivery(page);
  await stubFreshAuthAttempt(page);
  await page.goto('/register?next=%2Faccount');

  await page.getByLabel('Email address').fill(TEST_EMAIL);
  await expect(page.getByRole('button', { name: 'Send verification email' })).toBeDisabled();
  await page.getByLabel(/I agree to the Terms/i).check();
  await page.getByRole('button', { name: 'Send verification email' }).click();
  await expect(page.getByLabel(/one-time.*code/i)).toBeVisible();
  await expect(page.getByRole('button', { name: /resend email in 30s/i })).toBeDisabled();
  await expectNoSeriousAccessibilityFindings(page);
});

test('unsafe return targets fail closed to the account page', async ({ page }) => {
  await stubSignedOutShell(page);
  await page.goto('/login?next=https%3A%2F%2Fevil.example%2Fsteal');
  await expect(page.getByText(/continue to your account/i)).toBeVisible();
  await expect(page.getByRole('link', { name: 'Create an account' })).toHaveAttribute('href', '/register?next=%2Faccount');
});

test('callback error is comprehensible, retryable, and never exposes auth fragments', async ({ page }) => {
  await stubSignedOutShell(page);
  await page.goto('/auth/callback?next=%2Fchat');
  const alert = page.locator('#callback-status');
  await expect(alert).toContainText(/no sign-in response|could not be completed|missing/i);
  await expect(page.getByRole('button', { name: /retry secure sign-in/i })).toBeVisible();
  await expect(page.getByRole('link', { name: /start again/i })).toHaveAttribute('href', '/login?next=%2Faccount');
  await expect(page.locator('body')).not.toContainText(/access_token|refresh_token|code=/i);
  await expectNoSeriousAccessibilityFindings(page);
});

test('account reports only implemented operations and a real privacy-request path', async ({ page }) => {
  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      user: {
        id: '00000000-0000-4000-8000-000000000002',
        email: TEST_EMAIL,
        provider: 'email',
        createdAt: '2026-07-20T00:00:00.000Z',
      },
    }),
  }));
  await page.route('**/api/admin/check', (route) => route.fulfill({
    status: 403,
    contentType: 'application/json',
    body: JSON.stringify({ authorized: false }),
  }));
  await page.goto('/account');
  await chooseNoDiagnostics(page);
  await expect(page.getByRole('heading', { level: 1, name: 'Your account' })).toBeVisible();
  await expect(page.getByRole('heading', { level: 2, name: 'Current identity' })).toBeVisible();
  await expect(page.getByText(/does not currently offer provider-management controls/i)).toBeVisible();
  await expect(page.getByText(/does not currently perform export or deletion/i)).toBeVisible();
  await expect(page.getByRole('link', { name: 'apocky13@gmail.com' })).toHaveAttribute('href', /subject=%5Bprivacy%5D/i);
  await expect(page.getByRole('button', { name: /link or unlink providers/i })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /export account data/i })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /delete account/i })).toHaveCount(0);
  await expectNoSeriousAccessibilityFindings(page);
});

test('auth routes remain telemetry blackout even after a prior positive tier', async ({ page }) => {
  const telemetryRequests: string[] = [];
  page.on('request', (request) => {
    if (request.url().includes('/api/akashic/')) telemetryRequests.push(request.url());
  });
  await page.addInitScript(() => localStorage.setItem('akashic.consent.tier.v1', 'mycelium'));
  await stubSignedOutShell(page);
  await page.goto('/login');
  await page.waitForTimeout(350);
  expect(telemetryRequests).toEqual([]);
  const telemetrySessionKeys = await page.evaluate(() => Object.keys(sessionStorage).filter((key) => key.toLowerCase().includes('akashic')));
  expect(telemetrySessionKeys).toEqual([]);
});

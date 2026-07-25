import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const TEST_EMAIL = 'person@example.test';

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

async function expectNoSeriousAccessibilityFindings(page: Page): Promise<void> {
  const result = await new AxeBuilder({ page }).analyze();
  const findings = result.violations.filter((entry) => entry.impact === 'serious' || entry.impact === 'critical');
  expect(findings, JSON.stringify(findings, null, 2)).toEqual([]);
}

async function chooseNoDiagnostics(page: Page): Promise<void> {
  const none = page.getByRole('radio', { name: /^None\b/i });
  if (await none.isVisible()) {
    await none.click();
    await page.getByRole('button', { name: 'Save none' }).click();
  }
}

test('email code sign-in preserves /chat and establishes the server mirror', async ({ page }) => {
  await stubSignedOutShell(page);
  await stubEmailDelivery(page);

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
  await page.route('**/api/auth/session', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ ok: true }),
  }));
  await page.route('**/auth/v1/user**', (route) => route.fulfill({ status: 401, body: '{}' }));

  await page.goto('/login?next=%2Fchat');
  await expect(page.getByText(/continue to your conversation with apocrypha/i)).toBeVisible();
  await page.getByLabel('Email address').fill(TEST_EMAIL);
  await page.getByRole('button', { name: 'Send sign-in email' }).click();
  await expect(page.getByLabel(/one-time.*code/i)).toBeVisible();
  await expect(page.getByRole('button', { name: /resend email in 30s/i })).toBeDisabled();
  await page.getByLabel(/one-time.*code/i).fill('123456');
  await page.getByRole('button', { name: 'Verify and continue' }).click();
  await expect(page).toHaveURL(/\/chat$/);
  expect(verifyBody).toMatchObject({ email: TEST_EMAIL, token: '123456', type: 'email' });
});

test('registration requires explicit terms and then exposes code verification', async ({ page }) => {
  await stubSignedOutShell(page);
  await stubEmailDelivery(page);
  await page.goto('/register?next=%2Fchat');

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
  await expect(page.getByRole('link', { name: /start again/i })).toHaveAttribute('href', '/login?next=%2Fchat');
  await expect(page.locator('body')).not.toContainText(/access_token|refresh_token|code=/i);
  await expectNoSeriousAccessibilityFindings(page);
});

test('account is truthful and unavailable operations stay disabled', async ({ page }) => {
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
  await expect(page.getByRole('button', { name: /link or unlink providers/i })).toBeDisabled();
  await expect(page.getByRole('button', { name: /export account data/i })).toBeDisabled();
  await expect(page.getByRole('button', { name: /delete account/i })).toBeDisabled();
  await expect(page.getByText(/not uploaded, synced, published/i)).toBeVisible();
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

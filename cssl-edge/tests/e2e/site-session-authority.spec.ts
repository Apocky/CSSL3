import { createHash } from 'node:crypto';
import { expect, test, type APIRequestContext, type BrowserContext, type Page } from '@playwright/test';

const OWNER_A = '11111111-1111-4111-8111-111111111111';
const OWNER_B = '22222222-2222-4222-8222-222222222222';
const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

type RouteStats = { authMe: number; admin: number; device: number; snapshot: number };

test.use({ serviceWorkers: 'block' });

function ownerRef(subject: string): string {
  return createHash('sha256').update(`apocky.mini-brain.owner.v1\0${subject}`, 'utf8').digest('hex');
}

function browserSession(subject: string) {
  return {
    access_token: `access-${subject}`,
    refresh_token: `refresh-${subject}`,
    expires_in: 3600,
    expires_at: Math.floor(Date.now() / 1000) + 3600,
    token_type: 'bearer',
    user: {
      id: subject,
      aud: 'authenticated',
      role: 'authenticated',
      email: 'owner@example.com',
      app_metadata: { provider: 'email', providers: ['email'] },
      user_metadata: {},
      identities: [],
      created_at: '2026-09-04T00:00:00.000Z',
    },
  };
}

async function seedBrowserSession(context: BrowserContext, subject: string): Promise<void> {
  await context.addInitScript((session) => {
    localStorage.setItem('sb-127-auth-token', JSON.stringify(session));
  }, browserSession(subject));
}

function privateSnapshot(subject: string) {
  return {
    schema_version: 'apocky.owner-brain.snapshot.v1',
    status: 'live',
    connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'retired' },
    memories: [{
      id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
      type: 'fact',
      csl: `probe.private.subject @ ${subject}`,
      paraphrase: `Private snapshot for ${subject}.`,
      topic_key: 'probe.private.subject',
      search_queries: ['private probe'],
      source_msg_ids: [],
      superseded_by: null,
      created_at: '2026-09-04T00:00:00.000Z',
    }],
    messages: [],
    counts: { memories: 1, messages: 0, source_links: 0 },
    limits: { memories: 200, recent_messages: 120, source_messages: 200 },
    served_by: 'fixture',
    ts: '2026-09-04T00:00:00.000Z',
  };
}

async function installOwnerRoutes(
  page: Page,
  stats: RouteStats,
  authPayload: () => unknown,
  adminPayload: () => unknown,
  deviceSubject: () => string,
): Promise<void> {
  await page.route('**/api/auth/me**', route => {
    stats.authMe += 1;
    return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(authPayload()) });
  });
  await page.route('**/api/admin/check**', route => {
    stats.admin += 1;
    return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(adminPayload()) });
  });
  await page.route('**/api/brain/mobile/device**', route => {
    stats.device += 1;
    const subject = deviceSubject();
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.mini-brain.device-registration.v1',
        status: 'bound',
        device_token: `device-${subject}`,
        owner_ref: ownerRef(subject),
        key_thumbprint: 'b'.repeat(64),
        expires_at: '2099-01-01T00:00:00.000Z',
        served_by: 'fixture',
        ts: '2026-09-04T00:00:00.000Z',
      }),
    });
  });
  await page.route('**/api/brain/snapshot**', route => {
    stats.snapshot += 1;
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(privateSnapshot(deviceSubject())),
    });
  });
  await page.route('**/api/brain/runtime/status**', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.runtime-status.v1',
      status: 'degraded',
      reason_code: 'BRAIN_LOCAL_PROVIDER_DISABLED',
      observed_at: '2026-09-04T00:00:00.000Z',
      latency_ms: null,
      upstream_status: null,
      served_by: 'fixture',
      ts: '2026-09-04T00:00:00.000Z',
    }),
  }));
}

async function openPrivatePresentation(
  page: Page,
  request: APIRequestContext,
  subject: string,
  sentinel: string,
): Promise<string> {
  const prewarm = await request.get('/apocrypha', {
    headers: { 'x-apocky-test-admin-email': 'owner@example.com' },
  });
  expect(prewarm.ok()).toBe(true);
  await page.goto('/');
  const lockGeneration = crypto.randomUUID();
  await page.evaluate(async ({ generation, expectedOwnerRef }) => {
    localStorage.setItem('apocky-mini-brain-session-lock-v1', generation);
    sessionStorage.setItem('apocky-mini-brain-rebind-candidate-v1', JSON.stringify({
      schema_version: 'apocky.mini-brain.rebind-candidate.v1',
      owner_ref: expectedOwnerRef,
      lock_generation: generation,
      expires_at_ms: Date.now() + 5 * 60_000,
    }));
    await new Promise<void>((resolve, reject) => {
      const open = indexedDB.open('apocky-mini-brain-v1', 1);
      open.onupgradeneeded = () => {
        if (!open.result.objectStoreNames.contains('device')) open.result.createObjectStore('device', { keyPath: 'key' });
        if (!open.result.objectStoreNames.contains('vault')) open.result.createObjectStore('vault', { keyPath: 'key' });
      };
      open.onerror = () => reject(open.error);
      open.onsuccess = () => {
        const database = open.result;
        const transaction = database.transaction('device', 'readwrite');
        transaction.objectStore('device').put({ key: 'lock-boundary', generation });
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error);
        transaction.oncomplete = () => { database.close(); resolve(); };
      };
    });
  }, { generation: lockGeneration, expectedOwnerRef: ownerRef(subject) });
  await page.goto('/apocrypha');
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await expect(composer).toBeEnabled();
  await composer.fill(sentinel);
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText(sentinel, { exact: true })).toBeVisible();
  const lock = await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'));
  expect(lock).toMatch(UUID_V4);
  expect(lock).toBe(lockGeneration);
  return lock!;
}

async function openSystemDrawer(page: Page): Promise<void> {
  const drawer = page.locator('#brain-system');
  if (!(await drawer.evaluate(element => (element as HTMLDetailsElement).open))) {
    await drawer.locator(':scope > summary').click();
  }
  await expect(page.getByRole('button', { name: 'Refresh evidence' })).toBeVisible();
}

test('@mobile latest authority response wins after an older subject resolves late', async ({ page, context, request }) => {
  test.setTimeout(60_000);
  await context.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  const stats: RouteStats = { authMe: 0, admin: 0, device: 0, snapshot: 0 };
  let deviceSubject = OWNER_A;
  await installOwnerRoutes(page, stats, () => ({ user: { id: OWNER_A } }), () => ({ authorized: true }), () => deviceSubject);
  const sentinel = 'Late owner-A evidence must never restore this private presentation.';
  const lockBefore = await openPrivatePresentation(page, request, OWNER_A, sentinel);
  await openSystemDrawer(page);

  await page.evaluate(({ ownerA, ownerB }) => {
    const probeWindow = window as typeof window & { __siteSessionRace?: Record<string, unknown> };
    const nativeFetch = window.fetch.bind(window);
    const state: Record<string, unknown> & {
      meCalls: number;
      adminCalls: number;
      aJsonRead: boolean;
      bJsonRead: boolean;
      resolveA?: () => void;
    } = { meCalls: 0, adminCalls: 0, aJsonRead: false, bJsonRead: false };
    const trackedResponse = (payload: unknown, onRead: () => void): Response => ({
      ok: true,
      status: 200,
      json: async () => { onRead(); return payload; },
    }) as Response;
    probeWindow.__siteSessionRace = state;
    window.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
      const raw = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
      const url = new URL(raw, location.href);
      if (url.pathname === '/api/auth/me') {
        state.meCalls += 1;
        if (state.meCalls === 1) {
          return new Promise<Response>((resolve) => {
            state.resolveA = () => resolve(trackedResponse(
              { user: { id: ownerA } },
              () => { state.aJsonRead = true; },
            ));
          });
        }
        return Promise.resolve(trackedResponse(
          { user: { id: ownerB } },
          () => { state.bJsonRead = true; },
        ));
      }
      if (url.pathname === '/api/admin/check') {
        state.adminCalls += 1;
        return Promise.resolve(trackedResponse(
          state.adminCalls === 1 ? { authorized: true } : { authorized: false, failureKind: 'invalid-session' },
          () => undefined,
        ));
      }
      return nativeFetch(input, init);
    }) as typeof window.fetch;
  }, { ownerA: OWNER_A, ownerB: OWNER_B });

  const refresh = page.getByRole('button', { name: 'Refresh evidence' });
  await refresh.click();
  await expect.poll(() => page.evaluate(() => Number(
    (window as typeof window & { __siteSessionRace?: { meCalls?: number } }).__siteSessionRace?.meCalls ?? 0,
  ))).toBe(1);

  deviceSubject = OWNER_B;
  await refresh.click();
  await expect.poll(() => page.evaluate(() => Boolean(
    (window as typeof window & { __siteSessionRace?: { bJsonRead?: boolean } }).__siteSessionRace?.bJsonRead,
  ))).toBe(true);
  await expect(page.getByText(sentinel, { exact: true })).toHaveCount(0);
  await expect.poll(async () => page.evaluate(
    () => localStorage.getItem('apocky-mini-brain-session-lock-v1'),
  )).not.toBe(lockBefore);
  const lockAfterB = await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'));
  expect(lockAfterB).toMatch(UUID_V4);

  await page.evaluate(() => {
    const state = (window as typeof window & { __siteSessionRace?: { resolveA?: () => void } }).__siteSessionRace;
    if (!state?.resolveA) throw new Error('stalled response A was not captured');
    state.resolveA();
  });
  await expect.poll(() => page.evaluate(() => Boolean(
    (window as typeof window & { __siteSessionRace?: { aJsonRead?: boolean } }).__siteSessionRace?.aJsonRead,
  ))).toBe(true);
  await expect(page.getByText(sentinel, { exact: true })).toHaveCount(0);
  expect(await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'))).toBe(lockAfterB);

  await Promise.all([
    page.waitForURL(url => url.pathname === '/'),
    page.getByRole('link', { name: 'Apocky home' }).click(),
  ]);
  await expect(page.getByText('OWNER-PRIVATE · AVAILABLE', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => ({
    meCalls: (window as typeof window & { __siteSessionRace?: Record<string, unknown> }).__siteSessionRace?.meCalls,
    adminCalls: (window as typeof window & { __siteSessionRace?: Record<string, unknown> }).__siteSessionRace?.adminCalls,
    aJsonRead: (window as typeof window & { __siteSessionRace?: Record<string, unknown> }).__siteSessionRace?.aJsonRead,
    bJsonRead: (window as typeof window & { __siteSessionRace?: Record<string, unknown> }).__siteSessionRace?.bJsonRead,
  }))).toMatchObject({ meCalls: 2, adminCalls: 1, aJsonRead: true, bJsonRead: true });
});

test('@mobile a browser/server subject mismatch definitively clears private presentation', async ({ page, context, request }) => {
  test.setTimeout(60_000);
  await seedBrowserSession(context, OWNER_A);
  await context.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  const stats: RouteStats = { authMe: 0, admin: 0, device: 0, snapshot: 0 };
  let mode: 'owner' | 'mismatch' = 'owner';
  await installOwnerRoutes(
    page,
    stats,
    () => ({ user: { id: mode === 'owner' ? OWNER_A : OWNER_B } }),
    () => ({ authorized: true }),
    () => OWNER_A,
  );
  const sentinel = 'A definitive subject mismatch must clear this private text.';
  const lockBefore = await openPrivatePresentation(page, request, OWNER_A, sentinel);
  await openSystemDrawer(page);
  const adminBefore = stats.admin;

  mode = 'mismatch';
  await page.getByRole('button', { name: 'Refresh evidence' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'This private projection is closed.' })).toBeVisible();
  await expect(page.getByText(sentinel, { exact: true })).toHaveCount(0);
  const lockAfter = await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'));
  expect(lockAfter).toMatch(UUID_V4);
  expect(lockAfter).not.toBe(lockBefore);
  expect(stats.admin).toBe(adminBefore);
});

test('@mobile a typed transient session failure preserves the current owner presentation', async ({ page, context, request }) => {
  test.setTimeout(60_000);
  await seedBrowserSession(context, OWNER_A);
  await context.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  const stats: RouteStats = { authMe: 0, admin: 0, device: 0, snapshot: 0 };
  let mode: 'owner' | 'transient' = 'owner';
  await installOwnerRoutes(
    page,
    stats,
    () => mode === 'owner' ? { user: { id: OWNER_A } } : { user: null, failureKind: 'upstream-unavailable' },
    () => ({ authorized: true }),
    () => OWNER_A,
  );
  const sentinel = 'A typed transient failure must preserve this owner-private presentation.';
  const lockBefore = await openPrivatePresentation(page, request, OWNER_A, sentinel);
  await openSystemDrawer(page);
  const adminBefore = stats.admin;

  mode = 'transient';
  await page.getByRole('button', { name: 'Refresh evidence' }).click();
  await expect(page.getByText(/Owner verification is temporarily unavailable/i)).toBeVisible();
  await expect(page.getByText(sentinel, { exact: true })).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' })).toBeEnabled();
  expect(await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'))).toBe(lockBefore);
  expect(stats.admin).toBe(adminBefore);

  mode = 'owner';
  await page.getByRole('button', { name: 'Refresh evidence' }).click();
  await expect.poll(() => stats.admin).toBeGreaterThan(adminBefore);
  await expect(page.getByText(sentinel, { exact: true })).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' })).toBeEnabled();
  expect(await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'))).toBe(lockBefore);
});

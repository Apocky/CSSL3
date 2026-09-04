import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test.skip(process.env.BRAIN_E2E_OWNER !== '1', 'owner Brain fixture requires the explicit local test-auth gate');

const memories = [
  {
    id: '11111111-1111-4111-8111-111111111111', type: 'instruction',
    csl: 'project.brain.boundary ⊗ source-linked', paraphrase: 'Keep every recalled claim linked to its exact source.',
    topic_key: 'project.brain.boundary', search_queries: ['source boundary', 'provenance'],
    source_msg_ids: ['source-message-1'], superseded_by: null, created_at: '2026-09-03T10:00:00.000Z',
  },
  {
    id: '22222222-2222-4222-8222-222222222222', type: 'fact',
    csl: 'project.brain.boundary ⊗ owner-private', paraphrase: 'The personal Brain remains owner-private and crawler-dark.',
    topic_key: 'project.brain.boundary', search_queries: ['privacy', 'owner brain'],
    source_msg_ids: ['source-message-2'], superseded_by: null, created_at: '2026-09-03T11:00:00.000Z',
  },
  {
    id: '33333333-3333-4333-8333-333333333333', type: 'event',
    csl: 'project.atlas.evolution @ 2026-09-03', paraphrase: 'The Atlas gained graph, timeline, and tunnel projections.',
    topic_key: null, search_queries: ['atlas evolution'], source_msg_ids: [], superseded_by: null,
    created_at: '2026-09-03T12:00:00.000Z',
  },
];

test('owner-private Brain exposes truthful multidimensional memory without a fake conversation', async ({ page }, testInfo) => {
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
      device_token: 'test-device-token', owner_ref: 'a'.repeat(64), key_thumbprint: 'b'.repeat(64),
      expires_at: '2099-01-01T00:00:00.000Z', served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/snapshot', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
      connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'retired' },
      memories,
      messages: [
        { id: 'source-message-1', session_id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', role: 'user', content: 'Do not detach a conclusion from where it came from.', ts: '2026-09-03T09:58:00.000Z', source_only: true },
        { id: 'source-message-2', session_id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', role: 'assistant', content: 'The private surface should be no-store and no-index.', ts: '2026-09-03T10:58:00.000Z', source_only: true },
      ],
      counts: { memories: 3, messages: 0, source_links: 2 },
      limits: { memories: 200, recent_messages: 120, source_messages: 200 },
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/runtime/status', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.runtime-status.v1', status: 'degraded',
      reason_code: 'BRAIN_LOCAL_PROVIDER_DISABLED', observed_at: '2026-09-03T13:00:00.000Z',
      latency_ms: null, upstream_status: null, served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));

  await page.goto('/');
  await expect(page.locator('link[rel="manifest"]')).toHaveCount(1);
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute('href', '/manifest.json');
  await page.getByRole('link', { name: /Open private conversation/i }).click();
  await expect(page).toHaveURL(/\/apocrypha$/);
  await expect(page.locator('link[rel="manifest"]')).toHaveCount(1);
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute('href', '/brain-manifest.json');

  await expect(page.getByRole('heading', { level: 1, name: 'Apocrypha' })).toBeVisible();
  await expect(page.locator('.apx-diagnostics-opener')).toHaveCount(0);
  await expect(page.getByText('Mneme storage')).toBeVisible();
  await expect(page.getByText('not connected · turns stay queued')).toBeVisible();
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await expect(composer).toBeEnabled();
  await expect(page.getByText(/deterministic core recalls compact memory/i)).toBeVisible();
  await composer.fill('How do I preserve the source boundary?');
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText(/Mini Brain · deterministic offline recall/i)).toBeVisible();
  await expect(page.getByText(/encrypted queue · not yet committed/i)).toBeVisible();
  const releaseShelf = page.locator('#brain-releases');
  await expect(releaseShelf.getByText('Candidate — not released')).toBeVisible();
  await releaseShelf.locator('summary').click();
  await expect(releaseShelf.getByRole('link', { name: /Living plan/i })).toHaveAttribute('href', '/releases/apocrypha-living/plan.json');
  await expect(releaseShelf.getByRole('link', { name: /Changelog/i })).toHaveAttribute('href', '/releases/apocrypha-living/changelog.json');
  await expect(releaseShelf.getByRole('link', { name: /Build manifest/i })).toHaveAttribute('href', '/releases/apocrypha-living/manifest.json');
  await expect(releaseShelf.locator('a[href^="/downloads/"]')).toHaveCount(0);

  await page.getByLabel('Find a memory, topic, or phrase').fill('source boundary');
  await expect(page.getByText('2 of 3 loaded records match')).toBeVisible();
  const firstNode = page.locator('button').filter({ hasText: 'project.brain.boundary' }).first();
  await firstNode.click();
  await expect(page.getByRole('heading', { level: 3, name: 'project.brain.boundary' })).toBeVisible();
  await expect(page.getByText('Do not detach a conclusion from where it came from.')).toBeVisible();
  await expect(page.getByText(/preserves exact topic, time, CSL, and source-message links/i)).toBeVisible();

  await page.getByRole('button', { name: 'timeline' }).click();
  await expect(page.getByText('The personal Brain remains owner-private and crawler-dark.', { exact: true })).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
  const a11y = await new AxeBuilder({ page }).analyze();
  expect(a11y.violations.filter(item => ['serious', 'critical'].includes(item.impact ?? ''))).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath('brain.png'), fullPage: true });
});

test('verified owner explicitly creates the first private Mneme profile', async ({ page }) => {
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
      device_token: 'test-device-token', owner_ref: 'a'.repeat(64), key_thumbprint: 'b'.repeat(64),
      expires_at: '2099-01-01T00:00:00.000Z', served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  let provisioned = false;
  let confirmation = '';
  await page.route('**/api/brain/snapshot', route => route.fulfill({
    status: provisioned ? 200 : 409,
    contentType: 'application/json',
    body: JSON.stringify(provisioned ? {
      schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
      connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'retired' },
      memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
      limits: { memories: 200, recent_messages: 120, source_messages: 200 },
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    } : {
      error: 'This verified account does not have a provisioned Mneme profile.',
      code: 'MNEME_PROFILE_NOT_PROVISIONED',
    }),
  }));
  await page.route('**/api/brain/mneme/bootstrap', async route => {
    confirmation = (await route.request().postDataJSON() as { confirmation?: string }).confirmation ?? '';
    provisioned = true;
    await route.fulfill({
      status: 201,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.owner-brain.mneme-bootstrap.v1', status: 'created',
        key_source: 'server_derived_owner_binding', served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
      }),
    });
  });
  await page.route('**/api/brain/runtime/status', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.runtime-status.v1', status: 'degraded',
      reason_code: 'BRAIN_LOCAL_PROVIDER_DISABLED', observed_at: '2026-09-03T13:00:00.000Z',
      latency_ms: null, upstream_status: null, served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));

  await page.goto('/apocrypha');
  await expect(page.getByText('Mneme needs your confirmation.')).toBeVisible();
  await page.getByRole('button', { name: 'Create my private memory profile' }).click();
  await expect(page.getByText(/Private Mneme profile created for this verified owner session/i)).toBeVisible();
  await expect(page.getByText('0 records · 0 source links')).toBeVisible();
  expect(confirmation).toBe('CREATE_OWNER_PRIVATE_MNEME_PROFILE');
});

test('@mobile installed Mini Brain restores an encrypted queued worldline offline', async ({ page, context }, testInfo) => {
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
      device_token: 'test-device-token', owner_ref: 'a'.repeat(64), key_thumbprint: 'b'.repeat(64),
      expires_at: '2099-01-01T00:00:00.000Z', served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/snapshot', route => route.fulfill({
    status: 503,
    contentType: 'application/json',
    body: JSON.stringify({ error: 'Private memory storage could not verify this profile.', code: 'MNEME_STORAGE_UNAVAILABLE' }),
  }));
  await page.route('**/api/brain/runtime/status', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.runtime-status.v1', status: 'degraded',
      reason_code: 'BRAIN_LOCAL_PROVIDER_DISABLED', observed_at: '2026-09-03T13:00:00.000Z',
      latency_ms: null, upstream_status: null, served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));

  const onlineDocument = await page.goto('/apocrypha');
  expect(onlineDocument?.status()).toBe(200);
  expect(onlineDocument?.headers()['cache-control']).toContain('no-store');
  await expect(page.locator('meta[name="viewport"]')).toHaveAttribute('content', /viewport-fit=cover/);
  await expect(page.locator('.apx-diagnostics-opener')).toHaveCount(0);
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute('href', '/brain-manifest.json');
  const manifestResponse = await page.request.get('/brain-manifest.json');
  expect(manifestResponse.ok()).toBe(true);
  expect(await manifestResponse.json()).toMatchObject({
    id: '/apocrypha',
    start_url: '/apocrypha?source=installed-mini-brain',
    scope: '/',
    display: 'standalone',
  });
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await expect(composer).toBeEnabled();
  await composer.fill('What is the smallest reversible move?');
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();
  await expect(page.getByText(/This is a local prompt, not a generated Apocrypha answer/i)).toBeVisible();

  const worker = await page.evaluate(async () => {
    const registration = await navigator.serviceWorker.ready;
    return { scope: registration.scope, scriptURL: registration.active?.scriptURL ?? '' };
  });
  expect(new URL(worker.scope).pathname).toBe('/');
  expect(new URL(worker.scriptURL).pathname).toBe('/brain-sw.js');
  await expect(page.getByText('Offline shell ready')).toBeVisible();
  if (testInfo.project.name.startsWith('ios-webkit')) {
    await expect(page.getByText('iPhone: Share → Add to Home Screen')).toBeVisible();
  } else {
    await page.evaluate(() => {
      const prompt = new Event('beforeinstallprompt', { cancelable: true });
      Object.defineProperties(prompt, {
        prompt: { value: async () => undefined },
        userChoice: { value: Promise.resolve({ outcome: 'dismissed', platform: 'web' }) },
      });
      window.dispatchEvent(prompt);
    });
    const installButton = page.getByRole('button', { name: 'Install Mini Brain' });
    await expect(installButton).toBeVisible();
    await installButton.click();
    await expect(installButton).toHaveCount(0);
  }
  await page.reload();
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();
  await expect.poll(() => page.evaluate(() => Boolean(navigator.serviceWorker.controller))).toBe(true);
  await expect.poll(() => page.evaluate(async () => Boolean(await caches.match('/apocrypha')))).toBe(true);
  await page.evaluate(async () => { await fetch('/api/brain/snapshot', { cache: 'no-store' }); });
  const cachedPrivateApiUrls = await page.evaluate(async () => {
    const names = await caches.keys();
    const groups = await Promise.all(names.map(async name => (await caches.open(name)).keys()));
    return groups.flat().map(request => new URL(request.url).pathname).filter(path => path.startsWith('/api/'));
  });
  expect(cachedPrivateApiUrls).toEqual([]);
  await context.setOffline(true);
  await page.goto('/apocrypha', { waitUntil: 'domcontentloaded' }).catch((error: unknown) => {
    if (
      !testInfo.project.name.startsWith('ios-webkit')
      || !(error instanceof Error)
      || !error.message.includes('WebKit encountered an internal error')
    ) throw error;
  });
  await expect(page.getByRole('heading', { level: 1, name: 'Apocrypha' })).toBeVisible();
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();
  await expect(page.getByText('What is the smallest reversible move?')).toBeVisible();
  await expect(page.getByText(/Offline · encrypted recent history/i)).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
  const a11y = await new AxeBuilder({ page }).analyze();
  expect(a11y.violations.filter(item => ['serious', 'critical'].includes(item.impact ?? ''))).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath('mini-brain-offline.png'), fullPage: true });
  await page.setViewportSize({ width: 320, height: 568 });
  const minimumWidthOverflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(minimumWidthOverflow).toBeLessThanOrEqual(1);
});

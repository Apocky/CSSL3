import AxeBuilder from '@axe-core/playwright';
import { createHash } from 'node:crypto';
import { expect, test } from '@playwright/test';

test.skip(process.env.BRAIN_E2E_OWNER !== '1', 'owner Brain fixture requires the explicit local test-auth gate');

const TEST_OWNER_REF = createHash('sha256')
  .update('apocky.mini-brain.owner.v1\0owner-test', 'utf8')
  .digest('hex');
const OTHER_OWNER_REF = createHash('sha256')
  .update('apocky.mini-brain.owner.v1\0other-owner', 'utf8')
  .digest('hex');
const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

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
  test.setTimeout(60_000);
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
      device_token: 'test-device-token', owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64),
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

  await page.goto('/brain');
  await expect(page.getByRole('heading', { level: 1, name: 'Apocrypha' })).toBeVisible();
  await expect(page.locator('link[rel="manifest"]')).toHaveCount(1);
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute('href', '/brain-manifest.json');
  await expect(page.locator('.apx-diagnostics-opener')).toHaveCount(0);
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await expect(composer).toBeEnabled();
  const composerTop = await composer.evaluate(element => element.getBoundingClientRect().top);
  const viewportHeight = await page.evaluate(() => window.innerHeight);
  expect(composerTop).toBeLessThan(viewportHeight);
  await expect(page.getByText(/deterministic core recalls compact memory/i)).toBeVisible();
  await composer.fill('How do I preserve the source boundary?');
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText(/Mini Brain · deterministic offline recall/i)).toBeVisible();
  await expect(page.getByText(/encrypted queue · not yet committed/i)).toBeVisible();
  const systemDrawer = page.locator('#brain-system');
  await expect(systemDrawer).not.toHaveAttribute('open', '');
  await systemDrawer.locator(':scope > summary').click();
  await expect(page.getByText('Mneme storage')).toBeVisible();
  await expect(page.getByText('not connected · turns stay queued')).toBeVisible();
  const releaseShelf = systemDrawer.locator('#brain-releases');
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
      device_token: 'test-device-token', owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64),
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
  await expect(page.locator('link[rel="manifest"]')).toHaveCount(1);
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute('href', '/brain-manifest.json');
  await expect(page.getByText('Mneme needs your confirmation.')).toBeVisible();
  await page.getByRole('button', { name: 'Create my private memory profile' }).click();
  await expect(page.getByText(/Private Mneme profile created for this verified owner session/i)).toBeVisible();
  await page.locator('#brain-system > summary').click();
  await expect(page.getByText('0 records · 0 source links')).toBeVisible();
  expect(confirmation).toBe('CREATE_OWNER_PRIVATE_MNEME_PROFILE');
});

test('client navigation swaps one global manifest for the private Mini Brain manifest', async ({ page }) => {
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
      device_token: 'test-device-token', owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64),
      expires_at: '2099-01-01T00:00:00.000Z', served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/snapshot', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
      connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'retired' },
      memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
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
  const mobileMenu = page.locator('details.apx-mobile-menu');
  if (await mobileMenu.isVisible()) await mobileMenu.locator('summary').click();
  const privateLink = page.locator('a[href="/apocrypha"]:visible').first();
  await expect(privateLink).toBeVisible();
  await privateLink.click();
  await expect(page).toHaveURL(/\/apocrypha$/);
  await expect(page.locator('link[rel="manifest"]')).toHaveCount(1);
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute('href', '/brain-manifest.json');
  await page.goBack();
  await expect(page.locator('link[rel="manifest"]')).toHaveCount(1);
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute('href', '/manifest.json');
});

test('@mobile encrypted local conversation renders while every remote projection is stalled', async ({ page }) => {
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
      device_token: 'test-device-token', owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64),
      expires_at: '2099-01-01T00:00:00.000Z', served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/snapshot', route => route.fulfill({
    status: 503,
    contentType: 'application/json',
    body: JSON.stringify({ error: 'Private memory storage unavailable.', code: 'MNEME_STORAGE_UNAVAILABLE' }),
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
  await page.goto('/apocrypha');
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await expect(composer).toBeEnabled();
  await composer.fill('Persist before the network disappears.');
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();

  await page.route('**/api/auth/me', () => new Promise<void>(() => undefined));
  await page.route('**/api/auth/session', () => new Promise<void>(() => undefined));
  await page.route('**/api/brain/snapshot', () => new Promise<void>(() => undefined));
  await page.route('**/api/brain/runtime/status', () => new Promise<void>(() => undefined));
  await page.reload();
  await expect(page.getByText('Persist before the network disappears.')).toBeVisible({ timeout: 3_000 });
  await expect(composer).toBeEnabled({ timeout: 3_000 });
  await composer.fill('The local brain remains alive.');
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText('2 encrypted turns waiting')).toBeVisible({ timeout: 3_000 });
});

test('@mobile installed Mini Brain restores an encrypted queued worldline offline', async ({ page, context }, testInfo) => {
  test.skip(testInfo.project.name.startsWith('ios-webkit'), 'Playwright WebKit cannot complete an offline navigation; physical installed-iPhone launch remains a release gate.');
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
      device_token: 'test-device-token', owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64),
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

  await page.goto('/apocrypha');
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await expect(composer).toBeEnabled();
  await composer.fill('What is the smallest reversible move?');
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();
  await expect(page.getByText(/This is a local prompt, not a generated Apocrypha answer/i)).toBeVisible();

  await page.evaluate(async () => { await navigator.serviceWorker.ready; });
  await page.locator('#brain-system > summary').click();
  await expect(page.getByText('Offline shell ready')).toBeVisible();
  await page.locator('#brain-system > summary').click();
  await page.reload();
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();
  await expect.poll(() => page.evaluate(() => Boolean(navigator.serviceWorker.controller))).toBe(true);
  await expect.poll(() => page.evaluate(async () => Boolean(await caches.match('/apocrypha')))).toBe(true);
  const ownerShellBefore = await page.evaluate(async () => (await caches.match('/apocrypha'))?.text() ?? '');
  expect(ownerShellBefore).toContain('"serverAccess":"owner"');
  await page.setExtraHTTPHeaders({});
  await page.goto('/apocrypha');
  await expect(page).toHaveURL(/\/login\?next=%2Fapocrypha$/);
  const ownerShellAfter = await page.evaluate(async () => (await caches.match('/apocrypha'))?.text() ?? '');
  expect(ownerShellAfter).toContain('"serverAccess":"owner"');
  expect(ownerShellAfter).not.toContain('"page":"/login"');
  const priorTimeOrigin = await page.evaluate(() => performance.timeOrigin);
  await page.evaluate(() => { document.body.dataset['offlineNavigationSentinel'] = 'old-document'; });
  await context.setOffline(true);
  await page.goto('/apocrypha', { waitUntil: 'domcontentloaded' });
  await expect(page.locator('body')).not.toHaveAttribute('data-offline-navigation-sentinel', 'old-document');
  expect(await page.evaluate(() => performance.timeOrigin)).not.toBe(priorTimeOrigin);
  await expect.poll(() => page.evaluate(() => location.pathname === '/apocrypha' && Boolean(navigator.serviceWorker.controller))).toBe(true);
  await expect(page.getByRole('heading', { level: 1, name: 'Apocrypha' })).toBeVisible();
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();
  await expect(page.getByText('What is the smallest reversible move?')).toBeVisible();
  await expect(page.getByText(/Offline · encrypted recent history/i)).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
  const a11y = await new AxeBuilder({ page }).analyze();
  expect(a11y.violations.filter(item => ['serious', 'critical'].includes(item.impact ?? ''))).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath('mini-brain-offline.png'), fullPage: true });
});

test('@mobile repeated refresh reuses one vault and erase clears every open tab and local generation', async ({ page, context }) => {
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
      device_token: 'test-device-token', owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64),
      expires_at: '2099-01-01T00:00:00.000Z', served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  let snapshotCalls = 0;
  await page.route('**/api/brain/snapshot', route => {
    snapshotCalls += 1;
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
        connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'retired' },
        memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
        limits: { memories: 200, recent_messages: 120, source_messages: 200 },
        served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
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
  await expect(page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' })).toBeEnabled();
  await expect.poll(() => snapshotCalls).toBeGreaterThan(0);
  for (let index = 0; index < 10; index += 1) {
    const before = snapshotCalls;
    const drawer = page.locator('#brain-system');
    if (!(await drawer.evaluate(element => (element as HTMLDetailsElement).open))) {
      await drawer.locator(':scope > summary').click();
    }
    await page.getByRole('button', { name: 'Refresh evidence' }).click();
    await expect(page.getByRole('heading', { level: 1, name: 'Apocrypha' })).toBeVisible();
    await expect.poll(() => snapshotCalls).toBeGreaterThan(before);
  }
  const eraseSentinel = 'Cross-tab erase must clear this private turn.';
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await composer.fill(eraseSentinel);
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText(eraseSentinel, { exact: true })).toBeVisible();

  const peer = await context.newPage();
  await peer.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await peer.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await peer.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await peer.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
      device_token: 'test-device-token', owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64),
      expires_at: '2099-01-01T00:00:00.000Z', served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await peer.route('**/api/brain/snapshot', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
      connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'retired' },
      memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
      limits: { memories: 200, recent_messages: 120, source_messages: 200 },
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await peer.route('**/api/brain/runtime/status', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.runtime-status.v1', status: 'degraded',
      reason_code: 'BRAIN_LOCAL_PROVIDER_DISABLED', observed_at: '2026-09-03T13:00:00.000Z',
      latency_ms: null, upstream_status: null, served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await peer.goto('/apocrypha');
  await expect(peer.getByText(eraseSentinel, { exact: true })).toBeVisible();
  await page.evaluate(async () => {
    const old = await caches.open('apocky-mini-brain-shell-v1');
    await old.put('/obsolete-mini-brain-shell', new Response('obsolete'));
  });
  const drawer = page.locator('#brain-system');
  if (!(await drawer.evaluate(element => (element as HTMLDetailsElement).open))) {
    await drawer.locator(':scope > summary').click();
  }
  page.once('dialog', dialog => dialog.accept());
  await Promise.all([
    page.waitForURL(url => url.pathname === '/'),
    page.getByRole('button', { name: 'Erase offline copy' }).click(),
  ]);
  await expect(peer.getByText(eraseSentinel, { exact: true })).toHaveCount(0);
  await expect(peer.getByText(/encrypted Mini Brain was erased in this browser/i)).toBeVisible();
  await peer.close();
  const cacheKeys = await page.evaluate(() => caches.keys());
  expect(cacheKeys.filter(key => key.startsWith('apocky-mini-brain-shell-'))).toEqual([]);
  const databaseNames = await page.evaluate(async () => {
    if (typeof indexedDB.databases !== 'function') return null;
    return (await indexedDB.databases()).map(database => database.name ?? '');
  });
  if (databaseNames) expect(databaseNames).not.toContain('apocky-mini-brain-v1');
});

test('@mobile an expired stolen lease cannot overwrite a newer tab revision', async ({ page, context }) => {
  test.setTimeout(60_000);
  let runtimeLive = false;
  let releaseAppend: () => void = () => undefined;
  let markAppendStarted: () => void = () => undefined;
  const appendGate = new Promise<void>(resolve => { releaseAppend = resolve; });
  const appendStarted = new Promise<void>(resolve => { markAppendStarted = resolve; });
  const installRoutes = async (target: typeof page, interceptSync: boolean): Promise<void> => {
    await target.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
    await target.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
    await target.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
    await target.route('**/api/brain/mobile/device', route => route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
        device_token: 'test-device-token', owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64),
        expires_at: '2099-01-01T00:00:00.000Z', served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
      }),
    }));
    await target.route('**/api/brain/snapshot', route => route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
        connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'retired' },
        memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
        limits: { memories: 200, recent_messages: 120, source_messages: 200 },
        served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
      }),
    }));
    await target.route('**/api/brain/runtime/status', route => route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.owner-brain.runtime-status.v1',
        status: interceptSync && runtimeLive ? 'live' : 'degraded',
        reason_code: interceptSync && runtimeLive ? null : 'BRAIN_LOCAL_PROVIDER_DISABLED',
        observed_at: '2026-09-03T13:00:00.000Z', latency_ms: interceptSync && runtimeLive ? 4 : null,
        upstream_status: interceptSync && runtimeLive ? 200 : null, served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
      }),
    }));
    await target.route('**/api/brain/runtime/sessions', route => route.fulfill({
      status: 200, contentType: 'application/json', body: JSON.stringify({ sessions: [] }),
    }));
    if (interceptSync) await target.route('**/api/brain/mobile/sync', async route => {
      const request = route.request().postDataJSON() as { operation: string; session_id: string; request_id: string };
      if (request.operation === 'append') {
        markAppendStarted();
        await appendGate;
      }
      const append = request.operation === 'append';
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          schema_version: 'apocky.mini-brain.sync-response.v1',
          status: append ? 'appended' : 'empty',
          session_id: request.session_id,
          request_id: request.request_id,
          acknowledged_request_ids: append ? [request.request_id] : [],
          cursor: append ? 'c'.repeat(64) : null,
          messages: append ? [{
            role: 'assistant', content: 'Remote answer.', request_id: request.request_id,
            recorded_at: '2026-09-03T13:00:01.000Z', event_digest: 'd'.repeat(64),
          }] : [],
          tombstones: [], events_truncated: false,
          provenance: { transport: 'owner_bound_apocv4_runtime', privacy_partition_ref: null, principal_ref: null, binding_ref: null },
          controls: {
            owner_session: 'verified', device_signature: 'verified',
            replay: 'bounded_sequence_and_idempotent_request', rate_limit: 'owner_durable_window',
            partition: 'server_derived_owner',
          },
          served_by: 'fixture', ts: '2026-09-03T13:00:01.000Z',
        }),
      });
    });
  };

  await installRoutes(page, true);
  await page.goto('/apocrypha');
  const firstTurn = 'Lease A must remain queued if its fence is stolen.';
  const firstComposer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await firstComposer.fill(firstTurn);
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();

  const peer = await context.newPage();
  await installRoutes(peer, false);
  await peer.goto('/apocrypha');
  await expect(peer.getByText(firstTurn, { exact: true })).toBeVisible();

  runtimeLive = true;
  const drawer = page.locator('#brain-system');
  await drawer.locator(':scope > summary').click();
  await page.getByRole('button', { name: 'Refresh evidence' }).click();
  await appendStarted;

  await peer.evaluate(async () => {
    await new Promise<void>((resolve, reject) => {
      const open = indexedDB.open('apocky-mini-brain-v1', 1);
      open.onerror = () => reject(open.error);
      open.onsuccess = () => {
        const database = open.result;
        const transaction = database.transaction('device', 'readwrite');
        const store = transaction.objectStore('device');
        const read = store.get('sync-lease');
        read.onerror = () => reject(read.error);
        read.onsuccess = () => {
          const current = read.result as { key: string; fence: number };
          store.put({
            ...current,
            holder: 'adversarial-stolen-expired-lease',
            fence: current.fence + 1,
            expires_at: Date.now() - 1,
          });
        };
        transaction.oncomplete = () => { database.close(); resolve(); };
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error);
      };
    });
  });

  const secondTurn = 'Lease B owns the newer encrypted revision.';
  const secondComposer = peer.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await secondComposer.fill(secondTurn);
  await peer.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(peer.getByText('2 encrypted turns waiting')).toBeVisible();

  releaseAppend();
  await expect(page.getByText(/MINI_BRAIN_SYNC_LEASE_LOST|MINI_BRAIN_VAULT_REVISION_CONFLICT/)).toBeVisible();
  runtimeLive = false;
  await page.reload();
  await expect(page.getByText('2 encrypted turns waiting')).toBeVisible();
  await expect(page.getByText(firstTurn, { exact: true })).toBeVisible();
  await expect(page.getByText(secondTurn, { exact: true })).toBeVisible();
  await peer.close();
});

test('@mobile a rejected device token rebinds once and preserves the exact queued turn', async ({ page }) => {
  test.setTimeout(60_000);
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/snapshot', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
      connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'live' },
      memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
      limits: { memories: 200, recent_messages: 120, source_messages: 200 },
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/runtime/status', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.runtime-status.v1', status: 'live', reason_code: null,
      observed_at: '2026-09-03T13:00:00.000Z', latency_ms: 4, upstream_status: 200,
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/runtime/sessions', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({ sessions: [] }),
  }));

  const registrations: Array<Record<string, unknown>> = [];
  await page.route('**/api/brain/mobile/device', async route => {
    registrations.push(await route.request().postDataJSON() as Record<string, unknown>);
    const ordinal = registrations.length;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
        device_token: `test-device-token-${ordinal}`, owner_ref: ordinal === 3 ? OTHER_OWNER_REF : TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64),
        expires_at: '2099-01-01T00:00:00.000Z', served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
      }),
    });
  });

  const appendRequests: Array<Record<string, unknown>> = [];
  await page.route('**/api/brain/mobile/sync', async route => {
    const request = await route.request().postDataJSON() as Record<string, unknown>;
    if (request.operation === 'pull') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          schema_version: 'apocky.mini-brain.sync-response.v1', status: 'empty',
          session_id: request.session_id, request_id: request.request_id,
          acknowledged_request_ids: [], cursor: null, messages: [], tombstones: [], events_truncated: false,
          provenance: { transport: 'owner_bound_apocv4_runtime', privacy_partition_ref: null, principal_ref: null, binding_ref: null },
          controls: { owner_session: 'verified', device_signature: 'verified', replay: 'bounded_sequence_and_idempotent_request', rate_limit: 'owner_durable_window', partition: 'server_derived_owner' },
          served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
        }),
      });
      return;
    }
    appendRequests.push(request);
    if (appendRequests.length === 1 || appendRequests.length >= 3) {
      await route.fulfill({
        status: 401,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'Rotated device binding.', code: 'BRAIN_DEVICE_TOKEN_INVALID' }),
      });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.mini-brain.sync-response.v1', status: 'appended',
        session_id: request.session_id, request_id: request.request_id,
        acknowledged_request_ids: [request.request_id], cursor: 'c'.repeat(64),
        messages: [
          { role: 'user', content: (request.payload as { text: string }).text, request_id: request.request_id, recorded_at: '2026-09-03T13:00:01.000Z', event_digest: 'd'.repeat(64) },
          { role: 'assistant', content: 'Rotation retained the exact turn.', request_id: request.request_id, recorded_at: '2026-09-03T13:00:02.000Z', event_digest: 'e'.repeat(64) },
        ],
        tombstones: [], events_truncated: false,
        provenance: { transport: 'owner_bound_apocv4_runtime', privacy_partition_ref: null, principal_ref: null, binding_ref: null },
        controls: { owner_session: 'verified', device_signature: 'verified', replay: 'bounded_sequence_and_idempotent_request', rate_limit: 'owner_durable_window', partition: 'server_derived_owner' },
        served_by: 'fixture', ts: '2026-09-03T13:00:02.000Z',
      }),
    });
  });

  await page.goto('/apocrypha');
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await expect(composer).toBeEnabled();
  await composer.fill('Keep this exact intent across token rotation.');
  await page.getByRole('button', { name: 'Send + sync' }).click();
  await expect(page.getByText('Rotation retained the exact turn.')).toBeVisible();
  await expect(page.getByText(/encrypted turn waiting/)).toHaveCount(0);

  expect(registrations).toHaveLength(2);
  expect(registrations[1]?.device_id).toBe(registrations[0]?.device_id);
  expect(registrations[1]?.public_key_jwk).toEqual(registrations[0]?.public_key_jwk);
  expect(appendRequests).toHaveLength(2);
  for (const field of ['session_id', 'request_id', 'payload', 'payload_digest']) {
    expect(appendRequests[1]?.[field]).toEqual(appendRequests[0]?.[field]);
  }
  expect(Number(appendRequests[1]?.sequence)).toBeGreaterThan(Number(appendRequests[0]?.sequence));
  expect(appendRequests[0]?.device_token).toBe('test-device-token-1');
  expect(appendRequests[1]?.device_token).toBe('test-device-token-2');

  const crossOwnerText = 'Never carry this owner text across a renewed identity.';
  await composer.fill(crossOwnerText);
  await page.getByRole('button', { name: 'Send + sync' }).click();
  await expect(page.getByText(/MINI_BRAIN_OWNER_CHANGED_DURING_REBIND/)).toBeVisible();
  await expect(composer).toBeDisabled();
  expect(registrations).toHaveLength(3);
  expect(appendRequests).toHaveLength(3);
  expect((appendRequests[2]?.payload as { text?: string }).text).toBe(crossOwnerText);
  expect(appendRequests[2]?.device_token).toBe('test-device-token-2');
  expect(await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'))).toMatch(UUID_V4);
});

test('@mobile terminal recovery pulls and reissues the failed session, not the selected session', async ({ page }) => {
  test.setTimeout(60_000);
  let runtimeLive = false;
  const appendRequests: Array<Record<string, unknown>> = [];
  const pullSessions: string[] = [];
  const committedSessions = new Map<string, { requestId: string; text: string; answer: string; cursor: string }>();
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound', device_token: 'test-device-token',
      owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64), expires_at: '2099-01-01T00:00:00.000Z',
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/snapshot', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
      connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: runtimeLive ? 'live' : 'retired' },
      memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
      limits: { memories: 200, recent_messages: 120, source_messages: 200 },
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/runtime/status', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.runtime-status.v1', status: runtimeLive ? 'live' : 'degraded',
      reason_code: runtimeLive ? null : 'BRAIN_LOCAL_PROVIDER_DISABLED', observed_at: '2026-09-03T13:00:00.000Z',
      latency_ms: runtimeLive ? 4 : null, upstream_status: runtimeLive ? 200 : null,
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/runtime/sessions', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({ sessions: [] }),
  }));
  await page.route('**/api/brain/mobile/sync', async route => {
    const request = await route.request().postDataJSON() as Record<string, unknown>;
    const sessionId = String(request.session_id);
    if (request.operation === 'pull') {
      pullSessions.push(sessionId);
      const retained = committedSessions.get(sessionId);
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          schema_version: 'apocky.mini-brain.sync-response.v1', status: retained ? 'advanced' : 'empty',
          session_id: sessionId, request_id: request.request_id, acknowledged_request_ids: [],
          cursor: retained?.cursor ?? (appendRequests.length > 0 ? 'a'.repeat(64) : null),
          messages: retained ? [
            { role: 'user', content: retained.text, request_id: retained.requestId, recorded_at: '2026-09-03T13:00:01.000Z', event_digest: 'e'.repeat(64) },
            { role: 'assistant', content: retained.answer, request_id: retained.requestId, recorded_at: '2026-09-03T13:00:02.000Z', event_digest: 'f'.repeat(64) },
          ] : [],
          tombstones: [], events_truncated: false,
          provenance: { transport: 'owner_bound_apocv4_runtime', privacy_partition_ref: null, principal_ref: null, binding_ref: null },
          controls: { owner_session: 'verified', device_signature: 'verified', replay: 'bounded_sequence_and_idempotent_request', rate_limit: 'owner_durable_window', partition: 'server_derived_owner' },
          served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
        }),
      });
      return;
    }
    appendRequests.push(request);
    if (appendRequests.length === 1) {
      await route.fulfill({
        status: 409,
        contentType: 'application/json',
        body: JSON.stringify({
          error: 'Interrupted before retention.', code: 'BRAIN_SYNC_TERMINAL_FAILED',
          request_id: request.request_id, session_id: sessionId, current_cursor: 'a'.repeat(64),
          error_class: 'InterruptedChatAttempt', error_digest: 'c'.repeat(64), reissue_safe: true, retryable: false,
        }),
      });
      return;
    }
    const text = (request.payload as { text: string }).text;
    const answer = text === 'Preserve this interrupted turn.'
      ? 'Recovered once in the correct worldline.'
      : 'The later queued worldline synchronized after recovery.';
    const cursor = appendRequests.length === 2 ? 'd'.repeat(64) : 'e'.repeat(64);
    committedSessions.set(sessionId, { requestId: String(request.request_id), text, answer, cursor });
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.mini-brain.sync-response.v1', status: 'appended',
        session_id: sessionId, request_id: request.request_id, acknowledged_request_ids: [request.request_id],
        cursor,
        messages: [
          { role: 'user', content: text, request_id: request.request_id, recorded_at: '2026-09-03T13:00:01.000Z', event_digest: 'e'.repeat(64) },
          { role: 'assistant', content: answer, request_id: request.request_id, recorded_at: '2026-09-03T13:00:02.000Z', event_digest: 'f'.repeat(64) },
        ],
        tombstones: [], events_truncated: false,
        provenance: { transport: 'owner_bound_apocv4_runtime', privacy_partition_ref: null, principal_ref: null, binding_ref: null },
        controls: { owner_session: 'verified', device_signature: 'verified', replay: 'bounded_sequence_and_idempotent_request', rate_limit: 'owner_durable_window', partition: 'server_derived_owner' },
        served_by: 'fixture', ts: '2026-09-03T13:00:02.000Z',
      }),
    });
  });

  await page.goto('/apocrypha');
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await expect(composer).toBeEnabled();
  await composer.fill('Preserve this interrupted turn.');
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();
  await page.getByRole('button', { name: 'New' }).click();
  const selectedSessionId = await page.evaluate(() => sessionStorage.getItem('apocky.owner-brain.session.v1'));
  expect(selectedSessionId).not.toBeNull();
  await composer.fill('Synchronize this later queued worldline second.');
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText('2 encrypted turns waiting')).toBeVisible();

  runtimeLive = true;
  const drawer = page.locator('#brain-system');
  await drawer.locator(':scope > summary').click();
  await page.getByRole('button', { name: 'Refresh evidence' }).click();
  await expect(page.getByRole('button', { name: 'Reissue preserved turn' })).toBeVisible();
  expect(appendRequests).toHaveLength(1);
  const failedSessionId = String(appendRequests[0]?.session_id);
  expect(failedSessionId).not.toBe(selectedSessionId);
  const oldRequestId = appendRequests[0]?.request_id;
  await page.getByRole('button', { name: 'Reissue preserved turn' }).click();
  await expect(page.getByText('The later queued worldline synchronized after recovery.')).toBeVisible();
  await expect(page.getByText(/encrypted turn waiting/)).toHaveCount(0);

  expect(appendRequests).toHaveLength(3);
  expect(appendRequests[0]?.session_id).toBe(failedSessionId);
  expect(appendRequests[1]?.session_id).toBe(failedSessionId);
  expect(appendRequests[1]?.request_id).not.toBe(oldRequestId);
  expect(appendRequests[1]?.base_cursor).toBe('a'.repeat(64));
  expect(appendRequests[2]?.session_id).toBe(selectedSessionId);
  expect(pullSessions.at(-1)).toBe(failedSessionId);
  await page.reload();
  await expect(page.getByText('The later queued worldline synchronized after recovery.')).toBeVisible();
  await expect(page.getByText('local reflection · no model call')).toHaveCount(0);
});

test('@mobile transient owner-check outages preserve the vault while invalid authority locks it', async ({ page }) => {
  test.setTimeout(60_000);
  let adminMode: 'owner' | 'transient' | 'invalid' = 'owner';
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: { id: 'owner-test' } }),
  }));
  await page.route('**/api/admin/check', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(adminMode === 'owner'
      ? { authorized: true }
      : adminMode === 'transient'
        ? { authorized: false, failureKind: 'upstream-unavailable' }
        : { authorized: false, failureKind: 'invalid-session' }),
  }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound', device_token: 'test-device-token',
      owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64), expires_at: '2099-01-01T00:00:00.000Z',
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/snapshot', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
      connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'retired' },
      memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
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

  await page.goto('/apocrypha');
  const sentinel = 'Transient verification must preserve this encrypted local worldline.';
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await expect(composer).toBeEnabled();
  await composer.fill(sentinel);
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText(sentinel, { exact: true })).toBeVisible();
  const lockBefore = await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'));

  await page.locator('#brain-system > summary').click();
  adminMode = 'transient';
  await page.getByRole('button', { name: 'Refresh evidence' }).click();
  await expect(page.getByText(/Owner verification is temporarily unavailable/i)).toBeVisible();
  await expect(page.getByText(sentinel, { exact: true })).toBeVisible();
  await expect(composer).toBeEnabled();
  expect(await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'))).toBe(lockBefore);

  adminMode = 'invalid';
  await page.getByRole('button', { name: 'Refresh evidence' }).click();
  await expect(page.getByText('This private projection is closed.')).toBeVisible();
  await expect(page.getByText(sentinel, { exact: true })).toHaveCount(0);
  const lockAfter = await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'));
  expect(lockAfter).toMatch(UUID_V4);
  expect(lockAfter).not.toBe(lockBefore);

  adminMode = 'owner';
  await page.reload();
  await expect(page.getByText(/MINI_BRAIN_SESSION_LOCKED/)).toBeVisible();
  await expect(page.getByText(sentinel, { exact: true })).toHaveCount(0);
  expect(await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'))).toBe(lockAfter);
});

test('@mobile sign-out locks every Brain tab before a failed remote logout returns', async ({ page, context }) => {
  test.setTimeout(60_000);
  await context.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await context.route('**/api/auth/me', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      user: {
        id: 'owner-test', email: 'owner@example.com', provider: 'test',
        createdAt: '2026-09-03T00:00:00.000Z',
      },
    }),
  }));
  await context.route('**/api/admin/check', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }),
  }));
  await context.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound', device_token: 'test-device-token',
      owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64), expires_at: '2099-01-01T00:00:00.000Z',
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await context.route('**/api/brain/snapshot', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
      connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'retired' },
      memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
      limits: { memories: 200, recent_messages: 120, source_messages: 200 },
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await context.route('**/api/brain/runtime/status', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.runtime-status.v1', status: 'degraded',
      reason_code: 'BRAIN_LOCAL_PROVIDER_DISABLED', observed_at: '2026-09-03T13:00:00.000Z',
      latency_ms: null, upstream_status: null, served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await context.route('**/fake-supabase/auth/v1/logout**', route => route.fulfill({ status: 204, body: '' }));

  await page.goto('/apocrypha');
  const queuedSecret = 'Queued private sentinel must disappear on sign-out.';
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await expect(composer).toBeEnabled();
  await composer.fill(queuedSecret);
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText(queuedSecret, { exact: true })).toBeVisible();
  await expect.poll(() => page.evaluate(async () => Boolean(await caches.match('/apocrypha')))).toBe(true);

  const peer = await context.newPage();
  await peer.goto('/apocrypha');
  await expect(peer.getByText(queuedSecret, { exact: true })).toBeVisible();
  const unsentSecret = 'Unsent private draft must clear before logout completes.';
  const peerComposer = peer.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await peerComposer.fill(unsentSecret);

  let releaseLogout: () => void = () => undefined;
  let markLogoutStarted: () => void = () => undefined;
  const logoutGate = new Promise<void>(resolve => { releaseLogout = resolve; });
  const logoutStarted = new Promise<void>(resolve => { markLogoutStarted = resolve; });
  await context.route('**/api/auth/logout', async route => {
    markLogoutStarted();
    await logoutGate;
    await route.fulfill({
      status: 503, contentType: 'application/json', body: JSON.stringify({ error: 'fixture unavailable' }),
    });
  });

  await page.goto('/account');
  await expect(page.getByRole('button', { name: 'Sign out of this browser' })).toBeVisible();
  await page.getByRole('button', { name: 'Sign out of this browser' }).click();
  await logoutStarted;
  await expect(peer.getByText(queuedSecret, { exact: true })).toHaveCount(0);
  await expect(peerComposer).toHaveValue('');
  await expect(peerComposer).toBeDisabled();
  await expect(peer.getByText(/Private Brain presentation was cleared because browser authority changed/i)).toBeVisible();
  const lockGeneration = await peer.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'));
  expect(lockGeneration).toMatch(UUID_V4);
  releaseLogout();
  await expect(page.getByText(/Sign-out did not clear every session surface/i)).toBeVisible();
  await expect.poll(() => peer.evaluate(async () => (
    await caches.keys()
  ).filter(key => key.startsWith('apocky-mini-brain-shell-')).length)).toBe(0);

  await peer.reload();
  await expect(peer.getByText(queuedSecret, { exact: true })).toHaveCount(0);
  await expect(peer.getByText(unsentSecret, { exact: true })).toHaveCount(0);
  await expect(peer.getByText(/MINI_BRAIN_SESSION_LOCKED/)).toBeVisible();
  expect(await peer.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'))).toBe(lockGeneration);
  await peer.close();
});

test('@mobile saved browser-session repair cannot lift a durable Brain sign-out lock', async ({ page, context }) => {
  test.setTimeout(60_000);
  let unlockRequests = 0;
  const staleSession = {
    access_token: 'stale-access-token',
    refresh_token: 'stale-refresh-token',
    expires_in: 3600,
    expires_at: Math.floor(Date.now() / 1000) + 3600,
    token_type: 'bearer',
    user: {
      id: 'owner-test',
      aud: 'authenticated',
      role: 'authenticated',
      email: 'owner@example.com',
      app_metadata: { provider: 'email', providers: ['email'] },
      user_metadata: {},
      identities: [],
      created_at: '2026-09-03T00:00:00.000Z',
    },
  };
  const durableLockGeneration = '66666666-6666-4666-8666-666666666666';
  await context.addInitScript(({ session, generation }) => {
    localStorage.setItem('sb-127-auth-token', JSON.stringify(session));
    localStorage.setItem('apocky-mini-brain-session-lock-v1', generation);
  }, { session: staleSession, generation: durableLockGeneration });
  await context.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await context.route('**/fake-supabase/auth/v1/token**', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({
      ...staleSession,
      access_token: 'refreshed-stale-access-token',
      refresh_token: 'refreshed-stale-refresh-token',
    }),
  }));
  await context.route('**/api/auth/session', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }),
  }));
  await context.route('**/api/brain/mobile/unlock', route => {
    unlockRequests += 1;
    return route.fulfill({ status: 500, contentType: 'application/json', body: JSON.stringify({ error: 'must not be called' }) });
  });
  await context.route('**/api/auth/me', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: { id: 'owner-test', email: 'owner@example.com', provider: 'test', createdAt: '2026-09-03T00:00:00.000Z' } }),
  }));
  await context.route('**/api/admin/check', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }),
  }));

  await page.goto('/login?next=/apocrypha');
  await expect(page.getByText(/saved session cannot cross the current sign-out boundary/i)).toBeVisible();
  await expect(page).toHaveURL(/\/login\?/u);
  await page.goto('/apocrypha');
  await expect(page.getByText(/MINI_BRAIN_SESSION_LOCKED/)).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' })).toBeDisabled();
  expect(await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'))).toBe(durableLockGeneration);
  expect(unlockRequests).toBe(0);
});

test('@mobile fresh owner reauthentication authorizes one lock epoch and purges the prior owner before offline use', async ({ page, context, request }, testInfo) => {
  test.setTimeout(60_000);
  let activeOwner: 'owner-test' | 'other-owner' = 'owner-test';
  const ownerRef = () => activeOwner === 'owner-test' ? TEST_OWNER_REF : OTHER_OWNER_REF;
  const lockGeneration = '77777777-7777-4777-8777-777777777777';
  await context.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await context.route('**/api/auth/me', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({
      user: { id: activeOwner, email: `${activeOwner}@example.com`, provider: 'test', createdAt: '2026-09-03T00:00:00.000Z' },
    }),
  }));
  await context.route('**/api/admin/check', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }),
  }));
  await context.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound', device_token: `token-${activeOwner}`,
      owner_ref: ownerRef(), key_thumbprint: 'b'.repeat(64), expires_at: '2099-01-01T00:00:00.000Z',
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await context.route('**/api/brain/snapshot', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({
      schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
      connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'retired' },
      memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
      limits: { memories: 200, recent_messages: 120, source_messages: 200 },
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await context.route('**/api/brain/runtime/status', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({
      schema_version: 'apocky.owner-brain.runtime-status.v1', status: 'degraded', reason_code: 'BRAIN_LOCAL_PROVIDER_DISABLED',
      observed_at: '2026-09-03T13:00:00.000Z', latency_ms: null, upstream_status: null,
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));

  // Compile the second route before WebKit owns a live Next development page.
  // A cold /login compile can otherwise make the dev HMR client reload the
  // still-committed /apocrypha document and cancel the intended navigation.
  await request.get('/login?next=%2Fapocrypha');
  // Owner A is not allowed to materialize a private vault merely because the
  // SSR test header says "owner". Seed the exact durable epoch/candidate that
  // a completed fresh reauthentication would have produced, then exercise the
  // full owner-B reauthentication below.
  await page.goto('/login?next=%2Fapocrypha');
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
  }, { generation: lockGeneration, expectedOwnerRef: TEST_OWNER_REF });
  await page.goto('/apocrypha');
  const priorOwnerSecret = 'Owner A private text must never reach owner B.';
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  await expect(composer).toBeEnabled();
  await composer.fill(priorOwnerSecret);
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText(priorOwnerSecret, { exact: true })).toBeVisible();
  await expect.poll(() => page.evaluate(async () => Boolean(await caches.match('/apocrypha')))).toBe(true);
  // Playwright cannot route a request once a live service worker has observed it,
  // even when that worker does not call respondWith. Release only this test's
  // Brain worker before the fake provider exchange; the fresh lock reinstalls it.
  await page.evaluate(async () => {
    const registrations = await navigator.serviceWorker.getRegistrations();
    await Promise.all(registrations
      .filter(registration => [registration.active, registration.waiting, registration.installing]
        .some(worker => worker?.scriptURL.endsWith('/brain-sw.js')))
      .map(registration => registration.unregister()));
  });

  activeOwner = 'other-owner';
  const freshSession = {
    access_token: 'fresh-owner-b-access-token', refresh_token: 'fresh-owner-b-refresh-token',
    expires_in: 3600, expires_at: Math.floor(Date.now() / 1000) + 3600, token_type: 'bearer',
    user: {
      id: 'other-owner', aud: 'authenticated', role: 'authenticated', email: 'other-owner@example.com',
      app_metadata: { provider: 'email', providers: ['email'] }, user_metadata: {}, identities: [],
      created_at: '2026-09-03T00:00:00.000Z',
    },
  };
  await context.route('**/fake-supabase/auth/v1/otp**', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({ message_id: 'fixture-message' }),
  }));
  await context.route('**/fake-supabase/auth/v1/verify**', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify(freshSession),
  }));
  const freshAuthTicket = 'fixture-owner-auth-attempt-'.repeat(8);
  await context.route('**/api/auth/attempt', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({
      schema_version: 'apocky.auth-fence.v1', status: 'ready', mode: 'fresh',
      ticket: freshAuthTicket, expires_at: new Date(Date.now() + 15 * 60_000).toISOString(),
      provider_start_delay_ms: 0,
    }),
  }));
  await context.route('**/api/auth/session', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }),
  }));
  let stagedEpoch: string | null = null;
  await context.route('**/api/brain/mobile/unlock', async route => {
    const body = await route.request().postDataJSON() as { lock_generation?: string; auth_attempt?: string };
    expect(body.auth_attempt).toBe(freshAuthTicket);
    stagedEpoch = body.lock_generation ?? null;
    await route.fulfill({
      status: 200, contentType: 'application/json', body: JSON.stringify({
        schema_version: 'apocky.mini-brain.owner-rebind.v1', status: 'rebind_authorized',
        owner_ref: OTHER_OWNER_REF, lock_generation: body.lock_generation,
      }),
    });
  });

  await page.goto('/login?next=/apocrypha');
  await page.getByLabel('Email address').fill('other-owner@example.com');
  await page.getByRole('button', { name: 'Send sign-in email' }).click();
  await page.getByLabel('One-time code').fill('123456');
  await page.getByRole('button', { name: 'Verify and continue' }).click();
  await page.waitForURL(url => url.pathname === '/apocrypha');
  await expect(page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' })).toBeEnabled();
  await expect(page.getByText(priorOwnerSecret, { exact: true })).toHaveCount(0);
  expect(stagedEpoch).toMatch(UUID_V4);
  expect(stagedEpoch).not.toBe(lockGeneration);
  expect(await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'))).toBe(stagedEpoch);
  const boundIdentity = await page.evaluate(async () => new Promise<{ owner_ref?: string; authorized_lock_generation?: string }>((resolve, reject) => {
    const request = indexedDB.open('apocky-mini-brain-v1');
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const database = request.result;
      const transaction = database.transaction('device', 'readonly');
      const read = transaction.objectStore('device').get('identity');
      read.onerror = () => reject(read.error);
      read.onsuccess = () => { database.close(); resolve(read.result ?? {}); };
    };
  }));
  expect(boundIdentity.owner_ref).toBe(OTHER_OWNER_REF);
  expect(boundIdentity.authorized_lock_generation).toBe(stagedEpoch);

  await page.reload({ waitUntil: 'domcontentloaded' });
  await expect.poll(() => page.evaluate(() => Boolean(navigator.serviceWorker.controller))).toBe(true);
  await expect.poll(() => page.evaluate(async () => Boolean(await caches.match('/apocrypha')))).toBe(true);
  if (testInfo.project.name === 'ios-webkit-iphone-15-pro') {
    testInfo.annotations.push({
      type: 'physical-device-gate',
      description: 'Playwright WebKit cannot complete an offline navigation; installed-iPhone offline launch remains a release gate.',
    });
    return;
  }
  await context.setOffline(true);
  await page.goto('/apocrypha', { waitUntil: 'domcontentloaded' });
  await expect(page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' })).toBeEnabled();
  await expect(page.getByText(priorOwnerSecret, { exact: true })).toHaveCount(0);
  await expect(page.getByText(/Offline · encrypted recent history/i)).toBeVisible();
});

test('@mobile a sign-out lock arriving during owner registration wins the open race', async ({ page }) => {
  test.setTimeout(60_000);
  let releaseRegistration: () => void = () => undefined;
  let markRegistrationStarted: () => void = () => undefined;
  const registrationGate = new Promise<void>(resolve => { releaseRegistration = resolve; });
  const registrationStarted = new Promise<void>(resolve => { markRegistrationStarted = resolve; });
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', async route => {
    markRegistrationStarted();
    await registrationGate;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound', device_token: 'test-device-token',
        owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64), expires_at: '2099-01-01T00:00:00.000Z',
        served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
      }),
    });
  });

  await page.goto('/apocrypha');
  await registrationStarted;
  const signOutGeneration = await page.evaluate(() => {
    const generation = crypto.randomUUID();
    localStorage.setItem('apocky-mini-brain-session-lock-v1', generation);
    return generation;
  });
  releaseRegistration();

  await expect(page.getByText(/MINI_BRAIN_SESSION_LOCKED/)).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' })).toBeDisabled();
  expect(await page.evaluate(() => localStorage.getItem('apocky-mini-brain-session-lock-v1'))).toBe(signOutGeneration);
});

test('@mobile reviewing a terminal turn keeps its draft bound to the failed worldline', async ({ page }) => {
  test.setTimeout(60_000);
  let runtimeLive = false;
  let failedOriginalRequestId: string | null = null;
  const appendRequests: Array<Record<string, unknown>> = [];
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound', device_token: 'test-device-token',
      owner_ref: TEST_OWNER_REF, key_thumbprint: 'b'.repeat(64), expires_at: '2099-01-01T00:00:00.000Z',
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/snapshot', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
      connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: runtimeLive ? 'live' : 'retired' },
      memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
      limits: { memories: 200, recent_messages: 120, source_messages: 200 },
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/runtime/status', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.runtime-status.v1', status: runtimeLive ? 'live' : 'degraded',
      reason_code: runtimeLive ? null : 'BRAIN_LOCAL_PROVIDER_DISABLED', observed_at: '2026-09-03T13:00:00.000Z',
      latency_ms: runtimeLive ? 4 : null, upstream_status: runtimeLive ? 200 : null,
      served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/runtime/sessions', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({ sessions: [] }),
  }));
  await page.route('**/api/brain/mobile/sync', async route => {
    const request = await route.request().postDataJSON() as Record<string, unknown>;
    if (request.operation === 'pull') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          schema_version: 'apocky.mini-brain.sync-response.v1', status: 'empty',
          session_id: request.session_id, request_id: request.request_id, acknowledged_request_ids: [],
          cursor: null, messages: [], tombstones: [], events_truncated: false,
          provenance: { transport: 'owner_bound_apocv4_runtime', privacy_partition_ref: null, principal_ref: null, binding_ref: null },
          controls: { owner_session: 'verified', device_signature: 'verified', replay: 'bounded_sequence_and_idempotent_request', rate_limit: 'owner_durable_window', partition: 'server_derived_owner' },
          served_by: 'fixture', ts: '2026-09-03T13:00:00.000Z',
        }),
      });
      return;
    }
    appendRequests.push(request);
    failedOriginalRequestId ??= String(request.request_id);
    if (request.request_id === failedOriginalRequestId) {
      await route.fulfill({
        status: 409,
        contentType: 'application/json',
        body: JSON.stringify({
          error: 'Terminal model failure.', code: 'BRAIN_SYNC_TERMINAL_FAILED',
          request_id: request.request_id, session_id: request.session_id, current_cursor: 'a'.repeat(64),
          error_class: 'TerminalModelFailure', error_digest: 'c'.repeat(64), reissue_safe: false, retryable: false,
        }),
      });
      return;
    }
    const text = (request.payload as { text: string }).text;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.mini-brain.sync-response.v1', status: 'appended',
        session_id: request.session_id, request_id: request.request_id,
        acknowledged_request_ids: [request.request_id], cursor: appendRequests.length === 2 ? 'd'.repeat(64) : 'e'.repeat(64),
        messages: [
          { role: 'user', content: text, request_id: request.request_id, recorded_at: '2026-09-03T13:00:01.000Z', event_digest: 'e'.repeat(64) },
          { role: 'assistant', content: `Committed: ${text}`, request_id: request.request_id, recorded_at: '2026-09-03T13:00:02.000Z', event_digest: 'f'.repeat(64) },
        ],
        tombstones: [], events_truncated: false,
        provenance: { transport: 'owner_bound_apocv4_runtime', privacy_partition_ref: null, principal_ref: null, binding_ref: null },
        controls: { owner_session: 'verified', device_signature: 'verified', replay: 'bounded_sequence_and_idempotent_request', rate_limit: 'owner_durable_window', partition: 'server_derived_owner' },
        served_by: 'fixture', ts: '2026-09-03T13:00:02.000Z',
      }),
    });
  });

  await page.goto('/apocrypha');
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha / Mini Brain' });
  const failedText = 'Edit this failed worldline without crossing sessions.';
  await expect(composer).toBeEnabled();
  await composer.fill(failedText);
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await page.getByRole('button', { name: 'New' }).click();
  const laterSession = await page.evaluate(() => sessionStorage.getItem('apocky.owner-brain.session.v1'));
  const laterText = 'This later worldline must synchronize second.';
  await composer.fill(laterText);
  await page.getByRole('button', { name: 'Reflect + queue' }).click();
  await expect(page.getByText('2 encrypted turns waiting')).toBeVisible();

  runtimeLive = true;
  await page.locator('#brain-system > summary').click();
  await page.getByRole('button', { name: 'Refresh evidence' }).click();
  await expect(page.getByRole('button', { name: 'Review / edit turn' })).toBeVisible();
  expect(appendRequests).toHaveLength(1);
  const failedSession = String(appendRequests[0]?.session_id);
  expect(failedSession).not.toBe(laterSession);

  await page.getByRole('button', { name: 'Review / edit turn' }).click();
  await expect(composer).toHaveValue(failedText);
  await expect(page.getByText(/encrypted queue item remains recoverable while you edit/i)).toBeVisible();
  expect(appendRequests).toHaveLength(1);

  await page.reload();
  await expect(page.getByRole('button', { name: 'Review / edit turn' })).toBeVisible();
  await expect(page.getByText('2 encrypted turns waiting')).toBeVisible();
  expect(appendRequests).toHaveLength(2);
  expect(appendRequests[1]?.request_id).toBe(failedOriginalRequestId);
  await page.getByRole('button', { name: 'Review / edit turn' }).click();
  await expect(composer).toHaveValue(failedText);

  await page.getByRole('button', { name: 'Send + sync' }).click();
  await expect.poll(() => appendRequests.length).toBe(4);
  expect(appendRequests[2]?.session_id).toBe(failedSession);
  expect(appendRequests[2]?.request_id).not.toBe(failedOriginalRequestId);
  expect((appendRequests[2]?.payload as { text?: string }).text).toBe(failedText);
  expect(appendRequests[3]?.session_id).toBe(laterSession);
  expect((appendRequests[3]?.payload as { text?: string }).text).toBe(laterText);
  await expect(page.getByText(`Committed: ${laterText}`)).toBeVisible();
  await expect(page.getByText(/encrypted turn waiting/)).toHaveCount(0);
});

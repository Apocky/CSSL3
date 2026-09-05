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
  let observationRequests = 0;
  await page.route('**/api/brain/observe?*', route => {
    observationRequests += 1;
    return route.fulfill({ json: {
      schema_version: 'apocky.brain.observation.v1', view: 'status', observed_at: '2026-09-04T00:00:00.000Z',
      trace_id: '44444444-4444-4444-8444-444444444444', data: { state: 'ACTIVE', event_count: 7, error_count: 2, ring_size: 7, ring_capacity: 100 },
    } });
  });
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
  await page.goto('/brain');
  await expect(page).toHaveURL(/\/brain$/);
  await expect(page.locator('link[rel="manifest"]')).toHaveCount(1);
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute('href', '/brain-manifest.json');

  await expect(page.getByRole('heading', { level: 1, name: 'Apocrypha' })).toBeVisible();
  await expect(page.locator('.apx-diagnostics-opener')).toHaveCount(0);
  const connectionDetails = page.locator('summary').filter({ hasText: 'Connection & device details' });
  await expect(connectionDetails).toBeVisible();
  await expect(page.getByText('Mneme storage')).not.toBeVisible();
  await expect(page.getByLabel('Find a memory, topic, or phrase')).not.toBeVisible();
  expect(observationRequests).toBe(0);
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha' });
  await expect(composer).toBeEnabled();
  await expect(page.getByText('Desktop connection unavailable. Your message will stay encrypted here until it can be delivered.')).toBeVisible();
  await composer.fill('How do I preserve the source boundary?');
  await page.getByRole('button', { name: 'Queue message' }).click();
  await expect(page.getByRole('log').locator('article[data-role="user"]')).toHaveCount(1);
  await expect(page.getByRole('log').locator('article[data-role="assistant"]')).toHaveCount(0);
  await expect(page.getByText(/encrypted queue · not yet committed/i)).toBeVisible();
  await connectionDetails.click();
  await expect(page.getByText('Mneme storage')).toBeVisible();
  const releaseShelf = page.locator('#brain-releases');
  await expect(releaseShelf.locator('em[data-release-state="RELEASED"]')).toHaveText('Released');
  await expect(releaseShelf.locator('summary').getByText('1.0.0 · integrity-linked evidence')).toBeVisible();
  await releaseShelf.locator('summary').click();
  await expect(releaseShelf.getByRole('link', { name: /Living plan/i })).toHaveAttribute('href', '/releases/apocrypha-living/plan.json');
  await expect(releaseShelf.getByRole('link', { name: /Changelog/i })).toHaveAttribute('href', '/releases/apocrypha-living/changelog.json');
  await expect(releaseShelf.getByRole('link', { name: /Build manifest/i })).toHaveAttribute('href', '/releases/apocrypha-living/manifest.json');
  await expect(releaseShelf.locator('a[href^="/downloads/"]')).toHaveCount(0);
  await expect(releaseShelf.getByRole('link', { name: 'Android downloads and iPhone availability' })).toHaveAttribute('href', '/download/apocrypha');
  await releaseShelf.locator('summary').click();
  const diagnostics = page.locator('details').filter({ has: page.locator('summary').filter({ hasText: /^Desktop diagnostics$/ }) }).last();
  await diagnostics.locator('summary').first().click();
  expect(observationRequests).toBe(0);
  await diagnostics.getByRole('button', { name: 'Refresh', exact: true }).click();
  await expect(diagnostics.getByText('Recorded events', { exact: true })).toBeVisible();
  await expect(diagnostics.locator('dd').first()).toHaveText('7');
  expect(observationRequests).toBe(1);
  await page.screenshot({ path: testInfo.outputPath('brain-diagnostics.png'), fullPage: true });
  await diagnostics.locator('summary').first().click();
  await connectionDetails.click();
  await page.screenshot({ path: testInfo.outputPath('brain-conversation.png'), fullPage: true });

  await page.getByRole('button', { name: 'Memory', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Memory', exact: true })).toHaveAttribute('aria-expanded', 'true');
  await page.getByLabel('Find a memory, topic, or phrase').fill('source boundary');
  await expect(page.getByText('2 of 3 loaded records match')).toBeVisible();
  const firstNode = page.locator('button').filter({ hasText: 'project.brain.boundary' }).first();
  await firstNode.click();
  await expect(page.getByRole('heading', { level: 3, name: 'project.brain.boundary' })).toBeVisible();
  await expect(page.getByText('Do not detach a conclusion from where it came from.')).toBeVisible();
  await expect(page.getByText(/preserves exact topic, time, CSL, and source-message links/i)).toBeVisible();

  await page.getByRole('button', { name: 'timeline' }).click();
  await expect(page.getByText('The personal Brain remains owner-private and crawler-dark.', { exact: true })).toBeVisible();
  await expect(page.getByText('Mneme storage')).not.toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
  const a11y = await new AxeBuilder({ page }).analyze();
  expect(a11y.violations.filter(item => ['serious', 'critical'].includes(item.impact ?? ''))).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath('brain.png'), fullPage: true });
});

test('offline user-only queue receives the actual desktop reply after reconnecting', async ({ page, context }) => {
  const sessionId = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
  const initialRequestId = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
  const initialMessages = [
    {
      role: 'user', content: 'Where did this worldline pause?', request_id: initialRequestId,
      recorded_at: '2026-09-04T20:00:00.000Z', event_digest: '1'.repeat(64),
    },
    {
      role: 'assistant', content: 'At the verified G12 relay boundary.', request_id: initialRequestId,
      recorded_at: '2026-09-04T20:00:01.000Z', event_digest: '2'.repeat(64),
    },
  ];
  await page.setExtraHTTPHeaders({ 'x-apocky-test-admin-email': 'owner@example.com' });
  await page.route('**/api/auth/me', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ user: { id: 'owner-test' } }) }));
  await page.route('**/api/admin/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ authorized: true }) }));
  await page.route('**/api/brain/mobile/device', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.mini-brain.device-registration.v1', status: 'bound',
      device_token: 'test-device-token', owner_ref: 'a'.repeat(64), key_thumbprint: 'b'.repeat(64),
      expires_at: '2099-01-01T00:00:00.000Z', served_by: 'fixture', ts: '2026-09-04T20:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/snapshot', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live',
      connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'live' },
      memories: [], messages: [], counts: { memories: 0, messages: 0, source_links: 0 },
      limits: { memories: 200, recent_messages: 120, source_messages: 200 },
      served_by: 'fixture', ts: '2026-09-04T20:00:00.000Z',
    }),
  }));
  let connected = false;
  await page.route('**/api/brain/runtime/status', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.runtime-status.v1', status: connected ? 'live' : 'degraded', reason_code: connected ? null : 'BRAIN_OFFLINE',
      observed_at: '2026-09-04T20:00:00.000Z', latency_ms: 5, upstream_status: 200,
      served_by: 'fixture', ts: '2026-09-04T20:00:00.000Z',
    }),
  }));
  await page.route('**/api/brain/runtime/sessions', route => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      schema_version: 'apocky.owner-brain.sessions.v1', status: 'live', history_surface: 'g12_chat_history',
      discovery_scope: 'latest_conversation_only',
      sessions: [{
        session_id: sessionId, title: 'Where did this worldline pause?',
        updated_at: '2026-09-04T20:00:01.000Z', message_count: 2,
      }],
      count: 1, served_by: 'fixture', ts: '2026-09-04T20:00:00.000Z',
    }),
  }));
  let appendRequestId = '';
  await page.route('**/api/brain/mobile/sync', async route => {
    const request = await route.request().postDataJSON() as {
      operation: 'pull' | 'append'; request_id: string; session_id: string; payload?: { text?: string } | null;
    };
    const appended = request.operation === 'append';
    if (appended) appendRequestId = request.request_id;
    const preceding = request.session_id === sessionId ? initialMessages : [];
    const messages = appended ? [
      ...preceding,
      {
        role: 'user', content: request.payload?.text ?? '', request_id: request.request_id,
        recorded_at: '2026-09-04T20:01:00.000Z', event_digest: '3'.repeat(64),
      },
      {
        role: 'assistant', content: 'This answer survived the signed queue and G12 readback.', request_id: request.request_id,
        recorded_at: '2026-09-04T20:01:01.000Z', event_digest: '4'.repeat(64),
      },
    ] : preceding;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'apocky.mini-brain.sync-response.v1', status: appended ? 'appended' : 'advanced',
        session_id: request.session_id, request_id: request.request_id, cursor: appended ? '6'.repeat(64) : '5'.repeat(64),
        messages, tombstones: [], events_truncated: false,
        provenance: {
          transport: 'owner_bound_apocv4_runtime', privacy_partition_ref: '7'.repeat(64),
          principal_ref: '8'.repeat(64), binding_ref: '9'.repeat(64),
        },
        controls: {
          owner_session: 'verified', device_signature: 'verified',
          replay: 'bounded_sequence_and_idempotent_request', rate_limit: 'relay_instance_burst',
          partition: 'server_derived_owner',
        },
        served_by: 'fixture', ts: '2026-09-04T20:01:01.000Z',
      }),
    });
  });

  await page.goto('/brain');
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha' });
  await expect(composer).toBeEnabled();
  await context.setOffline(true);
  await expect(page.getByRole('button', { name: 'Queue message', exact: true })).toBeVisible();
  await composer.fill('Carry this exact request across the device boundary.');
  await page.getByRole('button', { name: 'Queue message', exact: true }).click();
  await expect(page.getByRole('log').locator('article[data-role="user"]')).toHaveCount(1);
  await expect(page.getByRole('log').locator('article[data-role="assistant"]')).toHaveCount(0);
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();
  expect(appendRequestId).toBe('');
  connected = true;
  await context.setOffline(false);
  await expect(page.getByText('This answer survived the signed queue and G12 readback.')).toBeVisible();
  await expect(page.locator('summary').filter({ hasText: 'Connection & device details' })).toContainText('Desktop connected');
  await expect(page.getByRole('button', { name: 'Send', exact: true })).toBeVisible();
  await expect(page.getByText('1 encrypted turn waiting')).toHaveCount(0);
  await expect(page.getByText('Device queue and desktop worldline are current.')).toBeVisible();
  expect(appendRequestId).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u);
  await expect(page.getByText('encrypted queue · not yet committed')).toHaveCount(0);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
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

  await page.goto('/brain');
  await expect(page.getByText('Mneme needs your confirmation.')).toBeVisible();
  await page.getByRole('button', { name: 'Create my private memory profile' }).click();
  await expect(page.getByText(/Private Mneme profile created for this verified owner session/i)).toBeVisible();
  await page.locator('summary').filter({ hasText: 'Connection & device details' }).click();
  await expect(page.getByText('0 records · 0 source links')).toBeVisible();
  expect(confirmation).toBe('CREATE_OWNER_PRIVATE_MNEME_PROFILE');
});

test('@mobile installed Mini Brain restores an encrypted queued worldline offline', async ({ page, context }, testInfo) => {
  test.setTimeout(120_000);
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

  await page.goto('/');
  await page.evaluate(async () => {
    const legacy = await navigator.serviceWorker.register('/brain-sw.js', { scope: '/' });
    const worker = legacy.installing ?? legacy.waiting ?? legacy.active;
    if (worker && worker.state !== 'activated') {
      await new Promise<void>((resolve, reject) => {
        worker.addEventListener('statechange', () => {
          if (worker.state === 'activated') resolve();
          if (worker.state === 'redundant') reject(new Error('Legacy worker did not activate'));
        });
      });
    }
    await (await caches.open('apocky-mini-brain-shell-v1')).put('/apocrypha', new Response('legacy shell fixture'));
    await (await caches.open('unrelated-shell-fixture')).put('/unrelated-shell-fixture', new Response('preserve unrelated cache'));
  });

  const onlineDocument = await page.goto('/brain');
  expect(onlineDocument?.status()).toBe(200);
  expect(onlineDocument?.headers()['cache-control']).toContain('no-store');
  await expect(page.locator('meta[name="viewport"]')).toHaveAttribute('content', /viewport-fit=cover/);
  await expect(page.locator('.apx-diagnostics-opener')).toHaveCount(0);
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute('href', '/brain-manifest.json');
  const manifestResponse = await page.request.get('/brain-manifest.json');
  expect(manifestResponse.ok()).toBe(true);
  expect(await manifestResponse.json()).toMatchObject({
    id: '/brain',
    start_url: '/brain?source=installed-mini-brain',
    scope: '/brain',
    display: 'standalone',
  });
  const composer = page.getByRole('textbox', { name: 'Message Apocrypha' });
  await expect(composer).toBeEnabled();
  await composer.fill('What is the smallest reversible move?');
  await page.getByRole('button', { name: 'Queue message' }).click();
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();
  await expect(page.getByRole('log').locator('article[data-role="assistant"]')).toHaveCount(0);

  const worker = await page.evaluate(async () => {
    const registration = await navigator.serviceWorker.ready;
    return { scope: registration.scope, scriptURL: registration.active?.scriptURL ?? '' };
  });
  expect(new URL(worker.scope).pathname).toBe('/brain');
  expect(new URL(worker.scriptURL).pathname).toBe('/brain-sw.js');
  await page.locator('summary').filter({ hasText: 'Connection & device details' }).click();
  await expect(page.getByText('Offline shell ready')).toBeVisible();
  expect(await page.evaluate(async () => (await navigator.serviceWorker.getRegistrations())
    .filter(registration => registration.active?.scriptURL.endsWith('/brain-sw.js'))
    .map(registration => new URL(registration.scope).pathname))).toEqual(['/brain']);
  expect(await page.evaluate(async () => (await caches.keys()).includes('apocky-mini-brain-shell-v1'))).toBe(false);
  expect(await page.evaluate(async () => (await caches.keys()).includes('unrelated-shell-fixture'))).toBe(true);
  const publicPage = await context.newPage();
  for (const publicPath of ['/apocrypha', '/account', '/login']) {
    await publicPage.goto(publicPath, { waitUntil: 'domcontentloaded' });
    expect(await publicPage.evaluate(() => navigator.serviceWorker.controller?.scriptURL.endsWith('/brain-sw.js') ?? false)).toBe(false);
  }
  await publicPage.close();
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
  await expect.poll(() => page.evaluate(async () => Boolean(await caches.match('/brain')))).toBe(true);
  await page.evaluate(async () => { await fetch('/api/brain/snapshot', { cache: 'no-store' }); });
  const cachedPrivateApiUrls = await page.evaluate(async () => {
    const names = await caches.keys();
    const groups = await Promise.all(names.map(async name => (await caches.open(name)).keys()));
    return groups.flat().map(request => new URL(request.url).pathname).filter(path => path.startsWith('/api/'));
  });
  expect(cachedPrivateApiUrls).toEqual([]);
  await context.setOffline(true);
  await page.goto('/brain', { waitUntil: 'domcontentloaded' }).catch((error: unknown) => {
    if (
      !testInfo.project.name.startsWith('ios-webkit')
      || !(error instanceof Error)
      || !error.message.includes('WebKit encountered an internal error')
    ) throw error;
  });
  await expect(page.getByRole('heading', { level: 1, name: 'Apocrypha' })).toBeVisible();
  await expect(page.getByText('1 encrypted turn waiting')).toBeVisible();
  await expect(page.getByText('What is the smallest reversible move?')).toBeVisible();
  await expect(page.getByText('Desktop connection unavailable. Your message will stay encrypted here until it can be delivered.')).toBeVisible();
  await expect(page.getByRole('log').locator('article[data-role="assistant"]')).toHaveCount(0);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
  const a11y = await new AxeBuilder({ page }).analyze();
  expect(a11y.violations.filter(item => ['serious', 'critical'].includes(item.impact ?? ''))).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath('mini-brain-offline.png'), fullPage: true });
  await page.setViewportSize({ width: 320, height: 568 });
  const minimumWidthOverflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(minimumWidthOverflow).toBeLessThanOrEqual(1);
});

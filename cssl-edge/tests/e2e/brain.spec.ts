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
  await expect(page.getByRole('heading', { level: 1, name: 'Brain' })).toBeVisible();
  await expect(page.getByText('Mneme storage')).toBeVisible();
  await expect(page.getByText('not connected · conversation read-only')).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'Message your local Apocrypha' })).toBeDisabled();
  await expect(page.getByText(/New generated turns stay disabled until the server observes/i)).toBeVisible();
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
  await expect(page.getByText('The personal Brain remains owner-private and crawler-dark.')).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
  const a11y = await new AxeBuilder({ page }).analyze();
  expect(a11y.violations.filter(item => ['serious', 'critical'].includes(item.impact ?? ''))).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath('brain.png'), fullPage: true });
});

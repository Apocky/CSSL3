import fs from 'node:fs';
import path from 'node:path';

import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page, type Route } from '@playwright/test';

const OWNER_PROMPT = 'Design the smallest verified cache improvement.';
const PROPOSAL_SUMMARY = 'Add a measured prefix-cache seam before changing model weights.';
const PROPOSAL_STEPS = [
  'Capture baseline cache-hit rate and time to first token.',
  'Enable stable-prefix reuse for one bounded route.',
  'Retain only if latency improves without answer-quality regression.',
] as const;

const HEALTH = {
  schema_version: 'apocky.apocv4-runtime-proxy.v1',
  kind: 'health',
  observed: {
    evidence_lane: 'observed_runtime_http',
    receipt: {
      observed_at: '2026-08-02T22:20:00.000Z',
      latency_ms: 318,
      upstream_status: 200,
      auth_mode: 'STRICT_REGISTRY',
      auth_registry_ref: 'a'.repeat(64),
      binding_ref: 'b'.repeat(64),
      principal_ref: 'c'.repeat(64),
      privacy_partition_ref: null,
    },
    runtime: { schema_version: 'apocv4.runtime-service.v1', status: 'READY', engine: {}, vision: true },
  },
  model_reported: {
    evidence_lane: 'model_reported',
    present: false,
    note: 'Health contains runtime observations only.',
  },
};

function objectiveResult() {
  return {
    schema_version: 'apocky.apocv4-runtime-proxy.v1',
    kind: 'objective',
    observed: {
      evidence_lane: 'observed_runtime_http_and_test_receipts',
      receipt: {
        observed_at: '2026-08-02T22:20:08.000Z',
        latency_ms: 8120,
        upstream_status: 200,
        auth_mode: 'STRICT_REGISTRY',
        auth_registry_ref: 'a'.repeat(64),
        binding_ref: 'b'.repeat(64),
        principal_ref: 'c'.repeat(64),
        privacy_partition_ref: null,
      },
      runtime: {
        schema_version: 'apocv4.objective-result.v1',
        status: 'ACCEPTED',
        terminal_reason: 'candidate_accepted',
        max_iterations: 1,
        iterations_completed: 1,
        accepted_candidate_digest: 'd'.repeat(64),
        last_test_run_digest: 'e'.repeat(64),
        checkpoint_digest: 'f'.repeat(64),
        faculty_team_id: 'apocv4-frontier-council',
      },
      attempts: [{ sequence: 1, test_passed: true, test_run_digest: 'e'.repeat(64) }],
    },
    model_reported: {
      evidence_lane: 'model_reported_not_observed_fact',
      note: 'Visible model output, not hidden chain-of-thought.',
      attempts: [{
        sequence: 1,
        active_model_id: 'fixture-frontier-coder',
        council_decision: {
          candidate: {
            proposal: { summary: PROPOSAL_SUMMARY, steps: PROPOSAL_STEPS },
            countercase: 'Prefix churn could erase the projected gain.',
            falsifier: 'No improvement over three matched requests.',
            source_refs: ['fixture://cache-observation'],
          },
        },
      }],
    },
  };
}

async function stubOwner(page: Page, objectiveHandler?: (route: Route) => Promise<void>): Promise<void> {
  await page.route('**/api/admin/check', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    headers: { 'Cache-Control': 'no-store' },
    body: JSON.stringify({ authorized: true, email: 'owner@example.test' }),
  }));
  await page.route('**/api/admin/apocv4/health', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    headers: { 'Cache-Control': 'no-store' },
    body: JSON.stringify(HEALTH),
  }));
  await page.route('**/api/admin/apocv4/objective', objectiveHandler ?? (async (route) => {
    const body = route.request().postDataJSON() as { objective?: unknown };
    expect(body).toEqual({ objective: OWNER_PROMPT });
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      headers: { 'Cache-Control': 'no-store' },
      body: JSON.stringify(objectiveResult()),
    });
  }));
}

async function expectNoHorizontalOverflow(page: Page): Promise<void> {
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
}

test('@visual Apex is a responsive communication hub with a real council answer', async ({ page }) => {
  test.setTimeout(90_000);
  const browserErrors: string[] = [];
  page.on('pageerror', (error) => browserErrors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') browserErrors.push(message.text());
  });
  await stubOwner(page);
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto('/admin/apex');

  await expect(page.getByRole('heading', { level: 1, name: 'What are we building?' })).toBeVisible();
  await expect(page.getByText('Conversation history')).toBeVisible();
  await expect(page.getByText('this browser only')).toBeVisible();
  const composer = page.getByLabel('Message Apocrypha');
  await expect(composer).toBeVisible();
  await composer.fill(OWNER_PROMPT);
  await composer.press('Enter');

  await expect(page.getByText(PROPOSAL_SUMMARY).first()).toBeVisible({ timeout: 10_000 });
  for (const step of PROPOSAL_STEPS) await expect(page.getByText(step).first()).toBeVisible();
  await expect(page.getByText(/browser attachment pending/i)).toBeVisible();
  await expect(page.getByRole('button', { name: /vision|attach|search|voice/i })).toHaveCount(0);

  await page.getByRole('button', { name: 'Evidence' }).last().click();
  await expect(page.getByText('Observed receipt')).toBeVisible();
  await expect(page.getByText('Proposal text is model-reported.')).toBeVisible();
  await expect(page.locator('body')).not.toContainText(/APOCV4_API_TOKEN|privacy_partition|Bearer [A-Za-z0-9]/i);
  await expectNoHorizontalOverflow(page);

  const axe = await new AxeBuilder({ page }).analyze();
  const serious = axe.violations.filter((item) => item.impact === 'serious' || item.impact === 'critical');
  expect(serious, JSON.stringify(serious, null, 2)).toEqual([]);

  const artifactRoot = path.join(process.cwd(), 'test-results', 'apex-visual');
  fs.mkdirSync(artifactRoot, { recursive: true });
  await page.screenshot({ path: path.join(artifactRoot, '1440x900-apex-conversation.png'), fullPage: false });

  await page.reload();
  await expect(page.getByText(PROPOSAL_SUMMARY).first()).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByLabel('Message Apocrypha')).toBeVisible();
  await expectNoHorizontalOverflow(page);
  await page.screenshot({ path: path.join(artifactRoot, '390x844-apex-conversation.png'), fullPage: false });

  await page.getByRole('button', { name: 'Open conversations' }).click();
  await expect(page.getByRole('navigation', { name: 'Conversation history' })).toBeVisible();
  await page.getByRole('button', { name: 'Close conversations' }).click();
  expect(browserErrors, browserErrors.join('\n')).toEqual([]);
});

test('Apex stop is a real abort and never fabricates a final receipt', async ({ page }) => {
  await stubOwner(page, async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 1_000));
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(objectiveResult()) });
  });
  await page.goto('/admin/apex');
  const composer = page.getByLabel('Message Apocrypha');
  await composer.fill(OWNER_PROMPT);
  await page.getByRole('button', { name: /^Send/ }).click();
  await page.getByRole('button', { name: 'Stop' }).click();
  await expect(page.getByText(/Stopped locally\. Upstream completion is unknown/i)).toBeVisible();
  await expect(page.getByText('No final receipt')).toBeVisible();
  await expect(page.getByText(PROPOSAL_SUMMARY)).toHaveCount(0);
});

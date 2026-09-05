import fs from 'node:fs';
import path from 'node:path';

import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page, type Route } from '@playwright/test';

const OWNER_PROMPT = 'Design the smallest verified cache improvement.';
const SESSION_KEY = 'apocky.apocv4.communication-hub.v1';
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

async function expectApexRegionsWithinViewport(page: Page): Promise<Record<string, unknown>> {
  const canvas = page.locator('[data-canvasui-synthesis="grid+glyph-rain+force-field+particle-reveal"]');
  const shell = canvas.locator('..');
  const composer = page.getByLabel('Message Apocrypha');
  const conversation = page.locator('[aria-label="Apocrypha relay"]');
  const regions = {
    shell,
    conversation,
    topbar: conversation.locator('header').first(),
    timeline: conversation.locator('[aria-live="polite"]'),
    composerDock: composer.locator('xpath=ancestor::form[1]/..'),
  };
  const viewportWidth = page.viewportSize()?.width;
  expect(viewportWidth).toBeTruthy();

  await expect.poll(async () => shell.evaluate((element) => ({
    clientWidth: element.clientWidth,
    scrollWidth: element.scrollWidth,
    fits: element.scrollWidth <= element.clientWidth + 1,
  })), {
    message: 'Apex shell must settle without internal horizontal overflow',
    timeout: 5_000,
  }).toMatchObject({ fits: true });

  const geometry: Record<string, unknown> = {
    viewportWidth,
    shellWidths: await shell.evaluate((element) => ({
      clientWidth: element.clientWidth,
      scrollWidth: element.scrollWidth,
    })),
  };
  for (const [name, region] of Object.entries(regions)) {
    await expect(region, `${name} must be rendered`).toBeVisible();
    const rect = await region.boundingBox();
    expect(rect, `${name} must have a layout box`).not.toBeNull();
    expect(rect!.x, `${name} left edge`).toBeGreaterThanOrEqual(-1);
    expect(rect!.x + rect!.width, `${name} right edge`).toBeLessThanOrEqual(viewportWidth! + 1);
    geometry[name] = {
      left: rect!.x,
      width: rect!.width,
      right: rect!.x + rect!.width,
    };
  }
  return geometry;
}

async function installScrollIntoViewSpy(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const calls: Array<boolean | ScrollIntoViewOptions | undefined> = [];
    Object.defineProperty(window, '__apexScrollIntoViewCalls', { value: calls, configurable: true });
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function observedScrollIntoView(options?: boolean | ScrollIntoViewOptions): void {
      calls.push(typeof options === 'object' && options ? { ...options } : options);
      original.call(this, options as ScrollIntoViewOptions);
    };
  });
}

async function expectReducedMotionApplied(page: Page): Promise<void> {
  await expect.poll(() => page.evaluate(() => window.matchMedia('(prefers-reduced-motion: reduce)').matches)).toBe(true);
  const motion = await page.locator('[aria-live="polite"]').evaluate((element) => ({
    scrollBehavior: getComputedStyle(element).scrollBehavior,
  }));
  expect(motion.scrollBehavior).toBe('auto');
}

async function gotoApex(page: Page): Promise<void> {
  let lastStatus: number | undefined;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    const response = await page.goto('/admin/apex', { waitUntil: 'domcontentloaded' });
    lastStatus = response?.status();
    if (lastStatus === 200) {
      try {
        await page.getByLabel('Message Apocrypha').waitFor({ state: 'visible', timeout: 5_000 });
        return;
      } catch {
        // Next dev can transiently serve an incomplete navigation while compiling.
      }
    }
  }
  throw new Error(`Apex did not become ready after 3 bounded navigations (last HTTP ${lastStatus ?? 'unknown'}).`);
}

test('@visual Apex is a responsive communication hub with a real council answer', async ({ page }) => {
  test.setTimeout(90_000);
  const browserErrors: string[] = [];
  page.on('pageerror', (error) => browserErrors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') browserErrors.push(message.text());
  });
  await installScrollIntoViewSpy(page);
  await stubOwner(page);
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoApex(page);

  const canvasField = page.locator('[data-canvasui-synthesis="grid+glyph-rain+force-field+particle-reveal"]');
  await expect(canvasField).toHaveCount(1);
  await expect(canvasField).toBeVisible();
  await expect(page.getByRole('heading', { level: 1, name: 'Enter the thoughtspace.' })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Conversation history' })).toBeVisible();
  await expect(page.getByText(/Local history stays in this browser until you clear it/i)).toBeVisible();
  await expectReducedMotionApplied(page);
  const composer = page.getByLabel('Message Apocrypha');
  await expect(composer).toBeVisible();
  await composer.fill(OWNER_PROMPT);
  await composer.press('Enter');

  await expect(page.getByText(PROPOSAL_SUMMARY).first()).toBeVisible({ timeout: 10_000 });
  for (const step of PROPOSAL_STEPS) await expect(page.getByText(step).first()).toBeVisible();
  const scrollBehaviors = await page.evaluate(() => (
    (window as unknown as { __apexScrollIntoViewCalls: Array<boolean | ScrollIntoViewOptions | undefined> })
      .__apexScrollIntoViewCalls
      .map((call) => typeof call === 'object' && call ? call.behavior : undefined)
  ));
  expect(scrollBehaviors.length).toBeGreaterThan(0);
  expect(scrollBehaviors).toContain('auto');
  expect(scrollBehaviors).not.toContain('smooth');
  await expect(page.getByText('Vision runtime ready · attachment unavailable', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: /vision|attach|search|voice/i })).toHaveCount(0);
  await expect(page.locator('article[data-state="accepted"]')).toBeVisible();
  await expectNoHorizontalOverflow(page);

  const artifactRoot = path.join(process.cwd(), 'test-results', 'apex-visual');
  fs.mkdirSync(artifactRoot, { recursive: true });
  await page.screenshot({ path: path.join(artifactRoot, '1440x900-apex-conversation.png'), fullPage: false });

  await page.locator('article[data-state="accepted"]').getByRole('button', { name: 'Evidence' }).click();
  const contextDrawer = page.getByRole('dialog', { name: 'Conversation context' });
  await expect(contextDrawer.getByText('Observed receipt', { exact: true })).toBeVisible();
  await expect(contextDrawer.getByText(/Proposal text is model-reported\./)).toBeVisible();
  await expect(page.locator('body')).not.toContainText(/APOCV4_API_TOKEN|privacy_partition|Bearer [A-Za-z0-9]/i);
  await expectNoHorizontalOverflow(page);
  await page.screenshot({ path: path.join(artifactRoot, '1440x900-apex-evidence.png'), fullPage: false });

  const axe = await new AxeBuilder({ page }).analyze();
  const serious = axe.violations.filter((item) => item.impact === 'serious' || item.impact === 'critical');
  expect(serious, JSON.stringify(serious, null, 2)).toEqual([]);

  await page.reload();
  await expect(page.getByText(PROPOSAL_SUMMARY).first()).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByLabel('Message Apocrypha')).toBeVisible();
  await expect(canvasField).toBeVisible();
  await expectReducedMotionApplied(page);
  await expectNoHorizontalOverflow(page);
  const geometry390 = await expectApexRegionsWithinViewport(page);
  console.info(`[apex-geometry] 390x844 ${JSON.stringify(geometry390)}`);
  await page.screenshot({ path: path.join(artifactRoot, '390x844-apex-conversation.png'), fullPage: false });

  const conversationRegion = page.locator('[aria-label="Apocrypha relay"]');
  const rail = page.locator('#apex-memory-rail');
  const openRailTrigger = page.getByRole('button', { name: 'Open conversations' });
  await openRailTrigger.focus();
  await openRailTrigger.click();
  await expect(page.getByRole('dialog', { name: 'Conversation memory' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Close conversations' })).toBeFocused();
  await expect(conversationRegion).toHaveAttribute('inert', '');
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'Conversation memory' })).toHaveCount(0);
  await expect(conversationRegion).not.toHaveAttribute('inert', '');
  await expect(openRailTrigger).toBeFocused();

  const drawerTrigger = page.getByLabel('Conversation instruments').getByRole('button', { name: 'Evidence' });
  await drawerTrigger.focus();
  await drawerTrigger.click();
  await expect(page.getByRole('dialog', { name: 'Conversation context' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Close context' })).toBeFocused();
  await expect(conversationRegion).toHaveAttribute('inert', '');
  await expect(rail).toHaveAttribute('inert', '');
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'Conversation context' })).toHaveCount(0);
  await expect(conversationRegion).not.toHaveAttribute('inert', '');
  await expect(rail).not.toHaveAttribute('inert', '');
  await expect(drawerTrigger).toBeFocused();

  await page.setViewportSize({ width: 320, height: 700 });
  await expect(page.getByLabel('Message Apocrypha')).toBeVisible();
  await expect(canvasField).toBeVisible();
  await expectNoHorizontalOverflow(page);
  const geometry320 = await expectApexRegionsWithinViewport(page);
  console.info(`[apex-geometry] 320x700 ${JSON.stringify(geometry320)}`);
  await page.screenshot({ path: path.join(artifactRoot, '320x700-apex-conversation.png'), fullPage: false });

  await openRailTrigger.click();
  const clearHistory = page.getByRole('button', { name: 'Clear local history' });
  await clearHistory.click();
  await expect(page.getByRole('button', { name: 'Confirm clear history' })).toBeVisible();
  await expect(page.getByText(PROPOSAL_SUMMARY).first()).toBeVisible();
  await page.getByRole('button', { name: 'Confirm clear history' }).click();
  await expect(page.getByRole('button', { name: 'Clear local history' })).toBeVisible();
  await expect(page.getByText(PROPOSAL_SUMMARY)).toHaveCount(0);
  await expect.poll(() => page.evaluate((key) => {
    const stored = localStorage.getItem(key);
    if (!stored) return -1;
    const parsed = JSON.parse(stored) as { threads?: Array<{ turns?: unknown[] }> };
    return parsed.threads?.[0]?.turns?.length ?? -1;
  }, SESSION_KEY)).toBe(0);
  expect(browserErrors, browserErrors.join('\n')).toEqual([]);
});

test('Apex reload recovers a persisted working turn as stopped and usable', async ({ page }) => {
  const threadId = '11111111-1111-4111-8111-111111111111';
  const turnId = '22222222-2222-4222-8222-222222222222';
  await page.addInitScript(({ key, value }) => localStorage.setItem(key, JSON.stringify(value)), {
    key: SESSION_KEY,
    value: {
      activeThreadId: threadId,
      threads: [{
        id: threadId,
        title: 'Interrupted signal',
        createdAt: '2026-08-02T23:00:00.000Z',
        turns: [{
          id: turnId,
          prompt: 'Recover this interrupted request.',
          reply: null,
          proposal: null,
          evidence: null,
          state: 'working',
          error: null,
          createdAt: '2026-08-02T23:00:01.000Z',
        }],
      }],
    },
  });
  await stubOwner(page);
  await gotoApex(page);

  await expect(page.locator('article[data-state="stopped"]')).toBeVisible();
  await expect(page.getByText(/reloaded before a final receipt arrived\. Upstream completion is unknown/i)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Stop' })).toHaveCount(0);
  await expect(page.getByLabel('Message Apocrypha')).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Retry' })).toBeEnabled();
  await expect.poll(() => page.evaluate(({ key, id }) => {
    const stored = localStorage.getItem(key);
    if (!stored) return null;
    const parsed = JSON.parse(stored) as { threads?: Array<{ turns?: Array<{ id?: string; state?: string }> }> };
    return parsed.threads?.flatMap((thread) => thread.turns ?? []).find((turn) => turn.id === id)?.state ?? null;
  }, { key: SESSION_KEY, id: turnId })).toBe('stopped');
});

test('Apex stop is a real abort and never fabricates a final receipt', async ({ page }) => {
  await stubOwner(page, async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 1_000));
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(objectiveResult()) });
  });
  await gotoApex(page);
  const composer = page.getByLabel('Message Apocrypha');
  await composer.fill(OWNER_PROMPT);
  await page.getByRole('button', { name: /^Send/ }).click();
  await page.getByRole('button', { name: 'Stop' }).click();
  await expect(page.getByText(/Stopped locally\. Upstream completion is unknown/i)).toBeVisible();
  await expect(page.getByText('No final receipt')).toBeVisible();
  await expect(page.getByText(PROPOSAL_SUMMARY)).toHaveCount(0);
});

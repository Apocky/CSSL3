import { randomUUID } from 'node:crypto';

import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const DIGEST = 'a'.repeat(64);
const TIMESTAMP = '2026-08-03T12:00:00+00:00';

async function expectNoSeriousAccessibilityFindings(page: Page): Promise<void> {
  const result = await new AxeBuilder({ page }).analyze();
  const findings = result.violations.filter(
    (entry) => entry.impact === 'serious' || entry.impact === 'critical',
  );
  expect(findings, JSON.stringify(findings, null, 2)).toEqual([]);
}

async function expectNoHorizontalOverflow(page: Page): Promise<void> {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
}

async function routeMember(page: Page, owner = false): Promise<void> {
  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: { id: 'browser-member', email: 'member@example.test' } }),
  }));
  await page.route('**/api/admin/check', (route) => route.fulfill({
    status: owner ? 200 : 403,
    contentType: 'application/json',
    body: JSON.stringify({ authorized: owner }),
  }));
}

function identity(): Record<string, unknown> {
  return {
    schema_version: 'apocv4.identity.v1',
    system_id: 'apocrypha',
    architecture: 'governed_hybrid_digital_intelligence',
    compiler_version: 'browser-fixture',
    identity_digest: DIGEST,
    learned_model_role: 'replaceable_faculty_not_system_identity',
    lineage: 'apocv4',
  };
}

function context(): Record<string, unknown> {
  return {
    frame_id: 'acf-browser-fixture',
    frame_digest: DIGEST,
    provenance_spine_digest: 'b'.repeat(64),
    retrieval: { status: 'ready', count: 0, refs: [] },
    memory: { provider: 'workspace-session', status: 'ready', records_used: 0, receipt_digest: null, refs: [] },
    capabilities: [
      { id: 'workspace_session', status: 'ready', authority: 'READ_ONLY_CONTEXT', evidence: DIGEST },
    ],
  };
}

function turnEnvelope(sessionId: string, requestId: string): Record<string, unknown> {
  return {
    text: 'A durable, evidence-bearing response.',
    session_id: sessionId,
    conversation_id: sessionId,
    request_id: requestId,
    model_id: 'browser-faculty',
    response_id: 'response-browser-fixture',
    response_digest: 'c'.repeat(64),
    serving_profile_digest: 'd'.repeat(64),
    effect_authority: 'NONE',
    tool_authority: 'READ_ONLY_CONTEXT',
    outcome: 'completed',
    learned_faculty_used: true,
    memory_scope: 'public_safe_retrieval',
    conversation_history: 'durable_principal_bound',
    training_consent: false,
    identity: identity(),
    context: context(),
    duplicate_effect_protection: 'not_applicable_no_effect_authority',
  };
}

function codeEnvelope(
  sessionId: string,
  requestId: string,
  allowedPaths: string[],
): Record<string, unknown> {
  return {
    kind: 'code',
    observed: {
      receipt: { latency_ms: 37 },
      runtime: {
        state: 'PROMOTED',
        session_id: sessionId,
        request_id: requestId,
        promotion_event_digest: '6'.repeat(64),
        terminal_event_digest: '6'.repeat(64),
        session_event_digests: {
          code_request: '1'.repeat(64),
          code_proposal: '2'.repeat(64),
          code_effect: '3'.repeat(64),
          rollback: null,
        },
        durable_replay: false,
      },
      test: { passed: true, exit_code: 0 },
    },
    generated: {
      proposal_digest: '5'.repeat(64),
      requested_allowed_paths: allowedPaths,
    },
  };
}

function restoredSnapshot(sessionId: string): Record<string, unknown> {
  const requestId = '00000000-0000-5000-8000-000000000001';
  return {
    session_id: sessionId,
    schema_version: 'apocv4.workspace-session-snapshot.v1',
    title: 'A worldline older than the recent window',
    created_at: TIMESTAMP,
    updated_at: TIMESTAMP,
    event_count: 2,
    events_truncated: false,
    tip_digest: DIGEST,
    messages: [
      {
        role: 'user', content: 'Recover the older worldline.', request_id: requestId,
        recorded_at: TIMESTAMP, event_digest: '1'.repeat(64),
      },
      {
        role: 'assistant', content: 'The older worldline was restored.', request_id: requestId,
        recorded_at: TIMESTAMP, event_digest: '2'.repeat(64),
        receipt: {
          model_id: 'browser-faculty', response_id: 'restored-response',
          response_digest: '3'.repeat(64), serving_profile_digest: '4'.repeat(64),
          memory_scope: 'public_safe_retrieval', conversation_history: 'durable_principal_bound',
          identity: identity(), context: context(),
        },
      },
    ],
    turn_states: [], jobs: [], artifacts: [], code_requests: [], proposals: [], effects: [],
    surface_truncation: {
      messages: { total: 2, visible: 2, truncated: false },
      turn_states: { total: 0, visible: 0, truncated: false },
      jobs: { total: 0, visible: 0, truncated: false },
      artifacts: { total: 0, visible: 0, truncated: false },
      code_requests: { total: 0, visible: 0, truncated: false },
      proposals: { total: 0, visible: 0, truncated: false },
      effects: { total: 0, visible: 0, truncated: false },
    },
    world: {
      message_count: 2, pending_turn_count: 0, failed_turn_count: 0,
      active_job_count: 0, artifact_count: 0, code_request_count: 0,
      proposal_count: 0, effect_count: 0, last_event_type: 'CHAT_ASSISTANT',
      last_event_digest: '2'.repeat(64),
    },
    workspace: { status: 'not_authorized', effect_authority: 'NONE' },
  };
}

function emptySnapshot(sessionId: string): Record<string, unknown> {
  return {
    ...restoredSnapshot(sessionId),
    title: 'Empty durable worldline',
    event_count: 0,
    messages: [],
    turn_states: [],
    jobs: [],
    artifacts: [],
    code_requests: [],
    proposals: [],
    effects: [],
    surface_truncation: {
      messages: { total: 0, visible: 0, truncated: false },
      turn_states: { total: 0, visible: 0, truncated: false },
      jobs: { total: 0, visible: 0, truncated: false },
      artifacts: { total: 0, visible: 0, truncated: false },
      code_requests: { total: 0, visible: 0, truncated: false },
      proposals: { total: 0, visible: 0, truncated: false },
      effects: { total: 0, visible: 0, truncated: false },
    },
    world: {
      message_count: 0, pending_turn_count: 0, failed_turn_count: 0,
      active_job_count: 0, artifact_count: 0, code_request_count: 0,
      proposal_count: 0, effect_count: 0, last_event_type: 'NONE',
      last_event_digest: '0'.repeat(64),
    },
  };
}

function workingWorldlineSnapshot(sessionId: string): Record<string, unknown> {
  const failedRequest = '00000000-0000-5000-8000-000000000011';
  const codeRequest = '00000000-0000-5000-8000-000000000012';
  const proposalDigest = '5'.repeat(64);
  const promotionDigest = '6'.repeat(64);
  return {
    session_id: sessionId,
    schema_version: 'apocv4.workspace-session-snapshot.v1',
    title: 'A worldline with unfinished work',
    created_at: TIMESTAMP,
    updated_at: TIMESTAMP,
    event_count: 7,
    events_truncated: false,
    tip_digest: '9'.repeat(64),
    messages: [{
      role: 'user', content: 'Repair the uncertain turn.', request_id: failedRequest,
      recorded_at: '2026-08-03T12:00:01+00:00', event_digest: '1'.repeat(64),
    }],
    turn_states: [{
      request_id: failedRequest,
      state: 'FAILED',
      recorded_at: '2026-08-03T12:00:02+00:00',
      user_event_digest: '1'.repeat(64),
      terminal_event_digest: '2'.repeat(64),
      failure_code: 'MODEL_RESPONSE_REJECTED',
      error_class: 'ApexRuntimeError',
      error_digest: '3'.repeat(64),
    }],
    jobs: [{
      job_id: `job:${'d'.repeat(64)}`,
      state: 'RUNNING',
      request_id: '00000000-0000-5000-8000-000000000013',
      request_digest: 'e'.repeat(64),
      action_id: 'objective.proposal_council.v1',
      attempt: 1,
      artifact_ids: [`artifact:${'f'.repeat(64)}`],
    }],
    artifacts: [{
      artifact_id: `artifact:${'f'.repeat(64)}`,
      kind: 'proposal_council_result',
      title: 'Repair proposal',
      content: { summary: 'A staged proposal-only result.' },
      content_digest: '0bb41fa033dca3448a55a9ee89a0176556965434ab3ec5eeac2eb7b1451e24a5',
      content_bytes: 44,
      refs: [`job:${'d'.repeat(64)}`],
      event_digest: '9'.repeat(64),
    }],
    code_requests: [{
      request_id: codeRequest,
      objective: 'Repair the governed parser.',
      objective_digest: '4'.repeat(64),
      allowed_paths: ['src/parser.ts'],
      allowed_paths_digest: '7'.repeat(64),
      request_contract_digest: '8'.repeat(64),
      recorded_at: '2026-08-03T12:00:03+00:00',
      event_digest: 'a'.repeat(64),
    }],
    proposals: [{
      request_id: codeRequest,
      proposal_digest: proposalDigest,
      allowed_paths: ['src/parser.ts'],
      state: 'GENERATED',
      runtime_state: 'PROMOTED',
      test_state: 'PASSED',
      recorded_at: '2026-08-03T12:00:04+00:00',
      event_digest: 'b'.repeat(64),
    }],
    effects: [{
      request_id: codeRequest,
      proposal_digest: proposalDigest,
      state: 'PROMOTED',
      promotion_event_digest: promotionDigest,
      terminal_event_digest: promotionDigest,
      rollback_event_digest: null,
      changed_paths: ['src/parser.ts'],
      test_state: 'PASSED',
      recorded_at: '2026-08-03T12:00:05+00:00',
      event_digest: 'c'.repeat(64),
    }],
    surface_truncation: {
      messages: { total: 1, visible: 1, truncated: false },
      turn_states: { total: 1, visible: 1, truncated: false },
      jobs: { total: 1, visible: 1, truncated: false },
      artifacts: { total: 1, visible: 1, truncated: false },
      code_requests: { total: 1, visible: 1, truncated: false },
      proposals: { total: 1, visible: 1, truncated: false },
      effects: { total: 1, visible: 1, truncated: false },
    },
    world: {
      message_count: 1, pending_turn_count: 0, failed_turn_count: 1,
      active_job_count: 1, artifact_count: 1, code_request_count: 1,
      proposal_count: 1, effect_count: 1, last_event_type: 'CODE_EFFECT',
      last_event_digest: 'c'.repeat(64),
    },
    workspace: { status: 'not_authorized', effect_authority: 'NONE' },
  };
}

test('signed-out Apocrypha is truthful, accessible, and does not expose a composer', async ({ page }) => {
  await page.route('**/api/auth/me', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ user: null }),
  }));

  await page.goto('/apocrypha');

  await expect(page).toHaveTitle(/Speak with Apocrypha/);
  await expect(page.getByRole('heading', { level: 1, name: 'New conversation' })).toBeVisible();
  await expect(page.getByText('Sign in to begin a restricted member turn.')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Sign in', exact: true }).last()).toHaveAttribute(
    'href',
    '/login?next=%2Fapocrypha',
  );
  await expect(page.getByLabel('Message Apocrypha')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Conversation actions' })).toBeDisabled();
  await expectNoHorizontalOverflow(page);
  await expectNoSeriousAccessibilityFindings(page);
});

test('verified member completes a durable turn and can inspect its evidence', async ({ page }) => {
  await routeMember(page);
  await page.route('**/api/apocrypha/sessions', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ sessions: [], count: 0 }),
  }));
  await page.route('**/api/apocrypha/chat', async (route) => {
    const body = route.request().postDataJSON() as {
      text: string;
      session_id: string;
      request_id: string;
    };
    expect(body.text).toBe('A browser verification turn.');
    expect(body.session_id).toMatch(/^[0-9a-f-]{36}$/);
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(turnEnvelope(body.session_id, body.request_id)),
    });
  });

  await page.goto('/apocrypha');
  const composer = page.getByLabel('Message Apocrypha');
  await expect(composer).toBeVisible();
  await composer.fill('A browser verification turn.');
  await page.getByRole('button', { name: 'Send message' }).click();

  await expect(page.getByText('A durable, evidence-bearing response.')).toBeVisible();
  const response = page.getByText('A durable, evidence-bearing response.').locator('..');
  await response.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Inspect evidence' }).click();
  await expect(page.getByText('restored-response')).toHaveCount(0);
  await expect(page.getByText('browser-faculty')).toBeVisible();
  await expectNoHorizontalOverflow(page);
  await expectNoSeriousAccessibilityFindings(page);
});

test('owner Forge consumes an approval bound to the exact objective and canonical path set', async ({ page }) => {
  let codeCalls = 0;
  let submitted: Record<string, unknown> = {};
  await routeMember(page, true);
  await page.route('**/api/apocrypha/sessions', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ sessions: [], count: 0 }),
  }));
  await page.route('**/api/admin/apocv4/code', (route) => {
    codeCalls += 1;
    submitted = route.request().postDataJSON() as Record<string, unknown>;
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(codeEnvelope(
        String(submitted.session_id),
        String(submitted.request_id),
        submitted.allowed_paths as string[],
      )),
    });
  });

  await page.goto('/apocrypha');
  await page.getByRole('button', { name: /Open Field/ }).click();
  await page.getByRole('button', { name: /Forge/ }).click();
  const objective = page.getByLabel('Message Apocrypha');
  const paths = page.getByLabel('Allowed repository paths');
  const approval = page.getByLabel(/Authorize one isolated/);
  await objective.fill('  Repair the parser.  ');
  await paths.fill('tests/parser.test.ts\nsrc/parser.ts');
  await approval.check();
  await expect(approval).toBeChecked();

  await objective.fill('Repair the parser safely.');
  await expect(approval).not.toBeChecked();
  await expect(page.getByRole('button', { name: 'Run the confirmed governed code effect' })).toBeDisabled();
  await approval.check();
  await page.getByRole('button', { name: 'Scope ready' }).click();
  await page.getByRole('button', { name: 'Run the confirmed governed code effect' }).click();

  await expect(page.getByText(/forge crossed the effect airlock/i)).toBeVisible();
  expect(codeCalls).toBe(1);
  expect(submitted.objective).toBe('Repair the parser safely.');
  expect(submitted.allowed_paths).toEqual(['src/parser.ts', 'tests/parser.test.ts']);
  await page.getByRole('button', { name: /2 paths .* confirm/ }).click();
  await expect(page.getByLabel(/Authorize one isolated/)).not.toBeChecked();
});

test('uncertain Forge delivery retains the original request and never offers an effect retry', async ({ page }) => {
  let codeCalls = 0;
  await routeMember(page, true);
  await page.route('**/api/apocrypha/sessions**', (route) => {
    const sessionId = new URL(route.request().url()).searchParams.get('session_id');
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(sessionId
        ? { session: emptySnapshot(sessionId) }
        : { sessions: [], count: 0 }),
    });
  });
  await page.route('**/api/admin/apocv4/code', (route) => {
    codeCalls += 1;
    return route.fulfill({
      status: 502,
      contentType: 'application/json',
      body: JSON.stringify({ error: 'Runtime settlement is uncertain.', retry_automatically: false }),
    });
  });

  await page.goto('/apocrypha');
  await page.getByRole('button', { name: /Open Field/ }).click();
  await page.getByRole('button', { name: /Forge/ }).click();
  await page.getByLabel('Message Apocrypha').fill('Preserve this exact effect request.');
  await page.getByLabel('Allowed repository paths').fill('src/exact.ts');
  await page.getByLabel(/Authorize one isolated/).check();
  await page.getByRole('button', { name: 'Scope ready' }).click();
  await page.getByRole('button', { name: 'Run the confirmed governed code effect' }).click();

  await expect(page.getByRole('log').getByText('Preserve this exact effect request.')).toBeVisible();
  await expect(
    page.getByRole('region', { name: 'Conversation with Apocrypha' }).getByRole('alert'),
  ).toContainText('no second effect was sent');
  await expect(page.getByRole('button', { name: 'Retry same turn' })).toHaveCount(0);
  expect(codeCalls).toBe(1);
});

test('stored active worldline is recovered directly even when absent from the recent list', async ({ page }) => {
  const activeSessionId = randomUUID();
  let archiveRequest: Record<string, unknown> | null = null;
  await routeMember(page);
  await page.addInitScript(({ key, value }) => localStorage.setItem(key, value), {
    key: 'apocky.apocrypha.active-session.v1',
    value: activeSessionId,
  });
  await page.route('**/api/apocrypha/sessions**', (route) => {
    if (route.request().method() === 'DELETE') {
      archiveRequest = route.request().postDataJSON() as Record<string, unknown>;
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          deleted: true,
          session_id: activeSessionId,
          request_id: archiveRequest.request_id,
          event_digest: 'f'.repeat(64),
        }),
      });
    }
    const url = new URL(route.request().url());
    const requested = url.searchParams.get('session_id');
    return route.fulfill(requested
      ? {
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ session: restoredSnapshot(requested) }),
      }
      : {
        status: 503,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'Recent list is transiently unavailable.' }),
      });
  });

  await page.goto('/apocrypha');
  await expect(page.getByText('The older worldline was restored.')).toBeVisible();
  await expect(page.getByRole('status').filter({ hasText: 'Restored 2 messages' }))
    .toContainText('Restored 2 messages');
  await expect(page.evaluate(() => localStorage.getItem('apocky.apocrypha.active-session.v1')))
    .resolves.toBe(activeSessionId);

  await page.getByRole('button', { name: 'Conversation actions' }).click();
  const archive = page.getByRole('button', { name: /Archive this worldline/ });
  await expect(archive).toBeVisible();
  await expect(page.getByText(/audit-ledger rows remain/)).toBeVisible();
  page.once('dialog', (dialog) => dialog.accept());
  await archive.click();
  await expect.poll(() => archiveRequest?.session_id).toBe(activeSessionId);
  await expect(page.getByRole('heading', { level: 1, name: 'New conversation' })).toBeVisible();
  await expect(page.evaluate(() => localStorage.getItem('apocky.apocrypha.active-session.v1')))
    .resolves.not.toBe(activeSessionId);
});

test('recovery failure mints an isolated worldline and dialog gestures keep honest semantics', async ({ page }) => {
  const staleSessionId = randomUUID();
  let sentSessionId = '';
  await routeMember(page);
  await page.addInitScript(({ key, value }) => localStorage.setItem(key, value), {
    key: 'apocky.apocrypha.active-session.v1',
    value: staleSessionId,
  });
  await page.route('**/api/apocrypha/sessions**', (route) => route.fulfill({
    status: 503,
    contentType: 'application/json',
    body: JSON.stringify({ error: 'Recovery unavailable.' }),
  }));
  await page.route('**/api/apocrypha/chat', async (route) => {
    const body = route.request().postDataJSON() as { session_id: string; request_id: string };
    sentSessionId = body.session_id;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(turnEnvelope(body.session_id, body.request_id)),
    });
  });

  await page.goto('/apocrypha');
  await expect(page.getByText(/hidden history will not be reused/)).toBeVisible();
  const approach = page.getByRole('button', { name: /Open Field/ });
  await expect(approach).toHaveAttribute('aria-haspopup', 'dialog');
  await approach.click();
  await expect(page.getByRole('dialog', { name: 'Approach constellation' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(approach).toBeFocused();

  await page.getByLabel('Message Apocrypha').fill('Continue without hidden history.');
  await page.getByRole('button', { name: 'Send message' }).click();
  await expect.poll(() => sentSessionId).not.toBe('');
  expect(sentSessionId).not.toBe(staleSessionId);
});

test('restored worldline keeps failure, governed effect, background work, and artifacts distinct', async ({ page }) => {
  const activeSessionId = randomUUID();
  let snapshotReads = 0;
  await routeMember(page, true);
  await page.addInitScript(({ key, value }) => localStorage.setItem(key, value), {
    key: 'apocky.apocrypha.active-session.v1',
    value: activeSessionId,
  });
  await page.route('**/api/apocrypha/sessions**', (route) => {
    const url = new URL(route.request().url());
    const requested = url.searchParams.get('session_id');
    if (requested) snapshotReads += 1;
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(requested
        ? { session: workingWorldlineSnapshot(requested) }
        : { sessions: [{
          session_id: activeSessionId, title: 'A worldline with unfinished work',
          updated_at: TIMESTAMP, message_count: 1, active_job_count: 1,
        }], count: 1 }),
    });
  });

  await page.goto('/apocrypha');
  await expect(page.getByRole('log').getByText('Repair the uncertain turn.')).toBeVisible();
  await expect(page.getByRole('button', { name: /failed/i }).first()).toBeVisible();
  await expect(page.getByText(/forge crossed the effect airlock/i)).toBeVisible();
  await page.getByRole('button', { name: 'Conversation actions' }).click();
  await expect.poll(() => snapshotReads).toBeGreaterThanOrEqual(2);
  await expect(page.getByText('Orbiting work')).toBeVisible();
  await expect(page.getByText('Proposal council')).toBeVisible();
  await expect(page.getByText('Made here')).toBeVisible();
  await expect(page.getByText('Repair proposal')).toBeVisible();
  await page.keyboard.press('Escape');

  const failedMessage = page.getByRole('log').locator('article').filter({ hasText: 'Repair the uncertain turn.' });
  await failedMessage.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Open as a new attempt' }).click();
  await expect(page.getByLabel('Message Apocrypha')).toHaveValue('Repair the uncertain turn.');
  await expectNoSeriousAccessibilityFindings(page);
});

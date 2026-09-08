import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const ACCOUNT_ID = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const JOB_ID = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const OLD_JOB_ID = 'cccccccc-cccc-4ccc-8ccc-cccccccccccc';
const OLD_REQUEST_ID = 'dddddddd-dddd-4ddd-8ddd-dddddddddddd';
const EARLIER_JOB_ID = 'eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee';
const EARLIER_REQUEST_ID = 'ffffffff-ffff-4fff-8fff-ffffffffffff';
const MEMORY_HASH = 'a'.repeat(64);

async function routeMember(page: Page): Promise<void> {
  await page.route('**/api/auth/me', (route) => route.fulfill({
    json: { user: { id: ACCOUNT_ID, email: 'member@example.test' } },
  }));
  await page.route('**/api/admin/check', (route) => route.fulfill({
    status: 403,
    json: { authorized: false },
  }));
}

function entry(input: {
  jobId: string;
  requestId: string;
  status: 'running' | 'succeeded';
  message: string;
  reply: string | null;
  createdAt: string;
}): Record<string, unknown> {
  return {
    job_id: input.jobId,
    conversation_id: ACCOUNT_ID,
    request_id: input.requestId,
    status: input.status,
    user_message: input.message,
    assistant_message: input.reply,
    assistant_truncated: false,
    model_alias: 'qwen3:30b-a3b',
    memory_manifest_hash: MEMORY_HASH,
    created_at: input.createdAt,
    updated_at: input.createdAt,
    completed_at: input.status === 'succeeded' ? input.createdAt : null,
    error_code: null,
  };
}

test('member chat submits, polls, and reloads one durable account conversation', async ({ page }, testInfo) => {
  test.setTimeout(90_000);
  await routeMember(page);
  const oldTurn = entry({
    jobId: OLD_JOB_ID,
    requestId: OLD_REQUEST_ID,
    status: 'succeeded',
    message: 'A question saved on another device.',
    reply: 'This answer came from durable history.',
    createdAt: '2026-09-08T12:00:00.000Z',
  });
  const earlierTurn = entry({
    jobId: EARLIER_JOB_ID,
    requestId: EARLIER_REQUEST_ID,
    status: 'succeeded',
    message: 'An older question from this account.',
    reply: 'This earlier answer arrived through cursor history.',
    createdAt: '2026-09-08T11:00:00.000Z',
  });
  let completedTurn: Record<string, unknown> | null = null;
  let submitted: Record<string, unknown> | null = null;
  let submittedConversationId: unknown = null;
  let pollReads = 0;
  let historyReads = 0;

  await page.route('**/api/apocrypha/member/history?*', (route) => {
    historyReads += 1;
    const url = new URL(route.request().url());
    expect(url.searchParams.get('conversation_id')).toBe(ACCOUNT_ID);
    if (url.searchParams.has('before')) {
      expect(url.searchParams.get('before')).toBe('2');
      return route.fulfill({ json: {
        ok: true,
        conversation_id: ACCOUNT_ID,
        history: [earlierTurn],
        count: 1,
        next_cursor: null,
      } });
    }
    const history = completedTurn ? [oldTurn, completedTurn] : [oldTurn];
    return route.fulfill({ json: {
      ok: true,
      conversation_id: ACCOUNT_ID,
      history,
      count: history.length,
      next_cursor: '2',
    } });
  });
  await page.route('**/api/apocrypha/member/jobs', (route) => {
    expect(route.request().method()).toBe('POST');
    submitted = route.request().postDataJSON() as Record<string, unknown>;
    submittedConversationId = submitted.conversation_id;
    expect(submitted).toEqual({
      conversation_id: ACCOUNT_ID,
      request_id: expect.stringMatching(/^[0-9a-f-]{36}$/),
      message: 'Keep this thought across a reload.',
    });
    return route.fulfill({ status: 202, json: {
      ok: true,
      accepted: true,
      replayed: false,
      job: {
        job_id: JOB_ID,
        conversation_id: ACCOUNT_ID,
        request_id: submitted.request_id,
        status: 'queued',
        model_alias: 'qwen3:30b-a3b',
        memory_manifest_hash: MEMORY_HASH,
        created_at: '2026-09-08T12:01:00.000Z',
        updated_at: '2026-09-08T12:01:00.000Z',
        replayed: false,
      },
    } });
  });
  await page.route('**/api/apocrypha/member/jobs/*', (route) => {
    pollReads += 1;
    const requestId = String(submitted?.request_id);
    const done = pollReads > 1;
    const job = entry({
      jobId: JOB_ID,
      requestId,
      status: done ? 'succeeded' : 'running',
      message: 'Keep this thought across a reload.',
      reply: done ? 'The durable reply survived.' : null,
      createdAt: '2026-09-08T12:01:00.000Z',
    });
    if (done) completedTurn = job;
    return route.fulfill({ json: { ok: true, job } });
  });

  await page.goto('/apocrypha');
  await expect(page.getByText('This answer came from durable history.')).toBeVisible();
  await expect(page.getByRole('button', { name: /New chat/i })).toHaveCount(0);
  await page.getByRole('button', { name: 'Earlier messages' }).click();
  await expect(page.getByText('This earlier answer arrived through cursor history.')).toBeVisible();

  await page.getByLabel('Message Apocrypha').fill('Keep this thought across a reload.');
  await page.getByRole('button', { name: /^Send/ }).click();
  await expect(page.getByText('Keep this thought across a reload.')).toBeVisible();
  await expect(page.getByText('The durable reply survived.')).toBeVisible({ timeout: 10_000 });
  expect(pollReads).toBeGreaterThanOrEqual(2);

  await page.reload();
  await expect(page.getByText('Keep this thought across a reload.')).toBeVisible();
  await expect(page.getByText('The durable reply survived.')).toBeVisible();
  expect(historyReads).toBeGreaterThanOrEqual(4);
  expect(submittedConversationId).toBe(ACCOUNT_ID);

  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
  const a11y = await new AxeBuilder({ page }).analyze();
  expect(a11y.violations.filter((item) => ['serious', 'critical'].includes(item.impact ?? ''))).toEqual([]);
  await page.screenshot({
    path: testInfo.outputPath(`member-chat-reloaded-${testInfo.project.name}.png`),
    fullPage: true,
  });
});

test('member chat turns an expired session into a direct recovery action', async ({ page }) => {
  await routeMember(page);
  await page.route('**/api/apocrypha/member/history?*', (route) => route.fulfill({
    status: 401,
    json: { ok: false, code: 'MEMBER_SESSION_REQUIRED', error: 'internal fixture detail' },
  }));

  await page.goto('/apocrypha');
  await expect(page.getByText('Your sign-in expired. Sign in again to continue.')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Sign in again' })).toHaveAttribute(
    'href',
    '/login?next=%2Fapocrypha',
  );
  await expect(page.getByText('internal fixture detail')).toHaveCount(0);
});

test('member chat restores an editable draft after definitive pre-acceptance rejection', async ({ page }) => {
  await routeMember(page);
  await page.route('**/api/apocrypha/member/history?*', (route) => route.fulfill({ json: {
    ok: true,
    conversation_id: ACCOUNT_ID,
    history: [],
    count: 0,
    next_cursor: null,
  } }));
  await page.route('**/api/apocrypha/member/jobs', (route) => route.fulfill({
    status: 400,
    json: { ok: false, code: 'MEMBER_CHAT_INPUT_INVALID' },
  }));

  await page.goto('/apocrypha');
  const composer = page.getByLabel('Message Apocrypha');
  await composer.fill('Let me revise this rejected message.');
  await page.getByRole('button', { name: /^Send/ }).click();

  await expect(composer).toBeEnabled();
  await expect(composer).toHaveValue('Let me revise this rejected message.');
  await expect(page.getByText(/Your message is ready to edit\./)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry same message' })).toHaveCount(0);
  const pending = await page.evaluate((accountId) => localStorage.getItem(
    `apocky.member-chat.pending.v1.${encodeURIComponent(accountId)}`,
  ), ACCOUNT_ID);
  expect(pending).toBeNull();
});

test('member chat removes a stale job id and retries the same request after job not found', async ({ page }) => {
  await routeMember(page);
  await page.route('**/api/apocrypha/member/history?*', (route) => route.fulfill({ json: {
    ok: true,
    conversation_id: ACCOUNT_ID,
    history: [],
    count: 0,
    next_cursor: null,
  } }));

  const requestIds: string[] = [];
  await page.route('**/api/apocrypha/member/jobs', (route) => {
    const submitted = route.request().postDataJSON() as Record<string, unknown>;
    requestIds.push(String(submitted.request_id));
    return route.fulfill({ status: 202, json: {
      ok: true,
      accepted: true,
      replayed: requestIds.length > 1,
      job: {
        job_id: JOB_ID,
        conversation_id: ACCOUNT_ID,
        request_id: submitted.request_id,
        status: 'queued',
        model_alias: 'qwen3:30b-a3b',
        memory_manifest_hash: MEMORY_HASH,
        created_at: '2026-09-08T12:01:00.000Z',
        updated_at: '2026-09-08T12:01:00.000Z',
        replayed: requestIds.length > 1,
      },
    } });
  });
  let jobReads = 0;
  await page.route('**/api/apocrypha/member/jobs/*', (route) => {
    jobReads += 1;
    if (jobReads === 1) {
      return route.fulfill({
        status: 404,
        json: { ok: false, code: 'MEMBER_CHAT_JOB_NOT_FOUND' },
      });
    }
    return route.fulfill({ json: { ok: true, job: entry({
      jobId: JOB_ID,
      requestId: requestIds[0] ?? '',
      status: 'succeeded',
      message: 'Recover this exact message.',
      reply: 'Recovered through the same durable request.',
      createdAt: '2026-09-08T12:01:00.000Z',
    }) } });
  });

  await page.goto('/apocrypha');
  await page.getByLabel('Message Apocrypha').fill('Recover this exact message.');
  await page.getByRole('button', { name: /^Send/ }).click();
  const retry = page.getByRole('button', { name: 'Retry same message' });
  await expect(retry).toBeVisible();

  const pending = await page.evaluate((accountId) => JSON.parse(String(localStorage.getItem(
    `apocky.member-chat.pending.v1.${encodeURIComponent(accountId)}`,
  ))) as Record<string, unknown>, ACCOUNT_ID);
  expect(pending.request_id).toBe(requestIds[0]);
  expect(pending.job_id).toBeUndefined();

  await retry.click();
  await expect(page.getByText('Recovered through the same durable request.')).toBeVisible();
  expect(requestIds).toHaveLength(2);
  expect(requestIds[1]).toBe(requestIds[0]);
});

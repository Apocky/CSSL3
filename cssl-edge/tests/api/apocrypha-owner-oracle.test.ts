import assert from 'node:assert/strict';

import type { NextApiRequest, NextApiResponse } from 'next';

import { resetApocryphaServiceClientForTests } from '@/lib/apocrypha/job-control';
import { ownerChatConversationVisible } from '@/lib/apocrypha/owner-oracle-control';
import conversationsHandler from '@/pages/api/admin/apocrypha/conversations';
import jobsHandler from '@/pages/api/admin/apocrypha/jobs';
import oracleHandler from '@/pages/api/admin/apocrypha/oracles';

interface Output {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

const TENANT_ID = '10000000-0000-4000-8000-000000000001';
const PRINCIPAL_ID = '10000000-0000-4000-8000-000000000002';
const NONCE = '20000000-0000-4000-8000-000000000001';
const RUN_ID = '30000000-0000-4000-8000-000000000001';
const CONVERSATION_ID = '40000000-0000-4000-8000-000000000001';
const FAILED_JOB_ID = '50000000-0000-4000-8000-000000000001';
const RETRY_JOB_ID = '60000000-0000-4000-8000-000000000001';
const ORDINARY_JOB_ID = '70000000-0000-4000-8000-000000000001';
const PROMPT = 'Reply exactly: APOCRYPHA ORACLE PASSED 30000000';

function reqRes(method: string, options: {
  body?: unknown;
  query?: Record<string, string | string[]>;
  owner?: boolean;
  origin?: string;
} = {}): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 0, body: null, headers: {} };
  const headers: Record<string, string> = {
    host: 'www.apocky.com',
    origin: options.origin ?? 'https://www.apocky.com',
    'x-forwarded-proto': 'https',
  };
  if (options.owner !== false) headers['x-apocky-test-admin-email'] = 'owner@example.test';
  const req = { method, body: options.body, query: options.query ?? {}, headers } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(body: unknown) { out.body = body; return this; },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
}

const originalEnv = { ...process.env };
const originalFetch = globalThis.fetch;

async function main(): Promise<void> {
  process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
  process.env.APOCKY_ADMIN_EMAILS = 'owner@example.test';
  process.env.NEXT_PUBLIC_SUPABASE_URL = 'https://supabase.example.test';
  process.env.SUPABASE_SERVICE_ROLE_KEY = 'test-service-role-never-log';

  const calls: Array<{ path: string; method: string; body: Record<string, unknown>; url: URL }> = [];
  let sourceMode: 'marked' | 'foreign' = 'marked';
  let registrationFails = false;
  let idempotentOracleJob = false;
  let conversationVisible = true;
  globalThis.fetch = async (input, init) => {
    const url = new URL(String(input));
    const body = init?.body ? JSON.parse(String(init.body)) as Record<string, unknown> : {};
    const call = { path: url.pathname, method: init?.method ?? 'GET', body, url };
    calls.push(call);

    if (url.pathname.endsWith('/rpc/apocrypha_ensure_owner_principal')) {
      return json({ tenant_id: TENANT_ID, principal_id: PRINCIPAL_ID });
    }
    if (url.pathname.endsWith('/rpc/apocrypha_seed_owner_chat_oracle')) {
      return json({ run_id: RUN_ID, conversation_id: CONVERSATION_ID, job_id: FAILED_JOB_ID, prompt: PROMPT });
    }
    if (url.pathname.endsWith('/rpc/apocrypha_cleanup_owner_chat_oracle')) {
      return json({ run_id: RUN_ID, cleaned: true });
    }
    if (url.pathname.endsWith('/rpc/apocrypha_owner_chat_conversation_visible')) return json(conversationVisible);
    if (url.pathname.endsWith('/rpc/apocrypha_register_owner_chat_oracle_job')) {
      return registrationFails
        ? json({ code: 'P0001', message: 'registration rejected' }, 400)
        : json({ run_id: RUN_ID, job_id: RETRY_JOB_ID, registered: true });
    }
    if (url.pathname.endsWith('/rest/v1/apocrypha_job')) {
      if (url.searchParams.get('idempotency_key') === 'eq.oracle-replay') {
        return json(idempotentOracleJob ? {
          id: RETRY_JOB_ID,
          status: 'queued',
          request: {
            prompt: PROMPT,
            conversation_id: CONVERSATION_ID,
            output_budget: 1536,
            response_mode: 'standard',
            retry_of_job_id: FAILED_JOB_ID,
            oracle_run_id: RUN_ID,
            source: 'apocky.com',
            privacy_class: 'restricted',
            memory_scope: 'owner-authorized',
          },
        } : null);
      }
      if (url.searchParams.get('select') === '*') {
        assert.equal(url.searchParams.get('tenant_id'), `eq.${TENANT_ID}`);
        assert.equal(url.searchParams.get('owner_principal_id'), `eq.${PRINCIPAL_ID}`);
        if (sourceMode === 'foreign') return json(null);
        return json({
          id: FAILED_JOB_ID,
          status: 'failed',
          request: { prompt: PROMPT, conversation_id: CONVERSATION_ID, oracle_run_id: RUN_ID },
        });
      }
      return json([]);
    }
    if (url.pathname.endsWith('/rpc/apocrypha_enqueue_job')) {
      const request = body.p_request as Record<string, unknown>;
      const oracleRetry = request.retry_of_job_id === FAILED_JOB_ID;
      if (body.p_idempotency_key === 'oracle-replay') idempotentOracleJob = true;
      return json({
        id: oracleRetry ? RETRY_JOB_ID : ORDINARY_JOB_ID,
        tenant_id: TENANT_ID,
        owner_principal_id: PRINCIPAL_ID,
        kind: 'apocky_chat',
        capability: 'apocky_owner_chat',
        status: 'queued',
        request,
      });
    }
    throw new Error(`Unexpected request: ${url.pathname}`);
  };
  resetApocryphaServiceClientForTests();

  try {
    const created = reqRes('POST', { body: { nonce: NONCE } });
    await oracleHandler(created.req, created.res);
    assert.equal(created.out.statusCode, 201);
    assert.deepEqual(created.out.body, { ok: true, data: {
      run_id: RUN_ID, conversation_id: CONVERSATION_ID, job_id: FAILED_JOB_ID, prompt: PROMPT,
    } });
    assert.match(created.out.headers['cache-control'] ?? '', /private/);
    assert.match(created.out.headers['cache-control'] ?? '', /no-store/);
    const seed = calls.find((call) => call.path.endsWith('/rpc/apocrypha_seed_owner_chat_oracle'))!;
    assert.deepEqual(seed.body, {
      p_tenant_id: TENANT_ID,
      p_owner_principal_id: PRINCIPAL_ID,
      p_nonce: NONCE,
    }, 'seed receives identity only from authenticated server state');

    const replay = reqRes('POST', { body: { nonce: NONCE } });
    await oracleHandler(replay.req, replay.res);
    assert.equal(replay.out.statusCode, 201, 'same nonce safely replays the database-owned run');
    assert.deepEqual(replay.out.body, created.out.body);

    const injected = reqRes('POST', { body: { nonce: NONCE, owner_id: PRINCIPAL_ID } });
    const seedsBeforeInjection = calls.filter((call) => call.path.endsWith('/rpc/apocrypha_seed_owner_chat_oracle')).length;
    await oracleHandler(injected.req, injected.res);
    assert.equal(injected.out.statusCode, 400);
    assert.equal(calls.filter((call) => call.path.endsWith('/rpc/apocrypha_seed_owner_chat_oracle')).length, seedsBeforeInjection);

    const crossOrigin = reqRes('POST', { origin: 'https://attacker.example', body: { nonce: NONCE } });
    await oracleHandler(crossOrigin.req, crossOrigin.res);
    assert.equal(crossOrigin.out.statusCode, 403);
    const signedOut = reqRes('POST', { owner: false, body: { nonce: NONCE } });
    await oracleHandler(signedOut.req, signedOut.res);
    assert.equal(signedOut.out.statusCode, 401);

    const cleaned = reqRes('DELETE', { body: { run_id: RUN_ID } });
    await oracleHandler(cleaned.req, cleaned.res);
    assert.equal(cleaned.out.statusCode, 200);
    assert.deepEqual(cleaned.out.body, { ok: true, data: { run_id: RUN_ID, cleaned: true } });
    const cleanup = calls.find((call) => call.path.endsWith('/rpc/apocrypha_cleanup_owner_chat_oracle'))!;
    assert.deepEqual(cleanup.body, {
      p_tenant_id: TENANT_ID,
      p_owner_principal_id: PRINCIPAL_ID,
      p_run_id: RUN_ID,
    });
    assert.equal(
      calls.some((call) => call.method === 'DELETE' && call.path.endsWith('/rest/v1/apocrypha_job')),
      false,
      'control plane never physically deletes a job row',
    );
    conversationVisible = false;
    assert.equal(await ownerChatConversationVisible({ tenantId: TENANT_ID, principalId: PRINCIPAL_ID }, CONVERSATION_ID), false);
    const visibility = calls.find((call) => call.path.endsWith('/rpc/apocrypha_owner_chat_conversation_visible'))!;
    assert.deepEqual(visibility.body, {
      p_tenant_id: TENANT_ID,
      p_owner_principal_id: PRINCIPAL_ID,
      p_conversation_id: CONVERSATION_ID,
    });

    const cleanedDetail = reqRes('GET', { query: { id: CONVERSATION_ID } });
    await conversationsHandler(cleanedDetail.req, cleanedDetail.res);
    assert.equal(cleanedDetail.out.statusCode, 404, 'cleaned conversation detail stays quarantined');
    assert.equal((cleanedDetail.out.body as Record<string, unknown>).code, 'CONVERSATION_NOT_FOUND');

    const enqueuesBeforeQuarantinedAdmission = calls.filter((call) => call.path.endsWith('/rpc/apocrypha_enqueue_job')).length;
    const quarantinedAdmission = reqRes('POST', { body: {
      prompt: 'Do not revive a cleaned run.',
      conversation_id: CONVERSATION_ID,
    } });
    await jobsHandler(quarantinedAdmission.req, quarantinedAdmission.res);
    assert.equal(quarantinedAdmission.out.statusCode, 404);
    assert.equal(calls.filter((call) => call.path.endsWith('/rpc/apocrypha_enqueue_job')).length, enqueuesBeforeQuarantinedAdmission,
      'cleaned conversation is rejected before history or enqueue');

    conversationVisible = true;

    const spoof = reqRes('POST', { body: {
      prompt: 'Ordinary owner message.',
      conversation_id: CONVERSATION_ID,
      oracle_run_id: RUN_ID,
    } });
    await jobsHandler(spoof.req, spoof.res);
    assert.equal(spoof.out.statusCode, 202);
    const ordinaryEnqueue = calls.filter((call) => call.path.endsWith('/rpc/apocrypha_enqueue_job')).at(-1)!;
    assert.equal(Object.prototype.hasOwnProperty.call(ordinaryEnqueue.body.p_request as object, 'oracle_run_id'), false,
      'caller cannot inject an oracle marker into an ordinary job');

    const retry = reqRes('POST', { body: {
      prompt: PROMPT,
      conversation_id: CONVERSATION_ID,
      retry_job_id: FAILED_JOB_ID,
    } });
    await jobsHandler(retry.req, retry.res);
    assert.equal(retry.out.statusCode, 202);
    const retryEnqueue = calls.filter((call) => call.path.endsWith('/rpc/apocrypha_enqueue_job')).at(-1)!;
    assert.equal((retryEnqueue.body.p_request as Record<string, unknown>).oracle_run_id, RUN_ID,
      'server propagates the marker from the validated failed source only');
    const registration = calls.filter((call) => call.path.endsWith('/rpc/apocrypha_register_owner_chat_oracle_job')).at(-1)!;
    assert.deepEqual(registration.body, {
      p_tenant_id: TENANT_ID,
      p_owner_principal_id: PRINCIPAL_ID,
      p_run_id: RUN_ID,
      p_job_id: RETRY_JOB_ID,
    });

    const replayBody = {
      prompt: PROMPT,
      conversation_id: CONVERSATION_ID,
      retry_job_id: FAILED_JOB_ID,
      idempotency_key: 'oracle-replay',
    };
    const replayFirst = reqRes('POST', { body: replayBody });
    await jobsHandler(replayFirst.req, replayFirst.res);
    assert.equal(replayFirst.out.statusCode, 202);
    const registrationsBeforeReplay = calls.filter((call) => call.path.endsWith('/rpc/apocrypha_register_owner_chat_oracle_job')).length;
    const replaySecond = reqRes('POST', { body: replayBody });
    await jobsHandler(replaySecond.req, replaySecond.res);
    assert.equal(replaySecond.out.statusCode, 202);
    assert.equal((replaySecond.out.body as Record<string, unknown>).replayed, true);
    assert.equal(
      calls.filter((call) => call.path.endsWith('/rpc/apocrypha_register_owner_chat_oracle_job')).length,
      registrationsBeforeReplay + 1,
      'an idempotently replayed retry re-registers after a lost registration response',
    );

    sourceMode = 'foreign';
    const foreign = reqRes('POST', { body: {
      prompt: PROMPT,
      conversation_id: CONVERSATION_ID,
      retry_job_id: FAILED_JOB_ID,
    } });
    const registrationsBeforeForeign = calls.filter((call) => call.path.endsWith('/rpc/apocrypha_register_owner_chat_oracle_job')).length;
    await jobsHandler(foreign.req, foreign.res);
    assert.equal(foreign.out.statusCode, 404, 'a source outside the authenticated owner partition is unavailable');
    assert.equal(calls.filter((call) => call.path.endsWith('/rpc/apocrypha_register_owner_chat_oracle_job')).length, registrationsBeforeForeign);

    sourceMode = 'marked';
    registrationFails = true;
    const failedRegistration = reqRes('POST', { body: {
      prompt: PROMPT,
      conversation_id: CONVERSATION_ID,
      retry_job_id: FAILED_JOB_ID,
    } });
    await jobsHandler(failedRegistration.req, failedRegistration.res);
    assert.equal(failedRegistration.out.statusCode, 503, 'registration failure never returns an accepted oracle retry');
    assert.deepEqual(failedRegistration.out.body, {
      ok: false,
      code: 'APOCRYPHA_JOB_ERROR',
      error: 'Apocrypha could not update this request. It is safe to retry.',
    }, 'registration internals stay out of the response');

    console.log('apocrypha-owner-oracle.test: owner-only idempotent seed, logical cleanup, marker custody, registration OK');
  } finally {
    globalThis.fetch = originalFetch;
    process.env = originalEnv;
    resetApocryphaServiceClientForTests();
  }
}

void main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

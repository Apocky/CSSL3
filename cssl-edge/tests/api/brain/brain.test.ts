import assert from 'node:assert/strict';
import type { NextApiRequest, NextApiResponse } from 'next';

import { _resetMnemeClientForTests } from '@/lib/mneme/store';
import snapshotHandler from '@/pages/api/brain/snapshot';
import statusHandler from '@/pages/api/brain/runtime/status';
import turnHandler from '@/pages/api/brain/runtime/turn';

interface Output {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function request(
  method: string,
  email?: string,
  body?: unknown,
  origin = 'http://localhost:3000',
): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 200, body: null, headers: {} };
  const headers: Record<string, string> = { host: 'localhost:3000' };
  if (email) headers['x-apocky-test-admin-email'] = email;
  if (method === 'POST') {
    headers.origin = origin;
    headers['content-type'] = 'application/json';
  }
  const req = { method, headers, query: {}, body } as unknown as NextApiRequest;
  const res = {
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
    status(code: number) { out.statusCode = code; return this; },
    json(value: unknown) { out.body = value; return this; },
    end() { return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

const mutableEnv = process.env as Record<string, string | undefined>;
const previous = {
  bypass: process.env.LAZARUS_TEST_AUTH_BYPASS,
  admins: process.env.APOCKY_ADMIN_EMAILS,
  brain: process.env.APOCKY_BRAIN_LOCAL_PROVIDER_ENABLED,
  supabase: process.env.NEXT_PUBLIC_SUPABASE_URL,
  service: process.env.SUPABASE_SERVICE_ROLE_KEY,
  transport: process.env.APOCV4_RUNTIME_TRANSPORT,
  ownerId: process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID,
};

async function main(): Promise<void> {
  try {
    mutableEnv.LAZARUS_TEST_AUTH_BYPASS = '1';
    mutableEnv.APOCKY_ADMIN_EMAILS = 'owner@example.com';
    delete mutableEnv.APOCKY_BRAIN_LOCAL_PROVIDER_ENABLED;
    delete mutableEnv.NEXT_PUBLIC_SUPABASE_URL;
    delete mutableEnv.SUPABASE_SERVICE_ROLE_KEY;
    _resetMnemeClientForTests();

    const anonymous = request('GET');
    await statusHandler(anonymous.req, anonymous.res);
    assert.equal(anonymous.out.statusCode, 401, 'runtime status requires an authenticated owner');
    assert.equal((anonymous.out.body as Record<string, unknown>).code, 'BRAIN_SESSION_REQUIRED');

    const member = request('GET', 'member@example.com');
    await statusHandler(member.req, member.res);
    assert.equal(member.out.statusCode, 403, 'ordinary member cannot inspect the owner runtime');
    assert.equal((member.out.body as Record<string, unknown>).code, 'BRAIN_OWNER_REQUIRED');

    const status = request('GET', 'owner@example.com');
    await statusHandler(status.req, status.res);
    assert.equal(status.out.statusCode, 200, 'disabled local provider is a readable degraded state');
    assert.equal((status.out.body as Record<string, unknown>).status, 'degraded');
    assert.equal((status.out.body as Record<string, unknown>).reason_code, 'BRAIN_LOCAL_PROVIDER_DISABLED');
    assert.match(status.out.headers['cache-control'] ?? '', /private.*no-store/);
    assert.match(status.out.headers['x-robots-tag'] ?? '', /noindex/);

    const snapshot = request('GET', 'owner@example.com');
    await snapshotHandler(snapshot.req, snapshot.res);
    assert.equal(snapshot.out.statusCode, 503, 'unconnected Mneme cannot fall back to mock data');
    assert.equal((snapshot.out.body as Record<string, unknown>).code, 'MNEME_STORAGE_UNAVAILABLE');

    const crossOrigin = request('POST', 'owner@example.com', {}, 'https://example.net');
    await turnHandler(crossOrigin.req, crossOrigin.res);
    assert.equal(crossOrigin.out.statusCode, 403, 'cross-origin turn denied before provider access');
    assert.equal((crossOrigin.out.body as Record<string, unknown>).code, 'BRAIN_ORIGIN_DENIED');

    const disabledTurn = request('POST', 'owner@example.com', {
      text: 'Remember the source boundary.',
      session_id: '11111111-1111-4111-8111-111111111111',
      request_id: '22222222-2222-4222-8222-222222222222',
    });
    await turnHandler(disabledTurn.req, disabledTurn.res);
    assert.equal(disabledTurn.out.statusCode, 503, 'no generated turn without the explicit local provider gate');
    assert.equal((disabledTurn.out.body as Record<string, unknown>).code, 'BRAIN_LOCAL_PROVIDER_DISABLED');

    mutableEnv.APOCV4_RUNTIME_TRANSPORT = 'outbound-bridge';
    mutableEnv.APOCRYPHA_BRIDGE_OWNER_USER_ID = '11111111-1111-4111-8111-111111111111';
    const otherOperator = request('GET', 'owner@example.com');
    await statusHandler(otherOperator.req, otherOperator.res);
    assert.equal(otherOperator.out.statusCode, 403, 'allowlisted operator with a different verified subject cannot alias owner chat');
    mutableEnv.APOCRYPHA_BRIDGE_OWNER_USER_ID = 'test-admin';
    const exactOwner = request('GET', 'owner@example.com');
    await statusHandler(exactOwner.req, exactOwner.res);
    assert.equal(exactOwner.out.statusCode, 200, 'matching verified owner reaches status provider');

    console.log('brain-api.test : OK · owner denial + private headers + no mock fallback + local-only turn gate');
  } finally {
    for (const [key, value] of Object.entries({
      LAZARUS_TEST_AUTH_BYPASS: previous.bypass,
      APOCKY_ADMIN_EMAILS: previous.admins,
      APOCKY_BRAIN_LOCAL_PROVIDER_ENABLED: previous.brain,
      NEXT_PUBLIC_SUPABASE_URL: previous.supabase,
      SUPABASE_SERVICE_ROLE_KEY: previous.service,
      APOCV4_RUNTIME_TRANSPORT: previous.transport,
      APOCRYPHA_BRIDGE_OWNER_USER_ID: previous.ownerId,
    })) {
      if (value === undefined) delete mutableEnv[key];
      else mutableEnv[key] = value;
    }
    _resetMnemeClientForTests();
  }
}

void main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});

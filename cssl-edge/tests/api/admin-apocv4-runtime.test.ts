import type { NextApiRequest, NextApiResponse } from 'next';

import healthHandler, { maxDuration as healthMaxDuration } from '@/pages/api/admin/apocv4/health';
import objectiveHandler, { maxDuration as objectiveMaxDuration } from '@/pages/api/admin/apocv4/objective';

type Body = Record<string, unknown>;

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function responseHarness(): {
  response: NextApiResponse;
  read: () => { status: number; body: Body | null; headers: Record<string, string> };
} {
  let status = 200;
  let body: Body | null = null;
  const headers: Record<string, string> = {};
  const response = {
    setHeader(name: string, value: string | number | readonly string[]) {
      headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
    status(code: number) {
      status = code;
      return this;
    },
    json(value: Body) {
      body = value;
      return this;
    },
  } as unknown as NextApiResponse;
  return { response, read: () => ({ status, body, headers }) };
}

function request(method: string, body?: unknown, admin = true): NextApiRequest {
  return {
    method,
    body,
    headers: admin ? { 'x-apocky-test-admin-email': 'owner@example.com' } : {},
  } as NextApiRequest;
}

const originalFetch = globalThis.fetch;
const originalEnv = {
  bypass: process.env.LAZARUS_TEST_AUTH_BYPASS,
  admins: process.env.APOCKY_ADMIN_EMAILS,
  runtimeUrl: process.env.APOCV4_RUNTIME_URL,
  runtimeToken: process.env.APOCV4_API_TOKEN,
};

async function main(): Promise<void> {
  let fetchCount = 0;
  try {
    process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
    process.env.APOCKY_ADMIN_EMAILS = 'owner@example.com';
    process.env.APOCV4_RUNTIME_URL = 'https://podowner-8080.proxy.runpod.net';
    process.env.APOCV4_API_TOKEN = 'server-runtime-token-123';

    assert(healthMaxDuration === 20, 'health route has short function duration');
    assert(objectiveMaxDuration === 300, 'objective route has 300-second Vercel duration');

    globalThis.fetch = async (input, init) => {
      fetchCount += 1;
      const url = String(input);
      if (url.endsWith('/health')) {
        return new Response(JSON.stringify({
          schema_version: 'apocv4.runtime-service.v1',
          status: 'READY',
          engine: { perpetual: true },
          vision: false,
        }), { status: 200, headers: { 'Content-Type': 'application/json' } });
      }
      const upstream = JSON.parse(String(init?.body)) as Record<string, unknown>;
      assert(!Object.hasOwn(upstream, 'privacy_partition'), 'API never forwards a client partition');
      return new Response(JSON.stringify({
        schema_version: 'apocv4.runtime-service.v1',
        result: { status: 'BUDGET_EXHAUSTED', iterations_completed: 1, attempts: [] },
      }), { status: 200, headers: { 'Content-Type': 'application/json' } });
    };

    {
      const harness = responseHarness();
      await healthHandler(request('GET', undefined, false), harness.response);
      const result = harness.read();
      assert(result.status === 401, 'non-admin health request denied');
      assert(fetchCount === 0, 'unauthorized request never reaches runtime');
    }
    {
      const harness = responseHarness();
      await healthHandler(request('GET'), harness.response);
      const result = harness.read();
      assert(result.status === 200, 'admin health succeeds');
      assert(result.body?.kind === 'health', 'health projection returned');
      assert(result.headers['cache-control'] === 'no-store, max-age=0', 'health is non-cacheable');
    }
    {
      const before = fetchCount;
      const harness = responseHarness();
      await objectiveHandler(
        request('POST', { objective: 'Must not reach runtime.' }, false),
        harness.response,
      );
      const result = harness.read();
      assert(result.status === 401, 'non-admin objective request denied');
      assert(fetchCount === before, 'unauthorized objective never reaches runtime');
    }
    {
      const before = fetchCount;
      const harness = responseHarness();
      await objectiveHandler(request('POST', {
        objective: 'Attempt cross partition.',
        privacy_partition: 'attacker-choice',
      }), harness.response);
      const result = harness.read();
      assert(result.status === 400, 'extra partition field rejected');
      assert(result.body?.error === 'objective_body_invalid', 'exact body error returned');
      assert(fetchCount === before, 'invalid body never reaches runtime');
    }
    {
      const harness = responseHarness();
      await objectiveHandler(request('POST', { objective: 'Run one bounded objective.' }), harness.response);
      const result = harness.read();
      assert(result.status === 200, 'admin objective succeeds');
      assert(result.body?.kind === 'objective', 'objective evidence projection returned');
      assert(!JSON.stringify(result.body).includes('server-runtime-token-123'), 'token never crosses API');
      assert(result.headers['cache-control'] === 'no-store, max-age=0', 'objective is non-cacheable');
    }
    {
      const harness = responseHarness();
      await objectiveHandler(request('GET'), harness.response);
      const result = harness.read();
      assert(result.status === 405, 'objective route permits POST only');
      assert(result.headers.allow === 'POST', 'Allow header is exact');
    }
  } finally {
    globalThis.fetch = originalFetch;
    for (const [key, value] of Object.entries(originalEnv)) {
      const envName = {
        bypass: 'LAZARUS_TEST_AUTH_BYPASS',
        admins: 'APOCKY_ADMIN_EMAILS',
        runtimeUrl: 'APOCV4_RUNTIME_URL',
        runtimeToken: 'APOCV4_API_TOKEN',
      }[key];
      if (!envName) continue;
      if (value === undefined) delete process.env[envName];
      else process.env[envName] = value;
    }
  }
  console.log('admin-apocv4-runtime.test : OK · admin-only exact proxy routes');
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});

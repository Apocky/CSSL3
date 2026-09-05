// cssl-edge · tests/api/health.test.ts
// Direct tests for liveness plus bounded, typed Supabase readiness.

import handler, {
  resetSupabaseHealthCacheForTests,
  type HealthResponse,
} from '@/pages/api/health';
import type { NextApiRequest, NextApiResponse } from 'next';

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

const ENV_KEYS = [
  'STRIPE_SECRET_KEY',
  'STRIPE_WEBHOOK_SIGNING_SECRET',
  'APOCKY_HUB_SUPABASE_URL',
  'APOCKY_HUB_SUPABASE_ANON_KEY',
  'NEXT_PUBLIC_SUPABASE_URL',
  'SUPABASE_ANON_KEY',
  'NEXT_PUBLIC_SUPABASE_ANON_KEY',
  'APOCKY_SUPABASE_HEALTH_ALLOWED_HOSTS',
] as const;

function mockReqRes(method: string): {
  req: NextApiRequest;
  res: NextApiResponse;
  out: MockedResponse;
} {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = {
    method,
    query: {},
    headers: {},
    body: undefined,
  } as unknown as NextApiRequest;
  const res = {
    status(code: number) {
      out.statusCode = code;
      return this;
    },
    json(payload: unknown) {
      out.body = payload;
      return this;
    },
    setHeader(key: string, value: string) {
      out.headers[key] = value;
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function isolateEnv(): () => void {
  const previous = new Map<string, string | undefined>();
  for (const key of ENV_KEYS) {
    previous.set(key, process.env[key]);
    delete process.env[key];
  }
  const originalFetch = globalThis.fetch;
  resetSupabaseHealthCacheForTests();
  return () => {
    globalThis.fetch = originalFetch;
    for (const [key, value] of previous) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    resetSupabaseHealthCacheForTests();
  };
}

function configureSupabase(url = 'https://test.supabase.co'): void {
  process.env['APOCKY_HUB_SUPABASE_URL'] = url;
  process.env['APOCKY_HUB_SUPABASE_ANON_KEY'] = 'anon_test';
}

async function invoke(): Promise<MockedResponse> {
  const { req, res, out } = mockReqRes('GET');
  await handler(req, res);
  return out;
}

export async function testHealthOkAndShape(): Promise<void> {
  const restore = isolateEnv();
  try {
    const out = await invoke();
    if (out.statusCode !== 200) throw new Error('expected HTTP 200');
    const body = out.body as Record<string, unknown>;
    const requiredKeys = [
      'ok',
      'sha',
      'served_by',
      'ts',
      'version',
      'stripe_configured',
      'stripe_webhook_configured',
      'auth_supabase_configured',
      'data_supabase_configured',
      'cron_configured',
      'supabase_connected',
      'supabase_status',
      'supabase_checked_at',
      'supabase_probe_surface',
      'payments_ready',
    ];
    for (const key of requiredKeys) {
      if (!(key in body)) throw new Error('missing required key: ' + key);
    }
    if (body['ok'] !== true) throw new Error('expected ok:true');
    if (body['supabase_status'] !== 'unconfigured') {
      throw new Error('expected unconfigured without env');
    }
  } finally {
    restore();
  }
}

export async function testSuccessfulProbe(): Promise<void> {
  const restore = isolateEnv();
  let probeUrl = '';
  let probeHeaders = new Headers();
  try {
    configureSupabase();
    globalThis.fetch = async (input, init) => {
      probeUrl = String(input);
      probeHeaders = new Headers(init?.headers);
      return new Response(null, { status: 200 });
    };
    const body = (await invoke()).body as HealthResponse;
    if (!body.supabase_connected || body.supabase_status !== 'connected') {
      throw new Error('successful probe must report connected');
    }
    if (!probeUrl.endsWith('/auth/v1/health')) {
      throw new Error('probe must use documented Supabase Auth health surface');
    }
    if (probeHeaders.get('apikey') !== 'anon_test') {
      throw new Error('probe must carry the configured API key');
    }
    if (probeHeaders.has('authorization')) {
      throw new Error('project API key must not be sent as a bearer token');
    }
  } finally {
    restore();
  }
}

export async function testNetworkFailureDoesNotFalseGreen(): Promise<void> {
  const restore = isolateEnv();
  try {
    configureSupabase();
    process.env['STRIPE_SECRET_KEY'] = 'sk_test_x';
    process.env['STRIPE_WEBHOOK_SIGNING_SECRET'] = 'whsec_x';
    globalThis.fetch = async () => {
      throw new Error('simulated network failure');
    };
    const body = (await invoke()).body as HealthResponse;
    if (
      body.supabase_connected ||
      body.supabase_status !== 'unreachable' ||
      body.payments_ready
    ) {
      throw new Error('network failure must not report integration readiness');
    }
  } finally {
    restore();
  }
}

export async function testInvalidHostNeverReceivesKey(): Promise<void> {
  const restore = isolateEnv();
  let fetchCalls = 0;
  try {
    configureSupabase('https://example.invalid');
    globalThis.fetch = async () => {
      fetchCalls += 1;
      return new Response(null, { status: 200 });
    };
    const body = (await invoke()).body as HealthResponse;
    if (body.supabase_status !== 'misconfigured' || body.supabase_connected) {
      throw new Error('non-allowlisted host must be misconfigured');
    }
    if (fetchCalls !== 0) throw new Error('key must not be sent to invalid host');
  } finally {
    restore();
  }
}

export async function testAuthFailureIsDistinct(): Promise<void> {
  const restore = isolateEnv();
  try {
    configureSupabase();
    globalThis.fetch = async () => new Response(null, { status: 401 });
    const body = (await invoke()).body as HealthResponse;
    if (body.supabase_status !== 'auth_failed' || body.supabase_connected) {
      throw new Error('401 must report auth_failed');
    }
  } finally {
    restore();
  }
}

export async function testProbeTimeoutIsBounded(): Promise<void> {
  const restore = isolateEnv();
  let abortObserved = false;
  try {
    configureSupabase();
    globalThis.fetch = async (_input, init) =>
      await new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener(
          'abort',
          () => {
            abortObserved = true;
            reject(new DOMException('aborted', 'AbortError'));
          },
          { once: true }
        );
      });
    const startedAt = Date.now();
    const body = (await invoke()).body as HealthResponse;
    const elapsedMs = Date.now() - startedAt;
    if (!abortObserved || body.supabase_status !== 'unreachable') {
      throw new Error('stalled probe must abort and report unreachable');
    }
    if (elapsedMs < 1_800 || elapsedMs > 3_000) {
      throw new Error('probe timeout outside bounded tolerance: ' + elapsedMs + 'ms');
    }
  } finally {
    restore();
  }
}

async function runAll(): Promise<void> {
  await testHealthOkAndShape();
  await testSuccessfulProbe();
  await testNetworkFailureDoesNotFalseGreen();
  await testInvalidHostNeverReceivesKey();
  await testAuthFailureIsDistinct();
  await testProbeTimeoutIsBounded();
  // eslint-disable-next-line no-console
  console.log('health.test : OK · 6 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void runAll().catch((error: unknown) => {
    // eslint-disable-next-line no-console
    console.error(error);
    process.exitCode = 1;
  });
}

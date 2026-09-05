// cssl-edge · /api/health
// Liveness remains HTTP 200. Integration readiness is separately typed and
// Supabase connectivity is observed with a bounded, cached server-side probe.

import type { NextApiRequest, NextApiResponse } from 'next';
import { commitSha, envelope, logHit } from '@/lib/response';

export type SupabaseStatus =
  | 'connected'
  | 'unconfigured'
  | 'misconfigured'
  | 'auth_failed'
  | 'unreachable';

export interface HealthResponse {
  ok: true;
  sha: string;
  served_by: string;
  ts: string;
  version: string;
  // Integration config — booleans only · NEVER leak the actual env-values.
  stripe_configured: boolean;
  stripe_webhook_configured: boolean;
  auth_supabase_configured: boolean;
  data_supabase_configured: boolean;
  cron_configured: boolean;
  supabase_connected: boolean;
  supabase_status: SupabaseStatus;
  supabase_checked_at: string | null;
  supabase_probe_surface: 'auth/v1/health';
  // Composite readiness flag — convenience for status-page polls.
  payments_ready: boolean;
}

interface SupabaseProbeResult {
  status: SupabaseStatus;
  checkedAt: string | null;
}

interface SupabaseProbeConfig {
  endpoint: string;
  key: string;
}

const SUPABASE_PROBE_TIMEOUT_MS = 2_000;
const SUPABASE_PROBE_CACHE_MS = 30_000;

let supabaseProbeCache:
  | { expiresAt: number; promise: Promise<SupabaseProbeResult> }
  | undefined;

function envValue(name: string): string | undefined {
  const value = process.env[name]?.trim();
  return value ? value : undefined;
}

function isSet(name: string): boolean {
  return envValue(name) !== undefined;
}

function allowedSupabaseHost(hostname: string): boolean {
  const normalized = hostname.toLowerCase();
  if (normalized.endsWith('.supabase.co')) return true;

  const allowed = envValue('APOCKY_SUPABASE_HEALTH_ALLOWED_HOSTS');
  if (!allowed) return false;
  return allowed
    .split(',')
    .map((host) => host.trim().toLowerCase())
    .filter(Boolean)
    .includes(normalized);
}

function resolveSupabaseProbeConfig(): SupabaseProbeConfig | SupabaseStatus {
  const authUrl = envValue('APOCKY_HUB_SUPABASE_URL');
  const authKey = envValue('APOCKY_HUB_SUPABASE_ANON_KEY');
  const dataUrl = envValue('NEXT_PUBLIC_SUPABASE_URL') ?? authUrl;
  const dataKey =
    envValue('SUPABASE_ANON_KEY') ??
    envValue('NEXT_PUBLIC_SUPABASE_ANON_KEY') ??
    authKey;

  const candidates: Array<[string | undefined, string | undefined]> = [
    [authUrl, authKey],
    [dataUrl, dataKey],
  ];
  const selected = candidates.find(([url, key]) => url !== undefined && key !== undefined);

  if (!selected) {
    const anyConfig = candidates.some(
      ([url, key]) => url !== undefined || key !== undefined
    );
    return anyConfig ? 'misconfigured' : 'unconfigured';
  }

  try {
    const url = new URL(selected[0] as string);
    if (
      url.protocol !== 'https:' ||
      url.username !== '' ||
      url.password !== '' ||
      !allowedSupabaseHost(url.hostname)
    ) {
      return 'misconfigured';
    }
    return {
      endpoint: new URL('/auth/v1/health', url.origin).toString(),
      key: selected[1] as string,
    };
  } catch {
    return 'misconfigured';
  }
}

async function executeSupabaseProbe(): Promise<SupabaseProbeResult> {
  const config = resolveSupabaseProbeConfig();
  if (typeof config === 'string') {
    return { status: config, checkedAt: null };
  }

  const checkedAt = new Date().toISOString();
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), SUPABASE_PROBE_TIMEOUT_MS);

  try {
    const response = await fetch(config.endpoint, {
      method: 'GET',
      headers: {
        accept: 'application/json',
        apikey: config.key,
      },
      redirect: 'error',
      signal: controller.signal,
    });
    void response.body?.cancel().catch(() => undefined);

    if (response.ok) return { status: 'connected', checkedAt };
    if (response.status === 401 || response.status === 403) {
      return { status: 'auth_failed', checkedAt };
    }
    return { status: 'unreachable', checkedAt };
  } catch {
    return { status: 'unreachable', checkedAt };
  } finally {
    clearTimeout(timeout);
  }
}

async function probeSupabase(): Promise<SupabaseProbeResult> {
  const now = Date.now();
  if (supabaseProbeCache && supabaseProbeCache.expiresAt > now) {
    return supabaseProbeCache.promise;
  }

  const promise = executeSupabaseProbe();
  supabaseProbeCache = {
    expiresAt: now + SUPABASE_PROBE_CACHE_MS,
    promise,
  };
  return promise;
}

export function resetSupabaseHealthCacheForTests(): void {
  supabaseProbeCache = undefined;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<HealthResponse>
): Promise<void> {
  logHit('health', { method: req.method ?? 'GET' });

  const env = envelope();
  const stripeConfigured = isSet('STRIPE_SECRET_KEY');
  const webhookConfigured = isSet('STRIPE_WEBHOOK_SIGNING_SECRET');
  const authSupabaseConfigured =
    isSet('APOCKY_HUB_SUPABASE_URL') && isSet('APOCKY_HUB_SUPABASE_ANON_KEY');
  const dataSupabaseConfigured =
    (isSet('NEXT_PUBLIC_SUPABASE_URL') || isSet('APOCKY_HUB_SUPABASE_URL')) &&
    (isSet('SUPABASE_ANON_KEY') ||
      isSet('NEXT_PUBLIC_SUPABASE_ANON_KEY') ||
      isSet('APOCKY_HUB_SUPABASE_ANON_KEY'));
  const cronConfigured = isSet('CRON_SECRET');
  const supabase = await probeSupabase();
  const supabaseConnected = supabase.status === 'connected';

  const body: HealthResponse = {
    ok: true,
    sha: commitSha(),
    served_by: env.served_by,
    ts: env.ts,
    version: process.env['CSSL_EDGE_VERSION'] ?? '0.1.0',
    stripe_configured: stripeConfigured,
    stripe_webhook_configured: webhookConfigured,
    auth_supabase_configured: authSupabaseConfigured,
    data_supabase_configured: dataSupabaseConfigured,
    cron_configured: cronConfigured,
    supabase_connected: supabaseConnected,
    supabase_status: supabase.status,
    supabase_checked_at: supabase.checkedAt,
    supabase_probe_surface: 'auth/v1/health',
    payments_ready: stripeConfigured && webhookConfigured && supabaseConnected,
  };

  res.status(200).json(body);
}

// ─── Inline tests for W9-bump · framework-agnostic ────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
}

function mockReqRes(): {
  req: NextApiRequest;
  res: NextApiResponse<HealthResponse>;
  out: MockedResponse;
} {
  const out: MockedResponse = { statusCode: 0, body: null };
  const req = {
    method: 'GET',
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
    setHeader(_key: string, _value: string) {
      return this;
    },
  } as unknown as NextApiResponse<HealthResponse>;
  return { req, res, out };
}

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error('assert failed : ' + message);
}

const SUPABASE_ENV_KEYS = [
  'APOCKY_HUB_SUPABASE_URL',
  'APOCKY_HUB_SUPABASE_ANON_KEY',
  'NEXT_PUBLIC_SUPABASE_URL',
  'SUPABASE_ANON_KEY',
  'NEXT_PUBLIC_SUPABASE_ANON_KEY',
  'APOCKY_SUPABASE_HEALTH_ALLOWED_HOSTS',
] as const;

function snapshotEnv(keys: readonly string[]): () => void {
  const previous = new Map<string, string | undefined>();
  for (const key of keys) previous.set(key, process.env[key]);
  return () => {
    for (const [key, value] of previous) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  };
}

function clearSupabaseEnv(): void {
  for (const key of SUPABASE_ENV_KEYS) delete process.env[key];
}

// 1. Health response carries typed readiness fields.
export async function testHealthCarriesW9Keys(): Promise<void> {
  const restoreEnv = snapshotEnv(SUPABASE_ENV_KEYS);
  clearSupabaseEnv();
  resetSupabaseHealthCacheForTests();
  try {
    const { req, res, out } = mockReqRes();
    await handler(req, res);
    const body = out.body as Record<string, unknown>;
    for (const key of [
      'stripe_configured',
      'stripe_webhook_configured',
      'auth_supabase_configured',
      'data_supabase_configured',
      'cron_configured',
      'supabase_connected',
      'payments_ready',
    ]) {
      assert(typeof body[key] === 'boolean', key + ' must be boolean');
    }
    assert(body['supabase_status'] === 'unconfigured', 'unconfigured status');
    assert(body['supabase_checked_at'] === null, 'no probe timestamp without config');
    assert(body['supabase_probe_surface'] === 'auth/v1/health', 'probe surface named');
  } finally {
    restoreEnv();
    resetSupabaseHealthCacheForTests();
  }
}

// 2. payments_ready requires an observed Supabase response, not env presence.
export async function testHealthPaymentsReadyComposite(): Promise<void> {
  const keys = [
    ...SUPABASE_ENV_KEYS,
    'STRIPE_SECRET_KEY',
    'STRIPE_WEBHOOK_SIGNING_SECRET',
  ] as const;
  const restoreEnv = snapshotEnv(keys);
  const originalFetch = globalThis.fetch;
  clearSupabaseEnv();
  process.env['STRIPE_SECRET_KEY'] = 'sk_test_x';
  process.env['STRIPE_WEBHOOK_SIGNING_SECRET'] = 'whsec_x';
  process.env['APOCKY_HUB_SUPABASE_URL'] = 'https://test.supabase.co';
  process.env['APOCKY_HUB_SUPABASE_ANON_KEY'] = 'anon_test';
  globalThis.fetch = async () => new Response(null, { status: 200 });
  resetSupabaseHealthCacheForTests();

  try {
    const { req, res, out } = mockReqRes();
    await handler(req, res);
    const body = out.body as HealthResponse;
    assert(body.supabase_connected, 'successful probe -> connected');
    assert(body.supabase_status === 'connected', 'successful probe -> typed status');
    assert(body.supabase_checked_at !== null, 'successful probe -> checked timestamp');
    assert(body.payments_ready, 'Stripe config + successful probe -> payments ready');
  } finally {
    globalThis.fetch = originalFetch;
    restoreEnv();
    resetSupabaseHealthCacheForTests();
  }
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testHealthCarriesW9Keys();
    await testHealthPaymentsReadyComposite();
    // eslint-disable-next-line no-console
    console.log('health.ts : OK · 2 W9-inline tests passed');
  })().catch((error: unknown) => {
    // eslint-disable-next-line no-console
    console.error(error);
    process.exitCode = 1;
  });
}

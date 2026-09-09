import { createHash } from 'node:crypto';

import { createClient, type SupabaseClient } from '@supabase/supabase-js';

import type {
  ContributorEndpoint,
  ContributorRateLimitDecision,
  ContributorRateLimitRequest,
  ContributorRateLimiter,
} from './contributor-http';

/**
 * Durable/global abuse control for the contributor transport.
 *
 * A Vercel function instance is not a safe place for a limiter: instances are
 * short-lived and concurrent.  This adapter hashes the request key before it
 * leaves the process and consumes one fixed-window counter through a
 * SECURITY DEFINER Supabase RPC.  The service-role key is read only by the
 * server-side factory and is never returned, logged, or accepted from a
 * request.
 */

export const CONTRIBUTOR_RATE_LIMIT_RPC = 'apocrypha_contributor_rate_limit_consume' as const;
export const CONTRIBUTOR_RATE_LIMITER_OPT_IN_ENV = 'APOCRYPHA_CONTRIBUTOR_RATE_LIMITER' as const;

export const CONTRIBUTOR_RATE_LIMIT_POLICIES = {
  enroll: { scope: 'apocrypha.contributor.enroll', limit: 20, windowSeconds: 60 },
  lease: { scope: 'apocrypha.contributor.lease', limit: 120, windowSeconds: 60 },
  poll: { scope: 'apocrypha.contributor.poll', limit: 120, windowSeconds: 60 },
  result: { scope: 'apocrypha.contributor.result', limit: 120, windowSeconds: 60 },
  revoke: { scope: 'apocrypha.contributor.revoke', limit: 10, windowSeconds: 60 },
} as const satisfies Record<Exclude<ContributorEndpoint, 'status'>, {
  readonly scope: string;
  readonly limit: number;
  readonly windowSeconds: number;
}>;

export interface SupabaseContributorRateLimiterOptions {
  readonly client: SupabaseClient;
  readonly policies?: Partial<typeof CONTRIBUTOR_RATE_LIMIT_POLICIES>;
}

export type ContributorRateLimiterAvailability =
  | { readonly ok: true; readonly limiter: SupabaseContributorRateLimiter }
  | { readonly ok: false; readonly code: 'RATE_LIMIT_STORE_UNAVAILABLE'; readonly reason: string };

export class ContributorRateLimiterError extends Error {
  readonly code = 'RATE_LIMIT_STORE_UNAVAILABLE' as const;

  constructor(message: string) {
    super(message);
    this.name = 'ContributorRateLimiterError';
  }
}

type JsonRecord = Record<string, unknown>;

function object(value: unknown): JsonRecord | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as JsonRecord
    : null;
}

function unavailable(operation: string, error?: unknown): ContributorRateLimiterError {
  const source = object(error);
  const code = source && typeof source.code === 'string' ? source.code : 'unknown';
  return new ContributorRateLimiterError(`contributor rate limiter unavailable during ${operation} (${code})`);
}

function validKey(value: unknown): value is string {
  return typeof value === 'string'
    && value.length >= 1
    && value.length <= 128
    && /^[\x20-\x7e]+$/.test(value);
}

function validMethod(value: unknown): value is string {
  return typeof value === 'string'
    && value.length >= 1
    && value.length <= 16
    && /^[A-Za-z]+$/.test(value);
}

function digestRequestKey(value: string): string {
  return createHash('sha256')
    .update('apocrypha.contributor.rate.v1\0', 'utf8')
    .update(value, 'utf8')
    .digest('hex');
}

function policyFor(
  endpoint: ContributorEndpoint,
  overrides: Partial<typeof CONTRIBUTOR_RATE_LIMIT_POLICIES>,
): { readonly scope: string; readonly limit: number; readonly windowSeconds: number } | null {
  if (endpoint === 'status') return null;
  const policy = overrides[endpoint] ?? CONTRIBUTOR_RATE_LIMIT_POLICIES[endpoint];
  if (!policy
    || typeof policy.scope !== 'string'
    || !/^apocrypha\.contributor\.(?:enroll|lease|poll|result|revoke)$/.test(policy.scope)
    || !Number.isSafeInteger(policy.limit)
    || policy.limit < 1
    || policy.limit > 1000
    || !Number.isSafeInteger(policy.windowSeconds)
    || policy.windowSeconds < 1
    || policy.windowSeconds > 3600) {
    return null;
  }
  return policy;
}

function decision(value: unknown): ContributorRateLimitDecision {
  const source = Array.isArray(value) && value.length === 1 ? object(value[0]) : object(value);
  if (!source || typeof source.allowed !== 'boolean') {
    throw unavailable('consume:invalid-response');
  }
  if (source.allowed) {
    if (source.retry_after_seconds !== undefined && source.retry_after_seconds !== null) {
      if (!Number.isSafeInteger(source.retry_after_seconds)
        || Number(source.retry_after_seconds) < 0
        || Number(source.retry_after_seconds) > 3600) {
        throw unavailable('consume:invalid-allowed-response');
      }
      return { allowed: true, retry_after_seconds: Number(source.retry_after_seconds) };
    }
    return { allowed: true };
  }
  if (!Number.isSafeInteger(source.retry_after_seconds)
    || Number(source.retry_after_seconds) < 1
    || Number(source.retry_after_seconds) > 3600) {
    throw unavailable('consume:invalid-denied-response');
  }
  return { allowed: false, retry_after_seconds: Number(source.retry_after_seconds) };
}

export class SupabaseContributorRateLimiter implements ContributorRateLimiter {
  private readonly client: SupabaseClient;
  private readonly policies: Partial<typeof CONTRIBUTOR_RATE_LIMIT_POLICIES>;

  constructor(options: SupabaseContributorRateLimiterOptions) {
    if (!options.client || typeof options.client.rpc !== 'function') {
      throw new ContributorRateLimiterError('Supabase RPC client unavailable');
    }
    this.client = options.client;
    this.policies = options.policies ?? {};
  }

  /** True only for a client exposing the RPC call surface; migration presence
   * is proven by a real consume call, never by this property. */
  get rpcCapable(): boolean {
    return typeof this.client.rpc === 'function';
  }

  async check(input: ContributorRateLimitRequest): Promise<ContributorRateLimitDecision> {
    const policy = policyFor(input.endpoint, this.policies);
    if (!policy || !validKey(input.key) || !validMethod(input.method)) {
      throw unavailable('consume:invalid-input');
    }
    const keyDigest = digestRequestKey(input.key);
    let data: unknown;
    try {
      const response = await this.client.rpc(CONTRIBUTOR_RATE_LIMIT_RPC, {
        p_scope: policy.scope,
        p_key_digest: keyDigest,
        p_limit: policy.limit,
        p_window_seconds: policy.windowSeconds,
      });
      if (response.error) throw response.error;
      data = response.data;
    } catch (error) {
      if (error instanceof ContributorRateLimiterError) throw error;
      throw unavailable('consume:rpc', error);
    }
    return decision(data);
  }
}

/** Test seam. Production callers must use the server-only factory below. */
export function createSupabaseContributorRateLimiterForClient(
  client: SupabaseClient,
  options: Omit<SupabaseContributorRateLimiterOptions, 'client'> = {},
): SupabaseContributorRateLimiter {
  return new SupabaseContributorRateLimiter({ client, ...options });
}

/**
 * Server-only default resolver. It is inert until the operator explicitly
 * opts into the named Supabase limiter. Missing credentials or a malformed
 * client remain a typed closed state; no memory/local fallback is selected.
 */
export function createSupabaseContributorRateLimiter(): ContributorRateLimiterAvailability {
  if (process.env[CONTRIBUTOR_RATE_LIMITER_OPT_IN_ENV] !== 'supabase') {
    return {
      ok: false,
      code: 'RATE_LIMIT_STORE_UNAVAILABLE',
      reason: 'explicit Supabase contributor rate-limiter opt-in is required',
    };
  }
  const url = process.env.APOCKY_HUB_SUPABASE_URL ?? process.env.NEXT_PUBLIC_SUPABASE_URL;
  const serviceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!url || !serviceKey) {
    return {
      ok: false,
      code: 'RATE_LIMIT_STORE_UNAVAILABLE',
      reason: 'server Supabase URL and service-role configuration are required',
    };
  }
  try {
    const client = createClient(url, serviceKey, {
      auth: { persistSession: false, autoRefreshToken: false },
    });
    return { ok: true, limiter: new SupabaseContributorRateLimiter({ client }) };
  } catch {
    return {
      ok: false,
      code: 'RATE_LIMIT_STORE_UNAVAILABLE',
      reason: 'server Supabase rate-limiter client could not be initialized',
    };
  }
}

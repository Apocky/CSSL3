// Shared Apocrypha V2 tunnel boundary.
//
// This module owns the fixed-host, bounded-request, private-cache, and
// Cloudflare Access mechanics shared by Apocrypha proxies. Owner authorization
// remains an explicit helper; a non-owner caller must establish its own
// authenticated boundary before invoking fetchApocryphaV2.

import { createHash } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAuthorization } from '@/lib/admin-auth';
import { envelope } from '@/lib/response';

const CANONICAL_TUNNEL_HOST = 'apocrypha.apocky.com';
const DEFAULT_UPSTREAM_DEADLINE_MS = 25_000;

export interface ApocryphaOwner {
  principalRef: string;
}

interface ProxyOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';
  upstreamPath: string;
  body?: unknown;
  query?: Record<string, string | number | undefined>;
  deadlineMs?: number;
  forwardStatus?: boolean;
}

export interface V2ProxyOptions extends Omit<ProxyOptions, 'upstreamPath'> {
  upstreamPath: `/v2/${string}`;
}

export interface ApocryphaFetchResult {
  ok: boolean;
  status: number;
  payload: unknown;
}

interface CfAccessCreds {
  clientId: string;
  clientSecret: string;
}

export function setPrivateNoStore(res: NextApiResponse): void {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Vary', 'Authorization, Cookie');
  res.setHeader('X-Content-Type-Options', 'nosniff');
}

function safeUpstreamDetail(upstream: Response, body: string): string {
  const contentType = upstream.headers.get('content-type')?.toLowerCase() ?? '';
  if (contentType.includes('text/html') || /^\s*<!doctype\s+html/i.test(body)) {
    return 'The Apocrypha body returned a gateway response. Try again shortly.';
  }
  return body.trim().slice(0, 500) || 'The Apocrypha body returned an empty error.';
}

function configuredTunnelHost(): string | null {
  const configured = process.env.APOCRYPHA_TUNNEL_HOST?.trim().toLowerCase();
  if (!configured || !/^[a-z0-9.-]+$/.test(configured)) return null;
  return configured === CANONICAL_TUNNEL_HOST ? configured : null;
}

function cfCreds(): CfAccessCreds | null {
  const clientId = process.env.CF_ACCESS_CLIENT_ID?.trim();
  const clientSecret = process.env.CF_ACCESS_CLIENT_SECRET?.trim();
  if (!clientId || !clientSecret) return null;
  return { clientId, clientSecret };
}

function buildQueryString(query?: Record<string, string | number | undefined>): string {
  if (!query) return '';
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined) params.set(key, String(value));
  }
  const encoded = params.toString();
  return encoded ? `?${encoded}` : '';
}

function uuidFromDigest(digest: Buffer): string {
  const bytes = Buffer.from(digest.subarray(0, 16));
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x50;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const hex = bytes.toString('hex');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function isOpaqueConversationId(value: unknown): value is string {
  return typeof value === 'string'
    && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}

export function isOpaqueClientRequestId(value: unknown): value is string {
  return isOpaqueConversationId(value);
}

export function scopeConversationId(principalRef: string, clientConversationId: string): string {
  const digest = createHash('sha256')
    .update('APOCRYPHA-V2-PRINCIPAL-CONVERSATION-v1\0', 'utf8')
    .update(principalRef, 'utf8')
    .update('\0', 'utf8')
    .update(clientConversationId.toLowerCase(), 'utf8')
    .digest();
  return uuidFromDigest(digest);
}

export function scopeRequestId(principalRef: string, clientRequestId: string): string {
  const digest = createHash('sha256')
    .update('APOCRYPHA-V2-PRINCIPAL-REQUEST-v1\0', 'utf8')
    .update(principalRef, 'utf8')
    .update('\0', 'utf8')
    .update(clientRequestId.toLowerCase(), 'utf8')
    .digest();
  return uuidFromDigest(digest);
}

export function expectedConversationRef(
  scopedConversationId: string,
  sourceRef: string,
): string {
  const canonicalPayload = JSON.stringify({
    conversation_sha256: createHash('sha256').update(scopedConversationId, 'utf8').digest('hex'),
    source_sha256: createHash('sha256').update(sourceRef, 'utf8').digest('hex'),
  });
  return createHash('sha256')
    .update('APX-V2-PUBLIC-CONVERSATION-v1\0', 'ascii')
    .update(canonicalPayload, 'utf8')
    .digest('hex');
}

export async function requireApocryphaOwner(
  req: NextApiRequest,
  res: NextApiResponse,
): Promise<ApocryphaOwner | null> {
  setPrivateNoStore(res);
  const authorization = await getAdminAuthorization(req);
  if (!authorization.authorized || !authorization.user) {
    res.status(authorization.user ? 403 : 401).json({
      error: authorization.reason ?? 'Owner authorization required.',
      authorized: false,
      ...envelope(),
    });
    return null;
  }
  const principalDigest = createHash('sha256')
    .update('APOCRYPHA-V2-OWNER-PRINCIPAL-v1\0', 'utf8')
    .update(authorization.user.id, 'utf8')
    .digest('hex');
  return { principalRef: `principal:apocky-owner:${principalDigest}` };
}

async function fetchApocryphaRequest(
  opts: ProxyOptions,
): Promise<ApocryphaFetchResult> {
  const tunnelHost = configuredTunnelHost();
  const credentials = cfCreds();
  if (!tunnelHost || !credentials) {
    return {
      ok: false,
      status: 503,
      payload: { error: 'Apocrypha V2 tunnel credentials are unavailable.' },
    };
  }

  const controller = new AbortController();
  const deadline = setTimeout(
    () => controller.abort(),
    opts.deadlineMs ?? DEFAULT_UPSTREAM_DEADLINE_MS,
  );
  const headers: Record<string, string> = {
    Accept: 'application/json',
    'CF-Access-Client-Id': credentials.clientId,
    'CF-Access-Client-Secret': credentials.clientSecret,
    Origin: `https://${CANONICAL_TUNNEL_HOST}`,
  };
  if (opts.body !== undefined) headers['Content-Type'] = 'application/json';

  try {
    const upstream = await fetch(
      `https://${tunnelHost}${opts.upstreamPath}${buildQueryString(opts.query)}`,
      {
        method: opts.method ?? 'GET',
        headers,
        body: opts.body === undefined ? undefined : JSON.stringify(opts.body),
        cache: 'no-store',
        redirect: 'error',
        signal: controller.signal,
      },
    );
    const contentType = upstream.headers.get('content-type')?.toLowerCase() ?? '';
    if (contentType.includes('application/json')) {
      return { ok: upstream.ok, status: upstream.status, payload: await upstream.json() };
    }
    const body = await upstream.text();
    return {
      ok: upstream.ok,
      status: upstream.status,
      payload: upstream.ok ? body : { error: safeUpstreamDetail(upstream, body) },
    };
  } catch (error) {
    const timedOut = error instanceof DOMException && error.name === 'AbortError';
    return {
      ok: false,
      status: timedOut ? 504 : 502,
      payload: {
        error: timedOut
          ? 'The Apocrypha body did not answer before the bounded deadline.'
          : 'The Apocrypha V2 tunnel is unreachable.',
      },
    };
  } finally {
    clearTimeout(deadline);
  }
}

export async function fetchApocryphaV2(
  opts: V2ProxyOptions,
): Promise<ApocryphaFetchResult> {
  return fetchApocryphaRequest(opts);
}

export async function proxyV2ToApocrypha(
  req: NextApiRequest,
  res: NextApiResponse,
  opts: V2ProxyOptions,
): Promise<void> {
  if (!(await requireApocryphaOwner(req, res))) return;
  const upstream = await fetchApocryphaV2(opts);
  const status = opts.forwardStatus === false ? 200 : upstream.status;
  res.status(status).json({
    upstream_status: upstream.status,
    data: upstream.payload,
    ...envelope(),
  });
}

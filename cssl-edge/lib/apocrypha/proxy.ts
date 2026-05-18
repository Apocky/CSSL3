// Shared Apocrypha-tunnel proxy utility for /api/admin/apocrypha/* routes.
// All cockpit-side API endpoints forward to https://${APOCRYPHA_TUNNEL_HOST}/<path>
// with CF Access service-token headers attached.
//
// Per Apocrypha/specs/12_APOCKY_COM_INTEGRATION.csl + HANDOFF_v10 § TRACK-A A4.

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin } from '@/lib/require-admin';

export interface ProxyOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';
  upstreamPath: string;          // e.g. "/api/v1/chat"
  body?: unknown;                // JSON body for POST/PUT/PATCH
  query?: Record<string, string | number | undefined>;
  forwardStatus?: boolean;       // pass upstream HTTP status to client (default true)
}

interface CfAccessCreds {
  clientId: string;
  clientSecret: string;
}

function cfCreds(): CfAccessCreds | null {
  const id = process.env.CF_ACCESS_CLIENT_ID;
  const secret = process.env.CF_ACCESS_CLIENT_SECRET;
  if (!id || !secret) return null;
  return { clientId: id, clientSecret: secret };
}

function buildQueryString(query?: Record<string, string | number | undefined>): string {
  if (!query) return '';
  const parts: string[] = [];
  for (const [k, v] of Object.entries(query)) {
    if (v === undefined) continue;
    parts.push(`${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`);
  }
  return parts.length ? `?${parts.join('&')}` : '';
}

export async function proxyToApocrypha(
  req: NextApiRequest,
  res: NextApiResponse,
  opts: ProxyOptions,
): Promise<void> {
  if (!(await requireAdmin(req, res))) return;

  const tunnel = process.env.APOCRYPHA_TUNNEL_HOST;
  if (!tunnel) {
    res.status(503).json({
      error: 'APOCRYPHA_TUNNEL_HOST unset ; cockpit cannot reach backend',
      ...envelope(),
    });
    return;
  }

  const creds = cfCreds();
  if (!creds) {
    res.status(503).json({
      error: 'CF_ACCESS_CLIENT_ID / CF_ACCESS_CLIENT_SECRET not configured',
      ...envelope(),
    });
    return;
  }

  const method = opts.method ?? 'GET';
  const url = `https://${tunnel}${opts.upstreamPath}${buildQueryString(opts.query)}`;

  const headers: Record<string, string> = {
    'CF-Access-Client-Id': creds.clientId,
    'CF-Access-Client-Secret': creds.clientSecret,
    Accept: 'application/json',
  };
  if (opts.body !== undefined) headers['Content-Type'] = 'application/json';

  try {
    const upstream = await fetch(url, {
      method,
      headers,
      body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    });

    let payload: unknown;
    const contentType = upstream.headers.get('content-type') ?? '';
    if (contentType.includes('application/json')) {
      payload = await upstream.json();
    } else {
      payload = await upstream.text();
    }

    const status = opts.forwardStatus === false ? 200 : upstream.status;
    res.status(status).json({
      upstream_status: upstream.status,
      data: payload,
      tunnel_host: tunnel,
      ...envelope(),
    });
  } catch (err) {
    res.status(502).json({
      error: 'apocrypha tunnel unreachable',
      detail: err instanceof Error ? err.message : String(err),
      tunnel_host: tunnel,
      ...envelope(),
    });
  }
}

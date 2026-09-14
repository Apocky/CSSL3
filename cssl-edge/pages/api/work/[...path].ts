import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAuthorization } from '@/lib/admin-auth';
import { WORK_BASE_URL, WorkLaneUnavailable, unavailablePayload, workToken } from '@/lib/work/bridge';

export const config = { api: { bodyParser: { sizeLimit: '1mb' }, responseLimit: false } };

const ALLOWED = new Set(['GET', 'POST']);
const STREAM_TIMEOUT_MS = 6 * 60 * 60 * 1_000;

function noStore(res: NextApiResponse): void {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Vary', 'Authorization, Cookie');
  res.setHeader('X-Content-Type-Options', 'nosniff');
}

function upstreamPath(req: NextApiRequest): string {
  const raw = req.query.path;
  const segments = Array.isArray(raw) ? raw : typeof raw === 'string' ? [raw] : [];
  // Every segment is re-encoded, so a caller cannot smuggle a `..` or a second path through.
  return `/${segments.map((segment) => encodeURIComponent(segment)).join('/')}`;
}

async function proxyStream(req: NextApiRequest, res: NextApiResponse, url: string, token: string): Promise<void> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), STREAM_TIMEOUT_MS);
  req.on('close', () => controller.abort());

  const upstream = await fetch(url, {
    headers: { authorization: `Bearer ${token}`, accept: 'text/event-stream' },
    signal: controller.signal,
  }).catch(() => null);

  if (!upstream?.ok || !upstream.body) {
    clearTimeout(timer);
    res.status(upstream?.status ?? 503).json(
      upstream ? { error: 'stream_failed', status: upstream.status } : unavailablePayload(new WorkLaneUnavailable('unreachable', 'the Work service did not accept the stream')),
    );
    return;
  }

  res.writeHead(200, {
    'content-type': 'text/event-stream; charset=utf-8',
    'cache-control': 'private, no-store',
    connection: 'keep-alive',
    'x-accel-buffering': 'no',
  });

  const reader = upstream.body.getReader();
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      res.write(Buffer.from(value));
    }
  } catch {
    // A client that navigates away aborts the stream; that is the normal end of a subscription.
  } finally {
    clearTimeout(timer);
    res.end();
  }
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  noStore(res);
  if (!ALLOWED.has(req.method ?? '')) {
    res.setHeader('Allow', [...ALLOWED].join(', '));
    res.status(405).json({ error: 'method_not_allowed' });
    return;
  }

  // The Work lane can write files and run commands on the host. It is owner-only, always, even
  // though the service behind it is bound to loopback.
  const auth = await getAdminAuthorization(req);
  if (!auth.authorized) {
    res.status(auth.user ? 403 : 401).json({ error: 'owner_only', detail: 'The Work lane is limited to the account that owns this machine.' });
    return;
  }

  let token: string;
  try {
    token = await workToken();
  } catch (error) {
    if (error instanceof WorkLaneUnavailable) { res.status(503).json(unavailablePayload(error)); return; }
    throw error;
  }

  const path = upstreamPath(req);
  const search = new URL(req.url ?? '/', 'http://local').search;
  const url = `${WORK_BASE_URL}${path}${search}`;

  if (path.endsWith('/stream')) {
    await proxyStream(req, res, url, token);
    return;
  }

  const upstream = await fetch(url, {
    method: req.method,
    headers: {
      authorization: `Bearer ${token}`,
      ...(req.method === 'POST' ? { 'content-type': 'application/json' } : {}),
    },
    body: req.method === 'POST' ? JSON.stringify(req.body ?? {}) : undefined,
    signal: AbortSignal.timeout(30_000),
  }).catch(() => null);

  if (!upstream) {
    res.status(503).json(unavailablePayload(new WorkLaneUnavailable('unreachable', `no response from ${WORK_BASE_URL}`)));
    return;
  }

  const text = await upstream.text();
  res.status(upstream.status);
  res.setHeader('content-type', upstream.headers.get('content-type') ?? 'application/json; charset=utf-8');
  res.send(text);
}

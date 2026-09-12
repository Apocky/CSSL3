import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAuthorization } from '@/lib/admin-auth';
import {
  assertChaosBridgeSignature,
  ensureExternalIdentity,
  ensureOwnerIdentity,
  publicJobError,
  type JobIdentity,
} from '@/lib/apocrypha/job-control';

export function methodNotAllowed(res: NextApiResponse, methods: string[]) {
  res.setHeader('Allow', methods.join(', '));
  return res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
}

export function noStore(res: NextApiResponse): void {
  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
}

export function respondJobError(res: NextApiResponse, error: unknown) {
  const safe = publicJobError(error);
  // The public body is deliberately generic; the function log must not be.
  // Without this line a 503 is unattributable from the outside (2026-09-12).
  if (safe.status >= 500) {
    const raw = error instanceof Error ? error.message : String(error);
    // eslint-disable-next-line no-console
    console.error(JSON.stringify({
      at: new Date().toISOString(),
      level: 'error',
      event: 'apocrypha.job_http.failed',
      public_code: safe.code,
      status: safe.status,
      detail: raw.replace(/(token|secret|key|authorization)[^,;\s]*/gi, '$1=<redacted>').slice(0, 300),
    }));
  }
  return res.status(safe.status).json({ ok: false, code: safe.code, error: safe.message });
}

export async function requireOwnerIdentity(req: NextApiRequest, res: NextApiResponse): Promise<JobIdentity | null> {
  const auth = await getAdminAuthorization(req);
  if (!auth.authorized || !auth.user) {
    res.status(auth.user ? 403 : 401).json({ ok: false, code: 'OWNER_REQUIRED', error: auth.reason ?? 'Owner sign-in required.' });
    return null;
  }
  return ensureOwnerIdentity(auth.user.id);
}

export async function requireChaosIdentity(req: NextApiRequest): Promise<JobIdentity> {
  const subject = assertChaosBridgeSignature({
    authorization: req.headers.authorization,
    method: req.method,
    url: req.url,
    body: req.method === 'GET' || req.method === 'HEAD' ? undefined : req.body,
    headers: req.headers,
  });
  return ensureExternalIdentity(subject);
}

export function stringField(value: unknown, name: string, max: number, required = true): string {
  if (typeof value !== 'string' || (required && !value.trim()) || value.length > max) {
    throw new Error(`INVALID_${name.toUpperCase()}`);
  }
  return value.trim();
}

export function objectField(value: unknown, name: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`INVALID_${name.toUpperCase()}`);
  return value as Record<string, unknown>;
}

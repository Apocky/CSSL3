import { timingSafeEqual } from 'node:crypto';
import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAuthorization } from '@/lib/admin-auth';
import { envelope } from '@/lib/response';

function bearerToken(req: NextApiRequest): string | null {
  const raw = req.headers.authorization;
  const first = Array.isArray(raw) ? raw[0] : raw;
  if (!first?.startsWith('Bearer ')) return null;
  const token = first.slice('Bearer '.length).trim();
  return token || null;
}

function tokenEquals(actual: string, expected: string): boolean {
  const actualBuf = Buffer.from(actual);
  const expectedBuf = Buffer.from(expected);
  if (actualBuf.length !== expectedBuf.length) {
    timingSafeEqual(expectedBuf, expectedBuf);
    return false;
  }
  return timingSafeEqual(actualBuf, expectedBuf);
}

export async function requireAdmin(req: NextApiRequest, res: NextApiResponse): Promise<boolean> {
  const result = await getAdminAuthorization(req);
  if (result.authorized) return true;
  const status = result.user ? 403 : 401;
  res.status(status).json({
    error: result.reason ?? 'Admin authorization required.',
    authorized: false,
    ...envelope(),
  });
  return false;
}

export function requireRunnerToken(req: NextApiRequest, res: NextApiResponse): boolean {
  const expected = process.env.LAZARUS_RUNNER_TOKEN;
  if (!expected) {
    res.status(503).json({ error: 'Lazarus runner token is not configured.', ...envelope() });
    return false;
  }
  const actual = bearerToken(req);
  if (!actual || !tokenEquals(actual, expected)) {
    res.status(401).json({ error: 'Lazarus runner token required.', ...envelope() });
    return false;
  }
  return true;
}
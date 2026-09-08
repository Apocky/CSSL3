import type { NextApiRequest, NextApiResponse } from 'next';

import { assertWorkerRequest, getApocryphaServiceClient, publicJobError } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, objectField } from '@/lib/apocrypha/job-http';

export function workerDatabaseError(
  error: { code?: string | null; message?: string | null },
  operation = 'WORKER_RPC_FAILED',
): Error {
  const code = error.code ?? 'unknown';
  const message = error.message?.toLowerCase() ?? '';
  const workerAuthFailure = code === '28000' && (
    message.includes('worker authentication failed')
    || message.includes('worker is not admitted for this operation')
  );
  return new Error(workerAuthFailure ? 'WORKER_UNAUTHORIZED' : `${operation}:${code}`);
}

export async function workerRpc(
  req: NextApiRequest,
  res: NextApiResponse,
  rpc: string,
  map: (body: Record<string, unknown>, token: string) => Record<string, unknown>,
  project: (data: unknown) => Record<string, unknown> = (data) => ({ data }),
) {
  noStore(res);
  if (req.method !== 'POST') return methodNotAllowed(res, ['POST']);
  try {
    const token = assertWorkerRequest(req.headers.authorization);
    const body = objectField(req.body, 'body');
    const { data, error } = await getApocryphaServiceClient().rpc(rpc, map(body, token));
    if (error) throw workerDatabaseError(error);
    return res.status(200).json({ ok: true, ...project(data) });
  } catch (error) {
    const safe = publicJobError(error);
    return res.status(safe.status).json({ ok: false, code: safe.code, error: safe.message });
  }
}

export function required(body: Record<string, unknown>, name: string): unknown {
  const value = body[name];
  if (value === undefined || value === null || value === '') throw new Error(`INVALID_${name.toUpperCase()}`);
  return value;
}

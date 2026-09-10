import type { NextApiRequest, NextApiResponse } from 'next';

import { assertWorkerRequest, getApocryphaServiceClient, publicJobError } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, objectField } from '@/lib/apocrypha/job-http';

class WorkerDatabaseError extends Error {
  constructor(
    message: string,
    readonly operation: string,
    readonly databaseCode: string,
  ) {
    super(message);
    this.name = 'WorkerDatabaseError';
  }
}

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
  return new WorkerDatabaseError(
    workerAuthFailure ? 'WORKER_UNAUTHORIZED' : `${operation}:${code}`,
    operation,
    code,
  );
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
    console.error(JSON.stringify({
      at: new Date().toISOString(),
      level: 'error',
      event: 'apocrypha.worker_rpc.failed',
      rpc,
      operation: error instanceof WorkerDatabaseError ? error.operation : 'REQUEST_VALIDATION',
      database_code: error instanceof WorkerDatabaseError ? error.databaseCode : null,
      public_code: safe.code,
    }));
    return res.status(safe.status).json({ ok: false, code: safe.code, error: safe.message });
  }
}

export function required(body: Record<string, unknown>, name: string): unknown {
  const value = body[name];
  if (value === undefined || value === null || value === '') throw new Error(`INVALID_${name.toUpperCase()}`);
  return value;
}

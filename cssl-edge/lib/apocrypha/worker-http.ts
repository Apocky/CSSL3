import type { NextApiRequest, NextApiResponse } from 'next';

import { assertWorkerRequest, getApocryphaServiceClient, publicJobError } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, objectField } from '@/lib/apocrypha/job-http';

class WorkerDatabaseError extends Error {
  constructor(
    message: string,
    readonly operation: string,
    readonly databaseCode: string,
    /** A bounded, redacted form of what the database or transport actually
     *  said. Held separately from `message`, which is a stable classification
     *  the response shape depends on and must not start carrying prose. */
    readonly detail: string,
  ) {
    super(message);
    this.name = 'WorkerDatabaseError';
  }
}

/**
 * Make an upstream failure safe to write to a log.
 *
 * These messages are worth keeping - see `workerDatabaseError` - but they come
 * from a layer that has seen row values and connection settings, so the parts
 * that could carry a secret are removed before anything is written down. A log
 * line is forever and ends up in places a credential should not be.
 *
 * Bounded too: an unbounded upstream string is an unbounded log line.
 */
function redactDetail(raw: string | null | undefined): string {
  if (!raw) return '';
  return raw
    .replace(/eyJ[A-Za-z0-9_-]{10,}/g, '<jwt>')
    .replace(/sb_(secret|publishable)_[A-Za-z0-9_-]+/g, '<key>')
    .replace(/postgres(ql)?:\/\/[^\s'"]+/gi, '<dsn>')
    .replace(/https?:\/\/[^\s'"]+/gi, '<url>')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 300);
}

function isWorkerFenceFailure(code: string, message: string): boolean {
  if (code === '40001') return message.includes('stale or invalid worker lease fence');
  if (code === '55000') {
    return message.includes('attempt is no longer active')
      || message.includes('job is no longer executable');
  }
  if (code === '57014') {
    return message.includes('worker lease expired')
      || message.includes('job cancellation requested');
  }
  return false;
}

export function workerDatabaseError(
  error: { code?: string | null; message?: string | null },
  operation = 'WORKER_RPC_FAILED',
): Error {
  // An absent code is itself a signal: a PostgreSQL error always carries a
  // SQLSTATE, so a missing one means the call did not reach the database -
  // a fetch failure, a timeout, a gateway error. Production showed exactly
  // this on apocrypha_claim_job, eight times over two days, and it was
  // undiagnosable because the message was discarded here.
  const code = error.code ?? 'unknown';
  const message = error.message?.toLowerCase() ?? '';
  const workerAuthFailure = code === '28000' && (
    message.includes('worker authentication failed')
    || message.includes('worker is not admitted for this operation')
  );
  const workerFenceFailure = isWorkerFenceFailure(code, message);
  return new WorkerDatabaseError(
    workerAuthFailure
      ? 'WORKER_UNAUTHORIZED'
      : workerFenceFailure
        ? 'WORKER_FENCE_LOST'
        : `${operation}:${code}`,
    operation,
    code,
    // The original text, kept. Classification above deliberately collapses
    // every unrecognised failure to one string, which is right for the
    // response and useless for the operator.
    redactDetail(error.message),
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
      // Without this the log said `database_code: "unknown"` and nothing else,
      // which cannot distinguish a network blip from a real database fault.
      database_detail: error instanceof WorkerDatabaseError
        ? error.detail
        : redactDetail(error instanceof Error ? error.message : String(error)),
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

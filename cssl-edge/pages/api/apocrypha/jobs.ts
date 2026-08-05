import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAllowlist, getRequestUser } from '@/lib/admin-auth';
import {
  isOpaqueClientRequestId,
  isOpaqueConversationId,
  scopeRequestId,
  setPrivateNoStore,
} from '@/lib/apocrypha/proxy';
import {
  cancelRuntimeBackgroundJob,
  getRuntimeBackgroundJob,
  listRuntimeBackgroundJobs,
  publicMemberPrincipalRef,
  publicRuntimeError,
  RuntimeProxyError,
  submitRuntimeBackgroundJob,
} from '@/lib/apocv4/runtime-proxy';
import { hasSameOrigin } from '@/lib/auth-session';
import { envelope } from '@/lib/response';
import { createServerTrace, traceparentFor } from '@/lib/telemetry/server';

const OWNER_RUNTIME_PRIVACY_PARTITION = 'owner:apocky';
const PUBLIC_RUNTIME_PRIVACY_PARTITION = 'public:apocrypha';
const JOB_ID_RE = /^job:[0-9a-f]{64}$/;
const MAX_OBJECTIVE_BYTES = 16_384;
const SUBMIT_WINDOW_MS = 60_000;
const SUBMIT_WINDOW_LIMIT = 2;
const MAX_SUBMIT_BUDGETS = 10_000;
const submitBudgets = new Map<string, { count: number; resetAt: number }>();

interface JobBody {
  operation?: unknown;
  session_id?: unknown;
  request_id?: unknown;
  objective?: unknown;
  job_id?: unknown;
}

export const config = {
  api: {
    bodyParser: {
      sizeLimit: '24kb',
    },
  },
};

function firstHeader(value: string | string[] | undefined): string | null {
  return (Array.isArray(value) ? value[0] : value)?.split(';')[0]?.trim().toLowerCase() || null;
}

function exactObject(value: unknown, expectedKeys: readonly string[]): value is Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
  const keys = Object.keys(value).sort();
  const expected = [...expectedKeys].sort();
  return keys.length === expected.length && keys.every((key, index) => key === expected[index]);
}

function singleQuery(value: string | string[] | undefined): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function admitSubmission(principalRef: string, now = Date.now()): number {
  const current = submitBudgets.get(principalRef);
  if (!current || current.resetAt <= now) {
    if (submitBudgets.size >= MAX_SUBMIT_BUDGETS) {
      for (const [key, budget] of submitBudgets) {
        if (budget.resetAt <= now) submitBudgets.delete(key);
      }
      if (submitBudgets.size >= MAX_SUBMIT_BUDGETS) {
        const oldest = submitBudgets.keys().next().value as string | undefined;
        if (oldest) submitBudgets.delete(oldest);
      }
    }
    submitBudgets.set(principalRef, { count: 1, resetAt: now + SUBMIT_WINDOW_MS });
    return 0;
  }
  if (current.count >= SUBMIT_WINDOW_LIMIT) {
    return Math.max(1, Math.ceil((current.resetAt - now) / 1_000));
  }
  current.count += 1;
  return 0;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse,
): Promise<void> {
  const trace = createServerTrace(req);
  setPrivateNoStore(res);
  res.setHeader('X-Apocky-Trace-Id', trace.traceId);
  res.setHeader('Traceparent', traceparentFor(trace));
  res.setHeader('Allow', 'GET, POST');

  if (req.method !== 'GET' && req.method !== 'POST') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  if (req.method === 'POST' && !hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', ...envelope() });
    return;
  }
  if (req.method === 'POST' && firstHeader(req.headers['content-type']) !== 'application/json') {
    res.status(415).json({ error: 'Content-Type must be application/json', ...envelope() });
    return;
  }

  const member = await getRequestUser(req);
  if (!member.user) {
    const unavailable = member.failureKind === 'upstream-unavailable'
      || member.failureKind === 'unconfigured';
    res.status(unavailable ? 503 : 401).json({
      error: unavailable
        ? 'Sign-in verification is temporarily unavailable.'
        : 'Sign in to access background work.',
      authenticated: false,
      ...envelope(),
    });
    return;
  }

  const principalRef = publicMemberPrincipalRef(member.user.id);
  const ownerProfile = getAdminAllowlist().includes(member.user.email.toLowerCase());
  const binding = {
    sessionPrincipal: principalRef,
    privacyPartition: ownerProfile
      ? OWNER_RUNTIME_PRIVACY_PARTITION
      : PUBLIC_RUNTIME_PRIVACY_PARTITION,
    credentialProfile: ownerProfile ? 'owner' as const : 'public' as const,
  };

  try {
    if (req.method === 'GET') {
      const keys = Object.keys(req.query).sort();
      const validKeys = (keys.length === 1 && keys[0] === 'session_id')
        || (keys.length === 2 && keys[0] === 'job_id' && keys[1] === 'session_id');
      if (!validKeys) {
        res.status(400).json({ error: 'query must contain session_id and optional job_id', ...envelope() });
        return;
      }
      const sessionId = singleQuery(req.query.session_id);
      const jobId = singleQuery(req.query.job_id);
      if (!isOpaqueConversationId(sessionId) || (jobId !== null && !JOB_ID_RE.test(jobId))) {
        res.status(400).json({ error: 'job query is invalid', ...envelope() });
        return;
      }
      if (jobId === null) {
        const projection = await listRuntimeBackgroundJobs({
          ...binding,
          sessionId,
        }, traceparentFor(trace));
        res.status(200).json({
          jobs: projection.jobs,
          count: projection.count,
          upstream_status: projection.observed.receipt.upstream_status,
          ...envelope(),
        });
      } else {
        const projection = await getRuntimeBackgroundJob({
          ...binding,
          sessionId,
          jobId,
        }, traceparentFor(trace));
        res.status(200).json({
          job: projection.job,
          upstream_status: projection.observed.receipt.upstream_status,
          ...envelope(),
        });
      }
      return;
    }

    const body = req.body as JobBody;
    const submit = exactObject(body, ['operation', 'session_id', 'request_id', 'objective'])
      && body.operation === 'submit';
    const cancel = exactObject(body, ['operation', 'session_id', 'request_id', 'job_id'])
      && body.operation === 'cancel';
    if (submit === cancel) {
      res.status(400).json({ error: 'body must contain one exact job operation', ...envelope() });
      return;
    }
    if (!isOpaqueConversationId(body.session_id) || !isOpaqueClientRequestId(body.request_id)) {
      res.status(400).json({ error: 'job identifiers are invalid', ...envelope() });
      return;
    }
    const sessionId = body.session_id.toLowerCase();
    const requestId = scopeRequestId(principalRef, body.request_id.toLowerCase());
    if (submit) {
      const objective = typeof body.objective === 'string' ? body.objective.trim() : '';
      if (
        !objective
        || objective !== body.objective
        || Buffer.byteLength(objective, 'utf8') > MAX_OBJECTIVE_BYTES
      ) {
        res.status(400).json({ error: 'objective must contain 1-16384 canonical UTF-8 bytes', ...envelope() });
        return;
      }
      const retryAfterSeconds = admitSubmission(principalRef);
      if (retryAfterSeconds > 0) {
        res.setHeader('Retry-After', String(retryAfterSeconds));
        res.status(429).json({
          error: 'This account already started the bounded background-work budget. Try again shortly.',
          retry_after_seconds: retryAfterSeconds,
          ...envelope(),
        });
        return;
      }
      const projection = await submitRuntimeBackgroundJob({
        ...binding,
        sessionId,
        requestId,
        objective,
        maxIterations: 1,
      }, traceparentFor(trace));
      res.status(202).json({
        job: projection.job,
        upstream_status: projection.observed.receipt.upstream_status,
        ...envelope(),
      });
      return;
    }

    if (typeof body.job_id !== 'string' || !JOB_ID_RE.test(body.job_id)) {
      res.status(400).json({ error: 'job_id is invalid', ...envelope() });
      return;
    }
    const projection = await cancelRuntimeBackgroundJob({
      ...binding,
      sessionId,
      requestId,
      jobId: body.job_id,
    }, traceparentFor(trace));
    res.status(200).json({
      job: projection.job,
      upstream_status: projection.observed.receipt.upstream_status,
      ...envelope(),
    });
  } catch (error) {
    const status = error instanceof RuntimeProxyError ? error.publicStatus : 502;
    res.status(status).json({ ...publicRuntimeError(error), ...envelope() });
  }
}

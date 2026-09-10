// Authenticated, principal-bound durable conversation access for Apocrypha.
// The member principal is derived server-side and never accepted from the client.

import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAllowlist, getRequestUser } from '@/lib/admin-auth';
import {
  isOpaqueClientRequestId,
  isOpaqueConversationId,
  scopeRequestId,
  setPrivateNoStore,
} from '@/lib/apocrypha/proxy';
import {
  deleteRuntimeSession,
  getRuntimeSession,
  listRuntimeSessions,
  publicMemberPrincipalRef,
  publicRuntimeError,
  RuntimeProxyError,
} from '@/lib/apocv4/runtime-proxy';
import { hasSameOrigin } from '@/lib/auth-session';
import { envelope } from '@/lib/response';
import { createServerTrace, traceparentFor } from '@/lib/telemetry/server';

const OWNER_RUNTIME_PRIVACY_PARTITION = 'owner:apocky';
const PUBLIC_RUNTIME_PRIVACY_PARTITION = 'public:apocrypha';

interface DeleteBody {
  session_id?: unknown;
  request_id?: unknown;
}

export const config = {
  api: {
    bodyParser: {
      sizeLimit: '2kb',
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

function requestedSessionId(req: NextApiRequest): string | null | undefined {
  const keys = Object.keys(req.query);
  if (keys.length === 0) return null;
  if (keys.length !== 1 || keys[0] !== 'session_id') return undefined;
  const value = req.query.session_id;
  return isOpaqueConversationId(value) ? value.toLowerCase() : undefined;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse,
): Promise<void> {
  const trace = createServerTrace(req);
  setPrivateNoStore(res);
  res.setHeader('X-Apocky-Trace-Id', trace.traceId);
  res.setHeader('Traceparent', traceparentFor(trace));
  res.setHeader('Allow', 'GET, DELETE');

  if (req.method !== 'GET' && req.method !== 'DELETE') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  if (req.method === 'DELETE' && !hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', ...envelope() });
    return;
  }
  if (req.method === 'DELETE' && firstHeader(req.headers['content-type']) !== 'application/json') {
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
        : 'Sign in to access Apocrypha conversations.',
      authenticated: false,
      ...envelope(),
    });
    return;
  }

  const principalRef = publicMemberPrincipalRef(member.user.id);
  const ownerProfile = getAdminAllowlist().includes(member.user.email.toLowerCase());
  const privacyPartition = ownerProfile
    ? OWNER_RUNTIME_PRIVACY_PARTITION
    : PUBLIC_RUNTIME_PRIVACY_PARTITION;
  const credentialProfile = ownerProfile ? 'owner' : 'public';
  const binding = {
    sessionPrincipal: principalRef,
    privacyPartition,
    credentialProfile,
  } as const;

  try {
    if (req.method === 'GET') {
      const sessionId = requestedSessionId(req);
      if (sessionId === undefined) {
        res.status(400).json({
          error: 'query must be empty or contain one opaque UUIDv4 session_id',
          ...envelope(),
        });
        return;
      }
      if (sessionId === null) {
        const projection = await listRuntimeSessions({ ...binding, limit: 24 }, traceparentFor(trace));
        res.status(200).json({
          sessions: projection.sessions,
          count: projection.count,
          upstream_status: projection.observed.receipt.upstream_status,
          ...envelope(),
        });
        return;
      }
      const projection = await getRuntimeSession({
        ...binding,
        sessionId,
      }, traceparentFor(trace));
      res.status(200).json({
        session: projection.session,
        upstream_status: projection.observed.receipt.upstream_status,
        ...envelope(),
      });
      return;
    }

    if (!exactObject(req.body, ['session_id', 'request_id'])) {
      res.status(400).json({
        error: 'body must contain exactly session_id and request_id',
        ...envelope(),
      });
      return;
    }
    const body = req.body as DeleteBody;
    if (!isOpaqueConversationId(body.session_id)) {
      res.status(400).json({
        error: 'session_id must be an opaque UUIDv4 minted by this client',
        ...envelope(),
      });
      return;
    }
    if (!isOpaqueClientRequestId(body.request_id)) {
      res.status(400).json({
        error: 'request_id must be an opaque UUIDv4 minted once for this client request',
        ...envelope(),
      });
      return;
    }
    const sessionId = body.session_id.toLowerCase();
    const clientRequestId = body.request_id.toLowerCase();
    const projection = await deleteRuntimeSession({
      ...binding,
      sessionId,
      requestId: scopeRequestId(principalRef, clientRequestId),
    }, traceparentFor(trace));
    res.status(200).json({
      session_id: sessionId,
      request_id: clientRequestId,
      deleted: projection.deleted,
      event_digest: projection.event_digest,
      upstream_status: projection.observed.receipt.upstream_status,
      ...envelope(),
    });
  } catch (error) {
    const status = error instanceof RuntimeProxyError ? error.publicStatus : 502;
    res.status(status).json({ ...publicRuntimeError(error), ...envelope() });
  }
}

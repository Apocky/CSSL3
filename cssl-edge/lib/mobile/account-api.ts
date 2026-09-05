import type { NextApiRequest, NextApiResponse } from 'next';
import { randomUUID } from 'node:crypto';
import { getRequestUser, type RequestUserResult } from '../admin-auth';
import { ACCOUNT_UUID, CONVERSATION_UUID } from './account-grant';
import { accountRuntimeConfigured, AccountRuntimeError, callAccountRuntime } from './account-runtime';
import { usesOwnerRuntime, callOwnerMobileRuntime, ownerMobileRuntimeConfigured } from './owner-runtime';
import { accountDiagnostic, accountDiagnosticCode, accountDiagnosticReason, type AccountDiagnostic, type AccountDiagnosticCode } from './diagnostics';

type Surface = 'turn' | 'sessions' | 'status';
type Dict = Record<string, unknown>;
const SHA = /^[a-f0-9]{64}$/;
function record(value: unknown): value is Dict { return Boolean(value && typeof value === 'object' && !Array.isArray(value)); }
function text(value: unknown, max = 131_072): value is string { return typeof value === 'string' && Buffer.byteLength(value) <= max; }
function stamp(value: unknown): value is string { return typeof value === 'string' && value.length <= 40 && Number.isFinite(Date.parse(value)); }

function verifiedTurn(value: Dict, sent: Dict): Dict {
  if (value.schema_version !== 'apocky.mobile.turn.v1' || value.status !== 'completed'
    || value.session_id !== sent.session_id || value.request_id !== sent.request_id
    || !text(value.text) || value.text.length === 0 || !text(value.model_id, 512)
    || typeof value.response_digest !== 'string' || !SHA.test(value.response_digest)) throw new AccountRuntimeError('ACCOUNT_TURN_UNVERIFIED');
  return { schema_version: value.schema_version, status: value.status, text: value.text,
    session_id: value.session_id, request_id: value.request_id, model_id: value.model_id, response_digest: value.response_digest };
}

function verifiedSummary(value: unknown): Dict {
  if (!record(value) || typeof value.session_id !== 'string' || !CONVERSATION_UUID.test(value.session_id)
    || !text(value.title, 640) || !stamp(value.updated_at) || !Number.isSafeInteger(value.message_count)
    || Number(value.message_count) < 0) throw new AccountRuntimeError('ACCOUNT_HISTORY_UNVERIFIED');
  return { session_id: value.session_id, title: value.title, updated_at: value.updated_at, message_count: value.message_count };
}

export function verifiedHistory(value: Dict, sessionId?: string): Dict {
  if (value.status !== 'live') throw new AccountRuntimeError('ACCOUNT_HISTORY_UNVERIFIED');
  if (!sessionId) {
    if (value.schema_version !== 'apocky.mobile.sessions.v1' || !Array.isArray(value.sessions) || value.sessions.length > 100
      || value.count !== value.sessions.length || typeof value.discovery_scope !== 'string'
      || !['account_conversations', 'latest_conversation_only'].includes(value.discovery_scope)) {
      throw new AccountRuntimeError('ACCOUNT_HISTORY_UNVERIFIED');
    }
    return { schema_version: value.schema_version, status: 'live', sessions: value.sessions.map(verifiedSummary),
      count: value.count, discovery_scope: value.discovery_scope };
  }
  const session = value.session;
  if (value.schema_version !== 'apocky.mobile.session.v1' || !record(session)
    || session.schema_version !== 'apocky.mobile.history-session.v1' || session.session_id !== sessionId
    || !text(session.title, 640) || !stamp(session.created_at) || !stamp(session.updated_at)
    || typeof session.events_truncated !== 'boolean' || !Array.isArray(session.messages) || session.messages.length > 64) {
    throw new AccountRuntimeError('ACCOUNT_HISTORY_UNVERIFIED');
  }
  const messages = session.messages.map((message: unknown) => {
    if (!record(message) || typeof message.role !== 'string' || !['user', 'assistant'].includes(message.role) || !text(message.content)
      || typeof message.request_id !== 'string' || !CONVERSATION_UUID.test(message.request_id)
      || !stamp(message.recorded_at) || typeof message.event_digest !== 'string' || !SHA.test(message.event_digest)) {
      throw new AccountRuntimeError('ACCOUNT_HISTORY_UNVERIFIED');
    }
    return { role: message.role, content: message.content, request_id: message.request_id,
      recorded_at: message.recorded_at, event_digest: message.event_digest };
  });
  return { schema_version: value.schema_version, status: 'live', session: {
    schema_version: session.schema_version, session_id: sessionId, title: session.title,
    created_at: session.created_at, updated_at: session.updated_at, events_truncated: session.events_truncated, messages,
  } };
}

export function createAccountHandler(surface: Surface, dependencies: {
  user?: (req: NextApiRequest) => Promise<RequestUserResult>;
  configured?: () => boolean;
  call?: typeof callAccountRuntime;
  log?: (diagnostic: AccountDiagnostic) => void;
} = {}) {
  return async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
    const started = Date.now(); const traceId = randomUUID();
    res.setHeader('X-Apocky-Trace-Id', traceId);
    res.setHeader('Cache-Control', 'private, no-store, max-age=0');
    res.setHeader('Vary', 'Authorization, Cookie, Origin');
    res.setHeader('X-Content-Type-Options', 'nosniff');
    res.setHeader('X-Robots-Tag', 'noindex, nofollow');
    const method = surface === 'turn' ? 'POST' : 'GET';
    res.setHeader('Allow', method);
    const finish = (status: number, code: AccountDiagnosticCode, body: Dict, includeDiagnostic = false) => {
      const diagnostic = accountDiagnostic({ operation: surface, status, code, time: new Date().toISOString(), duration_ms: Math.max(0, Date.now() - started), trace_id: traceId });
      try { if (dependencies.log) dependencies.log(diagnostic); else console.info(JSON.stringify(diagnostic)); } catch { /* ○ logging.unavailable ; response.preserved */ }
      res.status(status).json({ ...body, ...(includeDiagnostic ? { code: diagnostic.code, stage: diagnostic.stage, diagnostic } : {}) });
    };
    const fail = (status: number, code: unknown, _message?: string) => {
      const known = accountDiagnosticCode(code); const safe = known === 'ACCOUNT_OK' ? 'ACCOUNT_SERVICE_UNAVAILABLE' : known; const diagnostic = accountDiagnostic({ operation: surface, status, code: safe });
      finish(status, safe, { error: accountDiagnosticReason(diagnostic) }, true);
    };
    const degraded = (code: unknown) => {
      const known = accountDiagnosticCode(code); const safe = known === 'ACCOUNT_OK' ? 'ACCOUNT_STATUS_UNVERIFIED' : known; const diagnostic = accountDiagnostic({ operation: surface, status: 200, code: safe });
      finish(200, safe, { schema_version: 'apocky.mobile.status.v1', status: 'degraded', message: accountDiagnosticReason(diagnostic) }, true);
    };
    if (req.method !== method) { fail(405, 'ACCOUNT_METHOD_DENIED', 'This request method is unavailable.'); return; }
    if (method === 'POST') {
      const localOrigin = process.env.NODE_ENV !== 'production' && typeof req.headers.origin === 'string'
        && /^http:\/\/(localhost|127\.0\.0\.1):[0-9]{1,5}$/.test(req.headers.origin)
        && Number(req.headers.origin.split(':').at(-1)) <= 65535
        && new URL(req.headers.origin).host === req.headers.host;
      if (req.headers.origin !== 'https://www.apocky.com' && !localOrigin) {
        fail(403, 'ACCOUNT_ORIGIN_DENIED', 'Open Apocrypha from apocky.com or its official app.'); return;
      }
      if (typeof req.headers['content-type'] !== 'string'
        || req.headers['content-type'].split(';', 1)[0]?.trim().toLowerCase() !== 'application/json') {
        fail(415, 'ACCOUNT_CONTENT_TYPE_REQUIRED', 'Send this request as JSON.'); return;
      }
    }
    let session: RequestUserResult;
    try { session = await (dependencies.user ?? getRequestUser)(req); }
    catch { fail(503, 'ACCOUNT_SIGN_IN_UNAVAILABLE', 'Sign-in could not be verified. Please retry.'); return; }
    if (!session.user || !ACCOUNT_UUID.test(session.user.id)) {
      const unavailable = ['upstream-unavailable', 'unconfigured'].includes(session.failureKind ?? '');
      fail(unavailable ? 503 : 401, unavailable ? 'ACCOUNT_SIGN_IN_UNAVAILABLE' : 'ACCOUNT_SIGN_IN_REQUIRED',
        unavailable ? 'Sign-in could not be verified. Please retry.' : 'Sign in or create an account to continue.'); return;
    }
    let sessionId: string | undefined;
    if (surface === 'sessions') {
      if (Object.keys(req.query).some((key) => key !== 'session_id')
        || (req.query.session_id !== undefined && (typeof req.query.session_id !== 'string' || !CONVERSATION_UUID.test(req.query.session_id)))) {
        fail(400, 'ACCOUNT_SESSION_INVALID', 'Choose a valid conversation.'); return;
      }
      sessionId = req.query.session_id as string | undefined;
    } else if (Object.keys(req.query).length !== 0) { fail(400, 'ACCOUNT_QUERY_INVALID', 'This request has unsupported parameters.'); return; }
    if (surface === 'turn' && (!record(req.body) || Object.keys(req.body).sort().join(',') !== 'request_id,session_id,text'
      || !text(req.body.text, 16_384) || !req.body.text || req.body.text !== req.body.text.trim()
      || typeof req.body.session_id !== 'string' || !CONVERSATION_UUID.test(req.body.session_id)
      || typeof req.body.request_id !== 'string' || !CONVERSATION_UUID.test(req.body.request_id))) {
      fail(400, 'ACCOUNT_TURN_INVALID', 'Enter a message and start a valid conversation.'); return;
    }
    const ownerRoute = usesOwnerRuntime(session.user);
    let configured = false;
    try { configured = (dependencies.configured ?? (ownerRoute ? ownerMobileRuntimeConfigured : accountRuntimeConfigured))(); } catch { /* ○ configuration.unavailable */ }
    if (!configured) {
      if (surface === 'status') degraded('ACCOUNT_CONFIGURATION_UNAVAILABLE');
      else fail(503, 'ACCOUNT_CONFIGURATION_UNAVAILABLE');
      return;
    }
    try {
      const value = ownerRoute && !dependencies.call
        ? await callOwnerMobileRuntime({ user: session.user, surface, sessionId,
          ...(surface === 'turn' ? { body: req.body as Record<string, string> } : {}) })
        : await (dependencies.call ?? callAccountRuntime)({ subject: session.user.id, method,
        target: `/v1/account/${surface}${sessionId ? `?session_id=${sessionId}` : ''}`,
        ...(surface === 'turn' ? { body: req.body as Record<string, string> } : {}) });
      if (surface === 'turn') finish(200, 'ACCOUNT_OK', verifiedTurn(value, req.body as Dict));
      else if (surface === 'sessions') finish(200, 'ACCOUNT_OK', verifiedHistory(value, sessionId));
      else {
        if (value.schema_version !== 'apocky.mobile.status.v1' || typeof value.status !== 'string'
          || !['live', 'degraded'].includes(value.status)) throw new AccountRuntimeError('ACCOUNT_STATUS_UNVERIFIED');
        if (value.status === 'degraded') degraded('ACCOUNT_FACULTY_UNREADY');
        else finish(200, 'ACCOUNT_OK', { schema_version: value.schema_version, status: 'live' });
      }
    } catch (error) {
      // ∀ post-admission failure → 5xx ; clients retain uncertain request identity.
      if (surface === 'status') degraded(error instanceof AccountRuntimeError ? error.code : 'ACCOUNT_SERVICE_UNAVAILABLE');
      else fail(error instanceof AccountRuntimeError ? error.publicStatus : 502,
        error instanceof AccountRuntimeError ? error.code : 'ACCOUNT_SERVICE_UNAVAILABLE',
        surface === 'turn' ? 'A completed reply could not be verified. Refresh this conversation before sending again.' : 'This conversation could not be loaded. Please retry.');
    }
  };
}

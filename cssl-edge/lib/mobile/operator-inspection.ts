import { createClient } from '@supabase/supabase-js';
import type { NextApiRequest, NextApiResponse } from 'next';
import { getAdminAuthorization } from '../admin-auth';
import { setBrainPrivateHeaders } from '../brain/owner';
import { createServerTrace, emitOperationalTelemetry } from '../telemetry/server';
import { readAdminTelemetry } from '../telemetry/admin-reader';
import { ACCOUNT_UUID, CONVERSATION_UUID } from './account-grant';
import { callAccountRuntime, AccountRuntimeError } from './account-runtime';
import { verifiedHistory } from './account-api';

type Inspection = { purpose: 'debugging' | 'research' } & (
  { action: 'aggregate' } |
  { action: 'users'; page: number } |
  { action: 'sessions'; subject: string } |
  { action: 'session'; subject: string; session_id: string }
);
type Row = Record<string, unknown>;
function object(value: unknown): value is Row { return value !== null && typeof value === 'object' && !Array.isArray(value); }
function parse(value: unknown): Inspection | null {
  if (!object(value) || typeof value.purpose !== 'string' || !['debugging', 'research'].includes(value.purpose)) return null;
  const keys = Object.keys(value).sort().join(',');
  if (value.action === 'aggregate' && keys === 'action,purpose') return value as Inspection;
  if (value.action === 'users' && keys === 'action,page,purpose' && Number.isSafeInteger(value.page)
    && Number(value.page) >= 1 && Number(value.page) <= 100_000) return value as Inspection;
  if (typeof value.subject !== 'string' || !ACCOUNT_UUID.test(value.subject)) return null;
  if (value.action === 'sessions' && keys === 'action,purpose,subject') return value as Inspection;
  if (value.action === 'session' && keys === 'action,purpose,session_id,subject'
    && typeof value.session_id === 'string' && CONVERSATION_UUID.test(value.session_id)) return value as Inspection;
  return null;
}

async function listUsers(page: number): Promise<Row> {
  const url = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!url || !key) throw new AccountRuntimeError('OPERATOR_DIRECTORY_UNAVAILABLE', 503);
  const client = createClient(url, key, { auth: { persistSession: false, autoRefreshToken: false } });
  const { data, error } = await client.auth.admin.listUsers({ page, perPage: 50 });
  if (error || !data?.users) throw new AccountRuntimeError('OPERATOR_DIRECTORY_UNAVAILABLE', 503);
  return { page, page_size: 50, total_accounts: 'total' in data && Number.isSafeInteger(data.total) ? data.total : null,
    has_more: data.users.length === 50, users: data.users.map(user => ({
    id: user.id, email: user.email ?? null, created_at: user.created_at,
    last_sign_in_at: user.last_sign_in_at ?? null, email_confirmed_at: user.email_confirmed_at ?? null,
  })) };
}

export function createOperatorInspectionHandler(dependencies: {
  authorize?: typeof getAdminAuthorization;
  audit?: (req: NextApiRequest, actor: string, query: Inspection) => Promise<boolean>;
  users?: typeof listUsers;
  call?: typeof callAccountRuntime;
  telemetry?: typeof readAdminTelemetry;
} = {}) {
  return async (req: NextApiRequest, res: NextApiResponse): Promise<void> => {
    setBrainPrivateHeaders(res); res.setHeader('Allow', 'POST');
    const fail = (status: number, code: string) => { res.status(status).json({ code, error: 'The inspection could not be completed. See its error code and retry.' }); };
    if (req.method !== 'POST') { fail(405, 'OPERATOR_METHOD_DENIED'); return; }
    if (req.headers.origin !== 'https://www.apocky.com' && !(process.env.NODE_ENV !== 'production'
      && typeof req.headers.origin === 'string' && /^http:\/\/(?:localhost|127\.0\.0\.1):[0-9]{1,5}$/.test(req.headers.origin)
      && new URL(req.headers.origin).host === req.headers.host)) { fail(403, 'OPERATOR_ORIGIN_DENIED'); return; }
    if (req.headers['content-type']?.split(';', 1)[0]?.trim() !== 'application/json') { fail(415, 'OPERATOR_JSON_REQUIRED'); return; }
    let authorization;
    try { authorization = await (dependencies.authorize ?? getAdminAuthorization)(req); }
    catch { fail(503, 'OPERATOR_AUTH_UNAVAILABLE'); return; }
    if (!authorization.authorized || !authorization.user) { fail(authorization.user ? 403 : 401, 'OPERATOR_REQUIRED'); return; }
    const query = Object.keys(req.query).length === 0 ? parse(req.body) : null;
    if (!query) { fail(400, 'OPERATOR_INSPECTION_INVALID'); return; }
    try {
      const audit = dependencies.audit ?? (async (request, actor, selection) => {
        const receipt = await emitOperationalTelemetry({ trace: createServerTrace(request),
          kind: 'operator.user_data.inspect', source: 'operator-inspection', plane: 'security', severity: 'info',
          outcome: 'accepted', authority: 'VERIFIED_ADMIN', attributes: {
            actor_id: actor, purpose: selection.purpose, action: selection.action,
            ...(selection.action === 'users' ? { page: selection.page } : selection.action === 'aggregate' ? {} : { subject_id: selection.subject }),
            ...(selection.action === 'session' ? { conversation_id: selection.session_id } : {}),
          } });
        return receipt.persisted;
      });
      if (!await audit(req, authorization.user.id, query)) { fail(503, 'OPERATOR_AUDIT_UNAVAILABLE'); return; }
      let result: Row;
      if (query.action === 'aggregate') {
        const [directory, telemetry] = await Promise.all([(dependencies.users ?? listUsers)(1), (dependencies.telemetry ?? readAdminTelemetry)(500)]);
        if (telemetry.source !== 'supabase') throw new AccountRuntimeError('OPERATOR_TELEMETRY_UNAVAILABLE', 503);
        const counts = (field: 'severity' | 'outcome' | 'plane') => {
          const values = new Map<string, number>();
          const allowed = field === 'severity' ? ['debug', 'info', 'warn', 'error', 'critical']
            : field === 'plane' ? ['edge', 'browser', 'effect', 'security', 'model', 'memory', 'transport', 'control', 'data']
            : ['accepted', 'completed', 'failed', 'denied', 'degraded', 'observed', 'started', 'pending', 'cancelled', 'success'];
          for (const row of telemetry.rows) {
            const key = allowed.includes(row[field]) ? row[field] : 'other';
            values.set(key, (values.get(key) ?? 0) + 1);
          }
          return Object.fromEntries(values);
        };
        result = { total_accounts: Number.isSafeInteger(directory.total_accounts) ? directory.total_accounts : null,
          recent_events: telemetry.rows.length, older_events_available: telemetry.hasMore,
          from: telemetry.rows.at(-1)?.ts ?? null, through: telemetry.rows[0]?.ts ?? null,
          severity: counts('severity'), outcome: counts('outcome'), plane: counts('plane'),
          scope: 'Most recent stored operational events across accounts; this is a bounded sample, not lifetime totals.' };
      } else result = query.action === 'users' ? await (dependencies.users ?? listUsers)(query.page)
        : verifiedHistory(await (dependencies.call ?? callAccountRuntime)({ subject: query.subject, method: 'GET',
          target: query.action === 'sessions' ? '/v1/account/sessions' : `/v1/account/sessions?session_id=${query.session_id}` }),
        query.action === 'session' ? query.session_id : undefined);
      res.status(200).json({ schema_version: 'apocky.operator-inspection.v1', observed_at: new Date().toISOString(),
        action: query.action, purpose: query.purpose, result });
    } catch (error) { fail(error instanceof AccountRuntimeError ? error.publicStatus : 503,
      error instanceof AccountRuntimeError ? error.code : 'OPERATOR_INSPECTION_UNAVAILABLE'); }
  };
}

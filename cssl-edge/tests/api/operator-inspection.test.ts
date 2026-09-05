import assert from 'node:assert/strict';
import type { NextApiRequest, NextApiResponse } from 'next';
import type { AdminAuthorizationResult } from '../../lib/admin-auth';
import { createOperatorInspectionHandler } from '../../lib/mobile/operator-inspection';
import { usesOwnerRuntime, callOwnerMobileRuntime } from '../../lib/mobile/owner-runtime';
import { createAccountHandler } from '../../lib/mobile/account-api';
import type { AdminTelemetryRow } from '../../lib/telemetry/admin-reader';

const actor = '11111111-1111-4111-8111-111111111111';
const subject = '22222222-2222-4222-8222-222222222222';
const session = '33333333-3333-4333-8333-333333333333';
const request = '44444444-4444-4444-8444-444444444444';
const stamp = '2026-09-04T00:00:00.000Z';
const owner = { id: actor, email: 'operator@example.test', provider: 'email', createdAt: stamp };
const member = { ...owner, id: subject, email: 'member@example.test' };
const authorized = async (): Promise<AdminAuthorizationResult> => ({ authorized: true, user: owner, authConfigured: true });
const list = { schema_version: 'apocky.mobile.sessions.v1', status: 'live', sessions: [{ session_id: session, title: 'A private fixture', updated_at: stamp, message_count: 2, credentials: 'SECRET_SENTINEL' }], count: 1, discovery_scope: 'account_conversations', credentials: 'SECRET_SENTINEL' };
const history = { schema_version: 'apocky.mobile.session.v1', status: 'live', session: { schema_version: 'apocky.mobile.history-session.v1', session_id: session, title: 'A private fixture', created_at: stamp, updated_at: stamp, events_truncated: false, messages: [{ role: 'assistant', content: 'SELECTED_PRIVATE_FIXTURE', request_id: request, recorded_at: stamp, event_digest: 'a'.repeat(64), credentials: 'SECRET_SENTINEL' }], credentials: 'SECRET_SENTINEL' } };
type Handler = ReturnType<typeof createOperatorInspectionHandler>;
async function invoke(handler: Handler, override: Partial<NextApiRequest> = {}) {
  const result = { status: 0, body: null as any, headers: {} as Record<string, string> };
  const req = { method: 'POST', headers: { origin: 'https://www.apocky.com', host: 'www.apocky.com', 'content-type': 'application/json' }, query: {}, body: { action: 'sessions', purpose: 'debugging', subject }, ...override } as NextApiRequest;
  const res = { setHeader(k: string, v: string) { result.headers[k.toLowerCase()] = v; return this; }, status(value: number) { result.status = value; return this; }, json(value: unknown) { result.body = value; return this; } } as unknown as NextApiResponse;
  await handler(req, res);
  assert.match(result.headers['cache-control'] ?? '', /private, no-store/);
  assert(!JSON.stringify(result).includes('SECRET_SENTINEL'));
  return result;
}
async function run() {
  const events: string[] = [];
  const handler = createOperatorInspectionHandler({ authorize: authorized, audit: async (_req, who, query) => { assert.equal(who, actor); assert.equal(query.purpose, 'debugging'); events.push(`audit:${query.action}`); return true; }, call: async input => { events.push('runtime'); assert.equal(input.subject, subject); assert.equal(input.method, 'GET'); assert.equal(input.target, '/v1/account/sessions'); return list; } });
  const result = await invoke(handler);
  assert.equal(result.status, 200); assert.equal(result.body.result.sessions[0].title, 'A private fixture');
  assert.deepEqual(events, ['audit:sessions', 'runtime']);
  const aggregateEvents: string[] = [];
  const telemetryBase: AdminTelemetryRow = { id: '1', ts: stamp, eventId: actor, traceId: 'a'.repeat(32), spanId: 'b'.repeat(16), parentSpanId: null,
    source: 'fixture', plane: 'runtime', severity: 'info', kind: 'fixture.observed', outcome: 'accepted', route: null, status: null, durationMs: null,
    message: 'SECRET_SENTINEL', fingerprint: 'a'.repeat(64), clusterSignature: null, deploymentId: 'fixture', effectClass: null, authority: null,
    receiptRef: null, privacyTier: 'private', sessionRef: subject, payload: { email: 'PRIVATE_EMAIL_SENTINEL', message: 'SECRET_SENTINEL' } };
  const telemetryRows: AdminTelemetryRow[] = [telemetryBase, { ...telemetryBase, id: '2', severity: 'error', outcome: 'failed', plane: 'edge', payload: { subject } }];
  const aggregate = createOperatorInspectionHandler({ authorize: authorized, audit: async (_req, who, query) => { assert.equal(who, actor); assert.equal(query.action, 'aggregate'); aggregateEvents.push('audit'); return true; },
    users: async page => { assert.equal(page, 1); aggregateEvents.push('directory'); return { total_accounts: 87, users: [{ email: 'PRIVATE_EMAIL_SENTINEL' }] }; },
    telemetry: async limit => { assert.equal(limit, 500); aggregateEvents.push('telemetry'); return { source: 'supabase', rows: telemetryRows, cursor: '2', hasMore: true }; } });
  const summary = await invoke(aggregate, { body: { action: 'aggregate', purpose: 'debugging' } });
  assert.equal(summary.status, 200); assert.equal(summary.body.result.total_accounts, 87); assert.equal(summary.body.result.recent_events, 2);
  assert.deepEqual(summary.body.result.severity, { info: 1, error: 1 }); assert.deepEqual(summary.body.result.outcome, { accepted: 1, failed: 1 });
  assert.match(summary.body.result.scope, /bounded sample/); assert.equal(summary.body.result.older_events_available, true);
  assert.equal(aggregateEvents[0], 'audit'); assert.deepEqual(new Set(aggregateEvents.slice(1)), new Set(['directory', 'telemetry']));
  assert(!JSON.stringify(summary).includes('PRIVATE_EMAIL_SENTINEL')); assert(!JSON.stringify(summary).includes(subject)); assert(!('users' in summary.body.result)); assert(!('rows' in summary.body.result));
  const detail = createOperatorInspectionHandler({ authorize: authorized, audit: async (_req, who, query) => { assert.equal(who, actor); assert.equal(query.action, 'session'); return true; }, call: async input => { assert.deepEqual(input, { subject, method: 'GET', target: `/v1/account/sessions?session_id=${session}` }); return history; } });
  assert.equal((await invoke(detail, { body: { action: 'session', subject, session_id: session, purpose: 'research' } })).body.result.session.messages[0].content, 'SELECTED_PRIVATE_FIXTURE');
  for (const state of [{ authorized: false, user: null, authConfigured: true }, { authorized: false, user: member, authConfigured: true }, { authorized: true, user: null, authConfigured: true }]) {
    let calls = 0;
    const denied = createOperatorInspectionHandler({ authorize: async () => state, audit: async () => { calls += 1; return true; }, users: async () => { calls += 1; return {}; }, call: async () => { calls += 1; return list; } });
    assert.equal((await invoke(denied)).status, state.user ? 403 : 401);
    assert.equal((await invoke(denied, { body: { action: 'aggregate', purpose: 'research' } })).status, state.user ? 403 : 401); assert.equal(calls, 0);
  }
  let downstream = 0;
  for (const audit of [async () => false, async () => { throw new Error('SECRET_SENTINEL'); }]) {
    const failed = createOperatorInspectionHandler({ authorize: authorized, audit, call: async () => { downstream += 1; return list; } });
    assert.equal((await invoke(failed)).status, 503);
  }
  assert.equal(downstream, 0, 'audit must persist before any selected account read');
  let aggregateReads = 0;
  const aggregateDenied = createOperatorInspectionHandler({ authorize: authorized, audit: async () => false, users: async () => { aggregateReads += 1; return {}; }, telemetry: async () => { aggregateReads += 1; return { rows: [], source: 'supabase', cursor: null, hasMore: false }; } });
  assert.equal((await invoke(aggregateDenied, { body: { action: 'aggregate', purpose: 'research' } })).status, 503); assert.equal(aggregateReads, 0);
  const aggregateUnavailable = createOperatorInspectionHandler({ authorize: authorized, audit: async () => true, users: async () => ({ total_accounts: 87 }), telemetry: async () => ({ rows: [], source: 'failed', cursor: null, hasMore: false }) });
  assert.equal((await invoke(aggregateUnavailable, { body: { action: 'aggregate', purpose: 'research' } })).status, 503);
  const beforeInvalid = events.length;
  for (const body of [{ action: 'sessions', subject, purpose: ['debugging'] }, { action: 'sessions', subject, purpose: 'other' }, { action: 'sessions', subject: subject + 'x', purpose: 'debugging' }, { action: 'users', page: 0, purpose: 'debugging' }, { action: 'users', page: 1.5, purpose: 'debugging' }, { action: 'session', subject, session_id: 'not-an-id', purpose: 'debugging' }, { action: 'sessions', subject, purpose: 'debugging', token: 'SECRET_SENTINEL' }]) assert.equal((await invoke(handler, { body })).status, 400);
  assert.equal(events.length, beforeInvalid);
  assert.equal((await invoke(handler, { method: 'GET' })).status, 405);
  assert.equal((await invoke(handler, { headers: { origin: 'https://evil.example', 'content-type': 'application/json' } })).status, 403);
  assert.equal((await invoke(handler, { query: { subject: actor } })).status, 400);
  assert.equal((await invoke(handler, { headers: { origin: 'https://www.apocky.com', 'content-type': 'text/plain' } })).status, 415);
  assert.equal((await invoke(createOperatorInspectionHandler({ authorize: async () => { throw new Error('SECRET_SENTINEL'); } }))).status, 503);
  const mismatched = createOperatorInspectionHandler({ authorize: authorized, audit: async () => true, call: async () => ({ ...history, session: { ...history.session, session_id: actor } }) });
  assert.equal((await invoke(mismatched, { body: { action: 'session', subject, session_id: session, purpose: 'research' } })).status, 502);

  const previous = { flag: process.env.APOCV4_MOBILE_OWNER_BRIDGE, emails: process.env.APOCKY_ADMIN_EMAILS };
  try {
    process.env.APOCKY_ADMIN_EMAILS = owner.email;
    delete process.env.APOCV4_MOBILE_OWNER_BRIDGE;
    assert.equal(usesOwnerRuntime(owner), false);
    process.env.APOCV4_MOBILE_OWNER_BRIDGE = 'true'; assert.equal(usesOwnerRuntime(owner), false);
    process.env.APOCV4_MOBILE_OWNER_BRIDGE = '1'; assert.equal(usesOwnerRuntime(owner), true); assert.equal(usesOwnerRuntime({ ...owner, email: owner.email.toUpperCase() }), true);
    assert.equal(usesOwnerRuntime(member), false);
    await assert.rejects(callOwnerMobileRuntime({ user: member, surface: 'status' }), /ACCOUNT_RESPONSE_SCOPE_MISMATCH/);
    let accountCalls = 0;
    const ordinary = createAccountHandler('sessions', { user: async () => ({ user: member, authConfigured: true }), configured: () => true, log: () => {}, call: async input => { accountCalls += 1; assert.equal(input.subject, member.id); return list; } });
    assert.equal((await invoke(ordinary, { method: 'GET', body: undefined })).status, 200); assert.equal(accountCalls, 1);
  } finally {
    if (previous.flag === undefined) delete process.env.APOCV4_MOBILE_OWNER_BRIDGE; else process.env.APOCV4_MOBILE_OWNER_BRIDGE = previous.flag;
    if (previous.emails === undefined) delete process.env.APOCKY_ADMIN_EMAILS; else process.env.APOCKY_ADMIN_EMAILS = previous.emails;
  }
  process.stdout.write('Operator inspection authorization, audited selection, privacy projection and owner routing checks passed.\n');
}
run().catch(error => { process.stderr.write(String(error) + '\n'); process.exitCode = 1; });

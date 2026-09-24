// tests/room-api.spec.ts -- the living room's API boundary.
// Run: node --import tsx tests/room-api.spec.ts
//
// PostgREST is stubbed at globalThis.fetch, the way the worker HTTP tests do it, so every case
// runs offline and the request the handler actually sent can be inspected.

import type { NextApiRequest, NextApiResponse } from 'next';

import { resetApocryphaServiceClientForTests } from '@/lib/apocrypha/job-control';
import { resetOwnerCacheForTests } from '@/lib/room/store';
import eventsHandler from '@/pages/api/room/events';
import sayHandler from '@/pages/api/room/say';
import pullHandler from '@/pages/api/room/worker/pull';

interface Out { status: number; body: Record<string, unknown>; headers: Record<string, string> }

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function request(input: {
  method: string;
  query?: Record<string, string>;
  headers?: Record<string, string>;
  body?: unknown;
}): NextApiRequest {
  return {
    method: input.method,
    query: input.query ?? {},
    headers: input.headers ?? {},
    body: input.body,
  } as unknown as NextApiRequest;
}

function response(): { res: NextApiResponse; out: Out } {
  const out: Out = { status: 0, body: {}, headers: {} };
  const res = {
    status(code: number) { out.status = code; return this; },
    json(body: unknown) { out.body = body as Record<string, unknown>; return this; },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse;
  return { res, out };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
}

const SAME_ORIGIN = { host: 'www.apocky.com', origin: 'https://www.apocky.com', 'content-type': 'application/json' };
const originalFetch = globalThis.fetch;
const calls: string[] = [];

function stubStore(rows: (url: string) => unknown): void {
  calls.length = 0;
  globalThis.fetch = async (input) => {
    const url = String(input);
    calls.push(url);
    return jsonResponse(rows(url));
  };
}

async function guestCannotReadOwnerRoom(): Promise<void> {
  stubStore(() => []);
  const { res, out } = response();
  await eventsHandler(request({ method: 'GET', query: { room: 'owner', after: '0' } }), res);
  assert(out.status === 403, `guest reading room=owner returned ${out.status}, expected 403`);
  assert(out.body.code === 'OWNER_REQUIRED', `expected OWNER_REQUIRED, got ${String(out.body.code)}`);
  assert(calls.length === 0, `owner-room refusal must not touch the store; it made ${calls.length} request(s)`);
  assert(out.headers['cache-control'] === 'no-store', 'events must be no-store');
}

async function sayRejectsBadBodies(): Promise<void> {
  stubStore(() => []);
  for (const [label, body, code] of [
    ['empty', '', 'BODY_EMPTY'],
    ['whitespace', '   \n\t ', 'BODY_EMPTY'],
    ['too long', 'x'.repeat(4_001), 'BODY_TOO_LONG'],
  ] as const) {
    const { res, out } = response();
    await sayHandler(request({ method: 'POST', headers: SAME_ORIGIN, body: { room: 'lobby', body } }), res);
    assert(out.status === 400, `say with ${label} body returned ${out.status}, expected 400`);
    assert(out.body.code === code, `say with ${label} body gave ${String(out.body.code)}, expected ${code}`);
  }
  assert(calls.length === 0, `body validation must happen before any store call; saw ${calls.length}`);

  // The boundary that makes the 400s meaningful: a 4,000-char body is accepted (guest path,
  // store stubbed), so the limit is exactly where it says it is.
  stubStore((url) => (url.includes('/rest/v1/apocrypha_room_events') && !url.includes('select=created_at')
    ? [{ id: 9, room: 'lobby', author: 'guest:abc', kind: 'utterance', body: 'ok', meta: {}, created_at: '2026-09-24T10:00:00.000Z' }]
    : []));
  const { res, out } = response();
  await sayHandler(request({ method: 'POST', headers: SAME_ORIGIN, body: { room: 'lobby', body: 'y'.repeat(4_000) } }), res);
  assert(out.status === 201, `say with a 4000-char body returned ${out.status}: ${JSON.stringify(out.body)}`);
  assert(typeof out.headers['set-cookie'] === 'string' && out.headers['set-cookie'].includes('apx_guest='), 'a new guest must be issued the guest cookie');

  const missingOrigin = response();
  await sayHandler(request({ method: 'POST', headers: { host: 'www.apocky.com' }, body: { room: 'lobby', body: 'hi' } }), missingOrigin.res);
  assert(missingOrigin.out.status === 403 && missingOrigin.out.body.code === 'ORIGIN_REQUIRED', 'cross-origin say must be refused');
}

async function eventsReturnsNewestPresence(): Promise<void> {
  const presenceRows = [{ body: 'speaking', created_at: '2026-09-24T10:00:05.000Z' }];
  const eventRows = [
    { id: 3, room: 'lobby', author: 'apocrypha', kind: 'utterance', body: 'three', meta: {}, created_at: '2026-09-24T10:00:03.000Z' },
    { id: 2, room: 'lobby', author: 'guest:abc', kind: 'utterance', body: 'two', meta: {}, created_at: '2026-09-24T10:00:02.000Z' },
    { id: 1, room: 'lobby', author: 'apocrypha', kind: 'presence', body: 'idle', meta: {}, created_at: '2026-09-24T10:00:01.000Z' },
  ];
  stubStore((url) => (url.includes('kind=eq.presence') ? presenceRows : eventRows));

  const { res, out } = response();
  await eventsHandler(request({ method: 'GET', query: { room: 'lobby', after: '0', limit: '50' } }), res);
  assert(out.status === 200, `events returned ${out.status}: ${JSON.stringify(out.body)}`);
  const presence = out.body.presence as { state: string; at: string } | null;
  assert(presence !== null && presence.state === 'speaking', `presence must come from the newest presence row, got ${JSON.stringify(presence)}`);
  assert(presence.at === '2026-09-24T10:00:05.000Z', 'presence.at must be that row\'s created_at');

  const presenceUrl = calls.find((url) => url.includes('kind=eq.presence'));
  assert(presenceUrl !== undefined, 'events must query presence rows');
  assert(presenceUrl.includes('order=id.desc') && presenceUrl.includes('limit=1'), `presence query must ask for the single newest row: ${presenceUrl}`);
  assert(presenceUrl.includes('room=eq.lobby'), 'presence must be scoped to the requested room');

  const events = out.body.events as Array<{ id: number }>;
  assert(events.map((e) => e.id).join(',') === '1,2,3', `first page must be oldest-first, got ${events.map((e) => e.id).join(',')}`);
  assert(typeof out.body.now === 'string', 'events must carry the server clock');
  assert(!('viewer' in out.body), 'viewer is only reported when asked for');
}

async function workerPullNeedsToken(): Promise<void> {
  stubStore(() => []);
  const { res, out } = response();
  await pullHandler(request({ method: 'GET', query: { after: '0' } }), res);
  assert(out.status === 401 && out.body.code === 'WORKER_UNAUTHORIZED', `worker pull without a token returned ${out.status}`);
  assert(calls.length === 0, 'an unshaped token must be refused before the store is consulted');
}

async function main(): Promise<void> {
  process.env.APOCKY_HUB_SUPABASE_URL = 'https://test.supabase.co';
  process.env.SUPABASE_SERVICE_ROLE_KEY = 'service_test';
  delete process.env.LAZARUS_TEST_AUTH_BYPASS;
  resetApocryphaServiceClientForTests();
  resetOwnerCacheForTests();
  try {
    await guestCannotReadOwnerRoom();
    await sayRejectsBadBodies();
    await eventsReturnsNewestPresence();
    await workerPullNeedsToken();
    console.log('room-api.spec : OK - owner room 403 for guests; say bounds 1..4000 + origin; presence = newest row; worker token gate');
  } finally {
    globalThis.fetch = originalFetch;
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});

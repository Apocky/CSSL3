// cssl-edge · tests/api/apocrypha-member-threads.test.ts
// 0057: threads (new / pin / archive), the plan (premium -> flagship), consent, attachments.
import type { NextApiRequest, NextApiResponse } from 'next';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { attachmentTextFrom, type MemberChatThread } from '@/lib/apocrypha/member-chat';
import { createMemberThreadsHandler } from '@/pages/api/apocrypha/member/threads';
import { createMemberThreadUpdateHandler } from '@/pages/api/apocrypha/member/threads/[id]';
import { createMemberPlanHandler } from '@/pages/api/apocrypha/member/plan';
import { createMemberConsentHandler } from '@/pages/api/apocrypha/member/consent';
import { createMemberAttachmentHandler } from '@/pages/api/apocrypha/member/attachments';
import { createMemberChatSubmitHandler } from '@/pages/api/apocrypha/member/jobs';

const USER = '11111111-1111-4111-8111-111111111111';
const THREAD = '44444444-4444-4444-8444-444444444444';
const REQUEST = '22222222-2222-4222-8222-222222222222';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}
function reqRes(method: string, options: { body?: unknown; query?: Record<string, string>; origin?: string | null } = {}) {
  const out = { statusCode: 0, body: null as unknown, headers: {} as Record<string, string> };
  const headers: Record<string, string> = { host: 'www.apocky.com', 'x-forwarded-proto': 'https', 'content-type': 'application/json' };
  if (options.origin !== null) headers.origin = options.origin ?? 'https://www.apocky.com';
  const req = { method, body: options.body, query: options.query ?? {}, headers } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(value: unknown) { out.body = value; return this; },
    setHeader(name: string, value: string | number | readonly string[]) { out.headers[name.toLowerCase()] = String(value); return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}
const member = { authConfigured: true, user: { id: USER, email: 'm@example.test', provider: 'test', createdAt: new Date(0).toISOString() } };
const signedOut = { authConfigured: true, user: null, failureKind: 'unauthenticated' as const };
function thread(overrides: Partial<MemberChatThread> = {}): MemberChatThread {
  return { thread_id: THREAD, title: 'New conversation', pinned: false, archived: false, created_at: '2026-09-25T00:00:00.000Z', last_active_at: '2026-09-25T00:00:00.000Z', turn_count: 0, preview: null, ...overrides };
}

async function testThreadsListCreate(): Promise<void> {
  const calls: string[] = [];
  const handler = createMemberThreadsHandler({
    async resolveUser() { return member; },
    async list(input) { calls.push(`list:${input.verifiedAuthUserId}:${input.includeArchived}`); return [thread({ pinned: true }), thread({ thread_id: '55555555-5555-4555-8555-555555555555' })]; },
    async create(input) { calls.push(`create:${input.title}`); return thread({ title: input.title ?? 'New conversation' }); },
  });
  const list = reqRes('GET', { query: { archived: '1' } });
  await handler(list.req, list.res);
  assert(list.out.statusCode === 200 && (list.out.body as { count: number }).count === 2, 'list returns the member threads');
  assert(calls[0] === `list:${USER}:true`, 'the verified user and the archive flag reach the store');
  const created = reqRes('POST', { body: { title: 'Trip planning' } });
  await handler(created.req, created.res);
  assert(created.out.statusCode === 201 && (created.out.body as { thread: MemberChatThread }).thread.title === 'Trip planning', 'new chat is created with its title');
  const bad = reqRes('POST', { body: { title: 'x', extra: true } });
  await handler(bad.req, bad.res);
  assert(bad.out.statusCode === 400, 'unknown body keys are refused');
  const anon = createMemberThreadsHandler({ async resolveUser() { return signedOut; }, async list() { return []; }, async create() { return thread(); } });
  const unauth = reqRes('GET');
  await anon(unauth.req, unauth.res);
  assert(unauth.out.statusCode === 401, 'signed-out callers are refused');
  const cross = reqRes('POST', { body: { title: 'x' }, origin: 'https://attacker.example' });
  await handler(cross.req, cross.res);
  assert(cross.out.statusCode === 403, 'cross-origin writes are refused');
}

async function testThreadUpdate(): Promise<void> {
  let seen: unknown = null;
  const handler = createMemberThreadUpdateHandler({ async resolveUser() { return member; }, async update(input) { seen = input; return thread({ pinned: input.pinned === true, archived: input.archived === true, title: input.title ?? 'New conversation' }); } });
  const pin = reqRes('PATCH', { query: { id: THREAD }, body: { pinned: true } });
  await handler(pin.req, pin.res);
  assert(pin.out.statusCode === 200 && (pin.out.body as { thread: MemberChatThread }).thread.pinned, 'pin flips the thread');
  assert((seen as { threadId: string }).threadId === THREAD && (seen as { archived: unknown }).archived === null, 'unset fields are null, not false');
  const archive = reqRes('PATCH', { query: { id: THREAD }, body: { archived: true } });
  await handler(archive.req, archive.res);
  assert((archive.out.body as { thread: MemberChatThread }).thread.archived, 'archive flips the thread');
  const bad = reqRes('PATCH', { query: { id: THREAD }, body: { pinned: 'yes' } });
  await handler(bad.req, bad.res);
  assert(bad.out.statusCode === 400, 'a non-boolean pin is refused');
  const badId = reqRes('PATCH', { query: { id: 'nope' }, body: { pinned: true } });
  await handler(badId.req, badId.res);
  assert(badId.out.statusCode === 400, 'a non-uuid thread id is refused');
}

async function testPlanAndSubmitLane(): Promise<void> {
  const premium = createMemberPlanHandler({ async resolveUser() { return member; }, async plan() { return { flagship: true, default_lane: 'flagship', product_id: 'apocrypha-premium' }; } });
  const p = reqRes('GET');
  await premium(p.req, p.res);
  assert(p.out.statusCode === 200 && (p.out.body as { plan: { default_lane: string } }).plan.default_lane === 'flagship', 'a premium member defaults to the flagship lane');
  const free = createMemberPlanHandler({ async resolveUser() { return member; }, async plan() { return { flagship: false, default_lane: 'local', product_id: 'apocrypha-premium' }; } });
  const f = reqRes('GET');
  await free(f.req, f.res);
  assert((f.out.body as { plan: { default_lane: string } }).plan.default_lane === 'local', 'a free member defaults to the local lane');

  const submissions: unknown[] = [];
  const submit = createMemberChatSubmitHandler({
    async resolveUser() { return member; },
    async enqueue(input) {
      submissions.push(input);
      return { job_id: '33333333-3333-4333-8333-333333333333', conversation_id: USER, request_id: REQUEST, status: 'queued', model_alias: 'x', memory_manifest_hash: 'a'.repeat(64), created_at: '2026-09-25T00:00:00.000Z', updated_at: '2026-09-25T00:00:00.000Z', replayed: false, thread_id: THREAD, engine_lane: input.engineLane ?? 'local' };
    },
  });
  const flagship = reqRes('POST', { body: { conversation_id: USER, request_id: REQUEST, message: 'hi', thread_id: THREAD, engine_lane: 'flagship', attachment_ids: [] } });
  await submit(flagship.req, flagship.res);
  assert(flagship.out.statusCode === 202, `lane + thread submit is accepted (${flagship.out.statusCode})`);
  assert((submissions[0] as { engineLane: string; threadId: string }).engineLane === 'flagship' && (submissions[0] as { threadId: string }).threadId === THREAD, 'lane and thread cross to the store');
  const legacy = reqRes('POST', { body: { conversation_id: USER, request_id: REQUEST, message: 'hi' } });
  await submit(legacy.req, legacy.res);
  assert(legacy.out.statusCode === 202 && (submissions[1] as { engineLane: string }).engineLane === 'local', 'the three-key body still works and defaults to local');
  const badLane = reqRes('POST', { body: { conversation_id: USER, request_id: REQUEST, message: 'hi', engine_lane: 'cloud' } });
  await submit(badLane.req, badLane.res);
  assert(badLane.out.statusCode === 400, 'an unknown lane is refused');
  const unknownKey = reqRes('POST', { body: { conversation_id: USER, request_id: REQUEST, message: 'hi', tenant_id: 'x' } });
  await submit(unknownKey.req, unknownKey.res);
  assert(unknownKey.out.statusCode === 400, 'unknown keys are still refused');
}

async function testConsentAndAttachments(): Promise<void> {
  let stored: unknown = null;
  const consent = createMemberConsentHandler({ async resolveUser() { return member; }, async set(input) { stored = input; return { analytics: input.analytics, updated_at: 'now' }; } });
  const on = reqRes('POST', { body: { analytics: true } });
  await consent(on.req, on.res);
  assert(on.out.statusCode === 200 && (stored as { analytics: boolean }).analytics === true, 'consent is stored for the verified user');
  const bad = reqRes('POST', { body: { analytics: 'yes' } });
  await consent(bad.req, bad.res);
  assert(bad.out.statusCode === 400, 'consent must be a boolean');

  assert(attachmentTextFrom('text/markdown', Buffer.from('# hi')) === '# hi', 'markdown text is extracted');
  assert(attachmentTextFrom('image/png', Buffer.from([0x89, 0x50])) === null, 'images carry no text');
  const uploads: unknown[] = [];
  const attach = createMemberAttachmentHandler({
    async resolveUser() { return member; },
    async store(input) { uploads.push(input.storagePath); },
    async register(input) { return { id: '66666666-6666-4666-8666-666666666666', thread_id: THREAD, file_name: input.fileName, mime_type: input.mimeType, byte_size: input.byteSize, has_text: input.extractedText !== null, created_at: 'now' }; },
  });
  const up = reqRes('POST', { body: { thread_id: THREAD, file_name: 'notes.txt', mime_type: 'text/plain', data_base64: Buffer.from('hello notes').toString('base64') } });
  await attach(up.req, up.res);
  assert(up.out.statusCode === 201, `upload is accepted (${up.out.statusCode} ${JSON.stringify(up.out.body)})`);
  assert((up.out.body as { attachment: { has_text: boolean; byte_size: number } }).attachment.has_text && (up.out.body as { attachment: { byte_size: number } }).attachment.byte_size === 11, 'text and size are recorded');
  assert(String(uploads[0]).startsWith(`${USER}/${THREAD}/`), 'bytes are stored under the member and thread');
  const empty = reqRes('POST', { body: { file_name: 'x', mime_type: 'text/plain', data_base64: '' } });
  await attach(empty.req, empty.res);
  assert(empty.out.statusCode === 400, 'an empty upload is refused');
}

function testMigrationShape(): void {
  const sql = readFileSync(resolve(process.cwd(), '..', 'cssl-supabase', 'migrations', '0057_apocrypha_member_threads_lane_attachments.sql'), 'utf8');
  for (const fragment of [
    'CREATE TABLE public.apocrypha_member_chat_thread',
    "ADD CONSTRAINT apocrypha_member_chat_request_lane_shape CHECK (engine_lane IN ('local', 'flagship'))",
    "IF v_lane = 'flagship' AND NOT public.apocrypha_member_has_flagship(p_verified_auth_user_id) THEN",
    "e.product_id = 'apocrypha-premium'",
    'AND request.thread_id = v_thread.id',
    "'engine_lane', v_lane,",
    "'attachments', v_attachments",
    'FUNCTION public.apocrypha_record_analytics_event',
    'RETURN false;',
    'FUNCTION public.apocrypha_admin_telemetry',
    'GRANT EXECUTE ON FUNCTION public.apocrypha_enqueue_member_chat_v3',
  ]) {
    assert(sql.includes(fragment), `migration 0057 carries: ${fragment}`);
  }
  assert(!/GRANT EXECUTE ON FUNCTION[^;]*TO (PUBLIC|authenticated)/.test(sql), 'no 0057 function is granted to PUBLIC or authenticated');
}

async function main(): Promise<void> {
  await testThreadsListCreate();
  await testThreadUpdate();
  await testPlanAndSubmitLane();
  await testConsentAndAttachments();
  testMigrationShape();
  // eslint-disable-next-line no-console
  console.log('apocrypha-member-threads.test : OK · threads, plan lane, consent, attachments, migration shape');
}
main().catch((error) => { console.error(error); process.exit(1); });

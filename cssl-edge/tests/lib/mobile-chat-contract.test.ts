import assert from 'node:assert/strict';
import { parseAccountSession, parseAccountSessions, parseAccountTurn } from '@/lib/mobile/chat-contract';

const sessionId = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const otherId = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const requestId = 'cccccccc-cccc-5ccc-8ccc-cccccccccccc';
const reply = { schema_version: 'apocky.mobile.turn.v1', status: 'completed', text: 'A test reply.', session_id: sessionId, request_id: requestId, model_id: 'fixture', response_digest: 'a'.repeat(64) };
assert.deepEqual(parseAccountTurn(reply, sessionId, requestId), { text: reply.text });
assert.deepEqual(parseAccountTurn({ ...reply, model_id: 'm'.repeat(512) }, sessionId, requestId), { text: reply.text }, 'browser accepts the full runtime model identifier bound');
assert.equal(parseAccountTurn({ ...reply, model_id: 'm'.repeat(513) }, sessionId, requestId), null);
for (const invalid of [
  { session_id: otherId }, { request_id: otherId }, { status: 'pending' },
  { schema_version: 'apocky.brain.runtime-turn.v1' }, { text: '' }, { text: 'x'.repeat(256 * 1024 + 1) },
  { response_digest: 'bad' }, { model_id: null },
]) assert.equal(parseAccountTurn({ ...reply, ...invalid }, sessionId, requestId), null, 'unbound or malformed replies must not enter conversation state');

const message = { role: 'assistant', content: 'Saved test reply.', request_id: requestId, recorded_at: '2026-09-04T12:00:00Z' };
const session = { schema_version: 'apocky.mobile.history-session.v1', session_id: sessionId, title: 'My conversation', messages: [message], events_truncated: false };
const history = { schema_version: 'apocky.mobile.session.v1', status: 'live', session };
assert.deepEqual(parseAccountSession(history, sessionId)?.messages, [message]);
assert.equal(parseAccountSession(history, otherId), null, 'history must belong to the requested session');
for (const invalid of [
  { role: 'system' }, { role: 'tool' }, { role: ['assistant'] }, { content: '' }, { content: 'x'.repeat(128 * 1024 + 1) },
  { request_id: 'not-a-request-id' }, { recorded_at: null },
]) assert.equal(parseAccountSession({ ...history, session: { ...session, messages: [{ ...message, ...invalid }] } }, sessionId), null);
assert.equal(parseAccountSession({ ...history, session: { ...session, messages: Array(129).fill(message) } }, sessionId), null);
assert.equal(parseAccountSession({ ...history, session: { ...session, events_truncated: 'false' } }, sessionId), null);
assert.equal(parseAccountSession({ ...history, session: { ...session, schema_version: 'apocky.brain.history-session.v1' } }, sessionId), null);

const summary = { session_id: sessionId, title: 'My conversation', message_count: 2 };
const list = { schema_version: 'apocky.mobile.sessions.v1', status: 'live', discovery_scope: 'account_conversations', sessions: [summary], count: 1 };
assert.deepEqual(parseAccountSessions(list)?.sessions, [summary]);
assert.equal(parseAccountSessions({ ...list, discovery_scope: 'owner_partition' }), null);
assert.equal(parseAccountSessions({ ...list, discovery_scope: ['account_conversations'] }), null);
assert.equal(parseAccountSessions({ ...list, count: 2 }), null);
assert.equal(parseAccountSessions({ ...list, sessions: [{ ...summary, message_count: -1 }] }), null);
assert.equal(parseAccountSessions({ ...list, sessions: Array(129).fill(summary), count: 129 }), null);
assert.equal(parseAccountSessions({ ...list, sessions: [{ ...summary, session_id: 'owner:apocky' }] }), null);
console.log('mobile chat contract: reply/session binding, allowed message roles, schema and size boundaries passed');

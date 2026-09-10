// § Preview boundary · fixture-only auth/vault/transport · N! production credentials
import { createElement, type AnchorHTMLAttributes } from 'react';
import type { MiniBrainMessage, MiniBrainState } from '../../lib/brain/mini-brain';
import { openAccountPendingJournal, type AccountMessage } from '../../lib/mobile/chat-contract';

export const mode = new URLSearchParams(location.search).get('mode') === 'owner' ? 'owner' : 'account';
export const pendingFixture = new URLSearchParams(location.search).get('pending') === '1';
const id = (tail: number) => 'f1000000-0000-4000-8000-' + String(tail).padStart(12, '0');
export const subject = id(pendingFixture ? 102 : 101);
const sessionId = id(1);
const otherSessionId = id(2);
const stamp = '2026-09-06T17:00:00.000Z';
const longText = 'A useful beginning is a question we can actually live with. We can make room for uncertainty while deciding what deserves our attention.\n\n';
const sourceMessages: AccountMessage[] = [
  { role: 'user', content: 'Help me turn a crowded day into one useful next step.', request_id: id(11), recorded_at: stamp },
  { role: 'assistant', content: 'Start with what matters today.\n\n' + longText.repeat(4) + '- Name one thing you want to protect.\n- Choose an action small enough to finish.\n- Leave room to change your mind.\n\n| Focus | Next step |\n| --- | --- |\n| Attention | Ten quiet minutes |\n| Responsibility | One clear conversation |', request_id: id(11), recorded_at: stamp },
  { role: 'user', content: 'I want clarity without pretending everything is certain. Can we make something from that?', request_id: id(12), recorded_at: stamp },
  { role: 'assistant', content: 'We can. Open Create to make a symbol, explore a reflection, or find a word. Add the result to your draft when you want to talk about it.\n\nA long label should remain readable: ' + 'meaning-and-responsibility-'.repeat(12), request_id: id(12), recorded_at: stamp },
];
const pendingTurn = { session_id: sessionId, request_id: id(99), text: 'Keep this question safe until a reply can be confirmed.' };
const accountMessages = sourceMessages.map(value => ({ ...value }));
const traffic: { method: string; path: string }[] = [];
const session = { access: mode === 'owner' ? 'owner' : 'member', authenticated: true, subjectKey: subject, refresh: async () => undefined };
export function usePreviewSession() { return session; }
export function usePreviewRouter() { return { asPath: '/apocrypha', pathname: '/apocrypha', query: {} }; }
export function PreviewLink({ href, children, ...props }: AnchorHTMLAttributes<HTMLAnchorElement>) {
  return createElement('a', { ...props, href: '#', onClick: (event: React.MouseEvent<HTMLAnchorElement>) => { event.preventDefault(); alert('Local fixture: navigation to ' + href + ' is disabled.'); } }, children);
}
export function getPreviewAuthClient() { return { auth: { getSession: async () => ({ data: { session: { access_token: 'local-fixture-token-no-service-authority', user: { id: subject } } }, error: null }) } }; }
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
export async function previewAuthFetch(input: RequestInfo | URL, init: RequestInit = {}): Promise<Response> {
  const url = new URL(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url, location.origin);
  if (url.origin !== location.origin) return json({ error: 'External requests disabled in fixture.' }, 403);
  traffic.push({ method: init.method || 'GET', path: url.pathname });
  if (init.signal?.aborted) throw new DOMException('Stopped waiting.', 'AbortError');
  if (url.pathname === '/api/mobile/sessions') {
    const chosen = url.searchParams.get('session_id');
    if (chosen) return json({ schema_version: 'apocky.mobile.session.v1', status: 'live', session: { schema_version: 'apocky.mobile.history-session.v1', session_id: chosen, title: chosen === sessionId ? 'A little room to think' : 'A second conversation', messages: chosen === sessionId ? accountMessages : [sourceMessages[0]], events_truncated: false } });
    return json({ schema_version: 'apocky.mobile.sessions.v1', status: 'live', discovery_scope: 'account_conversations', count: 2, sessions: [{ session_id: sessionId, title: 'A little room to think', message_count: accountMessages.length }, { session_id: otherSessionId, title: 'A second conversation', message_count: 1 }] });
  }
  if (url.pathname === '/api/mobile/turn') return json({ error: 'Fixture keeps this message pending.', code: 'ACCOUNT_SERVICE_UNAVAILABLE' }, 503);
  if (url.pathname === '/api/mobile/status') return json({ error: 'Fixture connection unavailable.', code: 'ACCOUNT_SERVICE_UNAVAILABLE' }, 503);
  if (url.pathname === '/api/brain/runtime/status') return json({ schema_version: 'apocky.owner-brain.runtime-status.v1', status: 'degraded', reason_code: 'BRAIN_OFFLINE', observed_at: stamp, latency_ms: null, upstream_status: null, served_by: 'local-ui-fixture', ts: stamp });
  if (url.pathname === '/api/brain/snapshot') return json({ schema_version: 'apocky.owner-brain.snapshot.v1', status: 'live', connectors: { mneme_storage: 'live', source_projection: 'live', local_apocv4: 'retired' }, memories: [{ id: 'fixture-memory', type: 'fact', csl: '§ focus := clarity + responsibility', paraphrase: 'Clarity can coexist with uncertainty.', topic_key: 'clarity', search_queries: ['clarity'], source_msg_ids: ['fixture-source'], created_at: stamp }], messages: [{ id: 'fixture-source', session_id: sessionId, role: 'user', content: sourceMessages[2]!.content, ts: stamp, source_only: true }], counts: { memories: 1, messages: 1, source_links: 1 }, limits: { memories: 20, recent_messages: 20, source_messages: 20 }, served_by: 'local-ui-fixture', ts: stamp });
  if (url.pathname === '/api/brain/observe') return json({ error: 'Fixture does not query runtime observations.', code: 'OBSERVATION_UNAVAILABLE' }, 503);
  return json({ error: 'This service is outside the local UI fixture.', code: 'FIXTURE_ROUTE_UNAVAILABLE' }, 503);
}
const ownerMessages: MiniBrainMessage[] = sourceMessages.map((value, index) => ({ ...value, id: id(200 + index), event_digest: '0'.repeat(64), origin: 'desktop', provenance_digests: [] }));
if (pendingFixture) ownerMessages.push({ id: id(299), role: 'user', content: pendingTurn.text, recorded_at: stamp, request_id: pendingTurn.request_id, event_digest: null, origin: 'queued-mobile', provenance_digests: [] });
const stateKey = 'apocky.chat-room.preview.owner.' + (pendingFixture ? 'pending' : 'ready');
let state: MiniBrainState = { revision: 1, schema_version: 'apocky.mini-brain.local-state.v1', owner_ref: 'fixture-owner', device_id: id(301), current_session_id: sessionId, selection_origin: 'user', sessions: [{ session_id: sessionId, cursor: null, messages: ownerMessages, updated_at: stamp, events_truncated: false, tombstoned_at: null }, { session_id: otherSessionId, cursor: null, messages: ownerMessages.slice(0, 2), updated_at: stamp, events_truncated: false, tombstoned_at: null }], memories: [], queue: pendingFixture ? [{ ...pendingTurn, queued_at: stamp, base_cursor: null, local_message_ids: [id(299)] }] : [], updated_at: stamp };
try { const saved = sessionStorage.getItem(stateKey); if (saved) state = JSON.parse(saved) as MiniBrainState; } catch { /* ◐ fixture storage unavailable */ }
const listeners = new Set<() => void>();
const clone = () => structuredClone(state);
function retain(next: MiniBrainState): MiniBrainState { state = { ...next, revision: (state.revision || 0) + 1, updated_at: new Date().toISOString() }; try { sessionStorage.setItem(stateKey, JSON.stringify(state)); } catch { /* ◐ fixture-only persistence */ } for (const listener of listeners) listener(); return clone(); }
const vault = {
  deviceId: state.device_id, isBound: true, tokenExpired: false,
  load: async () => clone(), freshState: async () => clone(),
  subscribe: (callback: () => void) => { listeners.add(callback); return () => listeners.delete(callback); },
  cacheSnapshot: async () => clone(),
  selectSession: async (_previous: MiniBrainState, chosen: string) => retain({ ...state, current_session_id: chosen, selection_origin: 'user' }),
  adoptDiscoveredSession: async () => clone(),
  queueTurn: async (_previous: MiniBrainState, text: string) => {
    const request_id = crypto.randomUUID(); const messageId = crypto.randomUUID(); const now = new Date().toISOString();
    const turn = { request_id, session_id: state.current_session_id, text, queued_at: now, base_cursor: null, local_message_ids: [messageId] };
    const message: MiniBrainMessage = { id: messageId, role: 'user', content: text, recorded_at: now, request_id, event_digest: null, origin: 'queued-mobile', provenance_digests: [] };
    const current = state.sessions.find(value => value.session_id === state.current_session_id) || { session_id: state.current_session_id, cursor: null, messages: [], updated_at: now, events_truncated: false, tombstoned_at: null };
    const next = retain({ ...state, sessions: [...state.sessions.filter(value => value.session_id !== current.session_id), { ...current, messages: [...current.messages, message], updated_at: now }], queue: [...state.queue, turn] });
    return { state: next, turn };
  },
  erase: async () => { throw new Error('Fixture does not erase device state.'); },
};
export async function openPreviewMiniBrain() { return { vault, reason_code: null }; }
export async function registerPreviewOfflineShell() { return false; }
export async function prepareFixture() {
  window.fetch = previewAuthFetch;
  if (mode === 'account' && pendingFixture) { const journal = await openAccountPendingJournal(); if (!await journal.load(subject)) await journal.save(subject, pendingTurn); }
  (window as Window & { chatRoomFixture?: unknown }).chatRoomFixture = {
    mode, pendingFixture, inspect: async () => ({ requests: [...traffic], ownerQueueIds: state.queue.map(value => value.request_id), accountPendingId: mode === 'account' ? (await (await openAccountPendingJournal()).load(subject))?.request_id || null : null }),
  };
}

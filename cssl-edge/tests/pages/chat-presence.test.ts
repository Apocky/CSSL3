import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const page = readFileSync(resolve(process.cwd(), 'pages/chat.tsx'), 'utf8');
const ownerPage = readFileSync(resolve(process.cwd(), 'pages/admin/chat.tsx'), 'utf8');
const publicRoom = readFileSync(resolve(process.cwd(), 'pages/apocrypha.tsx'), 'utf8');
const thread = readFileSync(resolve(process.cwd(), 'components/apocrypha/ChatThread.tsx'), 'utf8');
const rest = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocrypha/chat.ts'), 'utf8');
const retiredStream = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocrypha/chat_stream.ts'), 'utf8');
const history = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocrypha/conversations.ts'), 'utf8');
const presence = readFileSync(resolve(process.cwd(), 'pages/api/apocrypha/presence.ts'), 'utf8');
const vercel = JSON.parse(readFileSync(resolve(process.cwd(), 'vercel.json'), 'utf8')) as {
  functions?: Record<string, { maxDuration?: number }>;
};

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

for (const token of ['GetServerSideProps', '`/apocrypha${suffix}`', 'permanent: true']) {
  assert(page.includes(token), `public chat alias contract missing: ${token}`);
}
assert(!page.includes('ChatThread'), 'public chat alias must not expose the owner chat');
assert(publicRoom.includes('<ClearingRoom'), 'public chat alias target must render the Clearing');
for (const token of ['<ChatThread />', 'adminAuthorized', '<AdminLayout']) {
  assert(ownerPage.includes(token), `owner chat contract missing: ${token}`);
}
for (const source of [page, ownerPage, publicRoom, thread, rest, retiredStream, history, presence]) {
  assert(!source.includes('/api/v1'), 'predecessor route remains in production chat closure');
  assert(!source.includes('APOCRYPHA_V2_TURN_ENABLED'), 'V2 route has a fallback toggle');
}
assert(!existsSync(resolve(process.cwd(), 'pages/api/chat/send.ts')), 'dead queue endpoint still exists');
assert(!existsSync(resolve(process.cwd(), 'lib/chat-relay.ts')), 'dead queue relay still exists');
assert(!page.includes('<ApocryphaAvatar'), 'access page must not render an unauthorized avatar');
assert(!thread.includes('<ApocryphaAvatar'), 'owner chat must not render an unauthorized avatar');
assert(thread.includes('crypto.randomUUID()'), 'client UUID minting missing');
assert(thread.includes('sessionStorage.setItem(CONVERSATION_STORAGE_KEY'), 'client UUID retention missing');
assert(thread.includes('echoedConversationId !== conversationId'), 'echoed continuity verification missing');
assert(thread.includes('echoedRequestId !== requestId'), 'client request identity verification missing');
assert(thread.includes('setRetryTurn({ text, requestId })'), 'failed turn identity retention missing');
assert(thread.includes('send(retryTurn)'), 'same-turn retry path missing');
assert(thread.includes('PENDING_TURN_STORAGE_KEY'), 'pending turn storage missing');
assert(thread.includes('writePendingTurn(conversationId, { text, requestId })'), 'pre-dispatch turn persistence missing');
assert(thread.includes('readPendingTurn(resolved)'), 'reload recovery missing');
assert(thread.includes('response.status === 409'), '409 retry retention missing');
assert(thread.includes('message.id !== localMessageId'), 'failed optimistic bubble cleanup missing');
assert(thread.includes('data-capability-retry-dedupe="active"'), 'active retry-dedupe label missing');
assert(!thread.includes('backend_turn_contract_has_no_idempotency_field'), 'stale duplicate-commit blocker remains');
assert(thread.includes('/api/admin/apocrypha/chat'), 'one-final REST route missing');
assert(!thread.includes('chat_stream'), 'UI still calls retired synthetic stream');
assert(thread.includes('bootstrap_shallow'), 'shallow expression label missing');
assert(thread.includes('Learned field ·'), 'learned-field capability label missing');
assert(thread.includes('Audio ·'), 'audio capability label missing');
assert(history.includes('native_v2_history_projection_absent'), 'legacy history is not explicitly hidden');
assert(retiredStream.includes('Synthetic streaming is retired'), 'former stream is not an explicit tombstone');
assert(!retiredStream.includes('text/event-stream'), 'former stream still simulates SSE');
assert(rest.includes("upstreamPath: '/v2/turn'"), 'REST route does not call canonical V2 turn');
assert(rest.includes("privacy_class: 'restricted'"), 'authenticated content is not restricted');
assert(rest.includes('scopeConversationId'), 'server-side principal scoping missing');
assert(rest.includes('request_id: scopedRequestId'), 'scoped request identity is not forwarded');
assert(rest.includes('idempotency_key: scopedRequestId'), 'scoped replay identity is not forwarded');
assert(rest.includes('upstreamRequestId === scopedRequestId'), 'upstream request identity is not verified');
assert(rest.includes('expectedConversationRef'), 'backend continuity digest validation missing');
assert(rest.includes("outcome === 'committed'"), 'committed-outcome gate missing');
assert(rest.includes('payload.external_inference === false'), 'proprietary-inference gate missing');
assert(rest.includes("expressionMode === EXPECTED_EXPRESSION_MODE"), 'expression-mode gate missing');
assert(rest.includes('hasSameOrigin(req)'), 'state-changing turn lacks same-origin enforcement');
assert(presence.includes('isCanonicalHiddenPresence'), 'public presence is not schema-gated');
assert(presence.includes("display_authorized: false"), 'presence does not fail hidden');
assert(thread.includes('CHAT_BROWSER_DEADLINE_MS = 28_000'), 'browser request bound missing');
assert(rest.includes('UPSTREAM_DEADLINE_MS = 25_000'), 'upstream request bound missing');
assert(rest.includes('MAX_TEXT_BYTES = 16_384'), 'body UTF-8 percept bound mismatch remains');
assert(
  vercel.functions?.['pages/api/admin/apocrypha/chat.ts']?.maxDuration === 30,
  'REST proxy must request a 30-second configured bound',
);
assert(
  vercel.functions?.['pages/api/admin/apocrypha/chat_stream.ts']?.maxDuration === 30,
  'retired stream must not retain a fictitious long-running budget',
);
assert(ownerPage.includes("height: 'calc(100dvh - 120px)'"), 'owner chat must track the dynamic mobile viewport');
assert(thread.includes('@media (max-width:640px)'), 'mobile chat breakpoint missing');
assert(thread.includes('env(safe-area-inset-bottom)'), 'mobile safe-area padding missing');

console.log('chat-presence.test : OK · public alias, owner V2 REST, scoped continuity, retry safety, and capability truth passed');

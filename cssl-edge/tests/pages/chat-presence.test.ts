import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const page = readFileSync(resolve(process.cwd(), 'pages/chat.tsx'), 'utf8');
const thread = readFileSync(resolve(process.cwd(), 'components/apocrypha/ChatThread.tsx'), 'utf8');
const stream = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocrypha/chat_stream.ts'), 'utf8');
const vercel = JSON.parse(readFileSync(resolve(process.cwd(), 'vercel.json'), 'utf8')) as {
  functions?: Record<string, { maxDuration?: number }>;
};

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

for (const token of ['ChatThread', "'owner'", '/api/admin/check', 'ApocryphaAvatar']) {
  assert(page.includes(token), `owner-direct chat contract missing: ${token}`);
}
for (const forbidden of ['/api/chat/send', "from('chat_turn')", "from('chat_chunk')", 'cannot verify the machine’s sleep state', 'authenticated cognition telemetry']) {
  assert(!page.includes(forbidden), `dead queue/disclaimer path remains: ${forbidden}`);
}
assert(!existsSync(resolve(process.cwd(), 'pages/api/chat/send.ts')), 'dead queue endpoint still exists');
assert(!existsSync(resolve(process.cwd(), 'lib/chat-relay.ts')), 'dead queue relay still exists');
assert(thread.includes('Apocrypha is thinking…'), 'human thinking status is missing');
assert(!thread.includes('auto-invokes tools'), 'bootstrap UI must not claim unpromoted tool execution');
assert(thread.includes('instruments remain governed by Apocrypha'), 'governed-instrument copy is missing');
assert(thread.includes('CHAT_BROWSER_DEADLINE_MS'), 'browser deadline is missing');
assert(thread.includes('withDeadline'), 'outer browser deadline is not wired');
assert(thread.includes('signal: controller.signal'), 'browser abort signal is not wired');
for (const token of [
  "COMPACT_CHAT_QUERY = '(max-width: 767px)'",
  'window.matchMedia(COMPACT_CHAT_QUERY)',
  'aria-modal={compactViewport || undefined}',
  'min-height: 44px',
  'max-width: calc(100vw - 44px)',
  'env(safe-area-inset-bottom)',
]) {
  assert(thread.includes(token), `mobile chat contract missing: ${token}`);
}
assert(page.includes('height: 100dvh'), 'chat must track the dynamic mobile viewport');
assert(page.includes('@media (max-width: 640px)'), 'mobile access surface breakpoint is missing');
assert(stream.includes('UPSTREAM_DEADLINE_MS'), 'upstream deadline is missing');
assert(stream.includes('signal: controller.signal'), 'upstream abort signal is not wired');
assert(stream.includes('upstreamFailureDetail'), 'HTML gateway bodies must be sanitized');
assert(stream.includes('Try again shortly.'), 'gateway recovery copy is missing');
assert(stream.includes("v2Turn ? '/v2/turn' : '/api/v1/chat/stream'"), 'native V2 route is not the production default');
assert(stream.includes("privacy_class: 'restricted'"), 'authenticated content must use the canonical restricted privacy class');
assert(stream.includes("process.env.APOCRYPHA_V2_TURN_ENABLED !== '0'"), 'legacy cognition route is not explicit opt-in');
assert(
  vercel.functions?.['pages/api/admin/apocrypha/chat_stream.ts']?.maxDuration === 120,
  'stream proxy duration must exceed its 105-second deadline',
);

console.log('chat-presence.test : OK · owner-direct, mobile, copy, dead-path, and deadline contracts passed');

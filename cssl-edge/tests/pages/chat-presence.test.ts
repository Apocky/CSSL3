import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const alias = readFileSync(resolve(process.cwd(), 'pages/chat.tsx'), 'utf8');
const ownerAdminPage = readFileSync(resolve(process.cwd(), 'pages/admin/chat.tsx'), 'utf8');
const canonicalPage = readFileSync(resolve(process.cwd(), 'pages/apocrypha.tsx'), 'utf8');
const thread = readFileSync(resolve(process.cwd(), 'components/apocrypha/ChatThread.tsx'), 'utf8');
const jobs = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocrypha/jobs/index.ts'), 'utf8');
const status = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocrypha/status.ts'), 'utf8');
const presence = readFileSync(resolve(process.cwd(), 'pages/api/apocrypha/presence.ts'), 'utf8');

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

for (const token of ['GetServerSideProps', '`/apocrypha${suffix}`', 'permanent: true']) {
  assert(alias.includes(token), `chat alias contract missing: ${token}`);
}
assert(!alias.includes('ChatThread'), 'chat alias must not expose a second owner surface');
assert(canonicalPage.includes('<ChatThread />'), 'canonical page must render durable owner chat');
assert(canonicalPage.includes('<AccountChat'), 'canonical page must retain account chat');
for (const token of ['<ChatThread />', 'adminAuthorized', '<AdminLayout']) {
  assert(ownerAdminPage.includes(token), `owner admin chat contract missing: ${token}`);
}
for (const item of [alias, ownerAdminPage, canonicalPage, thread, jobs, status, presence]) {
  assert(!item.includes('/api/v1'), 'predecessor route remains in production chat closure');
  assert(!item.includes('APOCRYPHA_V2_TURN_ENABLED'), 'retired fallback toggle remains');
}
for (const token of [
  'ACTIVE_JOB_KEY',
  'crypto.randomUUID()',
  "authFetch('/api/admin/apocrypha/jobs'",
  'idempotency_key: idempotencyKey',
  'JOB_POLL_MS',
  "snapshot.job.status === 'succeeded'",
  'window.localStorage.removeItem(ACTIVE_JOB_KEY)',
  'cancelActiveJob',
  '@media (max-width: 767px)',
  'env(safe-area-inset-bottom)',
]) assert(thread.includes(token), `durable owner chat contract missing: ${token}`);
assert(!thread.includes('chat_stream'), 'canonical owner chat still calls retired synthetic streaming');
assert(!thread.includes('handleSendLegacy'), 'canonical owner chat still contains a second send rail');
assert(jobs.includes('requireOwnerIdentity'), 'durable job route lacks owner identity');
assert(jobs.includes('enqueueApocryphaJob'), 'durable job route lacks control-plane admission');
assert(status.includes("rail: 'durable-outbound-qwen'"), 'canonical rail identity is missing');
assert(presence.includes('fetchRuntimeHealth'), 'public presence lacks the bounded runtime health projection');
assert(presence.includes("display_authorized: false"), 'presence must fail hidden');
assert(!presence.includes('CF-Access-Client'), 'presence still sends Cloudflare Access credentials');
assert(!presence.includes('APOCRYPHA_TUNNEL_HOST'), 'presence still depends on the retired tunnel host');
assert(canonicalPage.includes("height: '100dvh'"), 'canonical owner chat must track the dynamic mobile viewport');

console.log('chat-presence.test : OK · one canonical page and durable Qwen rail passed');

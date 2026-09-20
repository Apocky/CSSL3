import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const alias = readFileSync(resolve(process.cwd(), 'pages/chat.tsx'), 'utf8');
const ownerAdminPage = readFileSync(resolve(process.cwd(), 'pages/admin/chat.tsx'), 'utf8');
const canonicalPage = readFileSync(resolve(process.cwd(), 'pages/apocrypha.tsx'), 'utf8');
const thread = readFileSync(resolve(process.cwd(), 'components/apocrypha/ApocryphaChat.tsx'), 'utf8');
const lanes = readFileSync(resolve(process.cwd(), 'lib/apocrypha/chat-lanes.ts'), 'utf8');
const roomStyle = readFileSync(resolve(process.cwd(), 'styles/ApocryphaChat.module.css'), 'utf8');
const jobs = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocrypha/jobs/index.ts'), 'utf8');
const status = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocrypha/status.ts'), 'utf8');
const presence = readFileSync(resolve(process.cwd(), 'pages/api/apocrypha/presence.ts'), 'utf8');

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

for (const token of ['GetServerSideProps', '`/apocrypha${suffix}`', 'permanent: true']) {
  assert(alias.includes(token), `chat alias contract missing: ${token}`);
}
assert(!alias.includes('ApocryphaChat'), 'chat alias must not expose a second chat surface');
assert(canonicalPage.includes('<ApocryphaChat'), 'canonical page must render the one chat component');
// 2026-09-20, Apocky: "The entire flow is too complicated for now just exclude sign-in."
// The public room is the guest lane for every reader. The owner rail was not deleted --
// it lives on /admin/apocrypha and tests/pages/admin-chat.test.ts still holds it there.
assert(canonicalPage.includes('guestLane()') && !canonicalPage.includes('ownerLane'),
  'the canonical room is the guest lane; the owner rail is on /admin/apocrypha');
assert(!canonicalPage.includes('memberLane'), 'no account-scoped branch; one room for everyone');
assert(canonicalPage.includes('guestLane()'), 'the signed-out room stays open');
for (const token of ['<ApocryphaChat', 'adminAuthorized', '<AdminLayout']) {
  assert(ownerAdminPage.includes(token), `owner admin chat contract missing: ${token}`);
}
for (const item of [alias, ownerAdminPage, canonicalPage, thread, lanes, jobs, status, presence]) {
  assert(!item.includes('/api/v1'), 'predecessor route remains in production chat closure');
  assert(!item.includes('APOCRYPHA_V2_TURN_ENABLED'), 'retired fallback toggle remains');
}
for (const token of [
  'activeJobKey(',
  'JOB_POLL_FAST_MS',
  'JOB_POLL_SLOW_MS',
  'dropStored(activeJobKey(laneId))',
  'lane.cancel(activeJob.id)',
]) assert(thread.includes(token), `durable chat contract missing: ${token}`);
for (const token of [
  'crypto.randomUUID()',
  "authFetch('/api/admin/apocrypha/jobs'",
  'idempotency_key: newId()',
  "TERMINAL.has(snapshot.job.status)",
]) assert(lanes.includes(token), `owner transport contract missing: ${token}`);
for (const token of ['@media (max-width: 767px)', 'env(safe-area-inset-bottom']) {
  assert(roomStyle.includes(token), `compact layout contract missing: ${token}`);
}
assert(!thread.includes('chat_stream') && !lanes.includes('chat_stream'), 'retired synthetic streaming is still called');
assert(!thread.includes('handleSendLegacy'), 'the room still contains a second send rail');
assert(jobs.includes('requireOwnerIdentity'), 'durable job route lacks owner identity');
assert(jobs.includes('enqueueApocryphaJob'), 'durable job route lacks control-plane admission');
assert(status.includes("rail: 'durable-outbound-qwen'"), 'canonical rail identity is missing');
assert(presence.includes('fetchRuntimeHealth'), 'public presence lacks the bounded runtime health projection');
assert(presence.includes("display_authorized: false"), 'presence must fail hidden');
assert(!presence.includes('CF-Access-Client'), 'presence still sends Cloudflare Access credentials');
assert(!presence.includes('APOCRYPHA_TUNNEL_HOST'), 'presence still depends on the retired tunnel host');
assert(roomStyle.includes('height: 100dvh'), 'the room must track the dynamic mobile viewport');

console.log('chat-presence.test : OK · one canonical page and durable Qwen rail passed');

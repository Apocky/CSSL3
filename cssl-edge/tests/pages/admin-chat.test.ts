import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

function source(path: string): string {
  return readFileSync(resolve(process.cwd(), path), 'utf8');
}

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

const adminPage = source('pages/admin/chat.tsx');
const ownerPage = source('pages/apocrypha.tsx');
// One chat interface now, so the durable-job contract below is asserted where it actually lives:
// the shared room component drives the loop, the lane module owns the owner transport.
const room = source('components/apocrypha/ApocryphaChat.tsx');
const lanes = source('lib/apocrypha/chat-lanes.ts');
const roomStyle = source('styles/ApocryphaChat.module.css');
const jobs = source('pages/api/admin/apocrypha/jobs/index.ts');
const status = source('pages/api/admin/apocrypha/status.ts');
const controls = source('pages/admin/controls.tsx');

assert(adminPage.includes('<ApocryphaChat'), 'admin chat uses the one chat component');
assert(adminPage.includes('ownerLane(authFetch)'), 'the admin route gets the owner transport');
assert(adminPage.includes('adminAuthorized'), 'admin chat remains owner-gated');
assert(ownerPage.includes('<ApocryphaChat'), 'the Apocrypha page uses the same one chat component');
assert(!ownerPage.includes('<ChatThread') && !ownerPage.includes('<AccountChat') && !ownerPage.includes('<GuestChat'),
  'signing in must not swap the reader into a different chat surface');
assert(ownerPage.includes('ownerLane(authFetch)') && ownerPage.includes('memberLane(authFetch)') && ownerPage.includes('guestLane()'),
  'every entitlement is still reachable, as a lane rather than a component');
assert(roomStyle.includes('height: 100dvh'), 'the room tracks the dynamic viewport');

for (const token of [
  'activeJobKey(',
  'readStored<ActiveJob>(activeJobKey(laneId))',
  'writeStored(activeJobKey(laneId), record)',
  'dropStored(activeJobKey(laneId))',
  'Connection interrupted. The job is safe; reconnecting',
  'lane.cancel(activeJob.id)',
]) assert(room.includes(token), `durable chat contract missing: ${token}`);

for (const token of [
  "authFetch('/api/admin/apocrypha/jobs'",
  'idempotency_key: newId()',
  'response_mode: input.text.length > 1200',
  "TERMINAL.has(snapshot.job.status)",
]) assert(lanes.includes(token), `owner transport contract missing: ${token}`);

assert(!room.includes('/api/admin/apocrypha/chat_stream'), 'the room must not call the retired stream rail');
assert(!room.includes('handleSendLegacy'), 'the room must have one send path');
assert((room.match(/lane\.send\(/gu) ?? []).length === 1, 'exactly one send path reaches the transport');
assert(jobs.includes('requireOwnerIdentity'), 'job submission requires owner identity');
assert(jobs.includes('enqueueApocryphaJob'), 'job submission enters the durable control plane');
assert(jobs.includes("capability: 'apocky_owner_chat'"), 'job capability is fixed server-side');
assert(status.includes("rail: 'durable-outbound-qwen'"), 'status reports the canonical Qwen rail');
assert(status.includes("requiredCapability: 'apocky_owner_chat'"), 'status does not require the owner chat capability');
assert(status.includes('projectApocryphaReadiness'), 'status bypasses the canonical operational readiness projection');
assert(status.includes('APOCRYPHA_MODEL_ALIAS'), 'status does not use the canonical model configuration');
assert(!controls.includes('/api/admin/apocrypha/chat'), 'controls never reinterpret chat as a command channel');

console.log('admin-chat.test : OK · one durable owner rail, recovery, and Qwen status passed');

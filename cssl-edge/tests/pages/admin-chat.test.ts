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
const thread = source('components/apocrypha/ChatThread.tsx');
const jobs = source('pages/api/admin/apocrypha/jobs/index.ts');
const status = source('pages/api/admin/apocrypha/status.ts');
const controls = source('pages/admin/controls.tsx');

assert(adminPage.includes('<ChatThread />'), 'admin chat uses the canonical durable chat component');
assert(adminPage.includes('adminAuthorized'), 'admin chat remains owner-gated');
assert(ownerPage.includes('<ChatThread />'), 'the owner-facing Apocrypha page uses the same durable chat component');
assert(ownerPage.includes('<AccountChat'), 'member conversations remain account-scoped');
assert(ownerPage.includes("height: '100dvh'"), 'owner chat tracks the dynamic viewport');

for (const token of [
  'ACTIVE_JOB_KEY',
  'window.localStorage.getItem(ACTIVE_JOB_KEY)',
  'window.localStorage.setItem(ACTIVE_JOB_KEY',
  'window.localStorage.removeItem(ACTIVE_JOB_KEY)',
  "authFetch('/api/admin/apocrypha/jobs'",
  'crypto.randomUUID()',
  'idempotency_key: idempotencyKey',
  'response_mode: text.length > 1200',
  "snapshot.job.status === 'succeeded'",
  'Connection interrupted. The job is safe; reconnecting',
  'cancelActiveJob',
]) assert(thread.includes(token), `durable chat contract missing: ${token}`);

assert(!thread.includes('/api/admin/apocrypha/chat_stream'), 'owner chat must not call the retired stream rail');
assert(!thread.includes('handleSendLegacy'), 'owner chat must have one send path');
assert(jobs.includes('requireOwnerIdentity'), 'job submission requires owner identity');
assert(jobs.includes('enqueueApocryphaJob'), 'job submission enters the durable control plane');
assert(jobs.includes("capability: 'apocky_owner_chat'"), 'job capability is fixed server-side');
assert(status.includes("rail: 'durable-outbound-qwen'"), 'status reports the canonical Qwen rail');
assert(status.includes("requiredCapability: 'apocky_owner_chat'"), 'status does not require the owner chat capability');
assert(status.includes('projectApocryphaReadiness'), 'status bypasses the canonical operational readiness projection');
assert(status.includes('APOCRYPHA_MODEL_ALIAS'), 'status does not use the canonical model configuration');
assert(!controls.includes('/api/admin/apocrypha/chat'), 'controls never reinterpret chat as a command channel');

console.log('admin-chat.test : OK · one durable owner rail, recovery, and Qwen status passed');

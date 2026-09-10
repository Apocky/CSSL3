import { createHash, createHmac } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { assertChaosBridgeSignature, requestHash, secretMatches } from '../lib/apocrypha/job-control';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

assert(
  requestHash({ cards: [{ name: 'Tower', reversed: false }], question: 'What changes?' })
    === requestHash({ question: 'What changes?', cards: [{ reversed: false, name: 'Tower' }] }),
  'request hashing must be stable across object key order',
);
assert(secretMatches('same-value', 'same-value'), 'constant-time secret comparison rejects a match');
assert(!secretMatches('same-value', 'different-value'), 'constant-time secret comparison accepts a mismatch');

const priorSecret = process.env.CHAOS_TAROT_BRIDGE_TOKEN;
process.env.CHAOS_TAROT_BRIDGE_TOKEN = 'bridge-test-secret';
const body = { kind: 'chaos_oracle', request: { question: 'What changes?' }, idempotency_key: 'reading-1' };
const timestamp = String(Date.now());
const principal = `ct_${'a'.repeat(43)}`;
const path = '/api/apocrypha/jobs?source=test';
const bodyHash = createHash('sha256').update(JSON.stringify(body)).digest('hex');
const stringToSign = `${timestamp}\nPOST\n${path}\n${principal}\n${bodyHash}`;
const signature = `v1=${createHmac('sha256', process.env.CHAOS_TAROT_BRIDGE_TOKEN).update(stringToSign).digest('hex')}`;
assertChaosBridgeSignature({
  authorization: 'Bearer bridge-test-secret',
  method: 'POST',
  url: path,
  body,
  headers: {
    'x-apocrypha-timestamp': timestamp,
    'x-apocrypha-principal': principal,
    'x-apocrypha-origin': 'chaos-tarot',
    'x-apocrypha-tenant': 'chaos-tarot',
    'x-apocrypha-content-sha256': bodyHash,
    'x-apocrypha-signature': signature,
  },
});
let rejectedTamper = false;
try {
  assertChaosBridgeSignature({
    authorization: 'Bearer bridge-test-secret',
    method: 'POST',
    url: path,
    body: { ...body, idempotency_key: 'tampered' },
    headers: {
      'x-apocrypha-timestamp': timestamp,
      'x-apocrypha-principal': principal,
      'x-apocrypha-origin': 'chaos-tarot',
      'x-apocrypha-tenant': 'chaos-tarot',
      'x-apocrypha-content-sha256': bodyHash,
      'x-apocrypha-signature': signature,
    },
  });
} catch {
  rejectedTamper = true;
}
assert(rejectedTamper, 'signed body tampering must be rejected');
if (priorSecret === undefined) delete process.env.CHAOS_TAROT_BRIDGE_TOKEN;
else process.env.CHAOS_TAROT_BRIDGE_TOKEN = priorSecret;

const thread = readFileSync(resolve(process.cwd(), 'components/apocrypha/ChatThread.tsx'), 'utf8');
for (const contract of [
  "ACTIVE_JOB_KEY = 'apocky.apocrypha.active-job.v1'",
  "authFetch('/api/admin/apocrypha/jobs'",
  'window.localStorage.setItem(ACTIVE_JOB_KEY',
  'Connection interrupted. The job is safe; reconnecting',
  'setStreamingText(visibleText)',
  'cancelActiveJob',
]) {
  assert(thread.includes(contract), `durable browser contract missing: ${contract}`);
}

console.log('apocrypha-jobs.test: OK');

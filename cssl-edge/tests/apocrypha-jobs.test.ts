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

// One chat interface: the room owns recovery and rendering, the lane module owns the transport.
const room = readFileSync(resolve(process.cwd(), 'components/apocrypha/ApocryphaChat.tsx'), 'utf8');
const lanes = readFileSync(resolve(process.cwd(), 'lib/apocrypha/chat-lanes.ts'), 'utf8');
for (const contract of [
  "function activeJobKey(lane: string): string { return `apx.chat.active-job.${lane}.v1`; }",
  'writeStored(activeJobKey(laneId), pending)',
  'Connection interrupted. The job is safe; reconnecting',
  'setStreamingText(snapshot.text)',
  'lane.cancel(activeJob.id)',
]) {
  assert(room.includes(contract), `durable browser contract missing: ${contract}`);
}
for (const contract of ["authFetch('/api/admin/apocrypha/jobs'", 'chunk.delta']) {
  assert(lanes.includes(contract), `durable owner transport contract missing: ${contract}`);
}

// RETRY OF A FAILED ATTEMPT — server side only, for now.
//
// The retired ChatThread carried a "Retry failed attempt" control that re-submitted a failed job
// with retry_job_id, so the retry was linked to the attempt it replaced instead of becoming an
// unrelated turn. Unifying the three chat surfaces into one room dropped that CONTROL; it did not
// drop the capability. The route still accepts and threads it, and these assertions keep it alive
// so the seam stays usable when the control is rebuilt.
const jobsRoute = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocrypha/jobs/index.ts'), 'utf8');
for (const contract of ['retry_job_id', 'retry_of_job_id']) {
  assert(jobsRoute.includes(contract), `owner retry seam missing from the route: ${contract}`);
}
assert(
  !room.includes('Retry failed attempt'),
  'the room has no retry control yet — if one is added, restore its assertions above rather than deleting this line',
);

console.log('apocrypha-jobs.test: OK');

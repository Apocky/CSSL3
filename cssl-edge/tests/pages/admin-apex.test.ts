import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

const source = readFileSync(resolve(process.cwd(), 'pages/admin/apex.tsx'), 'utf8');

for (const token of [
  '<AdminLayout',
  "authFetch('/api/admin/apocv4/health'",
  "authFetch('/api/admin/apocv4/objective'",
  'JSON.stringify({ objective: prompt })',
]) {
  assert(source.includes(token), `authenticated Apex proxy contract missing: ${token}`);
}

for (const token of [
  'council_decision',
  'council.candidate',
  'candidate.proposal',
  'proposal.summary',
  'proposal.steps',
]) {
  assert(source.includes(token), `selected council proposal is not rendered as the answer: ${token}`);
}

for (const token of [
  'Conversation history',
  'Message Apocrypha',
  'history stays in this browser',
  'localStorage.setItem(SESSION_KEY',
  'localStorage.removeItem(SESSION_KEY)',
  'Export current thread',
  'Clear local history',
  'abortRef.current?.abort()',
  '>Edit<',
  '>Branch<',
  '>Copy<',
  '>Retry<',
]) {
  assert(source.includes(token), `communication-hub affordance missing or unwired: ${token}`);
}

assert(source.includes('Proposal text is model-reported.'), 'model-reported answer is not visibly typed');
assert(source.includes('Observed receipt'), 'observed evidence is not visibly typed');
assert(source.includes('A stopped or incomplete turn is never presented as accepted.'), 'incomplete-turn truth boundary missing');
assert(source.includes('browser attachment pending'), 'unavailable vision attachment is not labeled honestly');
assert(source.includes("event.key === 'Enter' && !event.shiftKey"), 'keyboard send contract missing');
assert(source.includes("state: evidence.status === 'ACCEPTED' ? 'accepted' : 'failed'"), 'accepted state is not evidence-gated');
assert(!source.includes('privacy_partition'), 'privacy partition must not enter client source or screenshots');
assert(!source.includes('APOCV4_API_TOKEN'), 'runtime token is present in client source');
assert(!source.includes('APOCV4_RUNTIME_URL'), 'runtime origin is present in client source');

console.log('admin-apex.test : OK · private communication hub, selected proposal, action wiring, and evidence boundary passed');

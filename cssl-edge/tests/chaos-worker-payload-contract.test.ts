import { composeQwenRequest } from '../scripts/apocrypha-worker/prompt';
import { queryFromJob } from '../scripts/apocrypha-worker/retrieval';
import type { ClaimedJob, RetrievalBundle, WorkerConfig } from '../scripts/apocrypha-worker/types';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

const job: ClaimedJob = {
  jobId: '10000000-0000-4000-8000-000000000001',
  attemptId: '20000000-0000-4000-8000-000000000001',
  attemptNo: 1,
  leaseEpoch: 1,
  leaseToken: 'lease-token',
  leaseExpiresAt: '2026-09-07T21:00:00.000Z',
  tenantId: '30000000-0000-4000-8000-000000000001',
  ownerPrincipalId: '40000000-0000-4000-8000-000000000001',
  kind: 'followup',
  capability: 'chaos_tarot_reading',
  request: {
    question: 'What boundary does this pattern ask for?',
    source_text: 'The saved reading paired the Tower with the Star.',
    canonical_reading: {
      system: { id: 'chaos-tarot', name: 'Chaos Tarot' },
      spread: { id: 'three', name: 'Three Signals' },
      items: [
        { name: 'The Tower', is_reversed: false, position: { name: 'Pressure' } },
        { name: 'The Star', is_reversed: true, position: { name: 'Response' } },
      ],
    },
    conversation_history: [
      { role: 'user', content: 'What am I overlooking?' },
      { role: 'assistant', content: 'The first reading emphasized delayed repair.' },
    ],
    structured_context: { task: 'saved_reading_followup', practical_reflection: true },
    options: { style: 'practical', depth: 'deep' },
  },
  modelAlias: 'qwen35-35b-a3b-q4',
  profileHash: 'a'.repeat(64),
  toolRegistryVersion: 'apocrypha-readonly-v1',
  memoryManifestHash: 'b'.repeat(64),
};

const config = {
  contextWindowTokens: 8_192,
  maxOutputTokens: 1_024,
  toolRegistryVersion: job.toolRegistryVersion,
} as WorkerConfig;
const memory: RetrievalBundle = {
  query: '',
  results: [],
  records: [],
  digest: 'c'.repeat(64),
};

const query = queryFromJob(job);
assert(query.startsWith('What boundary does this pattern ask for?'), 'the user question is not the primary retrieval query');
assert(query.includes('Pressure: The Tower'), 'the canonical reading did not drive retrieval');
assert(query.includes('Response: The Star reversed'), 'reversal context did not drive retrieval');
assert(query.includes('What am I overlooking?'), 'recent user context was not included in retrieval');
assert(query.length <= 4_000, 'retrieval query exceeded its bound');
assert(query !== 'followup chaos_tarot_reading', 'retrieval fell back to generic capability text');

const composed = composeQwenRequest(config, job, memory);
const conversation = composed.messages.slice(1);
assert(conversation[0]?.role === 'user' && conversation[0].content === 'What am I overlooking?', 'user history was not preserved');
assert(conversation[1]?.role === 'assistant' && conversation[1].content.includes('delayed repair'), 'assistant history was not preserved');
const final = conversation.at(-1);
assert(final?.role === 'user', 'structured Chaos payload did not become a user turn');
assert(final.content.includes('Question:\nWhat boundary does this pattern ask for?'), 'question was absent from the model prompt');
assert(final.content.includes('<canonical-reading>') && final.content.includes('The Tower'), 'canonical reading was absent from the model prompt');
assert(final.content.includes('<saved-source>') && final.content.includes('paired the Tower with the Star'), 'saved interpretation context was absent');
assert(final.content.includes('saved_reading_followup'), 'structured task context was absent');
assert(composed.generation.maxTokens === 1_024, 'existing generation defaults changed');

const legacyJob = { ...job, request: { prompt: 'Preserve this legacy prompt.' } };
const legacy = composeQwenRequest(config, legacyJob, memory);
assert(legacy.messages.at(-1)?.content === 'Preserve this legacy prompt.', 'legacy prompt shape changed');

console.log('chaos-worker-payload-contract.test: OK');

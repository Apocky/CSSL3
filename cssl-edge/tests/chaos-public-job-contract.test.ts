import { externalJobSnapshot } from '../lib/apocrypha/job-control';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

const now = '2026-09-07T20:00:00.000Z';
const projected = externalJobSnapshot({
  job: {
    id: '10000000-0000-4000-8000-000000000001',
    tenant_id: '20000000-0000-4000-8000-000000000001',
    owner_principal_id: '30000000-0000-4000-8000-000000000001',
    kind: 'followup',
    capability: 'chaos_tarot_reading',
    status: 'succeeded',
    request: { question: 'What changes?' },
    current_attempt_id: '40000000-0000-4000-8000-000000000001',
    model_alias: 'qwen35-35b-a3b-q4',
    profile_hash: 'a'.repeat(64),
    tool_registry_version: 'apocrypha-readonly-v1',
    memory_manifest_hash: 'b'.repeat(64),
    created_at: now,
    updated_at: now,
    completed_at: now,
    error_code: null,
    error_detail: null,
  },
  chunks: [{ seq: 0, chunk_kind: 'token', delta: 'A durable answer.', metadata: { section_index: 0 }, created_at: now }],
  revisions: [{ id: 'revision-1', revision_no: 1, revision_role: 'primary', content: 'A durable answer.', provenance: {}, usage: {}, created_at: now }],
  events: [
    { ordinal: 1, event_type: 'job.queued', outcome: 'expected_fired', severity: 'info', source: 'control_plane.enqueue', flagged: false, metadata: { phase: 'queued' }, occurred_at: now },
    { ordinal: 2, event_type: 'job.completed', outcome: 'expected_fired', severity: 'info', source: 'worker.complete', flagged: false, metadata: { phase: 'complete' }, occurred_at: now },
  ],
} as never);

assert(projected.job.kind === 'followup', 'job kind was not preserved');
assert(projected.job.status === 'complete', 'succeeded status did not map to complete');
assert(projected.job.final_text === 'A durable answer.', 'terminal revision was not delivered as final text');
assert(projected.chunks[0]?.chunk_index === 0, 'worker chunk ordinal changed');
assert(projected.chunks[0]?.sequence === 1, 'public chunk cursor must be one-based and lossless from after=0');
assert(projected.chunks[0]?.committed === true, 'terminal chunk was not marked committed');
assert(projected.events[0]?.sequence === 1, 'event ordinal was not projected');
assert(projected.events[0]?.expected === true && projected.events[0]?.fired === true, 'expected_fired semantics were lost');
assert(projected.events[1]?.phase === 'complete', 'event metadata phase was not projected');
assert(projected.job.last_sequence === 2, 'final event revision cursor was not delivered');

console.log('chaos-public-job-contract.test: OK');

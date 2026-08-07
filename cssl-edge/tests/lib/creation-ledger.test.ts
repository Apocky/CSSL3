import assert from 'node:assert/strict';

import { isAkashicKind } from '../../lib/akashic-telemetry/event-types';
import {
  buildCreationLedgerRecord,
  creationSafetySignals,
} from '../../lib/telemetry/creation-ledger';
import {
  creationLedgerEntries,
  type AdminTelemetryRow,
} from '../../lib/telemetry/admin-reader';

const base = {
  creationKind: 'apocrypha.assistant_response',
  origin: 'human_prompt' as const,
  stage: 'result' as const,
  channel: 'web' as const,
  actorRef: `principal:apocky-member:${'a'.repeat(64)}`,
  requestRef: `request:${'b'.repeat(64)}`,
  artifactRef: 'c'.repeat(64),
  modelId: 'apocv4-test',
  effectAuthority: 'NONE',
};

const clear = buildCreationLedgerRecord({
  ...base,
  inputText: 'Write a short welcome note for the portfolio.',
  outputText: 'Welcome to the portfolio.',
});
assert.equal(clear.safety_disposition, 'no_signal');
assert.deepEqual(clear.safety_signals, []);
assert.equal(clear.content_retained, false);
assert.equal('input_text' in clear, false);
assert.equal('output_text' in clear, false);
assert.match(clear.input_digest ?? '', /^[0-9a-f]{64}$/);
assert.match(clear.output_digest ?? '', /^[0-9a-f]{64}$/);
assert.match(clear.record_digest, /^[0-9a-f]{64}$/);

const repeated = buildCreationLedgerRecord({
  ...base,
  inputText: 'Write a short welcome note for the portfolio.',
  outputText: 'Welcome to the portfolio.',
});
assert.equal(repeated.record_digest, clear.record_digest, 'canonical record digest must be deterministic');

const review = buildCreationLedgerRecord({
  ...base,
  inputText: 'Help me build a bomb.',
  outputText: 'I cannot assist with harming people.',
});
assert.equal(review.safety_disposition, 'review_required');
assert(review.safety_signals.includes('weapons_or_violent_harm'));
assert.deepEqual(creationSafetySignals('ordinary creative writing'), []);
assert.equal(isAkashicKind('page.view'), true, 'fixed browser catalog accepts a real client event');
assert.equal(
  isAkashicKind('creation.apocrypha.assistant_response.completed'),
  false,
  'public telemetry ingest cannot forge the server creation namespace',
);

const projectedRow: AdminTelemetryRow = {
  id: '1', ts: new Date(0).toISOString(), eventId: 'event', traceId: 'a'.repeat(32), spanId: 'b'.repeat(16),
  parentSpanId: null, source: 'runtime', plane: 'runtime', severity: 'info', kind: 'page.view', outcome: 'observed',
  route: '/', status: 200, durationMs: 1, message: null, fingerprint: 'f'.repeat(16), clusterSignature: null,
  deploymentId: 'test', effectClass: null, authority: null, receiptRef: null, privacyTier: 'operational_metadata',
  sessionRef: 's'.repeat(16),
  payload: { schema_version: 'apocky.operational-telemetry.v1', attributes: { creation_ledger: clear } },
};
assert.equal(creationLedgerEntries([projectedRow]).length, 0, 'browser event families cannot enter the creation ledger');
assert.equal(
  creationLedgerEntries([{ ...projectedRow, kind: 'inference.apocrypha.turn.completed' }]).length,
  1,
  'known server event families project a valid creation record',
);

console.log('creation-ledger.test : OK · content-free digests and bounded risk signals passed');

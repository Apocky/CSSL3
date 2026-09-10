import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import fixture from '../fixtures/bridge-history-gzip.json';
import { responseFromBridgeResult } from '../../lib/bridge/queue';
import { type BridgeInput, type BridgeHttpResult } from '../../lib/bridge/crypto';

const input: BridgeInput = { channel: 'owner', subject: '11111111-1111-4111-8111-111111111111', method: 'GET',
  target: '/v1/chat/history?privacy_partition=owner%3Aapocky&limit=32', body: Buffer.alloc(0) };
const bytes = Buffer.concat([Buffer.from(fixture.raw_prefix_base64, 'base64'), Buffer.alloc(fixture.repeat_count, fixture.repeat_byte), Buffer.from(fixture.raw_suffix_base64, 'base64')]);
const result: BridgeHttpResult = { schema_version: 'apocky.bridge.http-result.v1', job_id: '11111111-1111-5111-8111-111111111111', status: 200,
  headers: { 'content-type': 'application/json', 'content-encoding': 'gzip', 'x-apocv4-history-codec': 'v2' },
  body_base64: fixture.body_base64, completed_at: '2026-09-05T00:00:00.000Z' };
async function run() {
  assert(bytes.byteLength > 6 * 1024 * 1024 - 8 && bytes.byteLength <= 6 * 1024 * 1024);
  assert.equal(bytes.byteLength, fixture.raw_bytes);
  assert.equal(createHash('sha256').update(bytes).digest('hex'), fixture.raw_sha256);
  assert.equal(createHash('sha256').update(Buffer.from(result.body_base64, 'base64')).digest('hex'), fixture.gzip_sha256);
  const decoded = responseFromBridgeResult(input, result);
  assert.equal(decoded.headers.has('content-encoding'), false);
  assert.equal(decoded.headers.get('x-apocv4-history-codec'), 'v2');
  const actual = Buffer.from(await decoded.arrayBuffer());
  assert.equal(actual.byteLength, bytes.byteLength);
  assert.equal(createHash('sha256').update(actual).digest('hex'), createHash('sha256').update(bytes).digest('hex'));
  assert.throws(() => responseFromBridgeResult(input, { ...result, body_base64: gzipSync(Buffer.alloc(6 * 1024 * 1024 + 1)).toString('base64') }));
  assert.throws(() => responseFromBridgeResult(input, { ...result, body_base64: Buffer.from('broken gzip').toString('base64') }));
  assert.throws(() => responseFromBridgeResult({ ...input, channel: 'account' }, result));
  assert.throws(() => responseFromBridgeResult({ ...input, target: '/v1/auth/status' }, result));
  assert.throws(() => responseFromBridgeResult(input, { ...result, headers: { 'content-encoding': 'br' } }));
  process.stdout.write('Bridge history near6MiB byte parity, bounded decompression, wrong route and invalid encoding checks passed.\n');
}
run().catch(error => { console.error(error); process.exitCode = 1; });

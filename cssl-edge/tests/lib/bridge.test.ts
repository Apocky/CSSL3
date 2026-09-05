import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { Readable } from 'node:stream';
import type { NextApiRequest, NextApiResponse } from 'next';
import { BridgeQueue, type BridgePersistence, type BridgeJobRow, type BridgeSessionRow } from '../../lib/bridge/queue';
import { BridgeError, bridgeConfiguration, bridgeSessionId, createBridgeRequest, decryptBridge, encryptBridge, validBridgeTarget, validateBridgeRequest, validateBridgeResult, type BridgeInput } from '../../lib/bridge/crypto';
import { verifyWorkerAuthentication, workerAuthText, workerHeaders, WORKER_POLL_PATH, WORKER_COMPLETE_PATH } from '../../lib/bridge/worker-auth';
import { createWorkerPollHandler } from '../../pages/api/bridge/worker/poll';
import { createWorkerCompleteHandler } from '../../pages/api/bridge/worker/complete';

class MemoryPersistence implements BridgePersistence {
  sessions = new Map<string, BridgeSessionRow>(); jobs = new Map<string, BridgeJobRow>();
  async getSession(id: string) { return structuredClone(this.sessions.get(id) ?? null); }
  async insertSession(row: BridgeSessionRow) { if (this.sessions.has(row.id)) return false; this.sessions.set(row.id, structuredClone(row)); return true; }
  async casSession(previous: BridgeSessionRow, next: BridgeSessionRow, revision: number) {
    const actual = this.sessions.get(previous.id);
    if (!actual || actual.user_id !== previous.user_id || (actual.metadata as { revision: number }).revision !== revision) return false;
    this.sessions.set(previous.id, structuredClone(next)); return true;
  }
  async getJob(id: string) { return structuredClone(this.jobs.get(id) ?? null); }
  async insertJob(row: BridgeJobRow) { if (this.jobs.has(row.id)) return false; this.jobs.set(row.id, structuredClone(row)); return true; }
  async casJob(previous: BridgeJobRow, next: BridgeJobRow, revision: number) {
    const actual = this.jobs.get(previous.id);
    if (!actual || actual.user_id !== previous.user_id || actual.session_id !== previous.session_id || actual.status !== previous.status || (actual.metadata as { revision: number }).revision !== revision) return false;
    this.jobs.set(previous.id, structuredClone(next)); return true;
  }
}
const fixture = JSON.parse(readFileSync(new URL('../fixtures/bridge-wire.json', import.meta.url), 'utf8'));
const config = () => ({ key: Buffer.from(fixture.key_b64, 'base64'), keyId: fixture.key_id as string, workerId: fixture.worker_id as string, ownerSubject: fixture.owner_subject as string });
const initial = Date.parse('2026-09-04T00:00:00.000Z');
const input: BridgeInput = { channel: 'account', subject: fixture.owner_subject, method: 'POST', target: '/v1/account/turn', body: Buffer.from(fixture.request.body_base64, 'base64') };
const response = (job: string, text = 'fixture reply') => ({ schema_version: 'apocky.bridge.http-result.v1', job_id: job, status: 200, headers: { 'content-type': 'application/json' }, body_base64: Buffer.from(JSON.stringify({ text })).toString('base64'), completed_at: '2026-09-04T00:00:01.000Z' });
const coded = (code: string) => (error: unknown) => error instanceof BridgeError && error.code === code;

async function run() {
  const cfg = config();
  assert.equal(bridgeSessionId(cfg, input.subject), fixture.session_id);
  assert.deepEqual(createBridgeRequest(cfg, input, initial), fixture.request);
  assert.deepEqual(decryptBridge(cfg, 'request', fixture.request.job_id, fixture.request_envelope), fixture.request);
  assert.deepEqual(encryptBridge(cfg, 'request', fixture.request.job_id, fixture.request, Buffer.from(Array.from({ length: 12 }, (_, i) => i))), fixture.request_envelope);
  assert.deepEqual(decryptBridge(cfg, 'response', fixture.request.job_id, fixture.response_envelope), fixture.response);
  const pollBytes = Buffer.from(fixture.auth.body_base64, 'base64');
  assert.equal(workerAuthText(cfg, 'POST', WORKER_POLL_PATH, pollBytes, String(initial / 1000), fixture.auth.headers['x-apocky-worker-nonce']), fixture.auth.canonical);
  assert.deepEqual(workerHeaders(cfg, WORKER_POLL_PATH, pollBytes, initial, fixture.auth.headers['x-apocky-worker-nonce']), fixture.auth.headers);
  assert.doesNotThrow(() => verifyWorkerAuthentication(cfg, fixture.auth.headers, 'POST', WORKER_POLL_PATH, pollBytes, initial));
  for (const [headers, method, path, bytes, now] of [
    [{ ...fixture.auth.headers, 'x-apocky-worker-id': 'foreign' }, 'POST', WORKER_POLL_PATH, pollBytes, initial],
    [fixture.auth.headers, 'GET', WORKER_POLL_PATH, pollBytes, initial],
    [fixture.auth.headers, 'POST', WORKER_COMPLETE_PATH, pollBytes, initial],
    [fixture.auth.headers, 'POST', WORKER_POLL_PATH, Buffer.from('changed'), initial],
    [fixture.auth.headers, 'POST', WORKER_POLL_PATH, pollBytes, initial + 61_000],
  ] as const) assert.throws(() => verifyWorkerAuthentication(cfg, headers, method, path, bytes, now), coded('BRIDGE_WORKER_UNAUTHORIZED'));
  assert.throws(() => decryptBridge(cfg, 'response', fixture.request.job_id, fixture.request_envelope));
  assert.throws(() => decryptBridge(cfg, 'request', fixture.request.job_id, { ...fixture.request_envelope, tag_b64: Buffer.alloc(16).toString('base64') }));
  assert.throws(() => validateBridgeRequest(cfg, { ...fixture.request, subject: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa' }));
  assert.throws(() => createBridgeRequest(cfg, { ...input, channel: 'owner', subject: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', target: '/health', method: 'GET', body: Buffer.alloc(0) }));
  assert.throws(() => validateBridgeResult(fixture.request.job_id, { ...fixture.response, headers: { authorization: 'PRIVATE_SECRET' } }));
  assert.throws(() => bridgeConfiguration({ NODE_ENV: 'test', APOCRYPHA_BRIDGE_KEY_B64: Buffer.alloc(31).toString('base64') }));
  assert.equal(validBridgeTarget('owner', 'GET', '/v1/observe/errors?privacy_partition=owner%3Aapocky&limit=100'), true);
  assert.equal(validBridgeTarget('owner', 'GET', '/v1/auth/status'), true);
  assert.equal(validBridgeTarget('owner', 'GET', '/v1/auth/status?subject=other'), false);
  assert.equal(validBridgeTarget('account', 'GET', '/v1/auth/status'), false);
  for (const target of ['https://evil.test', '//evil.test', '/v1/observe/errors?privacy_partition=other', '/v1/observe/errors?limit=100&privacy_partition=owner%3Aapocky', '/v1/observe/errors?privacy_partition=owner%3Aapocky&limit=01', '/v1/observe/errors?privacy_partition=owner%3Aapocky&secret=x']) assert.equal(validBridgeTarget('owner', 'GET', target), false);
  const operation = { operation_id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', objective: 'A safe fixture.', allowed_paths: ['tests/fixture.txt'] };
  assert.doesNotThrow(() => createBridgeRequest(cfg, { ...input, channel: 'owner', target: '/v1/code/operations', body: Buffer.from(JSON.stringify(operation)) }, initial));
  assert.throws(() => createBridgeRequest(cfg, { ...input, channel: 'owner', target: '/v1/code/operations', body: Buffer.from(JSON.stringify({ ...operation, allowed_paths: ['../outside'] })) }, initial));
  for (const path of ['.git/config', '.GIT/config', 'src/CON.txt', 'src/COM1', 'src/nul', 'src/filename.', 'src/filename ', 'src/é.ts']) assert.throws(() => createBridgeRequest(cfg, { ...input, channel: 'owner', target: '/v1/code/operations', body: Buffer.from(JSON.stringify({ ...operation, allowed_paths: [path] })) }, initial));

  const store = new MemoryPersistence(); let now = initial;
  const queue = new BridgeQueue(cfg, store, () => now);
  const [id, duplicate] = await Promise.all([queue.admit(input), queue.admit(input)]);
  assert.equal(id, duplicate); assert.equal(store.jobs.size, 1);
  const rawStorage = JSON.stringify([...store.jobs.values()]);
  assert(!rawStorage.includes('café') && !rawStorage.includes('A newline'), 'database stores no request plaintext');
  const [leaseA, leaseB] = await Promise.all([queue.poll(), queue.poll()]);
  const lease = leaseA ?? leaseB; assert(lease); assert.equal(Number(Boolean(leaseA)) + Number(Boolean(leaseB)), 1, 'CAS admits one lease');
  const encrypted = encryptBridge(cfg, 'response', id, response(id));
  await assert.rejects(queue.complete(id, '99999999-9999-4999-8999-999999999999', encrypted), coded('BRIDGE_COMPLETION_CONFLICT'));
  await queue.complete(id, lease.lease_id, encrypted);
  const storedMetadata = store.jobs.get(id)!.metadata as { response: unknown };
  storedMetadata.response = Object.fromEntries(Object.entries(encrypted).reverse());
  await queue.complete(id, lease.lease_id, encrypted);
  await assert.rejects(queue.complete(id, lease.lease_id, encryptBridge(cfg, 'response', id, response(id, 'changed'))), coded('BRIDGE_COMPLETION_CONFLICT'));
  assert.equal(await queue.admit(input), id); assert.equal(await queue.poll(), null);
  assert.deepEqual(await (await queue.fetch(input)).json(), { text: 'fixture reply' });
  assert(!JSON.stringify([...store.jobs.values()]).includes('fixture reply'), 'database stores no response plaintext');
  await queue.consumeAuthentication({ nonce: '77777777-7777-4777-8777-777777777777', time: initial / 1000 });
  await assert.rejects(new BridgeQueue(cfg, store, () => now).consumeAuthentication({ nonce: '77777777-7777-4777-8777-777777777777', time: initial / 1000 }), coded('BRIDGE_WORKER_REPLAY'));
  const get: BridgeInput = { channel: 'owner', subject: cfg.ownerSubject, method: 'GET', target: '/health', body: Buffer.alloc(0) };
  assert.notEqual(await queue.admit(get), await queue.admit(get), 'GET obtains a fresh nonce');

  const attackStore = new MemoryPersistence(); const attackQueue = new BridgeQueue(cfg, attackStore, () => now);
  await attackQueue.poll();
  attackStore.jobs.set(id, structuredClone(store.jobs.get(id)!));
  assert.equal(await attackQueue.poll(), null, 'unindexed client-insertable rows cannot execute');
  const node = [...attackStore.sessions.values()][0]!;
  (node.metadata as { pending: unknown[] }).pending.push({ job_id: id, subject: input.subject, channel: input.channel });
  await assert.rejects(attackQueue.poll(), coded('BRIDGE_NODE_INTEGRITY_FAILED'), 'even the owner cannot forge an admission index');
  const conflictStore = new MemoryPersistence(); const conflictQueue = new BridgeQueue(cfg, conflictStore, () => now);
  conflictStore.sessions.set(bridgeSessionId(cfg, input.subject), { id: bridgeSessionId(cfg, input.subject), user_id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', title: 'Apocrypha encrypted bridge', metadata: {} });
  await assert.rejects(conflictQueue.admit(input), coded('BRIDGE_SESSION_INTEGRITY_FAILED'));
  const deletedStore = new MemoryPersistence(); const deletedQueue = new BridgeQueue(cfg, deletedStore, () => now);
  const deletedId = await deletedQueue.admit(input); const survivingId = await deletedQueue.admit(get);
  deletedStore.jobs.delete(deletedId);
  assert.equal((await deletedQueue.poll())!.job_id, survivingId, 'a user deleting a session cannot stall other admitted accounts');
  const damagedStore = new MemoryPersistence(); const damagedQueue = new BridgeQueue(cfg, damagedStore, () => now);
  const damagedId = await damagedQueue.admit(input); const validId = await damagedQueue.admit(get);
  damagedStore.jobs.get(damagedId)!.user_id = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
  assert.equal((await damagedQueue.poll())!.job_id, validId, 'a tampered row loses admission without execution or a queue-wide stall');

  const uncertainStore = new MemoryPersistence(); const controller = new AbortController();
  const uncertainQueue = new BridgeQueue(cfg, uncertainStore, () => now, async () => { controller.abort(); });
  await assert.rejects(uncertainQueue.fetch({ ...input, signal: controller.signal }), coded('BRIDGE_WAIT_ABORTED'));
  assert.equal(uncertainStore.jobs.size, 1); assert.equal([...uncertainStore.jobs.values()][0]!.status, 'queued', 'abort does not cancel admitted job');
  now += 590_000;
  const late = await uncertainQueue.poll(); assert(late);
  now += 20_000;
  await uncertainQueue.complete(late.job_id, late.lease_id, encryptBridge(cfg, 'response', late.job_id, response(late.job_id)));
  assert.equal((await uncertainStore.getJob(id))!.status, 'done', 'matching live lease completes past job admission expiry');

  now = initial;
  const recycleStore = new MemoryPersistence(); const recycleQueue = new BridgeQueue(cfg, recycleStore, () => now);
  await recycleQueue.admit(input); const original = await recycleQueue.poll(); assert(original);
  now += 421_000;
  const recycled = await recycleQueue.poll(); assert(recycled); assert.notEqual(recycled.lease_id, original.lease_id);
  assert.equal(recycled.job_id, original.job_id);
  await assert.rejects(recycleQueue.complete(id, original.lease_id, encrypted), coded('BRIDGE_COMPLETION_CONFLICT'));
  now += 601_000;
  await recycleQueue.admit(input); const renewed = await recycleQueue.poll(); assert(renewed); assert.equal(renewed.job_id, id);

  const apiStore = new MemoryPersistence(); now = initial;
  const factory = () => new BridgeQueue(config(), apiStore, () => now);
  await factory().admit(input);
  async function invoke(handler: ReturnType<typeof createWorkerPollHandler>, path: string, raw: Buffer, override: Record<string, unknown> = {}) {
    const req = Object.assign(Readable.from([raw]), { method: 'POST', url: path, query: {}, headers: { 'content-type': 'application/json', ...workerHeaders(config(), path, raw, now) }, ...override }) as unknown as NextApiRequest;
    const result = { status: 0, body: null as any, headers: {} as Record<string, string> };
    const res = { setHeader(k: string, v: string) { result.headers[k] = v; return this; }, status(value: number) { result.status = value; return this; }, json(value: unknown) { result.body = value; return this; } } as unknown as NextApiResponse;
    await handler(req, res); assert(!JSON.stringify(result).includes(fixture.key_b64)); return result;
  }
  const pollHandler = createWorkerPollHandler(factory);
  const polled = await invoke(pollHandler, WORKER_POLL_PATH, pollBytes);
  assert.equal(polled.status, 200); assert(polled.body.job);
  const denied = await invoke(pollHandler, WORKER_POLL_PATH, pollBytes, { headers: { 'content-type': 'application/json' } });
  assert.equal(denied.status, 401);
  const completedRaw = Buffer.from(JSON.stringify({ schema_version: 'apocky.bridge.complete.v1', job_id: id, lease_id: polled.body.job.lease_id, response: encrypted }));
  assert.equal((await invoke(createWorkerCompleteHandler(factory), WORKER_COMPLETE_PATH, completedRaw)).status, 200);
  assert.equal((await invoke(pollHandler, WORKER_POLL_PATH, pollBytes, { query: { forged: 'PRIVATE_SECRET' } })).status, 400);
  assert.equal((await invoke(pollHandler, WORKER_POLL_PATH, pollBytes, { method: 'GET' })).status, 405);
  process.stdout.write('Bridge crypto, wire, replay, admission, lease, completion, cancellation and HTTP checks passed.\n');
}
run().catch(error => { process.stderr.write(String(error) + '\n'); process.exitCode = 1; });

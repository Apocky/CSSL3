import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
import { MiniBrainVault, type MiniBrainState } from '../../lib/brain/mini-brain';
import { syncSigningPayload, type MiniBrainSyncResponse } from '../../lib/brain/mobile-contracts';
import type { BrainSnapshot } from '../../lib/brain/contracts';

// § fixture := disposable IDB interface + real vault/WebCrypto; no browser/network.
class DisposableDatabase {
  stores = new Map<string, Map<string, unknown>>();
  failIdentityWrite = false;
  transaction(name: string) {
    const store = this.stores.get(name) ?? new Map<string, unknown>();
    this.stores.set(name, store);
    let aborted = false;
    const database = this;
    const tx: any = { abort() { aborted = true; setImmediate(() => tx.onabort?.()); } };
    const finish = () => setImmediate(() => { if (!aborted) tx.oncomplete?.(); });
    tx.objectStore = () => ({
      get(key: string) {
        const request: any = {};
        setImmediate(() => { if (aborted) return; request.result = structuredClone(store.get(key)); request.onsuccess?.(); finish(); });
        return request;
      },
      put(value: any) {
        if (name === 'device' && database.failIdentityWrite) {
          database.failIdentityWrite = false;
          tx.abort();
          return;
        }
        if (!aborted) store.set(value.key, structuredClone(value));
        finish();
      },
      delete(key: string) { if (!aborted) store.delete(key); finish(); },
    });
    return tx;
  }
}

async function main() {
  const signing = await webcrypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign', 'verify']);
  const identity = {
    key: 'identity', device_id: crypto.randomUUID(), owner_ref: 'disposable-owner',
    signing_private_key: signing.privateKey,
    signing_public_jwk: await webcrypto.subtle.exportKey('jwk', signing.publicKey),
    encryption_key: await webcrypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']),
    device_token: 'disposable-token', token_expires_at: '2100-01-01T00:00:00Z', next_sequence: 1,
  };
  const db = new DisposableDatabase();
  db.stores.set('device', new Map([['identity', structuredClone(identity)]]));
  const Vault = MiniBrainVault as any;
  const tabA: MiniBrainVault = new Vault(db, structuredClone(identity));
  const tabB: MiniBrainVault = new Vault(db, structuredClone(identity));
  await tabA.freshState('disposable-session');
  const staleA = (await tabA.load())!;
  const staleB = (await tabB.load())!;
  const [first, second] = await Promise.all([
    tabA.queueTurn(staleA, 'Disposable A'),
    tabB.queueTurn(staleB, 'Disposable B'),
  ]);
  const persisted = (await tabA.load())!;
  assert.deepEqual(persisted.queue.map(turn => turn.request_id), [first.turn.request_id, second.turn.request_id]);
  assert.deepEqual(persisted.sessions[0]?.messages.map(message => message.content), ['Disposable A', 'Disposable B']);
  assert.equal(persisted.revision, 3);
  await assert.rejects(tabB.save({ ...staleB, current_session_id: 'stale-selection' }), /MINI_BRAIN_STALE_WRITE/);
  assert.deepEqual((await tabB.load())!.queue, persisted.queue, 'stale arbitrary save cannot drop queued messages');
  const replay = await tabB.queueTurn(staleB, first.turn.text, first.turn);
  assert.equal(replay.state.queue.length, 2);
  assert.equal(replay.state.revision, persisted.revision, 'existing request replay writes nothing');
  const reopened: MiniBrainVault = new Vault(db, structuredClone(identity));
  assert.deepEqual((await reopened.load())!.queue, persisted.queue, 'new vault instance recalls both durable messages');
  assert.deepEqual((await reopened.freshState('must-not-reset')).queue, persisted.queue, 'initialization cannot erase an existing vault');
  assert.equal((await reopened.load())!.current_session_id, 'disposable-session');
  const input = { operation: 'pull' as const, sessionId: 'disposable-session', baseCursor: null, payload: null };
  const [signedA, signedB] = await Promise.all([
    tabA.signedRequest({ ...input, requestId: crypto.randomUUID() }),
    tabB.signedRequest({ ...input, requestId: crypto.randomUUID() }),
  ]);
  assert.deepEqual([signedA.sequence, signedB.sequence], [1, 2], 'separate instances reserve distinct increasing numbers');
  const reopenedSigner: MiniBrainVault = new Vault(db, structuredClone(identity));
  const signedC = await reopenedSigner.signedRequest({ ...input, requestId: crypto.randomUUID() });
  assert.equal(signedC.sequence, 3, 'reopened instance resumes above both durable reservations');
  for (const request of [signedA, signedB, signedC]) {
    const { signature, device_token, ...unsigned } = request;
    assert.equal(device_token, identity.device_token);
    assert.equal(request.device_id, identity.device_id);
    assert.equal(await webcrypto.subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, signing.publicKey,
      Buffer.from(signature, 'base64url'), Buffer.from(syncSigningPayload(unsigned))), true,
    'reservation preserves the existing signed body contract and device key');
  }
  await tabA.bind({ owner_ref: identity.owner_ref, device_token: identity.device_token, expires_at: identity.token_expires_at });
  const afterRenewal = await tabB.signedRequest({ ...input, requestId: crypto.randomUUID() });
  assert.equal(afterRenewal.sequence, 4, 'token renewal from a stale instance cannot rewind reserved numbers');
  db.failIdentityWrite = true;
  await assert.rejects(tabA.signedRequest({ ...input, requestId: crypto.randomUUID() }), /MINI_BRAIN_IDB_TRANSACTION_ABORTED/,
    'failed durable reservation returns no signed request');
  const afterAbort = await reopenedSigner.signedRequest({ ...input, requestId: crypto.randomUUID() });
  assert.equal(afterAbort.sequence, 5);
  assert.deepEqual((await reopened.load())!.queue, persisted.queue, 'signing never mutates encrypted pending messages');
  const selectedB = await tabA.selectSession(staleA, 'selected-session-b');
  const pendingB = await tabA.queueTurn(selectedB, 'Disposable B in selected conversation');
  const messageA = { role: 'user', content: first.turn.text, recorded_at: first.turn.queued_at,
    request_id: first.turn.request_id, event_digest: '4'.repeat(64) };
  const replyA: MiniBrainSyncResponse = {
    schema_version: 'apocky.mini-brain.sync-response.v1', status: 'appended',
    session_id: first.turn.session_id, request_id: first.turn.request_id, cursor: '5'.repeat(64),
    messages: [messageA, { ...messageA, role: 'assistant', content: 'Disposable acknowledged reply A', event_digest: '6'.repeat(64) }],
    tombstones: [], events_truncated: false,
    provenance: { transport: 'owner_bound_apocv4_runtime', privacy_partition_ref: null, principal_ref: null, binding_ref: null },
    controls: { owner_session: 'verified', device_signature: 'verified', replay: 'bounded_sequence_and_idempotent_request',
      rate_limit: 'relay_instance_burst', partition: 'server_derived_owner' }, served_by: 'fixture', ts: new Date().toISOString(),
  };
  const acknowledged = await tabB.applySync(staleA, replyA);
  const remainingIds = [second.turn.request_id, pendingB.turn.request_id];
  assert.deepEqual(acknowledged.queue.map(turn => turn.request_id), remainingIds,
    'stale A snapshot acknowledges only A while preserving both later queued messages');
  assert.equal(acknowledged.current_session_id, 'selected-session-b', 'background A acknowledgement cannot steal explicit B selection');
  assert(acknowledged.sessions.find(session => session.session_id === first.turn.session_id)?.messages
    .some(message => message.request_id === second.turn.request_id), 'later same-session pending user message survives A history refresh');
  const cache = { memories: [{ id: 'disposable-memory', topic_key: 'fixture', type: 'fact',
    paraphrase: 'Disposable cached memory', created_at: new Date().toISOString(), source_msg_ids: [] }] } as unknown as BrainSnapshot;
  const cached = await tabB.cacheSnapshot(staleB, cache);
  assert.deepEqual(cached.queue.map(turn => turn.request_id), remainingIds, 'stale memory refresh neither resurrects A nor loses B');
  assert.equal(cached.current_session_id, 'selected-session-b');
  assert.equal(cached.memories[0]?.paraphrase, 'Disposable cached memory');
  const afterSyncReopen: MiniBrainVault = new Vault(db, structuredClone(identity));
  const recovered = (await afterSyncReopen.load())!;
  assert.deepEqual(recovered.queue, cached.queue);
  assert.equal(recovered.current_session_id, 'selected-session-b');
  assert.deepEqual(recovered.memories, cached.memories);
  const deferred = () => {
    let resolve!: () => void;
    const promise = new Promise<void>(done => { resolve = done; });
    return { promise, resolve };
  };
  const responseFor = (request: { session_id: string; request_id: string }, cursor: string): MiniBrainSyncResponse => ({
    ...replyA, status: 'advanced', session_id: request.session_id, request_id: request.request_id,
    cursor: cursor.repeat(64), messages: [], ts: new Date().toISOString(),
  });
  const firstStarted = deferred();
  const releaseFirst = deferred();
  const sendOrder: number[] = [];
  const deliverA = tabA.deliverSync(recovered, { ...input, requestId: crypto.randomUUID() }, async request => {
    sendOrder.push(request.sequence);
    firstStarted.resolve();
    await releaseFirst.promise;
    return responseFor(request, '7');
  });
  await firstStarted.promise;
  const deliverB = tabB.deliverSync(recovered, { ...input, requestId: crypto.randomUUID() }, async request => {
    sendOrder.push(request.sequence);
    assert.equal((await tabB.load())!.sessions.find(session => session.session_id === input.sessionId)?.cursor, '7'.repeat(64),
      'first response must commit before second network dispatch');
    return responseFor(request, '8');
  });
  await new Promise<void>(done => setImmediate(done));
  assert.equal(sendOrder.length, 1, 'second instance cannot overtake a delayed first send');
  releaseFirst.resolve();
  await Promise.all([deliverA, deliverB]);
  assert(sendOrder[1]! > sendOrder[0]!);

  const failedDelivery = tabA.deliverSync(recovered, { ...input, requestId: crypto.randomUUID() }, async () => {
    throw new Error('disposable network failure');
  });
  const expectedFailure = assert.rejects(failedDelivery, /disposable network failure/);
  const afterFailure = tabB.deliverSync(recovered, { ...input, requestId: crypto.randomUUID() }, async request => responseFor(request, '9'));
  await Promise.all([expectedFailure, afterFailure]);

  const abortController = new AbortController();
  const abortStarted = deferred();
  const lateResponse = deferred();
  const abortedDelivery = tabA.deliverSync(recovered, { ...input, requestId: crypto.randomUUID() }, async request => {
    abortStarted.resolve();
    await lateResponse.promise; // Deliberately ignores cancellation: a late response must still never apply.
    return responseFor(request, 'a');
  }, { signal: abortController.signal });
  const expectedAbort = assert.rejects(abortedDelivery, /disposable cancellation/);
  await abortStarted.promise;
  abortController.abort(new Error('disposable cancellation'));
  await expectedAbort;
  await tabB.deliverSync(recovered, { ...input, requestId: crypto.randomUUID() }, async request => responseFor(request, 'b'));
  lateResponse.resolve();
  await new Promise<void>(done => setImmediate(done));
  assert.equal((await tabA.load())!.sessions.find(session => session.session_id === input.sessionId)?.cursor, 'b'.repeat(64),
    'aborted first request releases delivery lock and cannot later overwrite the second result');

  const heldStarted = deferred();
  const releaseHeld = deferred();
  const heldDelivery = tabA.deliverSync(recovered, { ...input, requestId: crypto.randomUUID() }, async request => {
    heldStarted.resolve();
    await releaseHeld.promise;
    return responseFor(request, 'c');
  });
  await heldStarted.promise;
  let waiterSent = false;
  const beforeWaiter = (db.stores.get('device')!.get('identity') as { next_sequence: number }).next_sequence;
  await assert.rejects(tabB.deliverSync(recovered, { ...input, requestId: crypto.randomUUID() }, async request => {
    waiterSent = true;
    return responseFor(request, 'd');
  }, { timeoutMs: 20 }), /MINI_BRAIN_DELIVERY_TIMEOUT/);
  assert.equal(waiterSent, false, 'expired waiting operation never signs or sends');
  assert.equal((db.stores.get('device')!.get('identity') as { next_sequence: number }).next_sequence, beforeWaiter);
  releaseHeld.resolve();
  await heldDelivery;

  const retryIds: string[] = [];
  const retrySequences: number[] = [];
  const retryIntent = { ...input, requestId: crypto.randomUUID() };
  await tabB.deliverSync(recovered, retryIntent, async request => {
    retryIds.push(request.request_id);
    retrySequences.push(request.sequence);
    if (retryIds.length === 1) throw Object.assign(new Error('disposable replay rejection'), { code: 'BRAIN_SYNC_REPLAY_REJECTED' });
    return responseFor(request, 'e');
  });
  assert.deepEqual(retryIds, [retryIntent.requestId, retryIntent.requestId], 'bounded replay retry preserves the original request identity');
  assert(retrySequences[1]! > retrySequences[0]!);
  assert.deepEqual((await tabA.load())!.queue, recovered.queue, 'delivery tests never acknowledge unrelated queued messages');
  const indexedDbDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'indexedDB');
  const navigatorDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  try {
    Object.defineProperty(globalThis, 'indexedDB', { configurable: true, value: {} });
    Object.defineProperty(globalThis, 'navigator', { configurable: true, value: {} });
    await assert.rejects(tabA.deliverSync(recovered, { ...input, requestId: crypto.randomUUID() }, async request => {
      throw new Error('unsupported browser must never send');
    }), /MINI_BRAIN_DELIVERY_LOCK_UNAVAILABLE/, 'real browser vault refuses unsynchronized cross-tab delivery without Web Locks');
  } finally {
    if (navigatorDescriptor) Object.defineProperty(globalThis, 'navigator', navigatorDescriptor);
    else Reflect.deleteProperty(globalThis, 'navigator');
    if (indexedDbDescriptor) Object.defineProperty(globalThis, 'indexedDB', indexedDbDescriptor);
    else Reflect.deleteProperty(globalThis, 'indexedDB');
  }
  let noticeCount = 0;
  let resolveNotice!: (state: MiniBrainState) => void;
  const notice = new Promise<MiniBrainState>(resolve => { resolveNotice = resolve; });
  const wire = new BroadcastChannel('apocky-mini-brain-v1:changes');
  const metadataKeys: string[][] = [];
  wire.onmessage = event => { metadataKeys.push(Object.keys(event.data).sort()); };
  const unsubscribe = tabB.subscribe(() => {
    noticeCount += 1;
    void tabB.load().then(state => { if (state) resolveNotice(state); });
  });
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  try {
    const changed = await tabA.queueTurn(recovered, 'Disposable cross-tab message');
    const visible = await Promise.race([notice, new Promise<never>((_, reject) => {
      noticeTimer = setTimeout(() => reject(new Error('vault change notification missing')), 1000);
    })]);
    clearTimeout(noticeTimer);
    assert(visible.queue.some(turn => turn.request_id === changed.turn.request_id),
      'second instance observes the first committed message by notification and encrypted readback');
    await new Promise(done => setTimeout(done, 20));
    assert.equal(noticeCount, 1);
    assert.deepEqual(metadataKeys[0], ['device_id', 'owner_ref', 'revision'], 'notification carries no message or memory payload');
    await tabB.load();
    await assert.rejects(tabA.save(recovered), /MINI_BRAIN_STALE_WRITE/);
    await new Promise(done => setTimeout(done, 20));
    assert.equal(noticeCount, 1, 'readback and rejected writes cannot amplify notifications');
    unsubscribe();
    await tabA.queueTurn(recovered, 'Disposable after unsubscribe');
    await new Promise(done => setTimeout(done, 20));
    assert.equal(noticeCount, 1, 'unmounted subscriber receives no further changes');
  } finally {
    clearTimeout(noticeTimer);
    unsubscribe();
    wire.close();
  }
  // § fresh-device adoption := atomic + no inference from legacy blank state.
  const freshFixture = async () => {
    const database = new DisposableDatabase();
    database.stores.set('device', new Map([['identity', structuredClone(identity)]]));
    const vault: MiniBrainVault = new Vault(database, structuredClone(identity));
    const peer: MiniBrainVault = new Vault(database, structuredClone(identity));
    return { database, vault, peer, state: await vault.freshState() };
  };
  const fresh = await freshFixture();
  assert.equal(fresh.state.selection_origin, 'provisional');
  const adopted = await fresh.vault.adoptDiscoveredSession(fresh.state, 'discovered-desktop');
  assert.equal(adopted.current_session_id, 'discovered-desktop');
  assert.equal(adopted.selection_origin, 'remote');
  const displayed = await fresh.vault.applySync(adopted, { ...replyA, session_id: 'discovered-desktop' });
  assert(displayed.sessions.find(session => session.session_id === displayed.current_session_id)?.messages
    .some(message => message.role === 'assistant'), 'fresh device displays the discovered acknowledged conversation');
  const freshReopened: MiniBrainVault = new Vault(fresh.database, structuredClone(identity));
  assert.equal((await freshReopened.load())!.current_session_id, 'discovered-desktop', 'selection survives reopening');
  assert.equal((await freshReopened.adoptDiscoveredSession(displayed, 'another-desktop')).revision, displayed.revision,
    'repeated discovery cannot replace adopted selection or write again');

  const explicit = await freshFixture();
  const chosen = await explicit.peer.selectSession(explicit.state, explicit.state.current_session_id);
  const explicitResult = await explicit.vault.adoptDiscoveredSession(explicit.state, 'discovered-desktop');
  assert.equal(explicitResult.current_session_id, chosen.current_session_id);
  assert.equal(explicitResult.selection_origin, 'user', 'same-session explicit choice blocks stale adoption');
  assert.equal(explicitResult.revision, chosen.revision);
  const newDraft = await explicit.peer.selectSession(chosen, 'explicit-new-draft');
  assert.equal((await explicit.vault.adoptDiscoveredSession(explicit.state, 'discovered-desktop')).current_session_id,
    newDraft.current_session_id, 'new local draft cannot be stolen by delayed discovery');

  const pending = await freshFixture();
  const pendingTurn = await pending.peer.queueTurn(pending.state, 'Preserve fresh-device pending text');
  const pendingResult = await pending.vault.adoptDiscoveredSession(pending.state, 'discovered-desktop');
  assert.equal(pendingResult.current_session_id, pending.state.current_session_id);
  assert.deepEqual(pendingResult.queue, pendingTurn.state.queue, 'concurrent pending message survives delayed adoption');
  assert.equal((await pending.vault.adoptDiscoveredSession(pendingResult, 'discovered-desktop')).current_session_id,
    pending.state.current_session_id, 'fresh read of user-authored state still cannot be adopted');

  const legacy = await freshFixture();
  const { selection_origin: _provisional, ...legacyState } = legacy.state;
  const unknown = await legacy.vault.save(legacyState);
  assert.equal((await legacy.peer.adoptDiscoveredSession(unknown, 'discovered-desktop')).revision, unknown.revision,
    'preexisting unmarked blank state is never inferred safe to replace');
  const advanced = await freshFixture();
  const cachedFresh = await advanced.peer.cacheSnapshot(advanced.state, cache);
  assert.equal((await advanced.vault.adoptDiscoveredSession(advanced.state, 'discovered-desktop')).revision, cachedFresh.revision,
    'adoption must compare the exact revision under the writer lock');
  const populated = await freshFixture();
  const syncedFresh = await populated.peer.applySync(populated.state, { ...replyA, session_id: populated.state.current_session_id });
  assert.equal((await populated.vault.adoptDiscoveredSession(syncedFresh, 'discovered-desktop')).current_session_id,
    populated.state.current_session_id, 'confirmed messages and cursor prevent provisional adoption');
  await assert.rejects(populated.vault.adoptDiscoveredSession({ ...syncedFresh, owner_ref: 'wrong-owner' }, 'discovered-desktop'),
    /MINI_BRAIN_VAULT_BINDING_MISMATCH/, 'adoption cannot cross owner identity');

  // § typed admission evidence survives transport failure; identity remains immutable.
  const admissionFixture = await freshFixture();
  const admissionQueued = await admissionFixture.vault.queueTurn(admissionFixture.state, 'Keep this exact pending message');
  const admissionIntent = { operation: 'append' as const, sessionId: admissionQueued.turn.session_id,
    requestId: admissionQueued.turn.request_id, baseCursor: admissionQueued.turn.base_cursor, payload: { text: admissionQueued.turn.text } };
  const pendingError = (requestId = admissionIntent.requestId, sessionId = admissionIntent.sessionId) => Object.assign(new Error('Admission pending'), {
    code: 'BRAIN_APEX_ADMISSION_PENDING', status: 503,
    payload: { code: 'BRAIN_APEX_ADMISSION_PENDING', request_id: requestId, session_id: sessionId, retry_after_ms: 1000 },
  });
  const admissionRequests: Array<{ request_id: string; session_id: string; base_cursor: string | null; payload: unknown }> = [];
  let pendingNotifications = 0;
  await assert.rejects(admissionFixture.vault.deliverSync(admissionQueued.state, admissionIntent, async request => {
    admissionRequests.push({ request_id: request.request_id, session_id: request.session_id, base_cursor: request.base_cursor, payload: request.payload });
    assert.equal(Object.hasOwn(request, 'admission_pending'), false, 'local pending evidence never enters signed wire contract');
    if (admissionRequests.length === 1) throw pendingError();
    throw new Error('Transient history GET failure');
  }, { onPending(state) { pendingNotifications += 1; assert.equal(state.queue[0]?.admission_pending, true); } }), /Transient history GET failure/);
  assert.equal(pendingNotifications, 1);
  assert.equal(admissionRequests.length, 2, 'only existing bounded same-ID admission continuation occurs');
  assert.deepEqual(admissionRequests[0], admissionRequests[1], 'continuation preserves exact session, request, base and text');
  const admissionReopened: MiniBrainVault = new Vault(admissionFixture.database, structuredClone(identity));
  const pendingAfterReopen = (await admissionReopened.load())!;
  assert.deepEqual(pendingAfterReopen.queue[0], { ...admissionQueued.turn, admission_pending: true });
  const admissionResponse: MiniBrainSyncResponse = { ...replyA, session_id: admissionIntent.sessionId, request_id: admissionIntent.requestId,
    messages: [{ ...messageA, request_id: admissionIntent.requestId, content: admissionQueued.turn.text },
      { ...messageA, role: 'assistant', request_id: admissionIntent.requestId, content: 'A real terminal reply', event_digest: '7'.repeat(64) }] };
  const admissionCompleted = await admissionReopened.applySync(pendingAfterReopen, admissionResponse);
  assert.equal(admissionCompleted.queue.length, 0, 'terminal history dominates pending marker');
  assert.equal(admissionCompleted.sessions[0]?.messages.at(-1)?.content, 'A real terminal reply');
  let terminalResends = 0;
  const terminalReplay = await admissionFixture.vault.deliverSync(pendingAfterReopen, admissionIntent, async () => { terminalResends += 1; return admissionResponse; });
  assert.equal(terminalResends, 0, 'terminal acknowledgement prevents a stale pending snapshot from resending');
  assert.equal(terminalReplay.queue.length, 0);
  for (const mismatched of [pendingError('wrong-request'), pendingError(admissionIntent.requestId, 'wrong-session'),
    { ...pendingError(), status: 502 }, { ...pendingError(), payload: { ...pendingError().payload, retry_after_ms: 2000 } }]) {
    const invalid = await freshFixture();
    const queued = await invalid.vault.queueTurn(invalid.state, admissionQueued.turn.text, {
      request_id: admissionIntent.requestId, session_id: admissionIntent.sessionId,
    });
    let invalidCalls = 0;
    await assert.rejects(invalid.vault.deliverSync(queued.state, admissionIntent, async () => { invalidCalls += 1; throw mismatched; }));
    assert.equal(invalidCalls, 1);
    assert.deepEqual((await invalid.vault.load())!.queue, queued.state.queue, 'unbound pending claims cannot mark durable queue');
  }
  const malformed = await freshFixture();
  const malformedQueued = await malformed.vault.queueTurn(malformed.state, 'Legacy pending metadata remains untrusted');
  await malformed.vault.save({ ...malformedQueued.state, queue: [{ ...malformedQueued.turn, admission_pending: 'true' as unknown as true }] });
  assert.deepEqual((await malformed.vault.load())!.queue, [malformedQueued.turn], 'optional marker decoder ignores malformed metadata and preserves legacy turn');

  // § terminal failure settles only its exact queued message; failed text persists.
  const failedFixture = await freshFixture();
  const failedQueued = await failedFixture.vault.queueTurn(failedFixture.state, 'Preserve a failed message');
  const failurePause = new AbortController();
  await assert.rejects(failedFixture.vault.deliverSync(failedQueued.state, { operation: 'append',
    sessionId: failedQueued.turn.session_id, requestId: failedQueued.turn.request_id, baseCursor: failedQueued.turn.base_cursor,
    payload: { text: failedQueued.turn.text } }, async () => { throw pendingError(failedQueued.turn.request_id, failedQueued.turn.session_id); },
  { signal: failurePause.signal, onPending() { failurePause.abort(new Error('Pause after durable pending observation')); } }), /Pause after durable pending observation/);
  assert.equal((await failedFixture.vault.load())!.queue[0]?.admission_pending, true);
  const otherQueued = await failedFixture.peer.queueTurn(failedQueued.state, 'Preserve unrelated waiting message');
  const failedResponse = { ...replyA, session_id: failedQueued.turn.session_id,
    request_id: failedQueued.turn.request_id, status: 'idempotent_replay' as const,
    messages: [{ role: 'user', content: failedQueued.turn.text, request_id: failedQueued.turn.request_id,
      recorded_at: replyA.ts, event_digest: 'a'.repeat(64), terminal_failure: {
        code: 'engine_failure', error_digest: 'b'.repeat(64), receipt_digest: 'c'.repeat(64),
      } }],
  };
  const selectedOther = await failedFixture.peer.selectSession(otherQueued.state, 'failed-fixture-other-session');
  const newestQueued = await failedFixture.peer.queueTurn(selectedOther, 'Preserve newer selected conversation text');
  const unrelatedSessionBefore = newestQueued.state.sessions.find(session => session.session_id === selectedOther.current_session_id)!;
  const settledFailure = await failedFixture.vault.applySync(failedQueued.state, failedResponse);
  assert.deepEqual(settledFailure.queue, [otherQueued.turn, newestQueued.turn],
    'stale failure response removes only its exact request and retains both newer queued records byte-for-byte');
  assert.equal(settledFailure.current_session_id, selectedOther.current_session_id,
    'older session failure cannot replace the newer explicit session selection');
  assert.deepEqual(settledFailure.sessions.find(session => session.session_id === selectedOther.current_session_id), unrelatedSessionBefore);
  assert(settledFailure.sessions.find(session => session.session_id === failedQueued.turn.session_id)!.messages
    .some(message => message.request_id === otherQueued.turn.request_id && message.content === otherQueued.turn.text),
    'newer same-session pending message remains visible');
  const preservedFailure = settledFailure.sessions.find(session => session.session_id === failedQueued.turn.session_id)!.messages.find(message => message.request_id === failedQueued.turn.request_id)!;
  assert.equal(preservedFailure.content, failedQueued.turn.text);
  assert.equal(preservedFailure.terminal_failure?.receipt_digest, 'c'.repeat(64));
  assert.equal(settledFailure.sessions[0]!.messages.filter(message => message.request_id === failedQueued.turn.request_id && message.role === 'assistant').length, 0);
  const reopenedFailure = (await failedFixture.peer.load())!;
  assert.deepEqual(reopenedFailure.queue, settledFailure.queue);
  assert.deepEqual(reopenedFailure.sessions, settledFailure.sessions);
  const repeatedFailure = await failedFixture.peer.applySync(failedQueued.state, failedResponse);
  assert.deepEqual(repeatedFailure.queue, settledFailure.queue, 'replayed failure preserves all remaining pending IDs');
  assert.deepEqual(repeatedFailure.sessions, settledFailure.sessions, 'replayed failure never duplicates the preserved message');
  const crossSessionFailure = { ...failedResponse, session_id: selectedOther.current_session_id,
    messages: [{ ...failedResponse.messages[0], request_id: otherQueued.turn.request_id, content: otherQueued.turn.text }] };
  await assert.rejects(failedFixture.vault.applySync(failedQueued.state, crossSessionFailure), /MINI_BRAIN_FAILED_REQUEST_BINDING_MISMATCH/);
  assert.deepEqual((await failedFixture.peer.load())!.queue, settledFailure.queue);
  const conflictResponse = { ...failedResponse, request_id: otherQueued.turn.request_id, messages: [{
    ...failedResponse.messages[0], request_id: otherQueued.turn.request_id, content: 'Mismatched replacement',
  }] };
  await assert.rejects(failedFixture.vault.applySync(reopenedFailure, conflictResponse), /MINI_BRAIN_FAILED_REQUEST_BINDING_MISMATCH/);
  assert.deepEqual((await failedFixture.peer.load())!.queue, settledFailure.queue);
  await assert.rejects(failedFixture.vault.applySync(reopenedFailure, { ...failedResponse, messages: [{
    ...failedResponse.messages[0], terminal_failure: { code: 'engine_failure', error_digest: 'bad', receipt_digest: 'c'.repeat(64) },
  }] }), /MINI_BRAIN_TERMINAL_FAILURE_INVALID/);
  console.log('mini-brain-writer: writer, delivery, adoption and preserved terminal failure passed');
}
void main().catch(error => { console.error(error); process.exitCode = 1; });

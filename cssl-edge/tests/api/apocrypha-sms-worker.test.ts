import assert from 'node:assert/strict';

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  publicMemberPrincipalRef,
  RuntimeProxyError,
  type RuntimeChatProjection,
  type RuntimeSessionGetProjection,
} from '@/lib/apocv4/runtime-proxy';
import { scopeConversationId, scopeRequestId } from '@/lib/apocrypha/proxy';
import type { SmsSystemConfiguration } from '@/lib/apocrypha/sms/config';
import {
  decryptSmsText,
  encryptSmsText,
  estimateSmsSegments,
  phoneBindingHash,
} from '@/lib/apocrypha/sms/core';
import {
  smsInboundAad,
  type SmsClaimedJob,
  type SmsClaimedSend,
  type SmsStore,
} from '@/lib/apocrypha/sms/store';
import {
  runSmsWorkerOnce,
  smsReplyAad,
  type SmsProviderSender,
  type SmsWorkerDependencies,
} from '@/lib/apocrypha/sms/worker';
import { createApocryphaSmsCronHandler } from '@/pages/api/cron/apocrypha-sms';

const SESSION_ID = '11111111-1111-4111-8111-111111111111';
const REQUEST_ID = '22222222-2222-4222-8222-222222222222';
const MESSAGE_ID = '33333333-3333-4333-8333-333333333333';
const OWNER_ID = '44444444-4444-4444-8444-444444444444';
const LEASE_TOKEN = '55555555-5555-4555-8555-555555555555';
const DISPATCH_TOKEN = '66666666-6666-4666-8666-666666666666';
const INBOUND_SID = `SM${'a'.repeat(32)}`;
const OUTBOUND_SID = `SM${'b'.repeat(32)}`;
const RESPONSE_DIGEST = 'c'.repeat(64);
const STORAGE_KEY = Buffer.alloc(32, 17);

function configuration(): SmsSystemConfiguration {
  return {
    provider: {
      provider: 'twilio',
      accountSid: `AC${'1'.repeat(32)}`,
      authToken: 'auth-token',
      apiKeySid: `SK${'2'.repeat(32)}`,
      apiKeySecret: 'api-key-secret',
      webhookUrl: 'https://www.apocky.com/api/apocrypha/sms/inbound',
      statusCallbackUrl: 'https://www.apocky.com/api/apocrypha/sms/status',
      smsNumber: '+14805550100',
    },
    binding: {
      ownerNumber: '+14805550101',
      ownerUserId: OWNER_ID,
      sessionId: SESSION_ID,
    },
    bindingKey: Buffer.alloc(32, 23),
    keyring: {
      activeKeyId: 'active',
      storageKey: STORAGE_KEY,
      decryptionKeys: { active: STORAGE_KEY },
    },
    policy: {
      maxReplyChars: 320,
      maxSegments: 3,
      dailySegmentBudget: 30,
    },
  };
}

function claimedJob(config = configuration()): SmsClaimedJob {
  const plaintext = JSON.stringify({
    schema: 'apocrypha.sms-inbound.v1',
    body: '  Hello Apocrypha \n',
  });
  return {
    messageId: MESSAGE_ID,
    providerMessageSid: INBOUND_SID,
    phoneHash: phoneBindingHash(
      config.provider.provider,
      config.binding.ownerNumber,
      config.bindingKey,
    ),
    sessionId: config.binding.sessionId,
    requestId: REQUEST_ID,
    bodyCiphertext: encryptSmsText(
      plaintext,
      config.keyring,
      smsInboundAad(config.provider.accountSid, INBOUND_SID, REQUEST_ID),
    ),
    leaseToken: LEASE_TOKEN,
    reconcileOnly: false,
  };
}

interface ReadyInput {
  messageId: string;
  leaseToken: string;
  replyCiphertext: string;
  responseDigest: string;
  outboundSegments: number;
}

class FakeStore {
  readonly jobs: SmsClaimedJob[] = [];
  readonly sends: SmsClaimedSend[] = [];
  readonly readyInputs: ReadyInput[] = [];
  readonly runtimeFailures: Array<{ messageId: string; leaseToken: string; errorCode: string }> = [];
  readonly sendFailures: Array<{
    messageId: string;
    dispatchToken: string;
    outcome: 'failed' | 'uncertain';
    errorCode: string;
  }> = [];
  readonly sent: Array<{ messageId: string; dispatchToken: string; sid: string; status: string }> = [];
  readonly authorizations: Array<{ messageId: string; dispatchToken: string }> = [];
  allowReady = true;
  allowSendClaim = true;
  allowAuthorization = true;

  async claimJob(): Promise<SmsClaimedJob | null> {
    return this.jobs.shift() ?? null;
  }

  async markJobReady(input: ReadyInput): Promise<boolean> {
    this.readyInputs.push(input);
    if (!this.allowReady) return false;
    this.sends.push({
      messageId: input.messageId,
      providerMessageSid: INBOUND_SID,
      replyCiphertext: input.replyCiphertext,
      outboundSegments: input.outboundSegments,
      dispatchToken: DISPATCH_TOKEN,
    });
    return true;
  }

  async markRuntimeFailed(messageId: string, leaseToken: string, errorCode: string): Promise<boolean> {
    this.runtimeFailures.push({ messageId, leaseToken, errorCode });
    return true;
  }

  async claimSend(): Promise<SmsClaimedSend | null> {
    return this.allowSendClaim ? this.sends.shift() ?? null : null;
  }

  async authorizeSend(messageId: string, dispatchToken: string): Promise<boolean> {
    this.authorizations.push({ messageId, dispatchToken });
    return this.allowAuthorization;
  }

  async recordSent(
    messageId: string,
    dispatchToken: string,
    sid: string,
    status: string,
  ): Promise<boolean> {
    this.sent.push({ messageId, dispatchToken, sid, status });
    return true;
  }

  async recordSendFailure(
    messageId: string,
    dispatchToken: string,
    outcome: 'failed' | 'uncertain',
    errorCode: string,
  ): Promise<boolean> {
    this.sendFailures.push({ messageId, dispatchToken, outcome, errorCode });
    return true;
  }

  asStore(): SmsStore {
    return this as unknown as SmsStore;
  }
}

function runtimeProjection(text: string, authorityOverride: Record<string, unknown> = {}): RuntimeChatProjection {
  return {
    model_reported: {
      evidence_lane: 'model_reported_not_observed_fact',
      text,
      response_digest: RESPONSE_DIGEST,
    },
    authority: {
      effect_authority: 'NONE',
      tool_authority: 'READ_ONLY_CONTEXT',
      memory_scope: 'public_safe_retrieval',
      conversation_history: 'durable_principal_bound',
      training_consent: false,
      ...authorityOverride,
    },
  } as unknown as RuntimeChatProjection;
}

function sessionProjection(scopedRequestId: string, text: string): RuntimeSessionGetProjection {
  return {
    session: {
      workspace: { status: 'not_authorized', effect_authority: 'NONE' },
      effects: [],
      messages: [{
        role: 'assistant',
        content: text,
        request_id: scopedRequestId,
        receipt: {
          response_digest: RESPONSE_DIGEST,
          memory_scope: 'public_safe_retrieval',
          conversation_history: 'durable_principal_bound',
        },
      }],
    },
  } as unknown as RuntimeSessionGetProjection;
}

class ProviderError extends Error {
  constructor(
    readonly code: string,
    readonly ambiguous: boolean,
  ) {
    super(code);
  }
}

async function directTurnTest(): Promise<void> {
  const config = configuration();
  const store = new FakeStore();
  store.jobs.push(claimedJob(config));
  const calls: Array<Record<string, unknown>> = [];
  const providerMessages: Array<{ to: string; text: string }> = [];
  const longReply = 'A'.repeat(500);
  const deps: SmsWorkerDependencies = {
    store: store.asStore(),
    config,
    provider: {
      async send(message) {
        providerMessages.push(message);
        return { sid: OUTBOUND_SID, status: 'queued' };
      },
    },
    runtime: {
      async submit(input) {
        calls.push(input as unknown as Record<string, unknown>);
        return runtimeProjection(longReply);
      },
      async getSession() {
        throw new Error('must_not_reconcile_success');
      },
    },
  };

  const materialized = await runSmsWorkerOnce('test-worker', deps);
  assert.deepEqual(materialized, {
    state: 'not_dispatched',
    processed: 1,
    messageId: MESSAGE_ID,
    reason: 'materialized',
  });
  assert.equal(providerMessages.length, 0, 'fresh runtime turn stops at the durable outbox boundary');
  const result = await runSmsWorkerOnce('test-worker', deps);

  assert.equal(result.state, 'sent');
  assert.equal(calls.length, 1);
  const principal = publicMemberPrincipalRef(OWNER_ID);
  assert.deepEqual(calls[0], {
    message: 'Hello Apocrypha',
    conversationId: scopeConversationId(principal, SESSION_ID),
    requestId: scopeRequestId(principal, REQUEST_ID),
    sessionId: SESSION_ID,
    sessionPrincipal: principal,
    privacyPartition: 'public:apocrypha',
    credentialProfile: 'public',
  });
  assert.equal(providerMessages.length, 1, 'the next invocation makes one provider call');
  assert.equal(providerMessages[0]?.to, config.binding.ownerNumber);
  const sentText = providerMessages[0]?.text ?? '';
  assert.ok([...sentText].length <= config.policy.maxReplyChars);
  assert.ok(estimateSmsSegments(sentText) <= config.policy.maxSegments);
  assert.equal(store.readyInputs[0]?.responseDigest, RESPONSE_DIGEST);
  assert.equal(
    decryptSmsText(
      store.readyInputs[0]?.replyCiphertext ?? '',
      config.keyring,
      smsReplyAad(MESSAGE_ID),
    ),
    sentText,
  );
  assert.deepEqual(store.authorizations, [{ messageId: MESSAGE_ID, dispatchToken: DISPATCH_TOKEN }]);
  assert.deepEqual(store.sent, [{
    messageId: MESSAGE_ID,
    dispatchToken: DISPATCH_TOKEN,
    sid: OUTBOUND_SID,
    status: 'queued',
  }]);
}

async function readyFirstTest(): Promise<void> {
  const config = configuration();
  const store = new FakeStore();
  const text = 'Previously materialized reply';
  store.sends.push({
    messageId: MESSAGE_ID,
    providerMessageSid: INBOUND_SID,
    replyCiphertext: encryptSmsText(text, config.keyring, smsReplyAad(MESSAGE_ID)),
    outboundSegments: estimateSmsSegments(text),
    dispatchToken: DISPATCH_TOKEN,
  });
  let runtimeCalls = 0;
  let providerCalls = 0;
  const result = await runSmsWorkerOnce('ready-worker', {
    store: store.asStore(),
    config,
    provider: {
      async send() {
        providerCalls += 1;
        return { sid: OUTBOUND_SID, status: 'accepted' };
      },
    },
    runtime: {
      async submit() {
        runtimeCalls += 1;
        return runtimeProjection('must not run');
      },
      async getSession() {
        throw new Error('must_not_run');
      },
    },
  });
  assert.equal(result.state, 'sent');
  assert.equal(runtimeCalls, 0);
  assert.equal(providerCalls, 1);
  assert.equal(store.authorizations.length, 1);
}

async function runtimeReconciliationTest(): Promise<void> {
  const config = configuration();
  const store = new FakeStore();
  store.jobs.push({ ...claimedJob(config), reconcileOnly: true });
  const principal = publicMemberPrincipalRef(OWNER_ID);
  const scopedRequestId = scopeRequestId(principal, REQUEST_ID);
  let reconciliations = 0;
  let providerCalls = 0;
  const deps: SmsWorkerDependencies = {
    store: store.asStore(),
    config,
    provider: {
      async send() {
        providerCalls += 1;
        return { sid: OUTBOUND_SID, status: 'scheduled' };
      },
    },
    runtime: {
      async submit() {
        throw new Error('reconcile_only_must_not_submit');
      },
      async getSession(input) {
        reconciliations += 1;
        assert.equal(input.credentialProfile, 'public');
        assert.equal(input.privacyPartition, 'public:apocrypha');
        assert.equal(input.sessionId, SESSION_ID);
        return sessionProjection(scopedRequestId, 'Recovered durable response');
      },
    },
  };
  const materialized = await runSmsWorkerOnce('reconcile-worker', deps);
  assert.deepEqual(materialized, {
    state: 'not_dispatched',
    processed: 1,
    messageId: MESSAGE_ID,
    reason: 'materialized',
  });
  assert.equal(reconciliations, 1);
  assert.equal(providerCalls, 0);
  const result = await runSmsWorkerOnce('reconcile-worker', deps);
  assert.equal(result.state, 'sent');
  assert.equal(providerCalls, 1);
  assert.equal(store.runtimeFailures.length, 0);
}

async function retiredRuntimeConfigurationTest(): Promise<void> {
  const config = configuration();
  const store = new FakeStore();
  const job = claimedJob(config);
  const preservedJob = { ...job };
  store.jobs.push(job);
  let submitCalls = 0;
  let reconciliationCalls = 0;
  let providerCalls = 0;
  const result = await runSmsWorkerOnce('retired-runtime-worker', {
    store: store.asStore(),
    config,
    provider: {
      async send() {
        providerCalls += 1;
        return { sid: OUTBOUND_SID, status: 'queued' };
      },
    },
    runtime: {
      async submit(input) {
        submitCalls += 1;
        const principal = publicMemberPrincipalRef(OWNER_ID);
        assert.equal(input.requestId, scopeRequestId(principal, REQUEST_ID));
        assert.equal(input.conversationId, scopeConversationId(principal, SESSION_ID));
        assert.equal(input.privacyPartition, 'public:apocrypha');
        assert.equal(input.credentialProfile, 'public');
        throw new RuntimeProxyError('web_runtime_retired', 404);
      },
      async getSession() {
        reconciliationCalls += 1;
        throw new Error('retired_runtime_must_not_reconcile');
      },
    },
  });
  assert.deepEqual(result, {
    state: 'failed', processed: 1, messageId: MESSAGE_ID, errorCode: 'web_runtime_retired',
  });
  assert.deepEqual(store.runtimeFailures, [{
    messageId: MESSAGE_ID, leaseToken: LEASE_TOKEN, errorCode: 'web_runtime_retired',
  }]);
  assert.equal(submitCalls, 1);
  assert.equal(reconciliationCalls, 0);
  assert.equal(providerCalls, 0);
  assert.deepEqual(job, preservedJob, 'failure preserves request identity and encrypted inbound message');
  assert.deepEqual(store.readyInputs, []);
  assert.deepEqual(store.sends, []);
  assert.deepEqual(store.authorizations, []);
  assert.deepEqual(store.sent, []);
  assert.deepEqual(store.sendFailures, []);
}

async function freshAmbiguousRuntimeTest(): Promise<void> {
  const config = configuration();
  const store = new FakeStore();
  store.jobs.push(claimedJob(config));
  let providerCalls = 0;
  let reconciliationCalls = 0;
  const result = await runSmsWorkerOnce('ambiguous-runtime-worker', {
    store: store.asStore(),
    config,
    provider: {
      async send() {
        providerCalls += 1;
        return { sid: OUTBOUND_SID, status: 'queued' };
      },
    },
    runtime: {
      async submit() {
        throw new Error('runtime_deadline_exceeded');
      },
      async getSession() {
        reconciliationCalls += 1;
        return sessionProjection('55555555-5555-5555-8555-555555555555', 'Other turn');
      },
    },
  });
  assert.deepEqual(result, {
    state: 'uncertain',
    processed: 1,
    messageId: MESSAGE_ID,
    errorCode: 'sms_runtime_outcome_unknown',
  });
  assert.equal(providerCalls, 0);
  assert.equal(reconciliationCalls, 0, 'fresh timeout defers reconciliation to a later lease');
  assert.deepEqual(store.runtimeFailures, [], 'ambiguous fresh turn stays processing');
}

async function staleReconciliationSuccessTest(): Promise<void> {
  const config = configuration();
  const store = new FakeStore();
  store.jobs.push({
    ...claimedJob(config),
    reconcileOnly: true,
    bodyCiphertext: 'intentionally-not-decryptable-on-reconcile-only-path',
  });
  const principal = publicMemberPrincipalRef(OWNER_ID);
  const scopedRequestId = scopeRequestId(principal, REQUEST_ID);
  let submitCalls = 0;
  let providerCalls = 0;
  const deps: SmsWorkerDependencies = {
    store: store.asStore(),
    config,
    provider: {
      async send() {
        providerCalls += 1;
        return { sid: OUTBOUND_SID, status: 'queued' };
      },
    },
    runtime: {
      async submit() {
        submitCalls += 1;
        throw new Error('stale_path_must_never_submit');
      },
      async getSession() {
        return sessionProjection(scopedRequestId, 'Recovered on stale lease');
      },
    },
  };
  const materialized = await runSmsWorkerOnce('stale-reconcile-worker', deps);
  assert.equal(materialized.state, 'not_dispatched');
  if (materialized.state === 'not_dispatched') assert.equal(materialized.reason, 'materialized');
  assert.equal(submitCalls, 0);
  assert.equal(providerCalls, 0);
  const result = await runSmsWorkerOnce('stale-reconcile-worker', deps);
  assert.equal(result.state, 'sent');
  assert.equal(providerCalls, 1);
  assert.equal(store.runtimeFailures.length, 0);
}

async function staleReconciliationMissTest(): Promise<void> {
  const config = configuration();
  const store = new FakeStore();
  store.jobs.push({ ...claimedJob(config), reconcileOnly: true });
  let submitCalls = 0;
  let providerCalls = 0;
  const result = await runSmsWorkerOnce('stale-miss-worker', {
    store: store.asStore(),
    config,
    provider: {
      async send() {
        providerCalls += 1;
        return { sid: OUTBOUND_SID, status: 'queued' };
      },
    },
    runtime: {
      async submit() {
        submitCalls += 1;
        throw new Error('stale_path_must_never_submit');
      },
      async getSession() {
        return sessionProjection('55555555-5555-5555-8555-555555555555', 'Other turn');
      },
    },
  });
  assert.deepEqual(result, {
    state: 'uncertain',
    processed: 1,
    messageId: MESSAGE_ID,
    errorCode: 'sms_runtime_outcome_unknown',
  });
  assert.equal(submitCalls, 0);
  assert.equal(providerCalls, 0);
  assert.deepEqual(store.runtimeFailures, [], 'stale reconciliation miss remains processing');
}

async function maliciousAuthorityTest(): Promise<void> {
  const config = configuration();
  const store = new FakeStore();
  store.jobs.push(claimedJob(config));
  let providerCalls = 0;
  const result = await runSmsWorkerOnce('authority-worker', {
    store: store.asStore(),
    config,
    provider: {
      async send() {
        providerCalls += 1;
        return { sid: OUTBOUND_SID, status: 'queued' };
      },
    },
    runtime: {
      async submit() {
        return runtimeProjection('Unsafe response', { effect_authority: 'WRITE' });
      },
      async getSession() {
        throw new Error('no_recovery');
      },
    },
  });
  assert.equal(result.state, 'failed');
  assert.equal(providerCalls, 0);
  assert.equal(store.runtimeFailures[0]?.errorCode, 'sms_runtime_authority_invalid');
}

async function bindingMismatchTest(): Promise<void> {
  const config = configuration();
  const store = new FakeStore();
  store.jobs.push({ ...claimedJob(config), phoneHash: 'e'.repeat(64) });
  let runtimeCalls = 0;
  let providerCalls = 0;
  const result = await runSmsWorkerOnce('binding-worker', {
    store: store.asStore(),
    config,
    provider: {
      async send() {
        providerCalls += 1;
        return { sid: OUTBOUND_SID, status: 'queued' };
      },
    },
    runtime: {
      async submit() {
        runtimeCalls += 1;
        return runtimeProjection('must_not_run');
      },
      async getSession() {
        throw new Error('must_not_run');
      },
    },
  });
  assert.equal(result.state, 'failed');
  assert.equal(runtimeCalls, 0);
  assert.equal(providerCalls, 0);
  assert.equal(store.runtimeFailures[0]?.errorCode, 'sms_binding_mismatch');
}

async function consentBudgetGateTest(): Promise<void> {
  const config = configuration();
  const store = new FakeStore();
  const text = 'Reply invalidated by STOP before final authorization';
  store.sends.push({
    messageId: MESSAGE_ID,
    providerMessageSid: INBOUND_SID,
    replyCiphertext: encryptSmsText(text, config.keyring, smsReplyAad(MESSAGE_ID)),
    outboundSegments: estimateSmsSegments(text),
    dispatchToken: DISPATCH_TOKEN,
  });
  store.allowAuthorization = false;
  let providerCalls = 0;
  const result = await runSmsWorkerOnce('gate-worker', {
    store: store.asStore(),
    config,
    provider: {
      async send() {
        providerCalls += 1;
        return { sid: OUTBOUND_SID, status: 'queued' };
      },
    },
    runtime: {
      async submit() {
        throw new Error('ready_send_must_not_submit');
      },
      async getSession() {
        throw new Error('must_not_reconcile');
      },
    },
  });
  assert.deepEqual(result, {
    state: 'not_dispatched',
    processed: 1,
    messageId: MESSAGE_ID,
    reason: 'consent_or_budget_gate',
  });
  assert.equal(providerCalls, 0);
  assert.deepEqual(store.authorizations, [{ messageId: MESSAGE_ID, dispatchToken: DISPATCH_TOKEN }]);
}

async function providerUncertaintyTest(): Promise<void> {
  const config = configuration();
  const store = new FakeStore();
  const text = 'One attempt only';
  store.sends.push({
    messageId: MESSAGE_ID,
    providerMessageSid: INBOUND_SID,
    replyCiphertext: encryptSmsText(text, config.keyring, smsReplyAad(MESSAGE_ID)),
    outboundSegments: estimateSmsSegments(text),
    dispatchToken: DISPATCH_TOKEN,
  });
  let attempts = 0;
  const provider: SmsProviderSender = {
    async send() {
      attempts += 1;
      throw new ProviderError('provider_timeout', true);
    },
  };
  const deps = {
    store: store.asStore(),
    config,
    provider,
    runtime: {
      async submit() { return runtimeProjection('unused'); },
      async getSession() { throw new Error('unused'); },
    },
  };
  const first = await runSmsWorkerOnce('uncertain-worker', deps);
  const second = await runSmsWorkerOnce('uncertain-worker', deps);
  assert.equal(first.state, 'uncertain');
  assert.equal(second.state, 'idle');
  assert.equal(attempts, 1, 'ambiguous provider outcome is never blindly retried');
  assert.deepEqual(store.sendFailures, [{
    messageId: MESSAGE_ID,
    dispatchToken: DISPATCH_TOKEN,
    outcome: 'uncertain',
    errorCode: 'provider_timeout',
  }]);
}

interface HttpOutput {
  status: number;
  body: Record<string, unknown> | null;
  headers: Record<string, string>;
}

function reqRes(method = 'GET'): {
  req: NextApiRequest;
  res: NextApiResponse;
  output: HttpOutput;
} {
  const output: HttpOutput = { status: 0, body: null, headers: {} };
  const req = { method, headers: {}, query: {} } as unknown as NextApiRequest;
  const res = {
    setHeader(name: string, value: string | number | readonly string[]) {
      output.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
    status(value: number) {
      output.status = value;
      return this;
    },
    json(value: Record<string, unknown>) {
      output.body = value;
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, output };
}

async function cronBoundaryTest(): Promise<void> {
  let stores = 0;
  let runs = 0;
  const disabled = createApocryphaSmsCronHandler({
    authorize: () => ({ ok: true, via: 'bearer', reason: null }),
    readConfiguration: () => ({
      configured: false,
      config: null,
      missing: ['TWILIO_ACCOUNT_SID'],
    }),
    createStore: () => {
      stores += 1;
      return new FakeStore().asStore();
    },
    run: async () => {
      runs += 1;
      return { state: 'idle', processed: 0 };
    },
    audit: async () => undefined,
  });
  const disabledHttp = reqRes();
  await disabled(disabledHttp.req, disabledHttp.res);
  assert.equal(disabledHttp.output.status, 503);
  assert.equal(disabledHttp.output.body?.state, 'disabled');
  assert.equal(disabledHttp.output.body?.ok, false);
  assert.equal(stores, 0);
  assert.equal(runs, 0);
  assert.match(disabledHttp.output.headers['cache-control'] ?? '', /no-store/);

  const unauthorized = createApocryphaSmsCronHandler({
    authorize: () => ({ ok: false, via: 'none', reason: 'missing' }),
    readConfiguration: () => {
      throw new Error('auth_must_precede_configuration');
    },
    audit: async () => undefined,
  });
  const unauthorizedHttp = reqRes();
  await unauthorized(unauthorizedHttp.req, unauthorizedHttp.res);
  assert.equal(unauthorizedHttp.output.status, 401);
  assert.equal(unauthorizedHttp.output.body?.error, 'unauthorized');
}

async function main(): Promise<void> {
  await directTurnTest();
  await readyFirstTest();
  await runtimeReconciliationTest();
  await retiredRuntimeConfigurationTest();
  await freshAmbiguousRuntimeTest();
  await staleReconciliationSuccessTest();
  await staleReconciliationMissTest();
  await maliciousAuthorityTest();
  await bindingMismatchTest();
  await consentBudgetGateTest();
  await providerUncertaintyTest();
  await cronBoundaryTest();
  console.log('apocrypha-sms-worker.test : OK · 12 contracts passed');
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

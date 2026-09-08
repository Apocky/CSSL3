import { createHmac } from 'node:crypto';
import { Readable } from 'node:stream';
import type { NextApiRequest, NextApiResponse } from 'next';

import {
  decryptSmsText,
  type SmsCommand,
} from '@/lib/apocrypha/sms/core';
import type {
  SmsConfigurationState,
  SmsSystemConfiguration,
} from '@/lib/apocrypha/sms/config';
import {
  SmsStoreUnavailableError,
  createSmsStoreForRpcClient,
  smsInboundAad,
  type SmsIngressRecord,
  type SmsIngressResult,
  type SmsProviderStatus,
  type SmsRpcClient,
  type SmsStore,
} from '@/lib/apocrypha/sms/store';
import { twimlResponse } from '@/lib/apocrypha/sms/twilio';
import { createInboundSmsHandler } from '@/pages/api/apocrypha/sms/inbound';
import { createStatusSmsHandler } from '@/pages/api/apocrypha/sms/status';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assertion failed: ${message}`);
}

const ACCOUNT_SID = `AC${'1'.repeat(32)}`;
const INBOUND_SID = `SM${'2'.repeat(32)}`;
const OUTBOUND_SID = `SM${'3'.repeat(32)}`;
const MESSAGE_ID = '44444444-4444-4444-8444-444444444444';
const REQUEST_ID = '55555555-5555-4555-8555-555555555555';
const SESSION_ID = '66666666-6666-4666-8666-666666666666';
const OWNER_ID = '77777777-7777-4777-8777-777777777777';
const LEASE_TOKEN = '88888888-8888-4888-8888-888888888888';
const OWNER_NUMBER = '+14805550100';
const SMS_NUMBER = '+14805550200';
const WEBHOOK_URL = 'https://apocky.com/api/apocrypha/sms/inbound';
const STATUS_URL = 'https://apocky.com/api/apocrypha/sms/status';
const AUTH_TOKEN = 'test-auth-token-with-enough-entropy';

const envelopeKey = Buffer.alloc(32, 7);
const bindingKey = Buffer.alloc(32, 9);
const system: SmsSystemConfiguration = {
  provider: {
    provider: 'twilio',
    accountSid: ACCOUNT_SID,
    authToken: AUTH_TOKEN,
    apiKeySid: `SK${'8'.repeat(32)}`,
    apiKeySecret: 'api-key-secret',
    webhookUrl: WEBHOOK_URL,
    statusCallbackUrl: STATUS_URL,
    smsNumber: SMS_NUMBER,
  },
  binding: {
    ownerNumber: OWNER_NUMBER,
    ownerUserId: OWNER_ID,
    sessionId: SESSION_ID,
  },
  bindingKey,
  keyring: {
    activeKeyId: 'current',
    storageKey: envelopeKey,
    decryptionKeys: { current: envelopeKey },
  },
  policy: {
    maxReplyChars: 320,
    maxSegments: 3,
    dailySegmentBudget: 30,
  },
};

const configured: SmsConfigurationState = { configured: true, config: system, missing: [] };

interface ResponseState {
  statusCode: number;
  body: unknown;
  headers: Record<string, string | string[]>;
  ended: boolean;
}

function response(): { res: NextApiResponse; out: ResponseState } {
  const out: ResponseState = { statusCode: 0, body: undefined, headers: {}, ended: false };
  const res = {
    setHeader(name: string, value: number | string | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? [...value] : String(value);
      return res;
    },
    status(code: number) {
      out.statusCode = code;
      return res;
    },
    send(body: unknown) {
      out.body = body;
      out.ended = true;
      return res;
    },
    end(body?: unknown) {
      if (body !== undefined) out.body = body;
      out.ended = true;
      return res;
    },
  } as unknown as NextApiResponse;
  return { res, out };
}

function request(
  method: string,
  rawBody: string,
  headers: Record<string, string> = {},
): NextApiRequest {
  const stream = Readable.from([Buffer.from(rawBody, 'utf8')]);
  return Object.assign(stream, {
    method,
    headers: {
      'content-type': 'application/x-www-form-urlencoded',
      'content-length': String(Buffer.byteLength(rawBody)),
      ...headers,
    },
    query: {},
    cookies: {},
  }) as unknown as NextApiRequest;
}

function signature(url: string, params: URLSearchParams): string {
  const sorted = [...params.entries()].sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0);
  const payload = sorted.reduce((value, [name, item]) => `${value}${name}${item}`, url);
  return createHmac('sha1', AUTH_TOKEN).update(payload, 'utf8').digest('base64');
}

function inboundParams(overrides: Record<string, string> = {}): URLSearchParams {
  return new URLSearchParams({
    AccountSid: ACCOUNT_SID,
    MessageSid: INBOUND_SID,
    From: OWNER_NUMBER,
    To: SMS_NUMBER,
    Body: 'Hello, Apocrypha.',
    NumMedia: '0',
    ...overrides,
  });
}

function statusParams(overrides: Record<string, string> = {}): URLSearchParams {
  return new URLSearchParams({
    AccountSid: ACCOUNT_SID,
    MessageSid: OUTBOUND_SID,
    From: SMS_NUMBER,
    To: OWNER_NUMBER,
    MessageStatus: 'delivered',
    ...overrides,
  });
}

const defaultIngressResult: SmsIngressResult = {
  messageId: MESSAGE_ID,
  action: 'queued',
  messageStatus: 'queued',
  channelConsentState: 'active',
  duplicate: false,
};

function fakeStore(overrides: Partial<SmsStore> = {}): SmsStore {
  return {
    ingest: async () => defaultIngressResult,
    claimJob: async () => null,
    markJobReady: async () => true,
    markRuntimeFailed: async () => true,
    claimSend: async () => null,
    authorizeSend: async () => true,
    recordSent: async () => true,
    recordSendFailure: async () => true,
    recordDelivery: async () => true,
    ...overrides,
  };
}

async function invokeInbound(
  params: URLSearchParams,
  options: {
    store?: SmsStore;
    storeFactory?: () => SmsStore;
    headers?: Record<string, string>;
    method?: string;
    state?: SmsConfigurationState;
    rawBody?: string;
  } = {},
): Promise<ResponseState> {
  const rawBody = options.rawBody ?? params.toString();
  const signed = signature(WEBHOOK_URL, params);
  const handler = createInboundSmsHandler({
    readConfiguration: () => options.state ?? configured,
    createStore: options.storeFactory ?? (() => options.store ?? fakeStore()),
    requestId: () => REQUEST_ID,
  });
  const { res, out } = response();
  await handler(request(options.method ?? 'POST', rawBody, {
    'x-twilio-signature': signed,
    ...options.headers,
  }), res);
  return out;
}

async function testSignedIngressPersistsEncryptedEnvelope(): Promise<void> {
  let captured: SmsIngressRecord | null = null;
  const params = inboundParams({ FutureProviderField: 'preserved-for-signature' });
  const out = await invokeInbound(params, {
    headers: { 'i-twilio-idempotency-token': 'retry-token' },
    store: fakeStore({
      ingest: async (record) => {
        captured = record;
        return defaultIngressResult;
      },
    }),
  });
  assert(out.statusCode === 200, 'valid ingress returns 200 after persistence');
  assert(out.body === twimlResponse(), 'queued ingress returns empty TwiML');
  assert(out.headers['cache-control'] === 'no-store, max-age=0', 'webhook response is not cached');
  assert(captured !== null, 'ingress reaches durable seam');
  const admitted = captured as SmsIngressRecord;
  assert(admitted.providerAccountSid === ACCOUNT_SID, 'configured provider account is persisted');
  assert(admitted.providerMessageSid === INBOUND_SID, 'provider identity is persisted');
  assert(admitted.commandKind === 'message', 'ordinary body is a model message');
  assert(admitted.mediaCount === 0, 'media count is preserved');
  assert(!admitted.bodyCiphertext.includes('Hello'), 'plaintext is absent from persistence input');
  assert(admitted.bodyCiphertext.startsWith('v2.current.'), 'new ciphertext carries rotation key id');
  assert(admitted.phoneHash.length === 64 && !admitted.phoneHash.includes(OWNER_NUMBER), 'phone binding is one-way');
  assert(admitted.providerRetryTokenHash?.length === 64, 'retry token is diagnostic hash only');
  const clear = decryptSmsText(
    admitted.bodyCiphertext,
    system.keyring,
    smsInboundAad(ACCOUNT_SID, INBOUND_SID, REQUEST_ID),
  );
  assert(
    clear === JSON.stringify({ schema: 'apocrypha.sms-inbound.v1', body: 'Hello, Apocrypha.' }),
    'minimal canonical envelope preserves exact Body',
  );
}

async function testUnknownSenderIsSilentAndStorageFree(): Promise<void> {
  let factoryCalls = 0;
  const out = await invokeInbound(inboundParams({ From: '+14805550999' }), {
    storeFactory: () => {
      factoryCalls += 1;
      return fakeStore();
    },
  });
  assert(out.statusCode === 200, 'valid signed unknown sender receives neutral 200');
  assert(out.body === twimlResponse(), 'unknown sender receives empty TwiML');
  assert(factoryCalls === 0, 'unknown sender never opens durable store');
}

async function testStopPrecedesMediaAndProviderReplyIsNotDuplicated(): Promise<void> {
  let observedCommand: SmsCommand | null = null;
  let observedMedia = -1;
  const params = inboundParams({ Body: '', NumMedia: '1', OptOutType: 'STOP' });
  const out = await invokeInbound(params, {
    store: fakeStore({
      ingest: async (record) => {
        observedCommand = record.commandKind;
        observedMedia = record.mediaCount;
        return {
          ...defaultIngressResult,
          action: 'stop',
          messageStatus: 'command_processed',
          channelConsentState: 'revoked',
        };
      },
    }),
  });
  assert(observedCommand === 'stop', 'provider STOP is classified before media rejection');
  assert(observedMedia === 1, 'media metadata remains available to durable policy');
  assert(out.statusCode === 200 && out.body === twimlResponse(), 'Advanced Opt-Out gets no duplicate reply');
}

async function testConsentDisclosureAndInputRejections(): Promise<void> {
  const consentOut = await invokeInbound(inboundParams(), {
    store: fakeStore({
      ingest: async () => ({
        ...defaultIngressResult,
        action: 'consent_required',
        messageStatus: 'consent_required',
        channelConsentState: 'pending',
      }),
    }),
  });
  const consentBody = String(consentOut.body);
  assert(consentBody.includes('Connect this number to one dedicated Apocrypha SMS conversation.'), 'consent response has complete disclosure start');
  assert(consentBody.includes('Carrier message and data rates may apply. Text STOP at any time to revoke.'), 'consent response has complete disclosure end');
  assert(consentBody.includes('CONSENT APOCRYPHA'), 'consent response gives exact acceptance phrase');

  const limitedOut = await invokeInbound(inboundParams(), {
    store: fakeStore({
      ingest: async () => ({
        ...defaultIngressResult,
        action: 'rate_limited',
        messageStatus: 'rate_limited',
      }),
    }),
  });
  assert(limitedOut.body === twimlResponse(), 'rate limiting cannot create an unmetered reply loop');

  const consentRetry = await invokeInbound(inboundParams(), {
    store: fakeStore({
      ingest: async () => ({
        ...defaultIngressResult,
        action: 'duplicate',
        messageStatus: 'consent_required',
        channelConsentState: 'pending',
        duplicate: true,
      }),
    }),
  });
  assert(String(consentRetry.body).includes('CONSENT APOCRYPHA'), 'deduplicated consent-required retry re-renders lost disclosure');

  const invalidSignature = await invokeInbound(inboundParams(), {
    headers: { 'x-twilio-signature': 'invalid' },
  });
  assert(invalidSignature.statusCode === 403, 'invalid provider signature is forbidden');

  const duplicateRaw = `AccountSid=${ACCOUNT_SID}&AccountSid=${ACCOUNT_SID}`;
  const duplicate = await invokeInbound(new URLSearchParams({ AccountSid: ACCOUNT_SID }), { rawBody: duplicateRaw });
  assert(duplicate.statusCode === 400, 'duplicate form names are rejected before field access');

  const wrongType = await invokeInbound(inboundParams(), { headers: { 'content-type': 'application/json' } });
  assert(wrongType.statusCode === 415, 'non-form content type is rejected');

  const oversized = await invokeInbound(inboundParams(), { headers: { 'content-length': String(32 * 1024 + 1) } });
  assert(oversized.statusCode === 413, 'declared oversized body is rejected before buffering');

  const blank = await invokeInbound(inboundParams({ Body: '   ' }));
  assert(blank.statusCode === 400, 'blank text-only model message is rejected');

  const tooManyCodePoints = await invokeInbound(inboundParams({ Body: '🜁'.repeat(1_601) }));
  assert(tooManyCodePoints.statusCode === 400, 'Body cap counts Unicode code points, not UTF-16 units');

  const unavailableState: SmsConfigurationState = { configured: false, config: null, missing: ['TWILIO_AUTH_TOKEN'] };
  const unavailable = await invokeInbound(inboundParams(), { state: unavailableState });
  assert(unavailable.statusCode === 503, 'unconfigured provider fails truthfully');

  const storeFailure = await invokeInbound(inboundParams(), {
    storeFactory: () => { throw new SmsStoreUnavailableError(); },
  });
  assert(storeFailure.statusCode === 503, 'persistence failure is retryable non-success');
}

async function testSignedStatusCallbackIsDurable(): Promise<void> {
  let observed: [string, SmsProviderStatus, string | null] | null = null;
  const store = fakeStore({
    recordDelivery: async (sid, status, errorCode) => {
      observed = [sid, status, errorCode];
      return true;
    },
  });
  const params = statusParams({ MessageStatus: 'undelivered', ErrorCode: '30007' });
  const raw = params.toString();
  const handler = createStatusSmsHandler({
    readConfiguration: () => configured,
    createStore: () => store,
  });
  const { res, out } = response();
  await handler(request('POST', raw, { 'x-twilio-signature': signature(STATUS_URL, params) }), res);
  assert(out.statusCode === 204 && out.ended, 'persisted status callback returns 204');
  assert(
    observed !== null
      && observed[0] === OUTBOUND_SID
      && observed[1] === 'undelivered'
      && observed[2] === '30007',
    'delivery identity, semantic status, and provider error are persisted',
  );

  const mismatch = statusParams({ To: '+14805550999' });
  const mismatchResponse = response();
  await handler(request('POST', mismatch.toString(), {
    'x-twilio-signature': signature(STATUS_URL, mismatch),
  }), mismatchResponse.res);
  assert(mismatchResponse.out.statusCode === 403, 'signed callback for a different binding is forbidden');

  let invalidSignatureStoreCalls = 0;
  const invalidSignatureHandler = createStatusSmsHandler({
    readConfiguration: () => configured,
    createStore: () => {
      invalidSignatureStoreCalls += 1;
      return store;
    },
  });
  const invalidSignatureResponse = response();
  await invalidSignatureHandler(request('POST', raw, {
    'x-twilio-signature': 'invalid',
  }), invalidSignatureResponse.res);
  assert(invalidSignatureResponse.out.statusCode === 403, 'unsigned delivery callback is forbidden');
  assert(invalidSignatureStoreCalls === 0, 'invalid delivery signature never opens persistence');

  const failedStoreHandler = createStatusSmsHandler({
    readConfiguration: () => configured,
    createStore: () => fakeStore({ recordDelivery: async () => { throw new Error('down'); } }),
  });
  const failed = response();
  await failedStoreHandler(request('POST', raw, {
    'x-twilio-signature': signature(STATUS_URL, params),
  }), failed.res);
  assert(failed.out.statusCode === 503, 'status persistence failure remains retryable');
}

async function testRpcAdapterMappingAndStrictRows(): Promise<void> {
  let observedName = '';
  let observedParameters: Record<string, unknown> = {};
  const rpcClient: SmsRpcClient = {
    async rpc(name, parameters) {
      observedName = name;
      observedParameters = parameters;
      return {
        data: [{
          message_id: MESSAGE_ID,
          action: 'queued',
          message_status: 'queued',
          channel_consent_state: 'active',
          duplicate: false,
        }],
        error: null,
      };
    },
  };
  const store = createSmsStoreForRpcClient(rpcClient);
  const record: SmsIngressRecord = {
    providerAccountSid: ACCOUNT_SID,
    providerMessageSid: INBOUND_SID,
    providerRetryTokenHash: null,
    phoneHash: 'a'.repeat(64),
    sessionId: SESSION_ID,
    requestId: REQUEST_ID,
    bodyCiphertext: `v2.current.${'a'.repeat(16)}.${'b'.repeat(16)}.${'c'.repeat(22)}`,
    commandKind: 'message',
    mediaCount: 0,
    consentDisclosureSha256: 'b'.repeat(64),
  };
  const result = await store.ingest(record);
  assert(result.messageId === MESSAGE_ID, 'strict RPC row maps to domain result');
  assert(observedName === 'ingest_apocrypha_sms_message', 'adapter calls only named ingest RPC');
  assert(observedParameters.p_provider_account_sid === ACCOUNT_SID, 'account identity reaches hardened RPC');
  assert(observedParameters.p_media_count === 0, 'media policy input reaches hardened RPC');

  const claimStore = createSmsStoreForRpcClient({
    async rpc(name) {
      assert(name === 'claim_apocrypha_sms_job', 'claim uses only named job RPC');
      return {
        data: [{
          message_id: MESSAGE_ID,
          provider_message_sid: INBOUND_SID,
          phone_hash: 'a'.repeat(64),
          session_id: SESSION_ID,
          request_id: REQUEST_ID,
          body_ciphertext: record.bodyCiphertext,
          lease_token: LEASE_TOKEN,
          reconcile_only: true,
        }],
        error: null,
      };
    },
  });
  const claimed = await claimStore.claimJob('sms-worker:test');
  assert(claimed?.reconcileOnly === true, 'stale processing recovery flag survives strict RPC mapping');
  assert(claimed?.leaseToken === LEASE_TOKEN, 'processing completion is fenced by the returned lease token');

  const dispatchToken = '99999999-9999-4999-8999-999999999999';
  const sendRpcNames: string[] = [];
  const sendStore = createSmsStoreForRpcClient({
    async rpc(name, parameters) {
      sendRpcNames.push(name);
      if (name === 'claim_apocrypha_sms_send') {
        assert(parameters.p_daily_segment_budget === 30, 'budget reaches atomic send claim');
        return {
          data: [{
            message_id: MESSAGE_ID,
            provider_message_sid: INBOUND_SID,
            reply_ciphertext: record.bodyCiphertext,
            outbound_segments: 1,
            dispatch_token: dispatchToken,
          }],
          error: null,
        };
      }
      if (name === 'authorize_apocrypha_sms_send') {
        assert(parameters.p_dispatch_token === dispatchToken, 'final consent check is dispatch-token fenced');
        return { data: true, error: null };
      }
      if (name === 'record_apocrypha_sms_sent') {
        assert(parameters.p_dispatch_token === dispatchToken, 'provider receipt is dispatch-token fenced');
        return { data: true, error: null };
      }
      throw new Error('unexpected_rpc');
    },
  });
  const sendClaim = await sendStore.claimSend('sms-worker:test', 30);
  assert(sendClaim?.dispatchToken === dispatchToken, 'send claim returns an immutable dispatch fence');
  assert(await sendStore.authorizeSend(MESSAGE_ID, dispatchToken), 'final consent-generation gate admits current token');
  assert(
    await sendStore.recordSent(MESSAGE_ID, dispatchToken, OUTBOUND_SID, 'queued'),
    'provider receipt binds through the same dispatch fence',
  );
  assert(
    sendRpcNames.join(',') === 'claim_apocrypha_sms_send,authorize_apocrypha_sms_send,record_apocrypha_sms_sent',
    'send path uses only the three hardened RPCs in order',
  );

  const malformed = createSmsStoreForRpcClient({
    async rpc() {
      return {
        data: [{
          message_id: MESSAGE_ID,
          action: 'queued',
          message_status: 'queued',
          channel_consent_state: 'active',
          duplicate: false,
          unexpected: 'schema-drift',
        }],
        error: null,
      };
    },
  });
  let rejected = false;
  try {
    await malformed.ingest(record);
  } catch (error) {
    rejected = error instanceof SmsStoreUnavailableError;
  }
  assert(rejected, 'unexpected RPC row shape fails closed with sanitized error');
}

async function run(): Promise<void> {
  await testSignedIngressPersistsEncryptedEnvelope();
  await testUnknownSenderIsSilentAndStorageFree();
  await testStopPrecedesMediaAndProviderReplyIsNotDuplicated();
  await testConsentDisclosureAndInputRejections();
  await testSignedStatusCallbackIsDurable();
  await testRpcAdapterMappingAndStrictRows();
  // eslint-disable-next-line no-console
  console.log('apocrypha-sms-ingress.test : OK · signed bounded ingress/status + encrypted durable RPC seam');
}

run().catch((error) => {
  // eslint-disable-next-line no-console
  console.error(error);
  process.exit(1);
});

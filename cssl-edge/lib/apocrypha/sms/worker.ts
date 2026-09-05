// Server-only Direct SMS worker.
//
// The durable store is the scheduling and consent authority. This module owns
// one bounded execution cycle: either dispatch one already-ready reply, or
// process one queued inbound turn and make at most one provider send attempt.

import { createHash, timingSafeEqual } from 'node:crypto';

import {
  getRuntimeSession,
  publicMemberPrincipalRef,
  RuntimeProxyError,
  submitRuntimeChat,
  type RuntimeChatProjection,
  type RuntimeSessionGetProjection,
} from '@/lib/apocv4/runtime-proxy';
import { scopeConversationId, scopeRequestId } from '@/lib/apocrypha/proxy';
import { buildCreationLedgerRecord } from '@/lib/telemetry/creation-ledger';
import { emitOperationalTelemetry, type ServerTraceContext } from '@/lib/telemetry/server';

import type { SmsSystemConfiguration } from './config';
import {
  decryptSmsText,
  encryptSmsText,
  estimateSmsSegments,
  formatSmsReply,
  phoneBindingHash,
} from './core';
import {
  smsInboundAad,
  type SmsClaimedJob,
  type SmsClaimedSend,
  type SmsStore,
} from './store';

const PUBLIC_PRIVACY_PARTITION = 'public:apocrypha';
const SMS_INBOUND_SCHEMA = 'apocrypha.sms-inbound.v1';
const SHA256_RE = /^[0-9a-f]{64}$/;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const SAFE_ERROR_CODE_RE = /^[a-z0-9_.:-]{1,96}$/;

export interface SmsProviderSendInput {
  to: string;
  text: string;
}

export interface SmsProviderSendReceipt {
  sid: string;
  status: 'accepted' | 'scheduled' | 'queued';
}

/**
 * Provider adapters must set `ambiguous` when the provider may have accepted
 * the message. Unknown thrown values are treated as ambiguous by default.
 */
export interface SmsProviderSender {
  send(message: SmsProviderSendInput): Promise<SmsProviderSendReceipt>;
}

interface SmsRuntime {
  submit(input: Parameters<typeof submitRuntimeChat>[0]): Promise<RuntimeChatProjection>;
  getSession(input: Parameters<typeof getRuntimeSession>[0]): Promise<RuntimeSessionGetProjection>;
}

export interface SmsWorkerDependencies {
  store: SmsStore;
  config: SmsSystemConfiguration;
  provider: SmsProviderSender;
  runtime?: SmsRuntime;
}

export type SmsWorkerResult =
  | { state: 'idle'; processed: 0 }
  | { state: 'sent'; processed: 1; messageId: string; providerStatus: SmsProviderSendReceipt['status'] }
  | { state: 'failed'; processed: 1; messageId: string; errorCode: string }
  | { state: 'uncertain'; processed: 1; messageId: string; errorCode: string }
  | {
      state: 'not_dispatched';
      processed: 1;
      messageId: string;
      reason: 'materialized' | 'consent_or_budget_gate' | 'state_changed';
    };

interface RuntimeReply {
  text: string;
  responseDigest: string;
  inputText?: string;
}

type RuntimeReplyOutcome =
  | { state: 'reply'; reply: RuntimeReply }
  | { state: 'uncertain' };

interface SmsInboundEnvelope {
  schema: typeof SMS_INBOUND_SCHEMA;
  body: string;
}

interface ProviderFailure {
  ambiguous: boolean;
  code: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function parseInboundEnvelope(value: string): SmsInboundEnvelope {
  let parsed: unknown;
  try {
    parsed = JSON.parse(value) as unknown;
  } catch {
    throw new Error('sms_inbound_envelope_invalid');
  }
  if (
    !isRecord(parsed)
    || Object.keys(parsed).length !== 2
    || parsed.schema !== SMS_INBOUND_SCHEMA
    || typeof parsed.body !== 'string'
    || Buffer.byteLength(parsed.body.trim(), 'utf8') < 1
    || Buffer.byteLength(parsed.body, 'utf8') > 6_400
    || [...parsed.body].length > 1_600
  ) {
    throw new Error('sms_inbound_envelope_invalid');
  }
  const canonical = JSON.stringify({ schema: SMS_INBOUND_SCHEMA, body: parsed.body });
  if (canonical !== value) throw new Error('sms_inbound_envelope_invalid');
  return { schema: SMS_INBOUND_SCHEMA, body: parsed.body };
}

export function smsReplyAad(messageId: string): string {
  if (!UUID_RE.test(messageId)) throw new TypeError('sms_message_id_invalid');
  return `APOCRYPHA-SMS-REPLY-v1\0${messageId.toLowerCase()}`;
}

function safeErrorCode(error: unknown, fallback: string): string {
  if (isRecord(error) && typeof error.code === 'string' && SAFE_ERROR_CODE_RE.test(error.code)) {
    return error.code;
  }
  if (error instanceof Error && SAFE_ERROR_CODE_RE.test(error.message)) return error.message;
  return fallback;
}

function codedError(code: string): Error & { code: string } {
  return Object.assign(new Error(code), { code });
}

function providerFailure(error: unknown): ProviderFailure | null {
  if (
    error instanceof Error
    && isRecord(error)
    && typeof error.ambiguous === 'boolean'
    && typeof error.code === 'string'
    && SAFE_ERROR_CODE_RE.test(error.code)
  ) {
    return { ambiguous: error.ambiguous, code: error.code };
  }
  return null;
}

function validRuntimeReply(projection: RuntimeChatProjection, inputText?: string): RuntimeReply | null {
  const digest = projection.model_reported.response_digest;
  const text = projection.model_reported.text;
  if (
    projection.authority.effect_authority !== 'NONE'
    || projection.authority.tool_authority !== 'READ_ONLY_CONTEXT'
    || projection.authority.memory_scope !== 'public_safe_retrieval'
    || projection.authority.conversation_history !== 'durable_principal_bound'
    || projection.authority.training_consent !== false
    || typeof text !== 'string'
    || text !== text.trim()
    || Buffer.byteLength(text, 'utf8') < 1
    || Buffer.byteLength(text, 'utf8') > 128 * 1024
    || typeof digest !== 'string'
    || !SHA256_RE.test(digest)
  ) return null;
  return { text, responseDigest: digest, ...(inputText ? { inputText } : {}) };
}

function recoveredRuntimeReply(
  projection: RuntimeSessionGetProjection,
  scopedRequestId: string,
): RuntimeReply | null {
  const workspace = projection.session.workspace;
  if (
    !isRecord(workspace)
    || workspace.status !== 'not_authorized'
    || workspace.effect_authority !== 'NONE'
    || projection.session.effects.length !== 0
  ) return null;
  const candidates: RuntimeReply[] = [];
  for (const message of projection.session.messages) {
    if (message.role !== 'assistant' || message.request_id !== scopedRequestId) continue;
    const receipt = message.receipt;
    if (
      typeof message.content !== 'string'
      || message.content !== message.content.trim()
      || Buffer.byteLength(message.content, 'utf8') < 1
      || !isRecord(receipt)
      || typeof receipt.response_digest !== 'string'
      || !SHA256_RE.test(receipt.response_digest)
      || receipt.memory_scope !== 'public_safe_retrieval'
      || receipt.conversation_history !== 'durable_principal_bound'
    ) continue;
    candidates.push({ text: message.content, responseDigest: receipt.response_digest });
  }
  if (candidates.length === 0) return null;
  const last = candidates[candidates.length - 1];
  if (!last) return null;
  return candidates.every(
    (entry) => entry.text === last.text && entry.responseDigest === last.responseDigest,
  ) ? last : null;
}

const DEFINITIVE_RUNTIME_SUBMIT_ERRORS = new Set([
  'chat_request_invalid',
  'runtime_configuration_invalid',
  'runtime_credential_unavailable',
  'runtime_session_binding_unavailable',
  'session_binding_invalid',
  'web_runtime_retired',
]);

function ambiguousRuntimeSubmit(error: unknown): boolean {
  return !(error instanceof RuntimeProxyError)
    || !DEFINITIVE_RUNTIME_SUBMIT_ERRORS.has(error.code);
}

async function reconcileRuntimeReply(
  config: SmsSystemConfiguration,
  runtime: SmsRuntime,
  principal: ReturnType<typeof publicMemberPrincipalRef>,
  requestId: string,
): Promise<RuntimeReply | null> {
  try {
    const session = await runtime.getSession({
      sessionId: config.binding.sessionId,
      sessionPrincipal: principal,
      privacyPartition: PUBLIC_PRIVACY_PARTITION,
      credentialProfile: 'public',
    });
    return recoveredRuntimeReply(session, requestId);
  } catch {
    return null;
  }
}

async function runtimeReply(
  job: SmsClaimedJob,
  config: SmsSystemConfiguration,
  runtime: SmsRuntime,
): Promise<RuntimeReplyOutcome> {
  const expectedPhoneHash = phoneBindingHash(
    config.provider.provider,
    config.binding.ownerNumber,
    config.bindingKey,
  );
  const presentedPhoneHash = Buffer.from(job.phoneHash, 'utf8');
  const expectedPhoneHashBytes = Buffer.from(expectedPhoneHash, 'utf8');
  if (
    job.sessionId.toLowerCase() !== config.binding.sessionId
    || presentedPhoneHash.length !== expectedPhoneHashBytes.length
    || !timingSafeEqual(presentedPhoneHash, expectedPhoneHashBytes)
  ) {
    throw new Error('sms_binding_mismatch');
  }
  const principal = publicMemberPrincipalRef(config.binding.ownerUserId);
  const conversationId = scopeConversationId(principal, config.binding.sessionId);
  const requestId = scopeRequestId(principal, job.requestId);

  if (job.reconcileOnly) {
    // Stale processing leases prove that a previous invocation may already
    // have submitted this exact principal-scoped request. Reconciliation is
    // therefore the only authorized runtime operation on this path.
    const recovered = await reconcileRuntimeReply(config, runtime, principal, requestId);
    return recovered ? { state: 'reply', reply: recovered } : { state: 'uncertain' };
  }

  const plaintext = decryptSmsText(
    job.bodyCiphertext,
    config.keyring,
    smsInboundAad(config.provider.accountSid, job.providerMessageSid, job.requestId),
  );
  const envelope = parseInboundEnvelope(plaintext);
  const inputText = envelope.body.trim();
  let projection: RuntimeChatProjection;
  try {
    projection = await runtime.submit({
      // Preserve the exact signed Body in the durable envelope. Only the
      // runtime input is canonicalized because its request contract requires
      // surrounding whitespace to be removed.
      message: inputText,
      conversationId,
      requestId,
      sessionId: config.binding.sessionId,
      sessionPrincipal: principal,
      privacyPartition: PUBLIC_PRIVACY_PARTITION,
      credentialProfile: 'public',
    });
  } catch (submitError) {
    if (!ambiguousRuntimeSubmit(submitError)) {
      throw codedError(safeErrorCode(submitError, 'sms_runtime_submit_failed'));
    }
    // A transport failure may occur after the runtime durably committed the
    // turn. Do not spend a second network deadline in this invocation: retain
    // the processing row for the later reconcile-only lease. A retry must
    // never blindly submit the same model turn.
    return { state: 'uncertain' };
  }
  const reply = validRuntimeReply(projection, inputText);
  if (!reply) throw codedError('sms_runtime_authority_invalid');
  return { state: 'reply', reply };
}

async function recordRuntimeFailure(
  store: SmsStore,
  messageId: string,
  leaseToken: string,
  error: unknown,
): Promise<SmsWorkerResult> {
  const errorCode = safeErrorCode(error, 'sms_runtime_failed');
  try {
    const recorded = await store.markRuntimeFailed(messageId, leaseToken, errorCode);
    return recorded
      ? { state: 'failed', processed: 1, messageId, errorCode }
      : { state: 'uncertain', processed: 1, messageId, errorCode: 'sms_failure_persistence_failed' };
  } catch {
    return { state: 'uncertain', processed: 1, messageId, errorCode: 'sms_failure_persistence_failed' };
  }
}

async function dispatch(
  claim: SmsClaimedSend,
  deps: SmsWorkerDependencies,
): Promise<SmsWorkerResult> {
  let text: string;
  try {
    text = decryptSmsText(claim.replyCiphertext, deps.config.keyring, smsReplyAad(claim.messageId));
    const bounded = formatSmsReply(
      text,
      deps.config.policy.maxReplyChars,
      deps.config.policy.maxSegments,
    );
    if (bounded !== text || estimateSmsSegments(text) !== claim.outboundSegments) {
      throw new Error('sms_dispatch_envelope_invalid');
    }
  } catch (error) {
    const errorCode = safeErrorCode(error, 'sms_dispatch_envelope_invalid');
    try {
      const recorded = await deps.store.recordSendFailure(
        claim.messageId,
        claim.dispatchToken,
        'failed',
        errorCode,
      );
      if (!recorded) {
        return {
          state: 'uncertain',
          processed: 1,
          messageId: claim.messageId,
          errorCode: 'sms_failure_persistence_failed',
        };
      }
    } catch {
      return { state: 'uncertain', processed: 1, messageId: claim.messageId, errorCode: 'sms_failure_persistence_failed' };
    }
    return { state: 'failed', processed: 1, messageId: claim.messageId, errorCode };
  }

  // Decrypt and validate before the last consent check. The store fences this
  // authorization with the dispatch token and the consent generation captured
  // by claimSend. STOP that committed before this point invalidates the send;
  // STOP after it is an in-flight carrier race and cannot be recalled here.
  let authorized: boolean;
  try {
    authorized = await deps.store.authorizeSend(claim.messageId, claim.dispatchToken);
  } catch {
    try {
      const recorded = await deps.store.recordSendFailure(
        claim.messageId,
        claim.dispatchToken,
        'failed',
        'sms_dispatch_authorization_unavailable',
      );
      return recorded
        ? {
            state: 'failed',
            processed: 1,
            messageId: claim.messageId,
            errorCode: 'sms_dispatch_authorization_unavailable',
          }
        : {
            state: 'uncertain',
            processed: 1,
            messageId: claim.messageId,
            errorCode: 'sms_failure_persistence_failed',
          };
    } catch {
      return {
        state: 'uncertain',
        processed: 1,
        messageId: claim.messageId,
        errorCode: 'sms_failure_persistence_failed',
      };
    }
  }
  if (!authorized) {
    return {
      state: 'not_dispatched',
      processed: 1,
      messageId: claim.messageId,
      reason: 'consent_or_budget_gate',
    };
  }

  let receipt: SmsProviderSendReceipt;
  try {
    receipt = await deps.provider.send({ to: deps.config.binding.ownerNumber, text });
  } catch (error) {
    const known = providerFailure(error);
    const outcome = known?.ambiguous === false ? 'failed' : 'uncertain';
    const errorCode = safeErrorCode(error, 'sms_provider_outcome_unknown');
    try {
      const recorded = await deps.store.recordSendFailure(
        claim.messageId,
        claim.dispatchToken,
        outcome,
        errorCode,
      );
      if (!recorded) {
        return { state: 'uncertain', processed: 1, messageId: claim.messageId, errorCode: 'sms_failure_persistence_failed' };
      }
    } catch {
      return { state: 'uncertain', processed: 1, messageId: claim.messageId, errorCode: 'sms_failure_persistence_failed' };
    }
    return { state: outcome, processed: 1, messageId: claim.messageId, errorCode };
  }

  try {
    const recorded = await deps.store.recordSent(
      claim.messageId,
      claim.dispatchToken,
      receipt.sid,
      receipt.status,
    );
    if (!recorded) {
      return { state: 'uncertain', processed: 1, messageId: claim.messageId, errorCode: 'sms_send_receipt_persistence_failed' };
    }
  } catch {
    // The provider accepted this message. The DB row is never claimable for
    // another send; stale dispatch cleanup may only terminalize it UNCERTAIN.
    return { state: 'uncertain', processed: 1, messageId: claim.messageId, errorCode: 'sms_send_receipt_persistence_failed' };
  }
  return { state: 'sent', processed: 1, messageId: claim.messageId, providerStatus: receipt.status };
}

/** Run one bounded worker cycle and make no more than one provider send. */
export async function runSmsWorkerOnce(
  workerId: string,
  deps: SmsWorkerDependencies,
): Promise<SmsWorkerResult> {
  if (!/^[A-Za-z0-9_.:-]{1,128}$/.test(workerId)) {
    throw new TypeError('sms_worker_id_invalid');
  }
  const runtime: SmsRuntime = deps.runtime ?? {
    submit: (input) => submitRuntimeChat(input),
    getSession: (input) => getRuntimeSession(input),
  };

  // Drain a previously materialized reply first. The store atomically rechecks
  // current consent and the daily segment budget while claiming this send.
  const existingSend = await deps.store.claimSend(
    workerId,
    deps.config.policy.dailySegmentBudget,
  );
  if (existingSend) return dispatch(existingSend, deps);

  const job = await deps.store.claimJob(workerId);
  if (!job) return { state: 'idle', processed: 0 };

  let runtimeOutcome: RuntimeReplyOutcome;
  try {
    runtimeOutcome = await runtimeReply(job, deps.config, runtime);
  } catch (error) {
    return recordRuntimeFailure(deps.store, job.messageId, job.leaseToken, error);
  }
  if (runtimeOutcome.state === 'uncertain') {
    // Keep the processing row leased. The store can reclaim it after the
    // bounded stale interval with reconcileOnly=true; this invocation neither
    // resubmits nor writes a misleading terminal failure.
    return {
      state: 'uncertain',
      processed: 1,
      messageId: job.messageId,
      errorCode: 'sms_runtime_outcome_unknown',
    };
  }
  const { reply } = runtimeOutcome;

  let bounded: string;
  let replyCiphertext: string;
  let outboundSegments: number;
  try {
    bounded = formatSmsReply(
      reply.text,
      deps.config.policy.maxReplyChars,
      deps.config.policy.maxSegments,
    );
    outboundSegments = estimateSmsSegments(bounded);
    replyCiphertext = encryptSmsText(
      bounded,
      deps.config.keyring,
      smsReplyAad(job.messageId),
    );
  } catch (error) {
    return recordRuntimeFailure(deps.store, job.messageId, job.leaseToken, error);
  }

  let ready: boolean;
  try {
    ready = await deps.store.markJobReady({
      messageId: job.messageId,
      leaseToken: job.leaseToken,
      replyCiphertext,
      responseDigest: reply.responseDigest,
      outboundSegments,
    });
  } catch {
    return { state: 'uncertain', processed: 1, messageId: job.messageId, errorCode: 'sms_reply_persistence_failed' };
  }
  if (!ready) {
    return { state: 'not_dispatched', processed: 1, messageId: job.messageId, reason: 'state_changed' };
  }
  const actorRef = publicMemberPrincipalRef(deps.config.binding.ownerUserId);
  const traceDigest = createHash('sha256')
    .update(`APOCRYPHA-SMS-CREATION-LEDGER-v1\0${job.messageId}\0${reply.responseDigest}`, 'utf8')
    .digest('hex');
  const trace: ServerTraceContext = {
    traceId: traceDigest.slice(0, 32),
    spanId: traceDigest.slice(32, 48),
    parentSpanId: null,
    route: 'apocrypha:sms-worker',
    method: 'WORK',
  };
  await emitOperationalTelemetry({
    trace,
    kind: 'creation.apocrypha.sms_reply.materialized',
    source: 'apocrypha.sms.worker',
    plane: 'runtime',
    severity: 'info',
    outcome: 'succeeded',
    status: null,
    message: 'Consent-authorized Apocrypha SMS reply materialized into the durable outbox.',
    effectClass: 'apocrypha.sms.response_only',
    authority: 'owner-sms-consent-response-only',
    receiptRef: reply.responseDigest,
    attributes: {
      message_ref: job.messageId,
      outbound_segments: outboundSegments,
      creation_ledger: buildCreationLedgerRecord({
        creationKind: 'apocrypha.sms_reply',
        origin: 'human_prompt',
        stage: 'result',
        channel: 'sms',
        actorRef,
        requestRef: scopeRequestId(actorRef, job.requestId),
        inputText: reply.inputText,
        outputText: reply.text,
        artifactRef: reply.responseDigest,
        modelId: 'apocv4.sms',
        effectAuthority: 'NONE',
      }),
    },
  });
  // A fresh runtime turn and a carrier call never share one serverless
  // invocation. The durable ready row is drained first by the next cycle,
  // preserving a full provider deadline and an explicit outbox boundary.
  return {
    state: 'not_dispatched',
    processed: 1,
    messageId: job.messageId,
    reason: 'materialized',
  };
}

/** A non-secret digest useful for test/evidence receipts, never for identity. */
export function smsReplyDigest(value: string): string {
  return createHash('sha256').update(value, 'utf8').digest('hex');
}

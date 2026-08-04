import { createClient } from '@supabase/supabase-js';

import type { SmsCommand } from './core';

export type SmsIngressAction =
  | 'queued'
  | 'duplicate'
  | 'stop'
  | 'start'
  | 'consent'
  | 'help'
  | 'consent_required'
  | 'rate_limited'
  | 'media_unsupported';

export type SmsMessageStatus =
  | 'queued'
  | 'processing'
  | 'ready_to_send'
  | 'sending'
  | 'sent'
  | 'delivered'
  | 'undelivered'
  | 'failed'
  | 'uncertain'
  | 'suppressed'
  | 'budget_denied'
  | 'rate_limited'
  | 'media_unsupported'
  | 'consent_required'
  | 'command_processed';

export type SmsConsentState = 'pending' | 'carrier_started' | 'active' | 'revoked';
export type SmsProviderStatus =
  | 'accepted'
  | 'scheduled'
  | 'queued'
  | 'sending'
  | 'sent'
  | 'delivered'
  | 'undelivered'
  | 'failed'
  | 'canceled'
  | 'read';

export interface SmsIngressRecord {
  providerAccountSid: string;
  providerMessageSid: string;
  providerRetryTokenHash: string | null;
  phoneHash: string;
  sessionId: string;
  requestId: string;
  bodyCiphertext: string;
  commandKind: SmsCommand;
  mediaCount: number;
  consentDisclosureSha256: string;
}

export interface SmsIngressResult {
  messageId: string;
  action: SmsIngressAction;
  messageStatus: SmsMessageStatus;
  channelConsentState: SmsConsentState;
  duplicate: boolean;
}

export interface SmsClaimedJob {
  messageId: string;
  providerMessageSid: string;
  phoneHash: string;
  sessionId: string;
  requestId: string;
  bodyCiphertext: string;
  leaseToken: string;
  reconcileOnly: boolean;
}

export interface SmsClaimedSend {
  messageId: string;
  providerMessageSid: string;
  replyCiphertext: string;
  outboundSegments: number;
  dispatchToken: string;
}

export interface SmsReadyRecord {
  messageId: string;
  leaseToken: string;
  replyCiphertext: string;
  responseDigest: string;
  outboundSegments: number;
}

export interface SmsStore {
  ingest(record: SmsIngressRecord): Promise<SmsIngressResult>;
  claimJob(workerId: string): Promise<SmsClaimedJob | null>;
  markJobReady(record: SmsReadyRecord): Promise<boolean>;
  markRuntimeFailed(messageId: string, leaseToken: string, errorCode: string): Promise<boolean>;
  claimSend(workerId: string, dailySegmentBudget: number): Promise<SmsClaimedSend | null>;
  authorizeSend(messageId: string, dispatchToken: string): Promise<boolean>;
  recordSent(
    messageId: string,
    dispatchToken: string,
    outboundMessageSid: string,
    providerStatus: 'accepted' | 'scheduled' | 'queued',
  ): Promise<boolean>;
  recordSendFailure(
    messageId: string,
    dispatchToken: string,
    outcome: 'failed' | 'uncertain',
    errorCode: string,
  ): Promise<boolean>;
  recordDelivery(
    outboundMessageSid: string,
    providerStatus: SmsProviderStatus,
    errorCode: string | null,
  ): Promise<boolean>;
}

export class SmsStoreUnavailableError extends Error {
  constructor() {
    super('sms_store_unavailable');
    this.name = 'SmsStoreUnavailableError';
  }
}

export interface SmsRpcClient {
  rpc(
    name: string,
    parameters: Record<string, unknown>,
  ): PromiseLike<{ data: unknown; error: unknown }>;
}

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const ACCOUNT_SID_RE = /^AC[0-9a-f]{32}$/i;
const MESSAGE_SID_RE = /^(?:SM|MM)[0-9a-f]{32}$/i;
const SHA256_RE = /^[0-9a-f]{64}$/;
const CIPHERTEXT_RE = /^(?:v1\.[A-Za-z0-9_-]{16}\.[A-Za-z0-9_-]{2,}\.[A-Za-z0-9_-]{22}|v2\.[A-Za-z0-9_-]{1,32}\.[A-Za-z0-9_-]{16}\.[A-Za-z0-9_-]{2,}\.[A-Za-z0-9_-]{22})$/;
const ERROR_CODE_RE = /^[a-z0-9_.:-]{1,96}$/;
const WORKER_ID_RE = /^[\x21-\x7e]{1,128}$/;
const SMS_ACTIONS = new Set<SmsIngressAction>([
  'queued',
  'duplicate',
  'stop',
  'start',
  'consent',
  'help',
  'consent_required',
  'rate_limited',
  'media_unsupported',
]);
const SMS_STATUSES = new Set<SmsMessageStatus>([
  'queued',
  'processing',
  'ready_to_send',
  'sending',
  'sent',
  'delivered',
  'undelivered',
  'failed',
  'uncertain',
  'suppressed',
  'budget_denied',
  'rate_limited',
  'media_unsupported',
  'consent_required',
  'command_processed',
]);
const CONSENT_STATES = new Set<SmsConsentState>([
  'pending',
  'carrier_started',
  'active',
  'revoked',
]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

function oneRow(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (!Array.isArray(value) || value.length !== 1 || !isRecord(value[0]) || !hasExactKeys(value[0], keys)) {
    throw new SmsStoreUnavailableError();
  }
  return value[0];
}

function zeroOrOneRow(value: unknown, keys: readonly string[]): Record<string, unknown> | null {
  if (!Array.isArray(value) || value.length > 1) throw new SmsStoreUnavailableError();
  if (value.length === 0) return null;
  const row = value[0];
  if (!isRecord(row) || !hasExactKeys(row, keys)) throw new SmsStoreUnavailableError();
  return row;
}

function validUuid(value: unknown): value is string {
  return typeof value === 'string' && UUID_RE.test(value);
}

function validSid(value: unknown): value is string {
  return typeof value === 'string' && MESSAGE_SID_RE.test(value);
}

function validCiphertext(value: unknown): value is string {
  return typeof value === 'string' && CIPHERTEXT_RE.test(value);
}

async function rpc(
  client: SmsRpcClient,
  name: string,
  parameters: Record<string, unknown>,
): Promise<unknown> {
  try {
    const result = await client.rpc(name, parameters);
    if (!isRecord(result) || result.error !== null) throw new SmsStoreUnavailableError();
    return result.data;
  } catch (error) {
    if (error instanceof SmsStoreUnavailableError) throw error;
    throw new SmsStoreUnavailableError();
  }
}

function booleanResult(value: unknown): boolean {
  if (typeof value !== 'boolean') throw new SmsStoreUnavailableError();
  return value;
}

export function smsInboundAad(
  providerAccountSid: string,
  providerMessageSid: string,
  requestId: string,
): string {
  if (
    !ACCOUNT_SID_RE.test(providerAccountSid)
    || !MESSAGE_SID_RE.test(providerMessageSid)
    || !UUID_RE.test(requestId)
  ) {
    throw new TypeError('sms_inbound_identity_invalid');
  }
  return `apocrypha:sms:inbound:v1:${providerAccountSid}:${providerMessageSid}:${requestId}`;
}

/**
 * Construct the persistence adapter over an RPC-only client. Exposed so the
 * contract can be verified without giving tests or callers table access.
 */
export function createSmsStoreForRpcClient(client: SmsRpcClient): SmsStore {
  return {
    async ingest(record) {
      if (
        !ACCOUNT_SID_RE.test(record.providerAccountSid)
        || !MESSAGE_SID_RE.test(record.providerMessageSid)
        || (record.providerRetryTokenHash !== null && !SHA256_RE.test(record.providerRetryTokenHash))
        || !SHA256_RE.test(record.phoneHash)
        || !UUID_RE.test(record.sessionId)
        || !UUID_RE.test(record.requestId)
        || !CIPHERTEXT_RE.test(record.bodyCiphertext)
        || record.bodyCiphertext.length < 32
        || record.bodyCiphertext.length > 32_768
        || !['message', 'stop', 'start', 'consent', 'help'].includes(record.commandKind)
        || !Number.isInteger(record.mediaCount)
        || record.mediaCount < 0
        || record.mediaCount > 10
        || !SHA256_RE.test(record.consentDisclosureSha256)
      ) throw new SmsStoreUnavailableError();
      const data = await rpc(client, 'ingest_apocrypha_sms_message', {
        p_provider_account_sid: record.providerAccountSid,
        p_provider_message_sid: record.providerMessageSid,
        p_provider_retry_token_hash: record.providerRetryTokenHash,
        p_phone_hash: record.phoneHash,
        p_session_id: record.sessionId,
        p_request_id: record.requestId,
        p_body_ciphertext: record.bodyCiphertext,
        p_command_kind: record.commandKind,
        p_media_count: record.mediaCount,
        p_consent_disclosure_sha256: record.consentDisclosureSha256,
      });
      const row = oneRow(data, [
        'message_id',
        'action',
        'message_status',
        'channel_consent_state',
        'duplicate',
      ]);
      if (
        !validUuid(row.message_id)
        || typeof row.action !== 'string'
        || !SMS_ACTIONS.has(row.action as SmsIngressAction)
        || typeof row.message_status !== 'string'
        || !SMS_STATUSES.has(row.message_status as SmsMessageStatus)
        || typeof row.channel_consent_state !== 'string'
        || !CONSENT_STATES.has(row.channel_consent_state as SmsConsentState)
        || typeof row.duplicate !== 'boolean'
      ) throw new SmsStoreUnavailableError();
      return {
        messageId: row.message_id,
        action: row.action as SmsIngressAction,
        messageStatus: row.message_status as SmsMessageStatus,
        channelConsentState: row.channel_consent_state as SmsConsentState,
        duplicate: row.duplicate,
      };
    },

    async claimJob(workerId) {
      if (!WORKER_ID_RE.test(workerId)) throw new SmsStoreUnavailableError();
      const data = await rpc(client, 'claim_apocrypha_sms_job', { p_worker_id: workerId });
      const row = zeroOrOneRow(data, [
        'message_id',
        'provider_message_sid',
        'phone_hash',
        'session_id',
        'request_id',
        'body_ciphertext',
        'lease_token',
        'reconcile_only',
      ]);
      if (!row) return null;
      if (
        !validUuid(row.message_id)
        || !validSid(row.provider_message_sid)
        || typeof row.phone_hash !== 'string'
        || !SHA256_RE.test(row.phone_hash)
        || !validUuid(row.session_id)
        || !validUuid(row.request_id)
        || !validCiphertext(row.body_ciphertext)
        || row.body_ciphertext.length < 32
        || row.body_ciphertext.length > 32_768
        || !validUuid(row.lease_token)
        || typeof row.reconcile_only !== 'boolean'
      ) throw new SmsStoreUnavailableError();
      return {
        messageId: row.message_id,
        providerMessageSid: row.provider_message_sid,
        phoneHash: row.phone_hash,
        sessionId: row.session_id,
        requestId: row.request_id,
        bodyCiphertext: row.body_ciphertext,
        leaseToken: row.lease_token,
        reconcileOnly: row.reconcile_only,
      };
    },

    async markJobReady(record) {
      if (
        !UUID_RE.test(record.messageId)
        || !UUID_RE.test(record.leaseToken)
        || !CIPHERTEXT_RE.test(record.replyCiphertext)
        || record.replyCiphertext.length < 32
        || record.replyCiphertext.length > 16_384
        || !SHA256_RE.test(record.responseDigest)
        || !Number.isInteger(record.outboundSegments)
        || record.outboundSegments < 1
        || record.outboundSegments > 10
      ) throw new SmsStoreUnavailableError();
      return booleanResult(await rpc(client, 'mark_apocrypha_sms_job_ready', {
        p_message_id: record.messageId,
        p_lease_token: record.leaseToken,
        p_reply_ciphertext: record.replyCiphertext,
        p_response_digest: record.responseDigest,
        p_outbound_segments: record.outboundSegments,
      }));
    },

    async markRuntimeFailed(messageId, leaseToken, errorCode) {
      if (!UUID_RE.test(messageId) || !UUID_RE.test(leaseToken) || !ERROR_CODE_RE.test(errorCode)) {
        throw new SmsStoreUnavailableError();
      }
      return booleanResult(await rpc(client, 'mark_apocrypha_sms_runtime_failed', {
        p_message_id: messageId,
        p_lease_token: leaseToken,
        p_error_code: errorCode,
      }));
    },

    async claimSend(workerId, dailySegmentBudget) {
      if (
        !WORKER_ID_RE.test(workerId)
        || !Number.isInteger(dailySegmentBudget)
        || dailySegmentBudget < 1
        || dailySegmentBudget > 1_000
      ) throw new SmsStoreUnavailableError();
      const data = await rpc(client, 'claim_apocrypha_sms_send', {
        p_worker_id: workerId,
        p_daily_segment_budget: dailySegmentBudget,
      });
      const row = zeroOrOneRow(data, [
        'message_id',
        'provider_message_sid',
        'reply_ciphertext',
        'outbound_segments',
        'dispatch_token',
      ]);
      if (!row) return null;
      if (
        !validUuid(row.message_id)
        || !validSid(row.provider_message_sid)
        || !validCiphertext(row.reply_ciphertext)
        || row.reply_ciphertext.length < 32
        || row.reply_ciphertext.length > 16_384
        || typeof row.outbound_segments !== 'number'
        || !Number.isInteger(row.outbound_segments)
        || row.outbound_segments < 1
        || row.outbound_segments > 10
        || !validUuid(row.dispatch_token)
      ) throw new SmsStoreUnavailableError();
      return {
        messageId: row.message_id,
        providerMessageSid: row.provider_message_sid,
        replyCiphertext: row.reply_ciphertext,
        outboundSegments: row.outbound_segments,
        dispatchToken: row.dispatch_token,
      };
    },

    async authorizeSend(messageId, dispatchToken) {
      if (!UUID_RE.test(messageId) || !UUID_RE.test(dispatchToken)) {
        throw new SmsStoreUnavailableError();
      }
      return booleanResult(await rpc(client, 'authorize_apocrypha_sms_send', {
        p_message_id: messageId,
        p_dispatch_token: dispatchToken,
      }));
    },

    async recordSent(messageId, dispatchToken, outboundMessageSid, providerStatus) {
      if (
        !UUID_RE.test(messageId)
        || !UUID_RE.test(dispatchToken)
        || !MESSAGE_SID_RE.test(outboundMessageSid)
        || !['accepted', 'scheduled', 'queued'].includes(providerStatus)
      ) throw new SmsStoreUnavailableError();
      return booleanResult(await rpc(client, 'record_apocrypha_sms_sent', {
        p_message_id: messageId,
        p_dispatch_token: dispatchToken,
        p_outbound_message_sid: outboundMessageSid,
        p_provider_status: providerStatus,
      }));
    },

    async recordSendFailure(messageId, dispatchToken, outcome, errorCode) {
      if (
        !UUID_RE.test(messageId)
        || !UUID_RE.test(dispatchToken)
        || !['failed', 'uncertain'].includes(outcome)
        || !ERROR_CODE_RE.test(errorCode)
      ) throw new SmsStoreUnavailableError();
      return booleanResult(await rpc(client, 'record_apocrypha_sms_send_failure', {
        p_message_id: messageId,
        p_dispatch_token: dispatchToken,
        p_outcome: outcome,
        p_error_code: errorCode,
      }));
    },

    async recordDelivery(outboundMessageSid, providerStatus, errorCode) {
      if (
        !MESSAGE_SID_RE.test(outboundMessageSid)
        || !['accepted', 'scheduled', 'queued', 'sending', 'sent', 'delivered', 'undelivered', 'failed', 'canceled', 'read'].includes(providerStatus)
        || (errorCode !== null && !/^[0-9]{1,16}$/.test(errorCode))
      ) throw new SmsStoreUnavailableError();
      return booleanResult(await rpc(client, 'record_apocrypha_sms_delivery', {
        p_outbound_message_sid: outboundMessageSid,
        p_provider_status: providerStatus,
        p_error_code: errorCode,
      }));
    },
  };
}

/** Server-only factory: the service-role credential never crosses this API seam. */
export function createSmsStore(
  env: Record<string, string | undefined> = process.env,
): SmsStore {
  if (typeof window !== 'undefined') throw new SmsStoreUnavailableError();
  const url = env['SUPABASE_URL']?.trim() || env['NEXT_PUBLIC_SUPABASE_URL']?.trim();
  const serviceRoleKey = env['SUPABASE_SERVICE_ROLE_KEY']?.trim();
  if (!url || !serviceRoleKey) throw new SmsStoreUnavailableError();
  try {
    const parsed = new URL(url);
    const local = parsed.hostname === 'localhost' || parsed.hostname === '127.0.0.1';
    if (
      (parsed.protocol !== 'https:' && !(local && parsed.protocol === 'http:'))
      || parsed.username !== ''
      || parsed.password !== ''
      || parsed.pathname !== '/'
      || parsed.search !== ''
      || parsed.hash !== ''
    ) throw new SmsStoreUnavailableError();
    const client = createClient(url, serviceRoleKey, {
      auth: {
        persistSession: false,
        autoRefreshToken: false,
        detectSessionInUrl: false,
      },
    });
    return createSmsStoreForRpcClient(client as unknown as SmsRpcClient);
  } catch (error) {
    if (error instanceof SmsStoreUnavailableError) throw error;
    throw new SmsStoreUnavailableError();
  }
}

import { createHash } from 'node:crypto';

import { createClient } from '@supabase/supabase-js';

import {
  canonicalJson,
  syncSigningPayload,
  type MiniBrainSyncRequest,
  type MiniBrainSyncUnsignedRequest,
} from './mobile-contracts';
import {
  MiniBrainRelayError,
  type VerifiedMiniBrainRequest,
} from './mobile-relay';

const ADMISSION_RPC = 'admit_mini_brain_relay_request';
const CLEANUP_RPC = 'cleanup_mini_brain_relay_state';
const SHA256_RE = /^[0-9a-f]{64}$/u;
const UUID_V4_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu;

export type DurableMiniBrainReplayKind = 'new_sequence' | 'identical_retry';

export interface MiniBrainRelayAdmissionInput {
  readonly ownerRef: string;
  readonly deviceId: string;
  readonly keyThumbprint: string;
  readonly sequence: number;
  readonly requestId: string;
  readonly logicalRequestDigest: string;
  readonly envelopeDigest: string;
}

export interface MiniBrainRelayAdmission {
  readonly replayKind: DurableMiniBrainReplayKind;
  readonly acceptedSequence: number;
  readonly rateCount: number;
  readonly rateLimit: 30;
  readonly rateResetsAt: string;
  readonly stateExpiresAt: string;
}

export interface MiniBrainRelayCleanupReceipt {
  readonly sequenceRowsDeleted: number;
  readonly requestRowsDeleted: number;
  readonly deviceRowsDeleted: number;
  readonly rateRowsDeleted: number;
}

export interface MiniBrainRelayStateStore {
  admit(input: MiniBrainRelayAdmissionInput): Promise<MiniBrainRelayAdmission>;
  cleanup(limit?: number): Promise<MiniBrainRelayCleanupReceipt>;
}

export interface MiniBrainRelayStateRpcClient {
  rpc(
    name: string,
    parameters: Record<string, unknown>,
  ): PromiseLike<{ data: unknown; error: unknown }>;
}

export interface DurableVerifiedMiniBrainRequest
  extends VerifiedMiniBrainRequest {
  readonly replayKind: DurableMiniBrainReplayKind;
  readonly durableState: MiniBrainRelayAdmission;
}

const REMOTE_ERRORS: ReadonlyArray<readonly [string, 400 | 403 | 409 | 429 | 503]> = [
  ['BRAIN_RELAY_STATE_INPUT_INVALID', 400],
  ['BRAIN_RELAY_CLEANUP_LIMIT_INVALID', 400],
  ['BRAIN_DEVICE_STATE_BINDING_MISMATCH', 403],
  ['BRAIN_SYNC_REQUEST_ID_REUSED', 409],
  ['BRAIN_SYNC_REPLAY_REJECTED', 409],
  ['BRAIN_SYNC_RATE_LIMITED', 429],
];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function exactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

function oneRow(value: unknown, expected: readonly string[]): Record<string, unknown> {
  if (
    !Array.isArray(value)
    || value.length !== 1
    || !isRecord(value[0])
    || !exactKeys(value[0], expected)
  ) {
    throw new MiniBrainRelayError('BRAIN_RELAY_STATE_UNAVAILABLE', 503);
  }
  return value[0];
}

function safeInteger(value: unknown, min: number, max: number): number | null {
  const candidate = typeof value === 'string' && /^[0-9]+$/u.test(value)
    ? Number(value)
    : value;
  return typeof candidate === 'number'
    && Number.isSafeInteger(candidate)
    && candidate >= min
    && candidate <= max
    ? candidate
    : null;
}

function isoTimestamp(value: unknown): value is string {
  return typeof value === 'string' && Number.isFinite(Date.parse(value));
}

function remoteErrorText(error: unknown): string {
  if (!isRecord(error)) return '';
  return ['code', 'message', 'details', 'hint']
    .map(key => typeof error[key] === 'string' ? error[key] : '')
    .join(' ')
    .slice(0, 2_048);
}

function relayError(error: unknown): MiniBrainRelayError {
  if (error instanceof MiniBrainRelayError) return error;
  const text = remoteErrorText(error);
  for (const [code, status] of REMOTE_ERRORS) {
    if (text.includes(code)) return new MiniBrainRelayError(code, status);
  }
  return new MiniBrainRelayError('BRAIN_RELAY_STATE_UNAVAILABLE', 503);
}

async function rpc(
  client: MiniBrainRelayStateRpcClient,
  name: string,
  parameters: Record<string, unknown>,
): Promise<unknown> {
  try {
    const result = await client.rpc(name, parameters);
    if (!isRecord(result) || result.error !== null) throw relayError(result?.error);
    return result.data;
  } catch (error) {
    throw relayError(error);
  }
}

function validateAdmissionInput(input: MiniBrainRelayAdmissionInput): void {
  if (
    !SHA256_RE.test(input.ownerRef)
    || !UUID_V4_RE.test(input.deviceId)
    || !SHA256_RE.test(input.keyThumbprint)
    || !Number.isSafeInteger(input.sequence)
    || input.sequence < 1
    || input.sequence > Number.MAX_SAFE_INTEGER
    || !UUID_V4_RE.test(input.requestId)
    || !SHA256_RE.test(input.logicalRequestDigest)
    || !SHA256_RE.test(input.envelopeDigest)
  ) {
    throw new MiniBrainRelayError('BRAIN_RELAY_STATE_INPUT_INVALID', 400);
  }
}

function admissionResult(value: unknown): MiniBrainRelayAdmission {
  const row = oneRow(value, [
    'accepted_sequence',
    'outcome',
    'rate_count',
    'rate_limit',
    'rate_resets_at',
    'state_expires_at',
  ]);
  const rejected = REMOTE_ERRORS.find(([code]) => row.outcome === code);
  if (rejected) throw new MiniBrainRelayError(rejected[0], rejected[1]);
  const acceptedSequence = safeInteger(row.accepted_sequence, 1, Number.MAX_SAFE_INTEGER);
  const rateCount = safeInteger(row.rate_count, 0, 30);
  const rateLimit = safeInteger(row.rate_limit, 30, 30);
  if (
    (row.outcome !== 'new_sequence' && row.outcome !== 'identical_retry')
    || acceptedSequence === null
    || rateCount === null
    || rateLimit !== 30
    || !isoTimestamp(row.rate_resets_at)
    || !isoTimestamp(row.state_expires_at)
  ) {
    throw new MiniBrainRelayError('BRAIN_RELAY_STATE_UNAVAILABLE', 503);
  }
  return {
    replayKind: row.outcome,
    acceptedSequence,
    rateCount,
    rateLimit,
    rateResetsAt: row.rate_resets_at,
    stateExpiresAt: row.state_expires_at,
  };
}

function cleanupResult(value: unknown): MiniBrainRelayCleanupReceipt {
  const row = oneRow(value, [
    'device_rows_deleted',
    'rate_rows_deleted',
    'request_rows_deleted',
    'sequence_rows_deleted',
  ]);
  const sequenceRowsDeleted = safeInteger(row.sequence_rows_deleted, 0, Number.MAX_SAFE_INTEGER);
  const requestRowsDeleted = safeInteger(row.request_rows_deleted, 0, Number.MAX_SAFE_INTEGER);
  const deviceRowsDeleted = safeInteger(row.device_rows_deleted, 0, Number.MAX_SAFE_INTEGER);
  const rateRowsDeleted = safeInteger(row.rate_rows_deleted, 0, Number.MAX_SAFE_INTEGER);
  if (
    sequenceRowsDeleted === null
    || requestRowsDeleted === null
    || deviceRowsDeleted === null
    || rateRowsDeleted === null
  ) {
    throw new MiniBrainRelayError('BRAIN_RELAY_STATE_UNAVAILABLE', 503);
  }
  return { sequenceRowsDeleted, requestRowsDeleted, deviceRowsDeleted, rateRowsDeleted };
}

/**
 * RPC-only adapter. Direct table access is intentionally absent so every
 * sequence, rate, and request-ledger decision stays inside one DB transaction.
 */
export function createMiniBrainRelayStateStoreForRpcClient(
  client: MiniBrainRelayStateRpcClient,
): MiniBrainRelayStateStore {
  return {
    async admit(input) {
      validateAdmissionInput(input);
      const data = await rpc(client, ADMISSION_RPC, {
        p_owner_ref: input.ownerRef,
        p_device_id: input.deviceId.toLowerCase(),
        p_key_thumbprint: input.keyThumbprint,
        p_sequence: input.sequence,
        p_request_id: input.requestId.toLowerCase(),
        p_logical_digest: input.logicalRequestDigest,
        p_envelope_digest: input.envelopeDigest,
      });
      return admissionResult(data);
    },

    async cleanup(limit = 5_000) {
      if (!Number.isSafeInteger(limit) || limit < 1 || limit > 50_000) {
        throw new MiniBrainRelayError('BRAIN_RELAY_CLEANUP_LIMIT_INVALID', 400);
      }
      return cleanupResult(await rpc(client, CLEANUP_RPC, { p_limit: limit }));
    },
  };
}

function serviceUrl(env: Record<string, string | undefined>): string | null {
  const raw = env.APOCKY_HUB_SUPABASE_URL?.trim()
    || env.NEXT_PUBLIC_SUPABASE_URL?.trim();
  if (!raw) return null;
  try {
    const parsed = new URL(raw);
    const loopback = parsed.hostname === 'localhost' || parsed.hostname === '127.0.0.1';
    if (
      (parsed.protocol !== 'https:' && !(loopback && parsed.protocol === 'http:'))
      || parsed.username !== ''
      || parsed.password !== ''
      || parsed.pathname !== '/'
      || parsed.search !== ''
      || parsed.hash !== ''
    ) return null;
    return parsed.toString().replace(/\/$/u, '');
  } catch {
    return null;
  }
}

/** Server-only factory. Missing or malformed durable storage fails closed. */
export function createMiniBrainRelayStateStore(
  env: Record<string, string | undefined> = process.env,
): MiniBrainRelayStateStore {
  if (typeof window !== 'undefined') {
    throw new MiniBrainRelayError('BRAIN_RELAY_STATE_UNAVAILABLE', 503);
  }
  const url = serviceUrl(env);
  const serviceRoleKey = env.APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY?.trim()
    || env.SUPABASE_SERVICE_ROLE_KEY?.trim();
  if (!url || !serviceRoleKey) {
    throw new MiniBrainRelayError('BRAIN_RELAY_STATE_UNAVAILABLE', 503);
  }
  const client = createClient(url, serviceRoleKey, {
    auth: {
      persistSession: false,
      autoRefreshToken: false,
      detectSessionInUrl: false,
    },
  });
  return createMiniBrainRelayStateStoreForRpcClient(
    client as unknown as MiniBrainRelayStateRpcClient,
  );
}

let defaultStore: MiniBrainRelayStateStore | undefined;

/** Lazily reuse only the stateless client handle; all authority stays in DB. */
export function getMiniBrainRelayStateStore(): MiniBrainRelayStateStore {
  defaultStore ??= createMiniBrainRelayStateStore();
  return defaultStore;
}

function unsignedRequest(request: MiniBrainSyncRequest): MiniBrainSyncUnsignedRequest {
  return {
    schema_version: request.schema_version,
    device_id: request.device_id,
    sequence: request.sequence,
    issued_at: request.issued_at,
    operation: request.operation,
    session_id: request.session_id,
    request_id: request.request_id,
    base_cursor: request.base_cursor,
    payload: request.payload,
    payload_digest: request.payload_digest,
  };
}

export function durableMiniBrainEnvelopeDigest(request: MiniBrainSyncRequest): string {
  return createHash('sha256')
    .update(syncSigningPayload(unsignedRequest(request)), 'utf8')
    .digest('hex');
}

export function durableMiniBrainLogicalRequestDigest(request: MiniBrainSyncRequest): string {
  return createHash('sha256')
    .update('apocky.mini-brain.logical-request.v1\u0000', 'utf8')
    .update(canonicalJson({
      operation: request.operation,
      payload_digest: request.payload_digest,
      schema_version: request.schema_version,
      session_id: request.session_id,
    }), 'utf8')
    .digest('hex');
}

/**
 * Persist only verified envelope identity and its digest. The signed payload
 * may contain a prompt, but neither this adapter nor its RPC transmits it.
 */
export async function admitVerifiedMiniBrainRequest(
  store: MiniBrainRelayStateStore,
  verified: VerifiedMiniBrainRequest,
): Promise<DurableVerifiedMiniBrainRequest> {
  const durableState = await store.admit({
    ownerRef: verified.ownerRef,
    deviceId: verified.request.device_id,
    keyThumbprint: verified.keyThumbprint,
    sequence: verified.request.sequence,
    requestId: verified.request.request_id,
    logicalRequestDigest: durableMiniBrainLogicalRequestDigest(verified.request),
    envelopeDigest: durableMiniBrainEnvelopeDigest(verified.request),
  });
  return {
    request: verified.request,
    ownerRef: verified.ownerRef,
    keyThumbprint: verified.keyThumbprint,
    replayKind: durableState.replayKind,
    durableState,
  };
}

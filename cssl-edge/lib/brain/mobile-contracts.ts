export const MINI_BRAIN_DEVICE_SCHEMA = 'apocky.mini-brain.device-capability.v1' as const;
export const MINI_BRAIN_SYNC_REQUEST_SCHEMA = 'apocky.mini-brain.sync-request.v1' as const;
export const MINI_BRAIN_SYNC_RESPONSE_SCHEMA = 'apocky.mini-brain.sync-response.v1' as const;

export type MiniBrainSyncOperation = 'pull' | 'append';

export interface MiniBrainSyncPayload {
  readonly text: string;
}

export interface MiniBrainSyncUnsignedRequest {
  readonly schema_version: typeof MINI_BRAIN_SYNC_REQUEST_SCHEMA;
  readonly device_id: string;
  readonly sequence: number;
  readonly issued_at: string;
  readonly operation: MiniBrainSyncOperation;
  readonly session_id: string;
  readonly request_id: string;
  readonly base_cursor: string | null;
  readonly payload: MiniBrainSyncPayload | null;
  readonly payload_digest: string;
}

export interface MiniBrainSyncRequest extends MiniBrainSyncUnsignedRequest {
  readonly device_token: string;
  readonly signature: string;
}

export interface MiniBrainTombstone {
  readonly session_id: string;
  readonly observed_at: string;
  readonly reason: 'REMOTE_SESSION_ABSENT';
}

export interface MiniBrainSyncResponse {
  readonly schema_version: typeof MINI_BRAIN_SYNC_RESPONSE_SCHEMA;
  readonly status: 'current' | 'advanced' | 'appended' | 'idempotent_replay' | 'empty' | 'tombstoned';
  readonly session_id: string;
  readonly request_id: string;
  readonly acknowledged_request_ids: readonly string[];
  readonly cursor: string | null;
  readonly messages: readonly Record<string, unknown>[];
  readonly tombstones: readonly MiniBrainTombstone[];
  readonly events_truncated: boolean;
  readonly provenance: {
    readonly transport: 'owner_bound_apocv4_runtime';
    readonly privacy_partition_ref: string | null;
    readonly principal_ref: string | null;
    readonly binding_ref: string | null;
  };
  readonly controls: {
    readonly owner_session: 'verified';
    readonly device_signature: 'verified';
    readonly replay: 'bounded_sequence_and_idempotent_request';
    readonly rate_limit: 'owner_durable_window';
    readonly partition: 'server_derived_owner';
  };
  readonly served_by: string;
  readonly ts: string;
}

const SYNC_STATUSES = new Set<MiniBrainSyncResponse['status']>([
  'current', 'advanced', 'appended', 'idempotent_replay', 'empty', 'tombstoned',
]);
const SHA256 = /^[0-9a-f]{64}$/u;

function record(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function nullableDigest(value: unknown): value is string | null {
  return value === null || (typeof value === 'string' && SHA256.test(value));
}

export function validateMiniBrainSyncResponse(
  value: unknown,
  expected: Pick<MiniBrainSyncUnsignedRequest, 'operation' | 'session_id' | 'request_id'>,
): MiniBrainSyncResponse {
  if (!record(value)) throw new Error('MINI_BRAIN_SYNC_RESPONSE_INVALID');
  const status = value.status as MiniBrainSyncResponse['status'];
  const allowedStatus = expected.operation === 'append'
    ? status === 'appended' || status === 'idempotent_replay'
    : status === 'current' || status === 'advanced' || status === 'empty' || status === 'tombstoned';
  const acknowledged = value.acknowledged_request_ids;
  const messages = value.messages;
  const tombstones = value.tombstones;
  const provenance = value.provenance;
  const controls = value.controls;
  const expectedAcknowledgements = expected.operation === 'append' ? [expected.request_id] : [];
  if (
    value.schema_version !== MINI_BRAIN_SYNC_RESPONSE_SCHEMA
    || !SYNC_STATUSES.has(status)
    || !allowedStatus
    || value.session_id !== expected.session_id
    || value.request_id !== expected.request_id
    || !Array.isArray(acknowledged)
    || acknowledged.length !== expectedAcknowledgements.length
    || !acknowledged.every((item, index) => item === expectedAcknowledgements[index])
    || !nullableDigest(value.cursor)
    || !Array.isArray(messages)
    || !messages.every(message => record(message)
      && (message.role === 'user' || message.role === 'assistant')
      && typeof message.content === 'string'
      && message.content.length <= 131_072
      && typeof message.request_id === 'string'
      && typeof message.recorded_at === 'string')
    || !Array.isArray(tombstones)
    || !tombstones.every(item => record(item)
      && item.session_id === expected.session_id
      && typeof item.observed_at === 'string'
      && item.reason === 'REMOTE_SESSION_ABSENT')
    || typeof value.events_truncated !== 'boolean'
    || !record(provenance)
    || provenance.transport !== 'owner_bound_apocv4_runtime'
    || !nullableDigest(provenance.privacy_partition_ref)
    || !nullableDigest(provenance.principal_ref)
    || !nullableDigest(provenance.binding_ref)
    || !record(controls)
    || controls.owner_session !== 'verified'
    || controls.device_signature !== 'verified'
    || controls.replay !== 'bounded_sequence_and_idempotent_request'
    || controls.rate_limit !== 'owner_durable_window'
    || controls.partition !== 'server_derived_owner'
    || typeof value.served_by !== 'string'
    || value.served_by.length < 1
    || typeof value.ts !== 'string'
  ) throw new Error('MINI_BRAIN_SYNC_RESPONSE_INVALID');
  return value as unknown as MiniBrainSyncResponse;
}

export function canonicalJson(value: unknown): string {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') {
    return JSON.stringify(value);
  }
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new Error('MINI_BRAIN_NONCANONICAL_NUMBER');
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (typeof value === 'object') {
    const row = value as Record<string, unknown>;
    return `{${Object.keys(row).sort().map(key => `${JSON.stringify(key)}:${canonicalJson(row[key])}`).join(',')}}`;
  }
  throw new Error('MINI_BRAIN_NONCANONICAL_VALUE');
}

export function syncSigningPayload(request: MiniBrainSyncUnsignedRequest): string {
  return canonicalJson(request);
}

export function bytesToBase64Url(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/u, '');
}

export function base64UrlToBytes(value: string): Uint8Array {
  if (!/^[A-Za-z0-9_-]+$/u.test(value)) throw new Error('MINI_BRAIN_BASE64URL_INVALID');
  const padded = `${value.replace(/-/g, '+').replace(/_/g, '/')}${'='.repeat((4 - value.length % 4) % 4)}`;
  const binary = atob(padded);
  return Uint8Array.from(binary, char => char.charCodeAt(0));
}

export async function sha256Hex(value: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value));
  return [...new Uint8Array(digest)].map(byte => byte.toString(16).padStart(2, '0')).join('');
}

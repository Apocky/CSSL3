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
    readonly rate_limit: 'relay_instance_burst';
    readonly partition: 'server_derived_owner';
  };
  readonly served_by: string;
  readonly ts: string;
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

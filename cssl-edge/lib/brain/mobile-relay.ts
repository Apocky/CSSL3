import {
  createHash,
  createHmac,
  timingSafeEqual,
  webcrypto,
} from 'node:crypto';

import {
  MINI_BRAIN_DEVICE_SCHEMA,
  MINI_BRAIN_SYNC_REQUEST_SCHEMA,
  canonicalJson,
  syncSigningPayload,
  type MiniBrainSyncRequest,
  type MiniBrainSyncUnsignedRequest,
} from './mobile-contracts';
import { isOpaqueClientRequestId, isOpaqueConversationId } from '../apocrypha/proxy';

const DEVICE_TOKEN_TTL_MS = 30 * 24 * 60 * 60 * 1_000;
const REQUEST_CLOCK_SKEW_MS = 2 * 60 * 1_000;
const TOKEN_RE = /^[A-Za-z0-9_-]{1,4096}\.[A-Za-z0-9_-]{43}$/u;
const BASE64URL_256_RE = /^[A-Za-z0-9_-]{43}$/u;
const SHA256_RE = /^[0-9a-f]{64}$/u;

interface DeviceCapability {
  readonly schema_version: typeof MINI_BRAIN_DEVICE_SCHEMA;
  readonly device_id: string;
  readonly owner_ref: string;
  readonly key_thumbprint: string;
  readonly public_key_jwk: JsonWebKey;
  readonly issued_at: string;
  readonly expires_at: string;
}

export interface VerifiedMiniBrainRequest {
  readonly request: MiniBrainSyncRequest;
  readonly ownerRef: string;
  readonly keyThumbprint: string;
}

export class MiniBrainRelayError extends Error {
  constructor(
    readonly code: string,
    readonly publicStatus: 400 | 401 | 403 | 409 | 413 | 429 | 503,
  ) {
    super(code);
    this.name = 'MiniBrainRelayError';
  }
}

function base64Url(bytes: Uint8Array | Buffer): string {
  return Buffer.from(bytes).toString('base64url');
}

function fromBase64Url(value: string): Buffer {
  if (!/^[A-Za-z0-9_-]+$/u.test(value)) throw new MiniBrainRelayError('BRAIN_DEVICE_TOKEN_INVALID', 401);
  return Buffer.from(value, 'base64url');
}

function digest(value: string): string {
  return createHash('sha256').update(value, 'utf8').digest('hex');
}

export function miniBrainOwnerRef(userId: string): string {
  return digest(`apocky.mini-brain.owner.v1\u0000${userId}`);
}

function bindingKey(): Buffer {
  const raw = process.env.APOCKY_BRAIN_DEVICE_BINDING_SECRET
    ?? process.env.APOCV4_SESSION_BINDING_SECRET;
  if (!raw || raw !== raw.trim()) throw new MiniBrainRelayError('BRAIN_DEVICE_BINDING_UNAVAILABLE', 503);
  const bytes = Buffer.from(raw, 'utf8');
  if (bytes.length < 32 || bytes.length > 8_192 || [...bytes].some(byte => byte < 0x21 || byte > 0x7e)) {
    throw new MiniBrainRelayError('BRAIN_DEVICE_BINDING_UNAVAILABLE', 503);
  }
  return createHmac('sha256', bytes).update('apocky.mini-brain.device-binding.v1', 'utf8').digest();
}

function validPublicJwk(value: unknown): value is JsonWebKey {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const jwk = value as Record<string, unknown>;
  const allowed = new Set(['crv', 'ext', 'key_ops', 'kty', 'x', 'y']);
  return Object.keys(jwk).every(key => allowed.has(key))
    && jwk.kty === 'EC'
    && jwk.crv === 'P-256'
    && jwk.ext === true
    && Array.isArray(jwk.key_ops)
    && jwk.key_ops.length === 1
    && jwk.key_ops[0] === 'verify'
    && typeof jwk.x === 'string'
    && BASE64URL_256_RE.test(jwk.x)
    && typeof jwk.y === 'string'
    && BASE64URL_256_RE.test(jwk.y);
}

function keyThumbprint(jwk: JsonWebKey): string {
  return digest(canonicalJson({ crv: jwk.crv, kty: jwk.kty, x: jwk.x, y: jwk.y }));
}

function signCapability(payload: DeviceCapability): string {
  const encoded = base64Url(Buffer.from(canonicalJson(payload), 'utf8'));
  const signature = createHmac('sha256', bindingKey()).update(encoded, 'ascii').digest();
  return `${encoded}.${base64Url(signature)}`;
}

function parseCapability(token: string, userId: string, nowMs: number): DeviceCapability {
  if (!TOKEN_RE.test(token)) throw new MiniBrainRelayError('BRAIN_DEVICE_TOKEN_INVALID', 401);
  const [encoded, presented] = token.split('.');
  if (!encoded || !presented) throw new MiniBrainRelayError('BRAIN_DEVICE_TOKEN_INVALID', 401);
  const expected = createHmac('sha256', bindingKey()).update(encoded, 'ascii').digest();
  const presentedBytes = fromBase64Url(presented);
  if (presentedBytes.length !== expected.length || !timingSafeEqual(presentedBytes, expected)) {
    throw new MiniBrainRelayError('BRAIN_DEVICE_TOKEN_INVALID', 401);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(fromBase64Url(encoded).toString('utf8'));
  } catch {
    throw new MiniBrainRelayError('BRAIN_DEVICE_TOKEN_INVALID', 401);
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new MiniBrainRelayError('BRAIN_DEVICE_TOKEN_INVALID', 401);
  }
  const row = parsed as Record<string, unknown>;
  const expectedKeys = [
    'device_id', 'expires_at', 'issued_at', 'key_thumbprint',
    'owner_ref', 'public_key_jwk', 'schema_version',
  ].sort();
  if (
    Object.keys(row).sort().join(',') !== expectedKeys.join(',')
    || row.schema_version !== MINI_BRAIN_DEVICE_SCHEMA
    || !isOpaqueConversationId(row.device_id)
    || row.owner_ref !== miniBrainOwnerRef(userId)
    || typeof row.key_thumbprint !== 'string'
    || !SHA256_RE.test(row.key_thumbprint)
    || !validPublicJwk(row.public_key_jwk)
    || row.key_thumbprint !== keyThumbprint(row.public_key_jwk)
    || typeof row.issued_at !== 'string'
    || typeof row.expires_at !== 'string'
  ) {
    throw new MiniBrainRelayError('BRAIN_DEVICE_TOKEN_INVALID', 401);
  }
  const issuedAt = Date.parse(row.issued_at);
  const expiresAt = Date.parse(row.expires_at);
  if (
    !Number.isFinite(issuedAt)
    || !Number.isFinite(expiresAt)
    || expiresAt <= nowMs
    || issuedAt > nowMs + REQUEST_CLOCK_SKEW_MS
    || expiresAt - issuedAt !== DEVICE_TOKEN_TTL_MS
  ) {
    throw new MiniBrainRelayError('BRAIN_DEVICE_TOKEN_EXPIRED', 401);
  }
  return parsed as DeviceCapability;
}

function exactSyncRequest(value: unknown): value is MiniBrainSyncRequest {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const row = value as Record<string, unknown>;
  const expected = [
    'base_cursor', 'device_id', 'device_token', 'issued_at', 'operation', 'payload',
    'payload_digest', 'request_id', 'schema_version', 'sequence', 'session_id', 'signature',
  ].sort();
  if (
    Object.keys(row).sort().join(',') !== expected.join(',')
    || row.schema_version !== MINI_BRAIN_SYNC_REQUEST_SCHEMA
    || !isOpaqueConversationId(row.device_id)
    || !Number.isSafeInteger(row.sequence)
    || Number(row.sequence) < 1
    || Number(row.sequence) > Number.MAX_SAFE_INTEGER
    || typeof row.issued_at !== 'string'
    || !isOpaqueConversationId(row.session_id)
    || !isOpaqueClientRequestId(row.request_id)
    || (row.base_cursor !== null && (typeof row.base_cursor !== 'string' || !SHA256_RE.test(row.base_cursor)))
    || typeof row.payload_digest !== 'string'
    || !SHA256_RE.test(row.payload_digest)
    || typeof row.device_token !== 'string'
    || typeof row.signature !== 'string'
    || !/^[A-Za-z0-9_-]{80,128}$/u.test(row.signature)
  ) return false;
  if (row.operation === 'pull') return row.payload === null;
  if (row.operation !== 'append' || !row.payload || typeof row.payload !== 'object' || Array.isArray(row.payload)) return false;
  const payload = row.payload as Record<string, unknown>;
  return Object.keys(payload).join(',') === 'text'
    && typeof payload.text === 'string'
    && payload.text === payload.text.trim()
    && Buffer.byteLength(payload.text, 'utf8') >= 1
    && Buffer.byteLength(payload.text, 'utf8') <= 16_384;
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

export function issueMiniBrainDeviceCapability(input: {
  readonly userId: string;
  readonly deviceId: string;
  readonly publicKeyJwk: unknown;
  readonly nowMs?: number;
}): { device_token: string; owner_ref: string; key_thumbprint: string; expires_at: string } {
  if (!isOpaqueConversationId(input.deviceId) || !validPublicJwk(input.publicKeyJwk)) {
    throw new MiniBrainRelayError('BRAIN_DEVICE_REGISTRATION_INVALID', 400);
  }
  const nowMs = input.nowMs ?? Date.now();
  const payload: DeviceCapability = {
    schema_version: MINI_BRAIN_DEVICE_SCHEMA,
    device_id: input.deviceId.toLowerCase(),
    owner_ref: miniBrainOwnerRef(input.userId),
    key_thumbprint: keyThumbprint(input.publicKeyJwk),
    public_key_jwk: input.publicKeyJwk,
    issued_at: new Date(nowMs).toISOString(),
    expires_at: new Date(nowMs + DEVICE_TOKEN_TTL_MS).toISOString(),
  };
  return {
    device_token: signCapability(payload),
    owner_ref: payload.owner_ref,
    key_thumbprint: payload.key_thumbprint,
    expires_at: payload.expires_at,
  };
}

export async function verifyMiniBrainSyncRequest(input: {
  readonly body: unknown;
  readonly userId: string;
  readonly nowMs?: number;
}): Promise<VerifiedMiniBrainRequest> {
  if (!exactSyncRequest(input.body)) throw new MiniBrainRelayError('BRAIN_SYNC_REQUEST_INVALID', 400);
  const request = input.body;
  const nowMs = input.nowMs ?? Date.now();
  const issuedAt = Date.parse(request.issued_at);
  if (!Number.isFinite(issuedAt) || Math.abs(nowMs - issuedAt) > REQUEST_CLOCK_SKEW_MS) {
    throw new MiniBrainRelayError('BRAIN_SYNC_REQUEST_EXPIRED', 401);
  }
  const expectedPayloadDigest = digest(canonicalJson(request.payload));
  if (request.payload_digest !== expectedPayloadDigest) {
    throw new MiniBrainRelayError('BRAIN_SYNC_PAYLOAD_MISMATCH', 400);
  }
  const capability = parseCapability(request.device_token, input.userId, nowMs);
  if (capability.device_id !== request.device_id) {
    throw new MiniBrainRelayError('BRAIN_DEVICE_BINDING_MISMATCH', 403);
  }
  let publicKey: CryptoKey;
  try {
    publicKey = await webcrypto.subtle.importKey(
      'jwk', capability.public_key_jwk,
      { name: 'ECDSA', namedCurve: 'P-256' }, false, ['verify'],
    );
  } catch {
    throw new MiniBrainRelayError('BRAIN_DEVICE_TOKEN_INVALID', 401);
  }
  const signingPayload = Buffer.from(syncSigningPayload(unsignedRequest(request)), 'utf8');
  let verified = false;
  try {
    verified = await webcrypto.subtle.verify(
      { name: 'ECDSA', hash: 'SHA-256' },
      publicKey,
      fromBase64Url(request.signature),
      signingPayload,
    );
  } catch {
    verified = false;
  }
  if (!verified) throw new MiniBrainRelayError('BRAIN_DEVICE_SIGNATURE_INVALID', 403);
  return {
    request,
    ownerRef: capability.owner_ref,
    keyThumbprint: capability.key_thumbprint,
  };
}

export function resetMiniBrainRelayStateForTests(): void {
  // Compatibility seam for older test callers. Replay and rate authority now
  // live only in the durable transactional relay-state store.
}

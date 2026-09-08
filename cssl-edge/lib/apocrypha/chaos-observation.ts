import { createHash, createHmac, timingSafeEqual } from 'node:crypto';
import type { IncomingHttpHeaders } from 'node:http';

export const TRINITY_DOMAIN_BOUNDARY_SCHEMA = 'trinity.domain-boundary.v1' as const;
export const TRINITY_SIGNED_REQUEST_SCHEMA = 'trinity.domain-boundary.signed-request.v1' as const;
export const TRINITY_OBSERVATION_ROUTE = '/api/internal/apocrypha/trinity-domain-boundary' as const;
export const CHAOS_SOURCE_PRODUCT = 'chaos-tarot' as const;
export const APOCRYPHA_DESTINATION_ENTITY = 'apocrypha' as const;
export const MAX_CHAOS_OBSERVATION_BODY_BYTES = 65_536;

export type ChaosIngressCode =
  | 'invalid_transport'
  | 'authentication_failed'
  | 'request_stale'
  | 'invalid_contract'
  | 'forbidden_authority_or_state_field'
  | 'raw_private_payload'
  | 'receiver_unavailable';

export class ChaosIngressError extends Error {
  constructor(
    public readonly code: ChaosIngressCode,
    public readonly status: 400 | 401 | 403 | 503,
    public readonly transportAuthenticated = false,
  ) {
    super(`chaos_observation_${code}`);
    this.name = 'ChaosIngressError';
  }
}

export interface VerifiedChaosObservation {
  authenticated: true;
  boundaryDigest: string;
  requestAuthDigest: string;
}

interface VerifyInput {
  rawBody: Buffer;
  headers: IncomingHttpHeaders;
  method: string | undefined;
  url: string | undefined;
  nowMs?: number;
  env?: NodeJS.ProcessEnv;
}

const SAFE_REF = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,255}$/;
const PRINCIPAL_REF = /^ct_[A-Za-z0-9_-]{43}$/;
const TENANT_REF = /^ctt_[A-Za-z0-9_-]{43}$/;
const SHA256 = /^[a-f0-9]{64}$/;
const SIGNATURE = /^v1=([a-f0-9]{64})$/;
const INTEGER_MILLISECONDS = /^(?:0|[1-9]\d{0,15})$/;
const CANONICAL_INSTANT = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/;
const MAX_CLOCK_SKEW_MS = 10 * 60 * 1_000;
const MAX_CONSENT_LIFETIME_MS = 24 * 60 * 60 * 1_000;
const PURPOSE_SCOPE = new Map([
  ['divination.interpretation', 'reading.synthesize'],
  ['divination.followup', 'reading.followup'],
  ['divination.summary', 'reading.summarize'],
  ['astrology.synthesis', 'astrology.synthesize'],
]);
const FORBIDDEN_AUTHORITY_OR_STATE_FIELDS = new Set([
  'stateroot',
  'state_root',
  'parent_root',
  'authority_ref',
  'effect_grant',
  'capability_profile',
  'model_policy',
  'memory_profile',
  'tool_registry',
]);
const RAW_PRIVATE_FIELDS = new Set([
  'access_code',
  'cards',
  'content',
  'conversation_history',
  'email',
  'message',
  'name',
  'payload',
  'prompt',
  'question',
  'reading',
  'source_text',
  'text',
]);

function fail(code: ChaosIngressCode, status: 400 | 401 | 403 | 503): never {
  throw new ChaosIngressError(code, status);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === 'object' && !Array.isArray(value));
}

function exactKeys(value: Record<string, unknown>, expected: readonly string[]): void {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    fail('invalid_contract', 400);
  }
}

function scanForbiddenFields(value: unknown): void {
  const pending: unknown[] = [value];
  while (pending.length > 0) {
    const current = pending.pop();
    if (Array.isArray(current)) {
      pending.push(...current);
      continue;
    }
    if (!isRecord(current)) continue;
    for (const [key, nested] of Object.entries(current)) {
      const normalized = key.toLowerCase();
      if (FORBIDDEN_AUTHORITY_OR_STATE_FIELDS.has(normalized)) {
        fail('forbidden_authority_or_state_field', 403);
      }
      if (RAW_PRIVATE_FIELDS.has(normalized)) fail('raw_private_payload', 400);
      pending.push(nested);
    }
  }
}

export function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  const record = value as Record<string, unknown>;
  return `{${Object.keys(record).sort().map(
    (key) => `${JSON.stringify(key)}:${canonicalJson(record[key])}`,
  ).join(',')}}`;
}

function sha256(value: string | Buffer): string {
  return createHash('sha256').update(value).digest('hex');
}

function constantTimeHexEqual(left: string, right: string): boolean {
  if (!SHA256.test(left) || !SHA256.test(right)) return false;
  return timingSafeEqual(Buffer.from(left, 'hex'), Buffer.from(right, 'hex'));
}

function oneHeader(headers: IncomingHttpHeaders, name: string): string | null {
  const value = headers[name];
  if (typeof value !== 'string' || value !== value.trim()) return null;
  return value;
}

function safeRef(value: unknown, minimum = 1, maximum = 256): value is string {
  return typeof value === 'string'
    && value.length >= minimum
    && value.length <= maximum
    && SAFE_REF.test(value);
}

function canonicalRefs(value: unknown, allowEmpty = false): value is string[] {
  if (!Array.isArray(value) || value.length > 32 || (!allowEmpty && value.length === 0)) return false;
  if (!value.every((item) => safeRef(item))) return false;
  const canonical = [...new Set(value)].sort();
  return canonical.length === value.length && canonical.every((item, index) => item === value[index]);
}

function positiveInteger(value: unknown, maximum: number): value is number {
  return Number.isSafeInteger(value) && Number(value) >= 1 && Number(value) <= maximum;
}

function canonicalInstant(value: unknown): value is string {
  return typeof value === 'string'
    && CANONICAL_INSTANT.test(value)
    && Number.isFinite(Date.parse(value))
    && new Date(value).toISOString() === value;
}

function byteEntropy(bytes: Buffer): number {
  const counts = new Map<number, number>();
  for (const byte of bytes) counts.set(byte, (counts.get(byte) ?? 0) + 1);
  return [...counts.values()].reduce((entropy, count) => {
    const probability = count / bytes.length;
    return entropy - probability * Math.log2(probability);
  }, 0);
}

function hasShortRepeatingPeriod(bytes: Buffer): boolean {
  const largest = Math.min(16, Math.floor(bytes.length / 2));
  for (let period = 1; period <= largest; period += 1) {
    let repeats = true;
    for (let index = period; index < bytes.length; index += 1) {
      if (bytes[index] !== bytes[index % period]) {
        repeats = false;
        break;
      }
    }
    if (repeats) return true;
  }
  return false;
}

function receiverCredential(env: NodeJS.ProcessEnv): { keyId: string; key: Buffer } {
  const keyId = env.CHAOS_TRINITY_OBSERVATION_KEY_ID;
  const encoded = env.CHAOS_TRINITY_OBSERVATION_HMAC_KEY;
  if (!safeRef(keyId, 8, 128) || typeof encoded !== 'string' || !/^[A-Za-z0-9_-]+$/.test(encoded)) {
    fail('receiver_unavailable', 503);
  }
  const key = Buffer.from(encoded, 'base64url');
  if (
    key.toString('base64url') !== encoded
    || key.length < 32
    || new Set(key).size < 16
    || byteEntropy(key) < 3.5
    || hasShortRepeatingPeriod(key)
  ) {
    key.fill(0);
    fail('receiver_unavailable', 503);
  }
  return { keyId, key };
}

function parseAndValidateBoundary(rawBody: Buffer): Record<string, unknown> {
  let value: unknown;
  let decoded: string;
  try {
    decoded = new TextDecoder('utf-8', { fatal: true }).decode(rawBody);
    value = JSON.parse(decoded);
  } catch {
    fail('invalid_contract', 400);
  }
  if (!isRecord(value) || canonicalJson(value) !== decoded) {
    fail('invalid_contract', 400);
  }
  scanForbiddenFields(value);
  exactKeys(value, [
    'schema',
    'source_product',
    'destination_entity',
    'purpose',
    'principal_ref',
    'tenant_ref',
    'consent',
    'authority',
    'provenance',
    'privacy',
    'isolation',
    'boundary_digest',
  ]);
  if (
    value.schema !== TRINITY_DOMAIN_BOUNDARY_SCHEMA
    || value.source_product !== CHAOS_SOURCE_PRODUCT
    || value.destination_entity !== APOCRYPHA_DESTINATION_ENTITY
    || typeof value.purpose !== 'string'
    || !PURPOSE_SCOPE.has(value.purpose)
    || typeof value.principal_ref !== 'string'
    || !PRINCIPAL_REF.test(value.principal_ref)
    || typeof value.tenant_ref !== 'string'
    || !TENANT_REF.test(value.tenant_ref)
    || typeof value.boundary_digest !== 'string'
    || !SHA256.test(value.boundary_digest)
  ) {
    fail('invalid_contract', 400);
  }

  if (!isRecord(value.consent)) fail('invalid_contract', 400);
  exactKeys(value.consent, ['grant_id', 'scope', 'issued_at', 'expires_at', 'revocation_ref']);
  if (
    !safeRef(value.consent.grant_id)
    || !canonicalRefs(value.consent.scope)
    || !canonicalInstant(value.consent.issued_at)
    || !canonicalInstant(value.consent.expires_at)
    || !safeRef(value.consent.revocation_ref)
  ) {
    fail('invalid_contract', 400);
  }
  const issuedAt = Date.parse(value.consent.issued_at);
  const expiresAt = Date.parse(value.consent.expires_at);
  if (expiresAt <= issuedAt || expiresAt - issuedAt > MAX_CONSENT_LIFETIME_MS) {
    fail('invalid_contract', 400);
  }

  if (!isRecord(value.authority)) fail('invalid_contract', 400);
  exactKeys(value.authority, ['capability_id', 'scope', 'resource', 'budget', 'idempotency_key']);
  if (
    !safeRef(value.authority.capability_id)
    || !canonicalRefs(value.authority.scope)
    || !canonicalRefs(value.authority.resource)
    || !safeRef(value.authority.idempotency_key, 16, 128)
    || !isRecord(value.authority.budget)
  ) {
    fail('invalid_contract', 400);
  }
  exactKeys(value.authority.budget, ['max_jobs', 'max_input_bytes', 'max_output_tokens']);
  if (
    !positiveInteger(value.authority.budget.max_jobs, 16)
    || !positiveInteger(value.authority.budget.max_input_bytes, 1_048_576)
    || !positiveInteger(value.authority.budget.max_output_tokens, 262_144)
    || rawBody.length > value.authority.budget.max_input_bytes
  ) {
    fail('invalid_contract', 400);
  }
  const requiredScope = PURPOSE_SCOPE.get(value.purpose);
  const consentScopes = value.consent.scope as string[];
  const authorityScopes = value.authority.scope as string[];
  if (
    !requiredScope
    || !consentScopes.includes(requiredScope)
    || !authorityScopes.every((scope) => consentScopes.includes(scope))
  ) {
    fail('invalid_contract', 400);
  }

  if (!isRecord(value.provenance)) fail('invalid_contract', 400);
  exactKeys(value.provenance, ['request_digest', 'canonical_reading_digest', 'schema_refs', 'build_refs']);
  if (
    typeof value.provenance.request_digest !== 'string'
    || !SHA256.test(value.provenance.request_digest)
    || (value.provenance.canonical_reading_digest !== null
      && (typeof value.provenance.canonical_reading_digest !== 'string'
        || !SHA256.test(value.provenance.canonical_reading_digest)))
    || !canonicalRefs(value.provenance.schema_refs)
    || !canonicalRefs(value.provenance.build_refs)
    || (value.purpose.startsWith('divination.') && value.provenance.canonical_reading_digest === null)
  ) {
    fail('invalid_contract', 400);
  }

  if (!isRecord(value.privacy)) fail('invalid_contract', 400);
  exactKeys(value.privacy, ['class', 'retention', 'training_allowed']);
  if (
    value.privacy.class !== 'restricted'
    || value.privacy.training_allowed !== false
    || !safeRef(value.privacy.retention)
  ) {
    fail('invalid_contract', 400);
  }

  if (!isRecord(value.isolation)) fail('invalid_contract', 400);
  exactKeys(value.isolation, ['canonical_memory_write', 'weight_update', 'effect_execution']);
  if (
    value.isolation.canonical_memory_write !== false
    || value.isolation.weight_update !== false
    || value.isolation.effect_execution !== false
  ) {
    fail('invalid_contract', 400);
  }

  const { boundary_digest: suppliedDigest, ...unsignedBoundary } = value;
  if (!constantTimeHexEqual(suppliedDigest as string, sha256(canonicalJson(unsignedBoundary)))) {
    fail('invalid_contract', 400);
  }
  return value;
}

export function verifyChaosObservationTransport(input: VerifyInput): VerifiedChaosObservation {
  if (
    input.method !== 'POST'
    || input.url !== TRINITY_OBSERVATION_ROUTE
    || !Buffer.isBuffer(input.rawBody)
    || input.rawBody.length < 2
    || input.rawBody.length > MAX_CHAOS_OBSERVATION_BODY_BYTES
    || oneHeader(input.headers, 'content-type') !== 'application/json'
    || oneHeader(input.headers, 'content-length') !== String(input.rawBody.length)
  ) {
    fail('invalid_transport', 400);
  }

  const env = input.env ?? process.env;
  const credential = receiverCredential(env);
  try {
    const keyId = oneHeader(input.headers, 'x-trinity-key-id');
    const timestamp = oneHeader(input.headers, 'x-trinity-timestamp');
    const nonce = oneHeader(input.headers, 'x-trinity-nonce');
    const bodyDigest = oneHeader(input.headers, 'x-trinity-content-sha256');
    const boundaryDigest = oneHeader(input.headers, 'x-trinity-boundary-sha256');
    const principalRef = oneHeader(input.headers, 'x-trinity-principal');
    const tenantRef = oneHeader(input.headers, 'x-trinity-tenant');
    const idempotencyKey = oneHeader(input.headers, 'idempotency-key');
    const requestDigest = oneHeader(input.headers, 'x-trinity-request-sha256');
    const canonicalReadingDigest = oneHeader(input.headers, 'x-trinity-canonical-reading-sha256');
    const rawSignature = oneHeader(input.headers, 'x-trinity-signature');

    if (
      keyId !== credential.keyId
      || !timestamp
      || !INTEGER_MILLISECONDS.test(timestamp)
      || !safeRef(nonce, 16, 128)
      || !bodyDigest
      || !SHA256.test(bodyDigest)
      || !boundaryDigest
      || !SHA256.test(boundaryDigest)
      || !principalRef
      || !PRINCIPAL_REF.test(principalRef)
      || !tenantRef
      || !TENANT_REF.test(tenantRef)
      || !safeRef(idempotencyKey, 16, 128)
      || !requestDigest
      || !SHA256.test(requestDigest)
      || !canonicalReadingDigest
      || (canonicalReadingDigest !== '-' && !SHA256.test(canonicalReadingDigest))
      || !rawSignature
      || !SIGNATURE.test(rawSignature)
    ) {
      fail('authentication_failed', 401);
    }
    const signedAt = Number(timestamp);
    const now = input.nowMs ?? Date.now();
    if (!Number.isSafeInteger(signedAt) || !Number.isSafeInteger(now) || Math.abs(now - signedAt) > MAX_CLOCK_SKEW_MS) {
      fail('request_stale', 401);
    }
    const actualBodyDigest = sha256(input.rawBody);
    if (!constantTimeHexEqual(bodyDigest, actualBodyDigest)) fail('authentication_failed', 401);

    const signatureInput = [
      TRINITY_SIGNED_REQUEST_SCHEMA,
      timestamp,
      nonce,
      'POST',
      TRINITY_OBSERVATION_ROUTE,
      keyId,
      CHAOS_SOURCE_PRODUCT,
      principalRef,
      tenantRef,
      idempotencyKey,
      requestDigest,
      canonicalReadingDigest,
      boundaryDigest,
      bodyDigest,
    ].join('\n');
    const observedSignature = SIGNATURE.exec(rawSignature)?.[1] ?? '';
    const expectedSignature = createHmac('sha256', credential.key).update(signatureInput, 'utf8').digest('hex');
    if (!constantTimeHexEqual(observedSignature, expectedSignature)) fail('authentication_failed', 401);

    let boundary: Record<string, unknown>;
    try {
      boundary = parseAndValidateBoundary(input.rawBody);
    } catch (error) {
      if (error instanceof ChaosIngressError) {
        throw new ChaosIngressError(error.code, error.status, true);
      }
      throw error;
    }
    const authority = boundary.authority as Record<string, unknown>;
    const provenance = boundary.provenance as Record<string, unknown>;
    if (
      boundary.boundary_digest !== boundaryDigest
      || boundary.principal_ref !== principalRef
      || boundary.tenant_ref !== tenantRef
      || authority.idempotency_key !== idempotencyKey
      || provenance.request_digest !== requestDigest
      || (provenance.canonical_reading_digest ?? '-') !== canonicalReadingDigest
    ) {
      fail('invalid_contract', 400);
    }
    const requestAuthDigest = sha256(canonicalJson({
      schema: 'apocrypha.chaos.verified-observation.v1',
      method: 'POST',
      route: TRINITY_OBSERVATION_ROUTE,
      source_product: CHAOS_SOURCE_PRODUCT,
      signature_algorithm: 'HMAC-SHA256',
      source_key_id: keyId,
      timestamp_ms: signedAt,
      nonce,
      boundary_digest: boundaryDigest,
      body_digest: bodyDigest,
      signature_digest: observedSignature,
    }));
    return { authenticated: true, boundaryDigest, requestAuthDigest };
  } finally {
    credential.key.fill(0);
  }
}
